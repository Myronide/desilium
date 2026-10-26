//! The "zero IPC" promise: the app writes the settings file through the store, and anything
//! watching the directory (the compositor does exactly this) sees it and applies it.
//!
//! The compositor side is modelled with the same pieces it uses: an inotify watch on the
//! directory and `ConfigStore::reload_if_changed`. The real compositor is exercised by
//! `tests/e2e/live-accent.sh` (Docker).

use std::time::Instant;

use desilium_config::{ConfigStore, ReloadOutcome};
use desilium_settings::{
    app::{App, AppOptions},
    runtime::watch::ConfigWatcher,
    wallpapers::Env,
};
use desilium_ui::{Rect, UiAction, UiEvent};

fn action(name: &str, arg: &str) -> UiEvent {
    UiEvent::Action(UiAction {
        action: name.to_owned(),
        arg: Some(arg.to_owned()),
        element_id: None,
        rect: Rect::default(),
    })
}

#[test]
fn an_accent_change_in_the_app_reaches_a_watcher_of_the_same_directory() {
    let dir = tempfile::tempdir().expect("dir");
    let config_dir = dir.path().join("desilium");

    // "Compositor": opens the same directory first, before anything exists, and watches it.
    let mut compositor = ConfigStore::open(&config_dir).store;
    let mut watcher = ConfigWatcher::new(config_dir.clone()).expect("watcher");

    // The settings app.
    let mut options = AppOptions::from_environment(config_dir.clone());
    options.cache_dir = dir.path().join("cache");
    options.wallpaper_env = Env {
        override_dir: Some(dir.path().join("none").into_os_string()),
        ..Env::default()
    };
    let mut app = App::new(ConfigStore::open(&config_dir).store, options, None);
    let now = Instant::now();

    app.handle_event(&action("set", "appearance.accent=#E8348A"), now);
    assert!(
        watcher.drain().expect("drain"),
        "the watcher noticed the new file"
    );
    let outcome = compositor.reload_if_changed().expect("changed");
    assert!(matches!(outcome, ReloadOutcome::Applied { .. }));
    assert_eq!(
        compositor.get().appearance.accent.as_deref(),
        Some("#E8348A")
    );

    app.handle_event(&action("undo", ""), now);
    assert!(watcher.drain().expect("drain"), "undo is a write too");
    compositor.reload_if_changed().expect("changed");
    assert_eq!(compositor.get().appearance.accent, None);
}

#[test]
fn the_apps_own_writes_do_not_trigger_its_own_reload() {
    let dir = tempfile::tempdir().expect("dir");
    let config_dir = dir.path().join("desilium");
    let mut options = AppOptions::from_environment(config_dir.clone());
    options.cache_dir = dir.path().join("cache");
    options.wallpaper_env.override_dir = Some(dir.path().join("none").into_os_string());
    let mut app = App::new(ConfigStore::open(&config_dir).store, options, None);
    let now = Instant::now();
    app.handle_event(&action("set", "appearance.cursor_size=32"), now);
    let refresh = app.reload_external(now);
    assert!(refresh.is_empty(), "nothing new on disk, nothing to reload");
    assert_eq!(app.ctx(now).config.appearance.cursor_size, 32);
}
