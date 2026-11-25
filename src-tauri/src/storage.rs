use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, TransactionBehavior};
use serde::Serialize;

use crate::images::Variant;

const MARKER: &str = "FitSense.portable";
const SCHEMA_VERSION: u32 = 1;
const DIRECTORIES: &[&str] = &["db", "backups", "logs", "webview"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Installed,
    Portable,
    Development,
}

pub struct Storage {
    pub root: PathBuf,
    pub mode: Mode,
}

impl Storage {
    pub fn resolve(executable: &Path, installed: PathBuf, development: bool) -> io::Result<Self> {
        let binary_dir = executable.parent().ok_or(io::ErrorKind::InvalidInput)?;
        match fs::symlink_metadata(binary_dir.join(MARKER)) {
            Ok(marker) if marker.is_file() => {
                return Ok(Self {
                    root: binary_dir.join("data"),
                    mode: Mode::Portable,
                })
            }
            Ok(_) => return Err(io::ErrorKind::InvalidInput.into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let (root, mode) = if development {
            (installed.with_extension("development"), Mode::Development)
        } else {
            (installed, Mode::Installed)
        };
        Ok(Self { root, mode })
    }

    pub fn initialize_directories(&self) -> io::Result<()> {
        for directory in DIRECTORIES
            .iter()
            .copied()
            .chain(Variant::ALL.map(Variant::directory))
        {
            fs::create_dir_all(self.root.join(directory))?;
        }
        Ok(())
    }

    pub fn initialize_database(&self) -> rusqlite::Result<u32> {
        let mut db = Connection::open(self.root.join("db/foundation.sqlite3"))?;
        db.pragma_update(None, "foreign_keys", true)?;
        db.pragma_update(None, "temp_store", "MEMORY")?;
        let migration = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: u32 = migration.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            0 => migration.pragma_update(None, "user_version", SCHEMA_VERSION)?,
            SCHEMA_VERSION => {}
            _ => return Err(rusqlite::Error::InvalidQuery),
        }
        migration.commit()?;
        Ok(SCHEMA_VERSION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distribution_modes_share_the_layout_and_database_version() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("fitsense.exe");
        let local = temp.path().join("local/com.fitsense.desktop");
        let installed = Storage::resolve(&binary, local.clone(), false).unwrap();
        assert_eq!(installed.root, local);
        assert_eq!(installed.mode, Mode::Installed);
        let development = Storage::resolve(&binary, local.clone(), true).unwrap();
        assert_ne!(development.root, installed.root);
        fs::write(temp.path().join(MARKER), "").unwrap();
        let portable = Storage::resolve(&binary, local, false).unwrap();
        assert_eq!(portable.root, temp.path().join("data"));
        assert_eq!(portable.mode, Mode::Portable);

        for storage in [installed, portable] {
            storage.initialize_directories().unwrap();
            assert!(DIRECTORIES
                .iter()
                .all(|directory| storage.root.join(directory).is_dir()));
            assert!(Variant::ALL
                .iter()
                .all(|variant| storage.root.join(variant.directory()).is_dir()));
            assert_eq!(storage.initialize_database().unwrap(), 1);
            assert_eq!(storage.initialize_database().unwrap(), 1);
        }
    }

    #[test]
    fn future_schema_and_invalid_marker_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage {
            root: temp.path().into(),
            mode: Mode::Portable,
        };
        storage.initialize_directories().unwrap();
        let db = Connection::open(temp.path().join("db/foundation.sqlite3")).unwrap();
        db.pragma_update(None, "user_version", 99).unwrap();
        drop(db);
        assert!(storage.initialize_database().is_err());
        fs::create_dir(temp.path().join(MARKER)).unwrap();
        assert!(
            Storage::resolve(&temp.path().join("fitsense.exe"), temp.path().into(), false).is_err()
        );
    }
}
