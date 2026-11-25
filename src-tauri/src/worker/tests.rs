use std::fs;
use std::path::Path;

use super::*;

fn python() -> std::path::PathBuf {
    let path: std::path::PathBuf = std::env::var_os("FITSENSE_PYTHON")
        .expect("Set FITSENSE_PYTHON to the absolute path of Python for the process tests")
        .into();
    assert!(
        path.is_absolute() && path.is_file(),
        "FITSENSE_PYTHON must point to a local Python executable"
    );
    path
}

fn script() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../python-worker/worker.py")
}

fn logger(path: &Path) -> Arc<Diagnostics> {
    Arc::new(Diagnostics::open(path, Duration::from_secs(86_400)))
}

fn wait_for(worker: &Supervisor, status: Status) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let snapshot = worker.snapshot();
        if snapshot.status == status {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "expected {status:?}, got {:?} / {:?}",
            snapshot.status,
            snapshot.failure
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn real_worker_handshake_heartbeat_diagnostics_and_graceful_exit() {
    let temp = tempfile::tempdir().unwrap();
    let config = WorkerConfig {
        python: python(),
        script: script(),
    };
    let worker = Supervisor::start(config, logger(temp.path())).unwrap();
    assert!(wait_for(&worker, Status::Ready).latency_ms.is_some());
    let report = worker.diagnose().unwrap();
    assert_eq!(report.protocol_version, VERSION);
    assert!(report.python_version.starts_with("3."));
    worker.stop();
    let log = fs::read_dir(temp.path())
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<String>();
    for event in ["worker_ready", "worker_diagnostics", "python_stopped"] {
        assert!(log.contains(event), "missing {event}");
    }
    assert!(!log.contains("worker_forced_stop"));
}

#[test]
fn supervisor_detects_early_exit_and_restarts_on_request() {
    let temp = tempfile::tempdir().unwrap();
    let replacement = temp.path().join("worker.py");
    fs::write(&replacement, "import sys\nsys.exit(27)\n").unwrap();
    let worker = Supervisor::start(
        WorkerConfig {
            python: python(),
            script: replacement.clone(),
        },
        logger(temp.path()),
    )
    .unwrap();
    assert_eq!(
        wait_for(&worker, Status::Unavailable).failure,
        Some(Failure::Exited)
    );
    fs::copy(script(), replacement).unwrap();
    worker.retry().unwrap();
    wait_for(&worker, Status::Ready);
    assert!(worker.diagnose().is_ok());
    worker.stop();
}

#[test]
fn protocol_mismatch_and_response_timeout_are_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("fixture.py");
    fs::write(&fixture, r#"
import json
import sys
import time
print(json.dumps({'type': 'ready', 'version': 1, 'service': 'fitsense-worker'}))
sys.stdin.readline()
print(json.dumps({'type': 'response', 'version': 1, 'id': 'incorrect-id', 'result': {'status': 'ok'}}))
sys.stdin.readline()
time.sleep(30)
"#).unwrap();
    let config = WorkerConfig {
        python: python(),
        script: fixture,
    };
    let mut child = WorkerProcess::launch(&config, logger(temp.path())).unwrap();
    let stop = AtomicBool::new(false);
    child
        .ready(Instant::now() + STARTUP_TIMEOUT, Some(&stop))
        .unwrap();
    assert_eq!(
        child.health(Instant::now() + RESPONSE_TIMEOUT, &stop),
        Err(WireError::Protocol)
    );
    assert_eq!(
        child.health(Instant::now() + Duration::from_millis(100), &stop),
        Err(WireError::Timeout)
    );
    stop.store(true, Ordering::Relaxed);
    assert_eq!(
        child.health(Instant::now() + RESPONSE_TIMEOUT, &stop),
        Err(WireError::Cancelled)
    );
}
