mod backup;
pub mod commands;
mod database;
mod files;
mod media;
mod model;
mod service;

use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Error {
    Busy,
    Locked,
    Exists,
    InvalidInput,
    InvalidPassphrase,
    WrongPassphrase,
    LocalKey,
    EncryptionUnavailable,
    Database,
    Integrity,
    Io,
    InvalidImage,
    ImageMissing,
    NotFound,
    Incompatible,
    Cancelled,
    Cleanup,
}

impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}

impl From<rusqlite::Error> for Error {
    fn from(_: rusqlite::Error) -> Self {
        Self::Database
    }
}

pub type Result<T> = std::result::Result<T, Error>;
