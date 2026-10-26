//! The configuration store: opening, committing, backups, rollback and external edits.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::PathBuf;

use desilium_config_types::DesiliumConfig;

use crate::error::{ConfigError, io_at};
use crate::load::load_document;
use crate::recovery::Recovery;
use crate::storage::{self, BackupInfo, Fingerprint, Layout, Snapshot};
use crate::transaction::{CommitReport, Transaction};
use crate::validate::validate;

const APP_DIR_NAME: &str = "desilium";
const FALLBACK_CONFIG_BASE: &str = ".config";

type Listener = Box<dyn Fn(&DesiliumConfig, &DesiliumConfig) + Send>;

/// The store plus every repair needed to open it. Opening never fails.
pub struct OpenOutcome {
    pub store: ConfigStore,
    pub recovery: Vec<Recovery>,
}

/// Result of checking the file for an external edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadOutcome {
    /// The file on disk is usable and is now the active configuration.
    Applied { recovery: Vec<Recovery> },
    /// The file on disk is unusable; the last good configuration stays active.
    Rejected { reason: String },
}

/// Owns the active configuration and its file. All writes take `&mut self`.
pub struct ConfigStore {
    layout: Layout,
    current: DesiliumConfig,
    /// Identity of the file as last read or written. `None` means no file.
    disk: Option<Fingerprint>,
    listeners: Vec<Listener>,
}

impl ConfigStore {
    /// Opens the store in `dir`. A missing file means defaults; a broken file is quarantined.
    pub fn open(dir: impl Into<PathBuf>) -> OpenOutcome {
        let layout = Layout::new(dir.into());
        let mut recovery = Vec::new();
        let (current, disk) = match storage::read_snapshot(&layout.config_file()) {
            Ok(None) => (DesiliumConfig::default(), None),
            Ok(Some(snapshot)) => load_at_open(&layout, &snapshot, &mut recovery),
            Err(error) => {
                recovery.push(Recovery::Unreadable {
                    reason: error.to_string(),
                });
                (DesiliumConfig::default(), None)
            }
        };
        for item in &recovery {
            tracing::warn!("{item}");
        }
        let store = Self {
            layout,
            current,
            disk,
            listeners: Vec::new(),
        };
        OpenOutcome { store, recovery }
    }

    /// `$XDG_CONFIG_HOME/desilium`, or `~/.config/desilium` when that is unset.
    pub fn default_dir() -> PathBuf {
        default_dir_from(env::var_os("XDG_CONFIG_HOME"), env::var_os("HOME"))
    }

    /// The active configuration.
    pub fn get(&self) -> &DesiliumConfig {
        &self.current
    }

    /// Starts an edit on a copy of the active configuration.
    pub fn transaction(&mut self) -> Transaction<'_> {
        Transaction::new(self)
    }

    /// Lists stored backups, oldest first. Returns an empty list if they cannot be read.
    pub fn backups(&self) -> Vec<BackupInfo> {
        match storage::list_backups(&self.layout) {
            Ok(backups) => backups,
            Err(error) => {
                tracing::warn!("cannot list configuration backups: {error}");
                Vec::new()
            }
        }
    }

    /// Restores backup `seq` through the normal validate-and-write path.
    ///
    /// The file being replaced is itself backed up, so a rollback can be undone.
    pub fn rollback_to(&mut self, seq: u64) -> Result<(), ConfigError> {
        let path = self.layout.backup_file(seq);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(ConfigError::BackupNotFound(seq));
            }
            Err(error) => return Err(io_at(&path)(error)),
        };
        let restored = load_document(&bytes)
            .map_err(|error| ConfigError::InvalidBackup {
                seq,
                reason: error.to_string(),
            })?
            .config;
        let mut tx = self.transaction();
        tx.edit(|cfg| *cfg = restored);
        tx.commit().map(|_| ())
    }

    /// Registers a listener called with `(old, new)` after each change that is applied.
    pub fn subscribe(
        &mut self,
        listener: impl Fn(&DesiliumConfig, &DesiliumConfig) + Send + 'static,
    ) {
        self.listeners.push(Box::new(listener));
    }

    /// Detects and applies an external edit to the file. Returns `None` if nothing changed.
    pub fn reload_if_changed(&mut self) -> Option<ReloadOutcome> {
        let file = self.layout.config_file();
        let snapshot = match storage::read_snapshot(&file) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return Some(ReloadOutcome::Rejected {
                    reason: format!("cannot read the configuration file: {error}"),
                });
            }
        };
        let fingerprint = snapshot.as_ref().map(|found| found.fingerprint);
        if fingerprint == self.disk {
            return None;
        }
        // Remember the state even when it is rejected, so a broken file is reported once.
        self.disk = fingerprint;
        let outcome = match snapshot {
            Some(snapshot) => self.adopt_external(&snapshot),
            None => ReloadOutcome::Rejected {
                reason:
                    "the configuration file was removed; the last good configuration stays active"
                        .to_owned(),
            },
        };
        Some(outcome)
    }

    fn adopt_external(&mut self, snapshot: &Snapshot) -> ReloadOutcome {
        let loaded = match load_document(&snapshot.bytes) {
            Ok(loaded) => loaded,
            Err(error) => {
                return ReloadOutcome::Rejected {
                    reason: error.to_string(),
                };
            }
        };
        let mut config = loaded.config;
        let mut recovery = loaded.recovery;
        recovery.extend(
            validate(&mut config)
                .repairs
                .into_iter()
                .map(Recovery::Repaired),
        );
        self.replace_current(config);
        ReloadOutcome::Applied { recovery }
    }

    /// Writes `next` if it differs from the active config. Called by [`Transaction::commit`].
    pub(crate) fn apply(&mut self, next: DesiliumConfig) -> Result<CommitReport, ConfigError> {
        if next == self.current {
            return Ok(CommitReport::default());
        }
        let file = self.layout.config_file();
        let on_disk = storage::read_snapshot(&file).map_err(io_at(&file))?;
        if on_disk.as_ref().map(|found| found.fingerprint) != self.disk {
            return Err(ConfigError::ChangedOnDisk);
        }
        storage::ensure_dirs(&self.layout).map_err(io_at(self.layout.dir()))?;
        let backup = match on_disk {
            Some(snapshot) => Some(self.backup(&snapshot.bytes)?),
            None => None,
        };
        let text = storage::render(&next)?;
        storage::write_atomic(&file, text.as_bytes()).map_err(io_at(&file))?;
        self.disk = storage::read_snapshot(&file)
            .map_err(io_at(&file))?
            .map(|written| written.fingerprint);
        self.replace_current(next);
        Ok(CommitReport {
            changed: true,
            repairs: Vec::new(),
            backup,
        })
    }

    fn backup(&self, bytes: &[u8]) -> Result<u64, ConfigError> {
        let backups_dir = self.layout.backups_dir();
        storage::write_backup(&self.layout, bytes).map_err(io_at(&backups_dir))
    }

    fn replace_current(&mut self, next: DesiliumConfig) {
        if next == self.current {
            return;
        }
        let previous = std::mem::replace(&mut self.current, next);
        for listener in &self.listeners {
            listener(&previous, &self.current);
        }
    }
}

/// Loads the file found at open. Unusable files are quarantined and defaults are used.
fn load_at_open(
    layout: &Layout,
    snapshot: &Snapshot,
    recovery: &mut Vec<Recovery>,
) -> (DesiliumConfig, Option<Fingerprint>) {
    match load_document(&snapshot.bytes) {
        Ok(loaded) => {
            let mut config = loaded.config;
            recovery.extend(loaded.recovery);
            recovery.extend(
                validate(&mut config)
                    .repairs
                    .into_iter()
                    .map(Recovery::Repaired),
            );
            (config, Some(snapshot.fingerprint))
        }
        Err(error) => {
            let moved_to = quarantine_or_log(layout);
            // If the file could not be moved it still exists and must stay visible as a conflict.
            let disk = match moved_to {
                Some(_) => None,
                None => Some(snapshot.fingerprint),
            };
            recovery.push(Recovery::Corrupt {
                reason: error.to_string(),
                moved_to,
            });
            (DesiliumConfig::default(), disk)
        }
    }
}

fn quarantine_or_log(layout: &Layout) -> Option<PathBuf> {
    match storage::quarantine(layout) {
        Ok(path) => Some(path),
        Err(error) => {
            tracing::error!("could not move the corrupt configuration aside: {error}");
            None
        }
    }
}

fn default_dir_from(xdg: Option<OsString>, home: Option<OsString>) -> PathBuf {
    let base = match (
        xdg.filter(|value| !value.is_empty()),
        home.filter(|value| !value.is_empty()),
    ) {
        (Some(xdg), _) => PathBuf::from(xdg),
        (None, Some(home)) => PathBuf::from(home).join(FALLBACK_CONFIG_BASE),
        (None, None) => PathBuf::from(FALLBACK_CONFIG_BASE),
    };
    base.join(APP_DIR_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::write_atomic;
    use std::sync::{Arc, Mutex};

    const DEFAULT_CURSOR: u32 = 24;

    fn open_in(dir: &std::path::Path) -> ConfigStore {
        ConfigStore::open(dir).store
    }

    fn set_cursor(store: &mut ConfigStore, size: u32) -> CommitReport {
        let mut tx = store.transaction();
        tx.edit(|cfg| cfg.appearance.cursor_size = size);
        tx.commit().expect("commit")
    }

    fn config_path(dir: &std::path::Path) -> PathBuf {
        dir.join(storage::CONFIG_FILE_NAME)
    }

    #[test]
    fn first_run_creates_nothing_until_a_change_is_committed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("fresh");
        let outcome = ConfigStore::open(&root);
        assert!(outcome.recovery.is_empty());
        assert_eq!(outcome.store.get(), &DesiliumConfig::default());
        assert!(!root.exists());
    }

    #[test]
    fn commit_writes_private_file_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        let report = set_cursor(&mut store, 40);
        assert!(report.changed);
        assert_eq!(report.backup, None);
        let mode = fs::metadata(config_path(dir.path()))
            .expect("metadata")
            .permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600
        );
        let names: Vec<OsString> = fs::read_dir(dir.path())
            .expect("list")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert!(names.contains(&OsString::from("config.toml")));
        assert!(
            names
                .iter()
                .all(|name| !name.to_string_lossy().ends_with(".tmp"))
        );
        let reopened = open_in(dir.path());
        assert_eq!(reopened.get().appearance.cursor_size, 40);
    }

    #[test]
    fn unchanged_commit_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        let report = store.transaction().commit().expect("commit");
        assert!(!report.changed);
        assert!(!config_path(dir.path()).exists());
    }

    #[test]
    fn dropped_transaction_is_a_rollback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        {
            let mut tx = store.transaction();
            tx.edit(|cfg| cfg.appearance.cursor_size = 99);
        }
        assert_eq!(store.get().appearance.cursor_size, DEFAULT_CURSOR);
        assert!(!config_path(dir.path()).exists());
    }

    #[test]
    fn commit_validates_and_reports_repairs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        let report = set_cursor(&mut store, 500);
        assert_eq!(store.get().appearance.cursor_size, 128);
        assert_eq!(report.repairs.len(), 1);
    }

    #[test]
    fn corrupt_file_is_quarantined_and_defaults_used() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_atomic(&config_path(dir.path()), b"[appearance\n???").expect("write");
        let outcome = ConfigStore::open(dir.path());
        assert_eq!(outcome.store.get(), &DesiliumConfig::default());
        let moved = dir.path().join("config.toml.corrupt-1");
        assert!(matches!(
            outcome.recovery.as_slice(),
            [Recovery::Corrupt { moved_to: Some(path), .. }] if *path == moved
        ));
        assert_eq!(fs::read(&moved).expect("copy kept"), b"[appearance\n???");
        assert!(!config_path(dir.path()).exists());
    }

    #[test]
    fn partial_file_is_recovered_like_corrupt_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let full = "version = 1\n[appearance]\ntheme = \"desilium-dark\"\ncursor_size = 40\n";
        write_atomic(
            &config_path(dir.path()),
            &full.as_bytes()[..full.len() - 12],
        )
        .expect("write");
        let outcome = ConfigStore::open(dir.path());
        assert!(
            outcome
                .recovery
                .iter()
                .any(|r| matches!(r, Recovery::Corrupt { .. }))
        );
        assert_eq!(outcome.store.get().appearance.cursor_size, DEFAULT_CURSOR);
    }

    #[test]
    fn invalid_field_is_defaulted_and_reported_on_open() {
        let dir = tempfile::tempdir().expect("tempdir");
        let text = "version = 1\n[appearance]\nanimation_speed = \"fast\"\ncursor_size = 40\n";
        write_atomic(&config_path(dir.path()), text.as_bytes()).expect("write");
        let outcome = ConfigStore::open(dir.path());
        assert_eq!(outcome.store.get().appearance.cursor_size, 40);
        assert!(
            outcome
                .recovery
                .iter()
                .any(|r| matches!(r, Recovery::InvalidField { .. }))
        );
    }

    #[test]
    fn out_of_range_value_is_repaired_on_open() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_atomic(
            &config_path(dir.path()),
            b"version = 1\n[appearance]\nanimation_speed = 9.0\n",
        )
        .expect("write");
        let outcome = ConfigStore::open(dir.path());
        assert_eq!(outcome.store.get().appearance.animation_speed, 4.0);
        assert!(
            outcome
                .recovery
                .iter()
                .any(|r| matches!(r, Recovery::Repaired(_)))
        );
    }

    #[test]
    fn version_zero_file_is_migrated_on_open() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_atomic(&config_path(dir.path()), b"terminal = \"foot\"\n").expect("write");
        let outcome = ConfigStore::open(dir.path());
        assert_eq!(
            outcome.store.get().compositor.terminal.as_deref(),
            Some("foot")
        );
        assert!(
            outcome
                .recovery
                .iter()
                .any(|r| matches!(r, Recovery::Migrated(_)))
        );
    }

    #[test]
    fn backups_rotate_and_keep_the_newest_ten() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        for size in 30..42 {
            set_cursor(&mut store, size);
        }
        let seqs: Vec<u64> = store.backups().iter().map(|b| b.seq).collect();
        assert_eq!(seqs, (2..=11).collect::<Vec<u64>>());
    }

    #[test]
    fn rollback_restores_an_earlier_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        set_cursor(&mut store, 30);
        set_cursor(&mut store, 40);
        store.rollback_to(1).expect("rollback");
        assert_eq!(store.get().appearance.cursor_size, 30);
        assert_eq!(open_in(dir.path()).get().appearance.cursor_size, 30);
        assert_eq!(
            store.backups().len(),
            2,
            "the rolled-back file is itself backed up"
        );
    }

    #[test]
    fn rollback_to_unknown_seq_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        assert!(matches!(
            store.rollback_to(99),
            Err(ConfigError::BackupNotFound(99))
        ));
    }

    #[test]
    fn rollback_to_unusable_backup_is_an_error_and_changes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        set_cursor(&mut store, 30);
        write_atomic(&store.layout.backup_file(7), b"not = [toml").expect("write");
        assert!(matches!(
            store.rollback_to(7),
            Err(ConfigError::InvalidBackup { seq: 7, .. })
        ));
        assert_eq!(store.get().appearance.cursor_size, 30);
    }

    #[test]
    fn listener_receives_old_and_new_only_on_real_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        store.subscribe(move |old, new| {
            if let Ok(mut log) = sink.lock() {
                log.push((old.appearance.cursor_size, new.appearance.cursor_size));
            }
        });
        set_cursor(&mut store, 30);
        store.transaction().commit().expect("no-op");
        let log = seen.lock().map(|log| log.clone()).unwrap_or_default();
        assert_eq!(log, vec![(DEFAULT_CURSOR, 30)]);
    }

    #[test]
    fn valid_external_edit_is_applied_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        write_atomic(
            &config_path(dir.path()),
            b"[appearance]\ncursor_size = 48\n",
        )
        .expect("write");
        let outcome = store.reload_if_changed().expect("changed");
        assert!(matches!(outcome, ReloadOutcome::Applied { .. }));
        assert_eq!(store.get().appearance.cursor_size, 48);
        assert_eq!(store.reload_if_changed(), None);
    }

    #[test]
    fn invalid_external_edit_is_rejected_and_last_good_stays() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        set_cursor(&mut store, 30);
        write_atomic(&config_path(dir.path()), b"[appearance\nbroken").expect("write");
        assert!(matches!(
            store.reload_if_changed(),
            Some(ReloadOutcome::Rejected { .. })
        ));
        assert_eq!(store.get().appearance.cursor_size, 30);
        assert_eq!(
            store.reload_if_changed(),
            None,
            "a rejected edit is reported once"
        );
    }

    #[test]
    fn external_removal_is_reported_and_last_good_stays() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        set_cursor(&mut store, 30);
        fs::remove_file(config_path(dir.path())).expect("remove");
        assert!(matches!(
            store.reload_if_changed(),
            Some(ReloadOutcome::Rejected { .. })
        ));
        assert_eq!(store.get().appearance.cursor_size, 30);
    }

    #[test]
    fn commit_refuses_to_overwrite_an_unseen_external_edit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = open_in(dir.path());
        set_cursor(&mut store, 30);
        write_atomic(
            &config_path(dir.path()),
            b"[appearance]\ncursor_size = 50\n",
        )
        .expect("write");
        let mut tx = store.transaction();
        tx.edit(|cfg| cfg.appearance.cursor_size = 40);
        assert!(matches!(tx.commit(), Err(ConfigError::ChangedOnDisk)));
        assert!(matches!(
            store.reload_if_changed(),
            Some(ReloadOutcome::Applied { .. })
        ));
        set_cursor(&mut store, 40);
        assert_eq!(open_in(dir.path()).get().appearance.cursor_size, 40);
    }

    #[test]
    fn default_dir_prefers_xdg_then_home() {
        assert_eq!(
            default_dir_from(Some("/x".into()), Some("/h".into())),
            PathBuf::from("/x/desilium")
        );
        assert_eq!(
            default_dir_from(Some(OsString::new()), Some("/h".into())),
            PathBuf::from("/h/.config/desilium")
        );
        assert_eq!(
            default_dir_from(None, None),
            PathBuf::from(".config/desilium")
        );
    }
}
