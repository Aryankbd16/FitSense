use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(8);
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);
pub const HEALTH_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(debug_assertions)]
const DEV_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[derive(Clone)]
pub struct WorkerConfig {
    pub python: PathBuf,
    pub script: PathBuf,
}

impl WorkerConfig {
    pub fn resolve(resource_dir: &Path) -> io::Result<Self> {
        #[cfg(debug_assertions)]
        if let Some(python) = std::env::var_os("FITSENSE_PYTHON") {
            let python = PathBuf::from(python);
            if !python.is_absolute() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "expected absolute Python path",
                ));
            }
            return Ok(Self {
                python,
                script: Path::new(DEV_ROOT).join("../python-worker/worker.py"),
            });
        }

        let bundled = resource_dir.join("runtime/worker");
        Ok(Self {
            python: bundled.join("python.exe"),
            script: bundled.join("worker.py"),
        })
    }
}
