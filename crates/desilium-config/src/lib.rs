//! Persistent, transactional configuration store for Desilium.
//!
//! Rules from `concept.md` sections 3 and 4:
//! - Invalid configuration never prevents startup. [`ConfigStore::open`] always
//!   succeeds and lists every repair as a [`Recovery`].
//! - Changes go through a [`Transaction`]. Edits happen on a copy; `commit`
//!   validates, backs up the previous file, writes atomically and notifies
//!   listeners. Dropping a transaction discards it.
//! - The last [`KEEP_BACKUPS`] versions are kept as `backups/config-<seq>.toml`
//!   and can be restored with [`ConfigStore::rollback_to`].
//! - External edits are detected by [`ConfigStore::reload_if_changed`]. A
//!   broken external edit is rejected and the last good configuration stays active.
//! - TOML is the storage format only. Callers use the typed API.
//!
//! Operational notes:
//! - First run: defaults are held in memory and no file is created until a commit
//!   changes something.
//! - Corrupt or truncated files are moved to `config.toml.corrupt-<n>`, never deleted.
//! - Files are written `0600` and directories `0700`. This is Unix-only.
//! - Sensitive values (passwords, tokens, keys) must never be stored here. The file is plain text.
//! - Concurrency: the store is single-threaded by design. All writes take `&mut self`,
//!   so transactions cannot overlap. Another process editing the file is detected
//!   as an external change (commit refuses, reload applies or rejects); it is never merged.

mod error;
mod load;
pub mod migrate;
mod recovery;
mod storage;
mod store;
mod transaction;
mod validate;

pub use desilium_config_types::{AppearanceSettings, CURRENT_VERSION, DesiliumConfig};

pub use error::ConfigError;
pub use recovery::Recovery;
pub use storage::{BackupInfo, KEEP_BACKUPS};
pub use store::{ConfigStore, OpenOutcome, ReloadOutcome};
pub use transaction::{CommitReport, Transaction};
pub use validate::{Repair, ValidationReport, validate};
