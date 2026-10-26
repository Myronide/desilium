//! WC's controls use the normal registry, transaction and watcher path.
use desilium_config::{ConfigStore, ReloadOutcome};
use desilium_config_types::FocusPolicy;
use desilium_settings::{
    changes,
    registry::{self, Value},
};

#[test]
fn window_controls_are_transactional_and_reach_the_compositor_store() {
    let directory = tempfile::tempdir().expect("directory");
    let mut reader = ConfigStore::open(directory.path()).store;
    let mut writer = ConfigStore::open(directory.path()).store;
    for (id, value) in [
        ("compositor.focus", Value::Text("sloppy".into())),
        ("compositor.raise_on_focus", Value::Bool(false)),
        ("compositor.snap_enabled", Value::Bool(false)),
        ("compositor.snap_distance", Value::Number(24.0)),
        ("compositor.keyboard_repeat_delay", Value::Number(300.0)),
        ("compositor.keyboard_repeat_rate", Value::Number(30.0)),
    ] {
        let definition = registry::find(id).expect("GUI control");
        let changed = changes::set_value(&mut writer, definition, value).expect("apply");
        assert!(changed.changed);
        assert!(changed.repairs.is_empty());
        assert!(changed.undo.is_some());
    }
    assert!(matches!(
        reader.reload_if_changed(),
        Some(ReloadOutcome::Applied { .. })
    ));
    let settings = &reader.get().compositor;
    assert_eq!(settings.focus, FocusPolicy::Sloppy);
    assert!(!settings.raise_on_focus && !settings.snap_enabled);
    assert_eq!(settings.snap_distance, 24);
    assert_eq!(settings.keyboard_repeat_delay, 300);
    assert_eq!(settings.keyboard_repeat_rate, 30);
}

#[test]
fn invalid_controls_leave_the_file_and_active_config_unchanged() {
    let directory = tempfile::tempdir().expect("directory");
    let mut store = ConfigStore::open(directory.path()).store;
    for (id, value) in [
        ("compositor.focus", Value::Text("unknown".into())),
        ("compositor.raise_on_focus", Value::Text("yes".into())),
        ("compositor.snap_enabled", Value::Number(1.0)),
        ("compositor.snap_distance", Value::Text("invalid".into())),
        ("compositor.keyboard_repeat_delay", Value::Number(f64::NAN)),
        (
            "compositor.keyboard_repeat_rate",
            Value::Text("invalid".into()),
        ),
    ] {
        let before = store.get().clone();
        let definition = registry::find(id).expect("GUI control");
        assert!(changes::set_value(&mut store, definition, value).is_err());
        assert_eq!(*store.get(), before);
        assert!(!directory.path().join("config.toml").exists());
    }
}
