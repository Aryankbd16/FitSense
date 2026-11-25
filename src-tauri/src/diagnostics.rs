use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

const ROTATE_AT: u64 = 256 * 1024;
const DAY: u64 = 86_400;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    AppStarted,
    AppStopped,
    AppConfigurationError,
    StorageReady,
    StorageFailed,
    VaultOperationError,
    VaultIntegrityError,
    WorkerLaunching,
    WorkerReady,
    WorkerLaunchFailed,
    WorkerTimeout,
    WorkerExited,
    WorkerProtocolError,
    WorkerRemoteError,
    WorkerDiagnostics,
    WorkerForcedStop,
    PythonStarted,
    PythonInvalidFrame,
    PythonInvalidRequest,
    PythonStopped,
    PythonFatal,
}

impl Event {
    pub fn from_stderr(line: &[u8]) -> Option<Self> {
        match line {
            b"worker_started" => Some(Self::PythonStarted),
            b"worker_invalid_frame" => Some(Self::PythonInvalidFrame),
            b"worker_invalid_request" => Some(Self::PythonInvalidRequest),
            b"worker_stopped" => Some(Self::PythonStopped),
            b"worker_fatal" => Some(Self::PythonFatal),
            _ => None,
        }
    }

    fn source(self) -> &'static str {
        match self {
            Self::PythonStarted
            | Self::PythonInvalidFrame
            | Self::PythonInvalidRequest
            | Self::PythonStopped
            | Self::PythonFatal => "python",
            _ => "rust",
        }
    }
}

#[derive(Serialize)]
struct Record {
    unix_ms: u128,
    source: &'static str,
    event: Event,
    elapsed_ms: Option<u128>,
}

struct Segment {
    file: File,
    bytes: u64,
    index: u64,
}

pub struct Diagnostics {
    directory: PathBuf,
    prefix: String,
    segment: Mutex<Option<Segment>>,
    healthy: AtomicBool,
    retention: Duration,
    rotate_at: u64,
}

impl Diagnostics {
    pub fn open(directory: &Path, retention: Duration) -> Self {
        let prefix = format!("session-{}-{}", std::process::id(), unix_time().as_nanos());
        let logs = Self {
            directory: directory.into(),
            prefix,
            segment: Mutex::new(None),
            healthy: AtomicBool::new(true),
            retention,
            rotate_at: ROTATE_AT,
        };
        if logs.prune(None).is_err() {
            logs.healthy.store(false, Ordering::Relaxed);
        }
        logs
    }

    pub fn available(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }

    pub fn record(&self, event: Event, elapsed: Option<Duration>) {
        let row = Record {
            unix_ms: unix_time().as_millis(),
            source: event.source(),
            event,
            elapsed_ms: elapsed.map(|value| value.as_millis()),
        };
        if self.append(row).is_err() {
            self.healthy.store(false, Ordering::Relaxed);
        }
    }

    fn append(&self, row: Record) -> io::Result<()> {
        let mut line = serde_json::to_vec(&row)?;
        line.push(b'\n');
        let mut active = self.segment.lock().map_err(|_| io::ErrorKind::Other)?;
        let bytes = line.len() as u64;
        if active
            .as_ref()
            .is_none_or(|segment| segment.bytes + bytes > self.rotate_at)
        {
            let index = active.as_ref().map_or(0, |segment| segment.index + 1);
            let path = self
                .directory
                .join(format!("{}-{index}.jsonl", self.prefix));
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            *active = Some(Segment {
                file,
                bytes: 0,
                index,
            });
            if self.prune(Some(&path)).is_err() {
                self.healthy.store(false, Ordering::Relaxed);
            }
        }
        let segment = active.as_mut().expect("open log segment");
        segment.file.write_all(&line)?;
        segment.bytes += bytes;
        Ok(())
    }

    fn prune(&self, active_path: Option<&Path>) -> io::Result<()> {
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let path = entry.path();
            if is_log_segment(name)
                && active_path != Some(path.as_path())
                && entry.file_type()?.is_file()
            {
                let metadata = match entry.metadata() {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error),
                };
                let elapsed = metadata.modified()?.elapsed().unwrap_or_default();
                if elapsed >= self.retention {
                    match fs::remove_file(path) {
                        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}

fn is_log_segment(name: &str) -> bool {
    name.strip_prefix("session-")
        .and_then(|name| name.strip_suffix(".jsonl"))
        .is_some_and(|name| {
            name.split('-').count() == 3
                && name
                    .split('-')
                    .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        })
}

fn unix_time() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

pub fn retention(days: Option<&str>) -> Option<Duration> {
    let days = match days {
        Some(value) => value.parse().ok()?,
        None => 7_u64,
    };
    (1..=90)
        .contains(&days)
        .then(|| Duration::from_secs(days * DAY))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_rotate_and_never_copy_untrusted_stderr() {
        let temp = tempfile::tempdir().unwrap();
        let mut logs = Diagnostics::open(temp.path(), retention(None).unwrap());
        logs.rotate_at = 128;
        let secret = b"portrait.jpg; personal measurement; C:\\Users\\someone";
        assert!(Event::from_stderr(secret).is_none());
        logs.record(Event::AppStarted, None);
        logs.record(
            Event::from_stderr(b"worker_started").unwrap(),
            Some(Duration::from_millis(4)),
        );
        let entries: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(entries.len(), 2);
        for entry in entries {
            let line = fs::read_to_string(entry).unwrap();
            let row: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(row.as_object().unwrap().len(), 4);
            assert!(!line.contains("portrait.jpg") && !line.contains("someone"));
        }
    }

    #[test]
    fn expiry_only_touches_owned_segment_names() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("session-2-3-4.jsonl"), "old").unwrap();
        fs::write(temp.path().join("session-personal.jsonl"), "private").unwrap();
        Diagnostics::open(temp.path(), Duration::ZERO);
        assert!(!temp.path().join("session-2-3-4.jsonl").exists());
        assert!(temp.path().join("session-personal.jsonl").exists());
        assert!(retention(Some("0")).is_none() && retention(Some("invalid")).is_none());
    }
}
