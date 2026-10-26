use desilium_config::ConfigStore;
use desilium_config_types::{PanelEdge, PanelMode};
use desilium_settings::{
    changes,
    registry::{self, Page, Value},
};
#[test]
fn all_panel_preferences_are_graphical_validated_and_reversible() {
    let directory = tempfile::tempdir().expect("directory");
    let mut store = ConfigStore::open(directory.path()).store;
    for (id, value) in [
        ("shell.mode", Value::Text("taskbar".into())),
        ("shell.edge", Value::Text("top".into())),
        ("shell.icon_size", Value::Number(64.0)),
        ("shell.spacing", Value::Number(12.0)),
        ("shell.margin", Value::Number(20.0)),
        ("shell.opacity", Value::Number(80.0)),
        ("shell.show_labels", Value::Bool(true)),
        ("shell.show_running", Value::Bool(false)),
        ("shell.show_indicators", Value::Bool(false)),
        ("shell.show_workspaces", Value::Bool(false)),
        ("shell.show_clock", Value::Bool(false)),
        ("shell.clock_24h", Value::Bool(false)),
        ("shell.show_battery", Value::Bool(false)),
        ("shell.quick_settings", Value::Bool(false)),
    ] {
        let setting = registry::find(id).expect("graphical setting");
        assert_eq!(setting.page, Page::Panel);
        let result = changes::set_value(&mut store, setting, value).expect("set");
        assert!(result.changed);
        assert!(result.undo.is_some());
        assert!(result.repairs.is_empty());
    }
    let loaded = ConfigStore::open(directory.path()).store;
    assert_eq!(loaded.get(), store.get());
    assert_eq!(loaded.get().shell.mode, PanelMode::Taskbar);
    assert_eq!(loaded.get().shell.edge, PanelEdge::Top);
    assert!(!loaded.get().shell.quick_settings);
    let before = store.get().clone();
    assert!(
        changes::set_value(
            &mut store,
            registry::find("shell.mode").expect("mode"),
            Value::Text("invalid".into())
        )
        .is_err()
    );
    assert_eq!(*store.get(), before);
}
#[test]
fn old_documents_receive_panel_defaults_and_new_terminal_is_kitty() {
    let config = desilium_config_types::DesiliumConfig::default();
    assert_eq!(config.compositor.terminal.as_deref(), Some("kitty"));
    assert_eq!(config.shell.mode, PanelMode::Dock);
    let mut invalid = config;
    invalid.shell.icon_size = 0;
    invalid.shell.spacing = u16::MAX;
    invalid.shell.margin = u16::MAX;
    invalid.shell.opacity = 0;
    let report = desilium_config::validate(&mut invalid);
    assert_eq!(report.repairs.len(), 4);
    assert_eq!(invalid.shell.icon_size, 32);
    assert_eq!(invalid.shell.opacity, 40);
}
