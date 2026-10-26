//! Typed errors for the configuration store.

use std::io;
use std::path::{Path, PathBuf};

use crate::migrate::MigrationError;

/// Errors returned by fallible store operations. Opening never fails; see [`crate::OpenOutcome`].
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("I/O error at {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("the configuration file changed on disk; reload it before committing")]
    ChangedOnDisk,
    #[error("no backup with sequence number {0}")]
    BackupNotFound(u64),
    #[error("backup {seq} is unusable: {reason}")]
    InvalidBackup { seq: u64, reason: String },
    #[error("configuration is corrupt: {0}")]
    Corrupt(String),
    #[error(transparent)]
    Migration(#[from] MigrationError),
    #[error("cannot serialize configuration: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Builds a mapper that attaches `path` to an I/O error.
pub(crate) fn io_at(path: &Path) -> impl FnOnce(io::Error) -> ConfigError + '_ {
    move |source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    }
}
