use std::io::{self, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::config::{WorkerConfig, SHUTDOWN_TIMEOUT};
use crate::diagnostics::{Diagnostics, Event};
use crate::protocol::{self, Frame, Method, Payload, Request, WireError, WorkerState};

const CHECK_INTERVAL: Duration = Duration::from_millis(25);

pub struct WorkerProcess {
    child: Child,
    #[cfg(windows)]
    job: Option<std::os::windows::io::OwnedHandle>,
    input: Option<mpsc::SyncSender<Vec<u8>>>,
    output: mpsc::Receiver<Result<Frame, WireError>>,
    threads: Vec<JoinHandle<()>>,
    next_id: u64,
    logs: Arc<Diagnostics>,
}

impl WorkerProcess {
    pub fn launch(config: &WorkerConfig, logs: Arc<Diagnostics>) -> io::Result<Self> {
        let mut command = Command::new(&config.python);
        command
            .args(["-I", "-B", "-u"])
            .arg(&config.script)
            .current_dir(config.script.parent().ok_or(io::ErrorKind::InvalidInput)?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        }

        let child = command.spawn()?;
        let (send_frame, output) = mpsc::sync_channel(2);
        let (input, receive_input) = mpsc::sync_channel::<Vec<u8>>(1);
        let mut process = Self {
            child,
            input: Some(input),
            output,
            threads: Vec::new(),
            next_id: 0,
            logs,
            #[cfg(windows)]
            job: None,
        };
        #[cfg(windows)]
        {
            process.job = Some(super::job::attach(&process.child)?);
        }

        let mut stdin = process.child.stdin.take().expect("piped stdin");
        let stdout = process.child.stdout.take().expect("piped stdout");
        let stderr = process.child.stderr.take().expect("piped stderr");
        let writer_events = send_frame.clone();
        process
            .threads
            .push(
                thread::Builder::new()
                    .name("worker-stdin".into())
                    .spawn(move || {
                        for bytes in receive_input {
                            if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
                                let _ = writer_events.try_send(Err(WireError::Exited));
                                break;
                            }
                        }
                    })?,
            );
        process
            .threads
            .push(
                thread::Builder::new()
                    .name("worker-stdout".into())
                    .spawn(move || {
                        let mut reader = BufReader::new(stdout);
                        loop {
                            let frame =
                                match protocol::read_frame(&mut reader, protocol::MAX_FRAME, false)
                                {
                                    Ok(Some(line)) => Frame::decode(&line),
                                    Ok(None) => break,
                                    Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                                        Err(WireError::Protocol)
                                    }
                                    Err(_) => Err(WireError::Exited),
                                };
                            let failed = frame.is_err();
                            if send_frame.try_send(frame).is_err() || failed {
                                break;
                            }
                        }
                        let _ = send_frame.try_send(Err(WireError::Exited));
                    })?,
            );
        let python_logs = Arc::clone(&process.logs);
        process
            .threads
            .push(
                thread::Builder::new()
                    .name("worker-stderr".into())
                    .spawn(move || {
                        let mut reader = BufReader::new(stderr);
                        while let Ok(Some(line)) = protocol::read_frame(&mut reader, 128, true) {
                            if let Some(event) = Event::from_stderr(&line) {
                                python_logs.record(event, None);
                            }
                        }
                    })?,
            );
        Ok(process)
    }

    pub fn ready(
        &self,
        deadline: Instant,
        cancelled: Option<&AtomicBool>,
    ) -> Result<(), WireError> {
        self.receive(deadline, cancelled)?.ready()
    }

    pub fn request(
        &mut self,
        method: Method,
        deadline: Instant,
        cancelled: Option<&AtomicBool>,
    ) -> Result<Payload, WireError> {
        self.next_id += 1;
        let id = self.next_id.to_string();
        let request = Request {
            version: protocol::VERSION,
            id: &id,
            method,
        };
        let mut wire = serde_json::to_vec(&request).map_err(|_| WireError::Protocol)?;
        wire.push(b'\n');
        self.input
            .as_ref()
            .ok_or(WireError::Exited)?
            .try_send(wire)
            .map_err(|_| WireError::Exited)?;
        self.receive(deadline, cancelled)?.correlate(&id)
    }

    pub fn health(&mut self, deadline: Instant, cancelled: &AtomicBool) -> Result<(), WireError> {
        match self.request(Method::Health, deadline, Some(cancelled))? {
            Payload::Status {
                status: WorkerState::Ok,
            } => Ok(()),
            _ => Err(WireError::Protocol),
        }
    }

    fn receive(
        &self,
        deadline: Instant,
        cancelled: Option<&AtomicBool>,
    ) -> Result<Frame, WireError> {
        loop {
            if cancelled.is_some_and(|signal| signal.load(Ordering::Relaxed)) {
                return Err(WireError::Cancelled);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(WireError::Timeout);
            }
            match self.output.recv_timeout(remaining.min(CHECK_INTERVAL)) {
                Ok(frame) => return frame,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(WireError::Exited),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    pub fn exited(&mut self) -> bool {
        !matches!(self.child.try_wait(), Ok(None))
    }

    pub fn shutdown(mut self) {
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        let acknowledged = matches!(
            self.request(Method::Shutdown, deadline, None),
            Ok(Payload::Status {
                status: WorkerState::Stopping
            })
        );
        drop(self.input.take());
        while Instant::now() < deadline {
            if let Ok(Some(status)) = self.child.try_wait() {
                if !status.success() || !acknowledged {
                    self.logs.record(Event::WorkerExited, None);
                }
                return;
            }
            thread::sleep(CHECK_INTERVAL);
        }
    }
}

impl Drop for WorkerProcess {
    fn drop(&mut self) {
        drop(self.input.take());
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            self.logs.record(Event::WorkerForcedStop, None);
            #[cfg(windows)]
            drop(self.job.take());
            let _ = self.child.kill();
            if self.child.wait().is_err() {
                self.logs.record(Event::WorkerExited, None);
            }
        }
        for thread in self.threads.drain(..) {
            if thread.join().is_err() {
                self.logs.record(Event::WorkerExited, None);
            }
        }
    }
}
