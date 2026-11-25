#[cfg(windows)]
mod job;
mod process;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{WorkerConfig, HEALTH_INTERVAL, RESPONSE_TIMEOUT, STARTUP_TIMEOUT};
use crate::diagnostics::{Diagnostics, Event};
use crate::protocol::{Method, Payload, WireError, VERSION};
use process::WorkerProcess;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Starting,
    Ready,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    Launch,
    Timeout,
    Exited,
    Protocol,
    WorkerError,
    Busy,
    Unavailable,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub status: Status,
    pub failure: Option<Failure>,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    python_version: String,
    protocol_version: u16,
    response_ms: u64,
}

enum Operation {
    Retry,
    Diagnose {
        deadline: Instant,
        reply: mpsc::SyncSender<Result<Report, Failure>>,
    },
}

pub struct Supervisor {
    snapshot: Arc<Mutex<Snapshot>>,
    operations: mpsc::SyncSender<Operation>,
    cancelled: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Supervisor {
    pub fn start(config: WorkerConfig, logs: Arc<Diagnostics>) -> io::Result<Self> {
        let snapshot = Arc::new(Mutex::new(Snapshot {
            status: Status::Starting,
            failure: None,
            latency_ms: None,
        }));
        let (operations, pending) = mpsc::sync_channel(4);
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut actor = Actor {
            config,
            logs,
            pending,
            snapshot: Arc::clone(&snapshot),
            cancelled: Arc::clone(&cancelled),
            child: None,
        };
        let thread = thread::Builder::new()
            .name("fitsense-worker".into())
            .spawn(move || actor.run())?;
        Ok(Self {
            snapshot,
            operations,
            cancelled,
            thread: Mutex::new(Some(thread)),
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        *self.snapshot.lock().expect("snapshot lock")
    }

    pub fn retry(&self) -> Result<(), Failure> {
        self.submit(Operation::Retry)
    }

    pub fn diagnose(&self) -> Result<Report, Failure> {
        if self.snapshot().status != Status::Ready {
            return Err(Failure::Unavailable);
        }
        let (reply, result) = mpsc::sync_channel(1);
        let limit = RESPONSE_TIMEOUT + Duration::from_secs(1);
        self.submit(Operation::Diagnose {
            deadline: Instant::now() + limit,
            reply,
        })?;
        result.recv_timeout(limit).map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => Failure::Timeout,
            mpsc::RecvTimeoutError::Disconnected => Failure::Unavailable,
        })?
    }

    pub fn stop(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.lock().expect("supervisor lock").take() {
            if thread.join().is_err() {
                *self.snapshot.lock().expect("snapshot lock") = Snapshot {
                    status: Status::Unavailable,
                    failure: Some(Failure::Exited),
                    latency_ms: None,
                };
            }
        }
    }

    fn submit(&self, operation: Operation) -> Result<(), Failure> {
        self.operations
            .try_send(operation)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => Failure::Busy,
                mpsc::TrySendError::Disconnected(_) => Failure::Unavailable,
            })
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Actor {
    config: WorkerConfig,
    logs: Arc<Diagnostics>,
    pending: mpsc::Receiver<Operation>,
    snapshot: Arc<Mutex<Snapshot>>,
    cancelled: Arc<AtomicBool>,
    child: Option<WorkerProcess>,
}

impl Actor {
    fn run(&mut self) {
        self.launch();
        let mut next_health = Instant::now() + HEALTH_INTERVAL;
        while !self.cancelled.load(Ordering::Relaxed) {
            if self.child.as_mut().is_some_and(WorkerProcess::exited) {
                self.fail(WireError::Exited);
            }
            if Instant::now() >= next_health {
                self.heartbeat();
                next_health = Instant::now() + HEALTH_INTERVAL;
            }
            match self.pending.recv_timeout(Duration::from_millis(100)) {
                Ok(Operation::Retry) if self.child.is_none() => {
                    self.launch();
                    next_health = Instant::now() + HEALTH_INTERVAL;
                }
                Ok(Operation::Retry) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                Ok(Operation::Diagnose { deadline, reply })
                    if !self.cancelled.load(Ordering::Relaxed) =>
                {
                    self.diagnose(deadline, reply);
                }
                _ => break,
            }
        }
        if let Some(child) = self.child.take() {
            child.shutdown();
        }
    }

    fn launch(&mut self) {
        self.publish(Status::Starting, None, None);
        self.logs.record(Event::WorkerLaunching, None);
        let started = Instant::now();
        let child = match WorkerProcess::launch(&self.config, Arc::clone(&self.logs)) {
            Ok(child) => child,
            Err(_) => {
                self.publish(Status::Unavailable, Some(Failure::Launch), None);
                self.logs
                    .record(Event::WorkerLaunchFailed, Some(started.elapsed()));
                return;
            }
        };
        if let Err(error) = child.ready(started + STARTUP_TIMEOUT, Some(&self.cancelled)) {
            self.fail(error);
            return;
        }
        self.child = Some(child);
        self.heartbeat();
        if self.child.is_some() {
            self.logs
                .record(Event::WorkerReady, Some(started.elapsed()));
        }
    }

    fn heartbeat(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        let start = Instant::now();
        match child.health(start + RESPONSE_TIMEOUT, &self.cancelled) {
            Ok(()) => self.publish(
                Status::Ready,
                None,
                Some(start.elapsed().as_millis() as u64),
            ),
            Err(error) => {
                self.fail(error);
            }
        }
    }

    fn diagnose(&mut self, deadline: Instant, reply: mpsc::SyncSender<Result<Report, Failure>>) {
        let start = Instant::now();
        let result = if start >= deadline {
            Err(Failure::Timeout)
        } else if let Some(child) = self.child.as_mut() {
            let response = child
                .request(
                    Method::Diagnostics,
                    start + RESPONSE_TIMEOUT,
                    Some(&self.cancelled),
                )
                .and_then(decode_diagnostics);
            match response {
                Ok(python_version) => {
                    self.logs
                        .record(Event::WorkerDiagnostics, Some(start.elapsed()));
                    Ok(Report {
                        python_version,
                        protocol_version: VERSION,
                        response_ms: start.elapsed().as_millis() as u64,
                    })
                }
                Err(WireError::Remote(_)) => {
                    self.logs.record(Event::WorkerRemoteError, None);
                    Err(Failure::WorkerError)
                }
                Err(error) => Err(self.fail(error)),
            }
        } else {
            Err(Failure::Unavailable)
        };
        // A caller may have navigated away or reached its deadline.
        let _ = reply.send(result);
    }

    fn fail(&mut self, error: WireError) -> Failure {
        let (failure, event) = match error {
            WireError::Timeout => (Failure::Timeout, Event::WorkerTimeout),
            WireError::Exited => (Failure::Exited, Event::WorkerExited),
            WireError::Protocol => (Failure::Protocol, Event::WorkerProtocolError),
            WireError::Remote(_) => (Failure::WorkerError, Event::WorkerRemoteError),
            WireError::Cancelled => {
                drop(self.child.take());
                return Failure::Unavailable;
            }
        };
        self.publish(Status::Unavailable, Some(failure), None);
        self.logs.record(event, None);
        drop(self.child.take());
        failure
    }

    fn publish(&self, status: Status, failure: Option<Failure>, latency_ms: Option<u64>) {
        *self.snapshot.lock().expect("snapshot lock") = Snapshot {
            status,
            failure,
            latency_ms,
        };
    }
}

fn decode_diagnostics(payload: Payload) -> Result<String, WireError> {
    let Payload::Diagnostics { python_version } = payload else {
        return Err(WireError::Protocol);
    };
    let parts: Vec<_> = python_version.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty() || part.len() > 4 || !part.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(WireError::Protocol);
    }
    Ok(python_version)
}

#[cfg(test)]
mod tests;
