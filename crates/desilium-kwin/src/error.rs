//! Errors of the KWin bridge. None of them is fatal for the desktop session.

use std::io;

/// Why talking to KWin, KGlobalAccel or KConfig failed.
#[derive(Debug, thiserror::Error)]
pub enum KwinError {
    /// The session bus or a service on it failed.
    #[error("D-Bus call failed: {0}")]
    Bus(#[from] zbus::Error),
    /// A D-Bus service answered with an error.
    #[error("D-Bus service error: {0}")]
    Service(#[from] zbus::fdo::Error),
    /// `kwriteconfig6` could not be started.
    #[error("cannot start {program}: {source}")]
    Spawn {
        /// Program that failed to start.
        program: String,
        /// Operating system error.
        source: io::Error,
    },
    /// `kwriteconfig6` ran and reported failure.
    #[error("{program} failed ({status}): {stderr}")]
    ConfigWrite {
        /// Program that failed.
        program: String,
        /// Exit status as text.
        status: String,
        /// First part of its standard error.
        stderr: String,
    },
    /// An external program did not finish in time and was killed.
    #[error("{program} did not finish within {seconds} s")]
    Timeout {
        /// Program that was killed.
        program: String,
        /// The limit that was exceeded.
        seconds: u64,
    },
    /// A window uuid contained characters outside a uuid's alphabet.
    #[error("not a window uuid: {0:?}")]
    BadUuid(String),
    /// An input device name that cannot be one element of an object path.
    #[error("not an input device name: {0:?}")]
    BadDeviceName(String),
    /// An input device property that is missing or has an unexpected type.
    #[error("input device property {0} is missing or not a bool or a number")]
    BadProperty(String),
    /// A file or thread could not be created or removed.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// KWin did not accept a script.
    #[error("KWin rejected the script: {0}")]
    ScriptRejected(String),
    /// No runtime directory to put script files in.
    #[error("XDG_RUNTIME_DIR is not set")]
    NoRuntimeDir,
}
