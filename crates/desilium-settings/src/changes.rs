//! Every change to the configuration goes through here: transaction, validation, undo.
//!
//! The UI never touches `ConfigStore` directly. Each function runs one
//! `transaction()` and `commit()`, so one user action is one backup and one Undo.

use desilium_config::{ConfigError, ConfigStore, Repair};
use desilium_config_types::{DesiliumConfig, Shortcut};

use crate::{
    i18n::{tr, tr_args},
    palette,
    registry::{Control, InvalidValue, OFF_LABEL, SettingDef, Value, format_number},
};

/// How to get back to the state before a change.
#[derive(Debug, Clone, PartialEq)]
pub enum UndoTarget {
    /// The store kept the previous file as backup `seq`.
    Backup(u64),
    /// There was no previous file (first change ever); restore this configuration.
    Snapshot(Box<DesiliumConfig>),
}

/// What a successful change did.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeOutcome {
    /// False when nothing differed (no file was written).
    pub changed: bool,
    /// Values validation had to adjust before saving.
    pub repairs: Vec<Repair>,
    /// How to revert; `None` when nothing changed.
    pub undo: Option<UndoTarget>,
    /// Sentence for the toast, e.g. "Accent set to Violet".
    pub summary: String,
}

/// Why a change did not happen.
#[derive(Debug, thiserror::Error)]
pub enum ChangeError {
    /// The value was refused before touching the store.
    #[error(transparent)]
    Invalid(#[from] InvalidValue),
    /// The store could not save it.
    #[error(transparent)]
    Store(#[from] ConfigError),
}

/// What the user can do about a failure, shown as a button next to the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// Re-read the file from disk, then try again.
    Reload,
    /// Try the same thing again.
    Retry,
    /// Nothing to do but dismiss.
    Dismiss,
}

/// A failure phrased for people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// What happened and what stays true ("your settings were not changed").
    pub message: String,
    /// The suggested way out.
    pub recovery: Recovery,
}

/// Sets one setting. The value is validated first; the store then validates again.
pub fn set_value(
    store: &mut ConfigStore,
    def: &SettingDef,
    input: Value,
) -> Result<ChangeOutcome, ChangeError> {
    let value = def.normalize(input)?;
    let summary = summarize(def, &value);
    let before = store.get().clone();
    let mut tx = store.transaction();
    tx.edit(|config| (def.set)(config, &value));
    finish(tx.commit()?, before, summary)
}

/// Resets several settings in one transaction (one Undo for "Reset page").
pub fn reset_settings(
    store: &mut ConfigStore,
    defs: &[&SettingDef],
    label: &str,
) -> Result<ChangeOutcome, ChangeError> {
    let before = store.get().clone();
    let mut tx = store.transaction();
    for def in defs.iter().filter(|def| !def.is_informational()) {
        let default = def.default_value();
        tx.edit(|config| (def.set)(config, &default));
    }
    let summary = tr_args(
        "ui.change.reset_all",
        "{what} reset to defaults",
        &[("what", label)],
    );
    finish(tx.commit()?, before, summary)
}

/// Edits the appearance settings in one transaction (one Undo for several fields).
pub fn edit_appearance(
    store: &mut ConfigStore,
    summary: String,
    edit: impl FnOnce(&mut desilium_config_types::AppearanceSettings),
) -> Result<ChangeOutcome, ChangeError> {
    let before = store.get().clone();
    let mut tx = store.transaction();
    tx.edit(|config| edit(&mut config.appearance));
    finish(tx.commit()?, before, summary)
}

/// Edits the shortcut list in one transaction (one Undo per re-binding).
pub fn edit_shortcuts(
    store: &mut ConfigStore,
    summary: String,
    edit: impl FnOnce(&mut Vec<Shortcut>),
) -> Result<ChangeOutcome, ChangeError> {
    let before = store.get().clone();
    let mut tx = store.transaction();
    tx.edit(|config| edit(&mut config.compositor.shortcuts));
    finish(tx.commit()?, before, summary)
}

fn finish(
    report: desilium_config::CommitReport,
    before: DesiliumConfig,
    summary: String,
) -> Result<ChangeOutcome, ChangeError> {
    let undo = report.changed.then(|| match report.backup {
        Some(seq) => UndoTarget::Backup(seq),
        None => UndoTarget::Snapshot(Box::new(before)),
    });
    Ok(ChangeOutcome {
        changed: report.changed,
        repairs: report.repairs,
        undo,
        summary,
    })
}

/// Reverts a change. Goes through the store's normal validate-and-write path.
pub fn undo(store: &mut ConfigStore, target: &UndoTarget) -> Result<(), ConfigError> {
    match target {
        UndoTarget::Backup(seq) => store.rollback_to(*seq),
        UndoTarget::Snapshot(config) => {
            let restored = (**config).clone();
            let mut tx = store.transaction();
            tx.edit(|current| *current = restored);
            tx.commit().map(|_| ())
        }
    }
}

/// Phrases a store error and picks the recovery action.
pub fn friendly(error: &ChangeError) -> Failure {
    match error {
        ChangeError::Invalid(invalid) => Failure {
            message: tr_args(
                "ui.error.nothing_changed",
                "{reason} Nothing was changed.",
                &[("reason", &invalid.to_string())],
            ),
            recovery: Recovery::Dismiss,
        },
        ChangeError::Store(store) => friendly_store(store),
    }
}

/// Same as [`friendly`] for errors that did not come from a change (undo, reload).
pub fn friendly_store(error: &ConfigError) -> Failure {
    match error {
        ConfigError::ChangedOnDisk => Failure {
            message: tr(
                "ui.error.changed_on_disk",
                "The settings file was changed by another program. Your change was not saved.",
            ),
            recovery: Recovery::Reload,
        },
        ConfigError::Io { path, source } => Failure {
            message: tr_args(
                "ui.error.write_failed",
                "Desilium could not write {path} ({reason}). Your change was not saved.",
                &[
                    ("path", &path.display().to_string()),
                    ("reason", &source.to_string()),
                ],
            ),
            recovery: Recovery::Retry,
        },
        ConfigError::BackupNotFound(_) | ConfigError::InvalidBackup { .. } => Failure {
            message: tr(
                "ui.error.backup_gone",
                "The earlier version is no longer available, so this cannot be undone.",
            ),
            recovery: Recovery::Dismiss,
        },
        other => Failure {
            message: tr_args(
                "ui.error.not_saved",
                "Your change was not saved: {reason}.",
                &[("reason", &other.to_string())],
            ),
            recovery: Recovery::Retry,
        },
    }
}

/// "Accent set to Violet", "Color scheme set to Dark", "Cursor size set to 32 px".
pub fn summarize(def: &SettingDef, value: &Value) -> String {
    let what = def.title_text();
    let set_to = |shown: &str| {
        tr_args(
            "ui.change.set_to",
            "{what} set to {value}",
            &[("what", &what), ("value", shown)],
        )
    };
    match (&def.control, value) {
        (_, Value::Unset) => tr_args(
            "ui.change.reset_one",
            "{what} reset to default",
            &[("what", &what)],
        ),
        (Control::Accent, Value::Text(hex)) => {
            let name = palette::preset_for(hex).map_or_else(|| hex.clone(), |p| p.label());
            set_to(&name)
        }
        (Control::Segmented(choices) | Control::Select(choices), Value::Text(id)) => {
            let label = choices
                .iter()
                .find(|c| c.value == id)
                .map_or_else(|| id.clone(), |c| c.label_text(def.id));
            set_to(&label)
        }
        (Control::Switch, Value::Bool(on)) => {
            let (key, english) = if *on {
                ("ui.change.turned_on", "{what} turned on")
            } else {
                ("ui.change.turned_off", "{what} turned off")
            };
            tr_args(key, english, &[("what", &what)])
        }
        (Control::Slider(slider), Value::Number(number)) => {
            if *number <= 0.0 && slider.stops.first() == Some(&0.0) {
                return set_to(&tr("ui.common.off", OFF_LABEL));
            }
            set_to(&format!(
                "{}{}",
                format_number(*number),
                slider.unit.trim_start_matches(' ')
            ))
        }
        (Control::Wallpaper, Value::Text(path)) => {
            let name = std::path::Path::new(path)
                .file_stem()
                .map_or(path.clone(), |stem| stem.to_string_lossy().into_owned());
            set_to(&name)
        }
        (_, value) => set_to(&value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{self, Page};

    fn open() -> (tempfile::TempDir, ConfigStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ConfigStore::open(dir.path()).store;
        (dir, store)
    }

    fn def(id: &str) -> &'static SettingDef {
        registry::find(id).expect(id)
    }

    #[test]
    fn setting_the_accent_writes_the_config_and_offers_undo() {
        let (dir, mut store) = open();
        let outcome = set_value(
            &mut store,
            def("appearance.accent"),
            Value::Text("7b61ff".into()),
        )
        .expect("set");
        assert!(outcome.changed);
        assert_eq!(outcome.summary, "Accent color set to Violet");
        assert_eq!(store.get().appearance.accent.as_deref(), Some("#7B61FF"));
        assert!(dir.path().join("config.toml").exists());
        assert!(
            matches!(outcome.undo, Some(UndoTarget::Snapshot(_))),
            "first write has no backup"
        );
    }

    #[test]
    fn invalid_input_changes_nothing() {
        let (dir, mut store) = open();
        let error = set_value(
            &mut store,
            def("appearance.accent"),
            Value::Text("zzz".into()),
        )
        .expect_err("invalid");
        assert!(matches!(error, ChangeError::Invalid(_)));
        assert_eq!(store.get(), &DesiliumConfig::default());
        assert!(!dir.path().join("config.toml").exists());
        assert_eq!(friendly(&error).recovery, Recovery::Dismiss);
    }

    #[test]
    fn setting_the_same_value_is_not_a_change() {
        let (_dir, mut store) = open();
        let outcome = set_value(
            &mut store,
            def("appearance.theme"),
            Value::Text("desilium-dark".into()),
        )
        .expect("set");
        assert!(!outcome.changed);
        assert_eq!(outcome.undo, None);
    }

    #[test]
    fn undo_uses_the_backup_of_the_previous_file() {
        let (_dir, mut store) = open();
        set_value(
            &mut store,
            def("appearance.cursor_size"),
            Value::Number(32.0),
        )
        .expect("first");
        let second = set_value(
            &mut store,
            def("appearance.cursor_size"),
            Value::Number(64.0),
        )
        .expect("second");
        assert!(matches!(second.undo, Some(UndoTarget::Backup(_))));
        undo(&mut store, second.undo.as_ref().expect("undo")).expect("undo");
        assert_eq!(store.get().appearance.cursor_size, 32);
    }

    #[test]
    fn undo_of_the_first_change_restores_the_defaults() {
        let (_dir, mut store) = open();
        let first =
            set_value(&mut store, def("compositor.desktops"), Value::Number(8.0)).expect("set");
        undo(&mut store, first.undo.as_ref().expect("undo")).expect("undo");
        assert_eq!(store.get(), &DesiliumConfig::default());
    }

    #[test]
    fn page_reset_is_one_transaction_with_one_undo() {
        let (_dir, mut store) = open();
        set_value(
            &mut store,
            def("appearance.cursor_size"),
            Value::Number(64.0),
        )
        .expect("a");
        set_value(
            &mut store,
            def("appearance.theme"),
            Value::Text("desilium-light".into()),
        )
        .expect("b");
        let backups_before = store.backups().len();
        let defs: Vec<_> = registry::for_page(Page::Appearance).collect();
        let outcome = reset_settings(&mut store, &defs, "Appearance").expect("reset");
        assert!(outcome.changed);
        assert_eq!(store.get(), &DesiliumConfig::default());
        assert_eq!(store.backups().len(), backups_before + 1);
        undo(&mut store, outcome.undo.as_ref().expect("undo")).expect("undo");
        assert_eq!(store.get().appearance.cursor_size, 64);
    }

    #[test]
    fn external_edit_blocks_the_commit_with_a_reload_recovery() {
        let (dir, mut store) = open();
        set_value(
            &mut store,
            def("appearance.cursor_size"),
            Value::Number(32.0),
        )
        .expect("first");
        std::fs::write(
            dir.path().join("config.toml"),
            "[appearance]\ncursor_size = 48\n",
        )
        .expect("write");
        let error = set_value(
            &mut store,
            def("appearance.cursor_size"),
            Value::Number(64.0),
        )
        .expect_err("changed on disk");
        assert_eq!(friendly(&error).recovery, Recovery::Reload);
    }

    #[test]
    fn repairs_from_validation_are_reported() {
        let (_dir, mut store) = open();
        // Bypass the control (which snaps) to prove the store-side repair path is surfaced.
        let mut tx = store.transaction();
        tx.edit(|c| c.appearance.cursor_size = 500);
        let report = tx.commit().expect("commit");
        assert_eq!(report.repairs.len(), 1);
        assert_eq!(store.get().appearance.cursor_size, 128);
    }

    #[test]
    fn summaries_read_naturally() {
        let speed = def("appearance.animation_speed");
        assert_eq!(
            summarize(speed, &Value::Number(0.0)),
            "Animation speed set to Off"
        );
        assert_eq!(
            summarize(speed, &Value::Number(1.5)),
            "Animation speed set to 1.5x"
        );
        assert_eq!(
            summarize(
                def("appearance.theme"),
                &Value::Text("desilium-light".into())
            ),
            "Color scheme set to Light"
        );
        assert_eq!(
            summarize(def("appearance.reduce_motion"), &Value::Bool(true)),
            "Reduce motion turned on"
        );
        assert_eq!(
            summarize(
                def("appearance.wallpaper"),
                &Value::Text("/a/b/aurora.jpg".into())
            ),
            "Wallpaper set to aurora"
        );
        assert_eq!(
            summarize(def("appearance.accent"), &Value::Unset),
            "Accent color reset to default"
        );
    }
}
