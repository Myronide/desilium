//! Describes every automatic repair made while opening or reloading the configuration.

use std::fmt;
use std::path::PathBuf;

use crate::migrate::MigrationNote;
use crate::validate::Repair;

/// One repair the store performed, for display in the settings UI or logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recovery {
    /// The file could not be parsed. It was moved aside (if possible) and defaults are used.
    Corrupt {
        reason: String,
        moved_to: Option<PathBuf>,
    },
    /// The file exists but could not be read. Defaults are used and the file is left alone.
    Unreadable { reason: String },
    /// The file was written by a newer Desilium. It was loaded best-effort.
    NewerVersion { found: u32 },
    /// A schema migration step ran.
    Migrated(MigrationNote),
    /// A single field had an unusable value and was replaced by its default.
    InvalidField { field: String, reason: String },
    /// A value was clamped or repaired by validation.
    Repaired(Repair),
}

impl fmt::Display for Recovery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Corrupt {
                reason,
                moved_to: Some(path),
            } => write!(
                f,
                "configuration was unreadable ({reason}); moved to {} and defaults are in use",
                path.display()
            ),
            Self::Corrupt {
                reason,
                moved_to: None,
            } => write!(
                f,
                "configuration was unreadable ({reason}); defaults are in use"
            ),
            Self::Unreadable { reason } => {
                write!(
                    f,
                    "configuration could not be read ({reason}); defaults are in use"
                )
            }
            Self::NewerVersion { found } => write!(
                f,
                "configuration was written by a newer version ({found}); loaded best-effort"
            ),
            Self::Migrated(note) => note.fmt(f),
            Self::InvalidField { field, reason } => {
                write!(f, "{field} was invalid ({reason}); default used")
            }
            Self::Repaired(repair) => f.write_str(&repair.message),
        }
    }
}
