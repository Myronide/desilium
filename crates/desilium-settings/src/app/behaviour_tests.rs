//! End-to-end behaviour of the controller: actions in, configuration and view state out.

use std::{fs, time::Instant};

use desilium_ui::{Key, KeyEvent, Modifiers, Rect, UiAction, UiEvent};

use super::tests::app_in;
use super::*;
use crate::{
    model::{HIGHLIGHT_LIFETIME, TOAST_LIFETIME},
    refresh::Refresh,
    wallpapers::FitMode,
};

fn action(name: &str, arg: Option<&str>) -> UiEvent {
    UiEvent::Action(UiAction {
        action: name.to_owned(),
        arg: arg.map(str::to_owned),
        element_id: None,
        rect: Rect::default(),
    })
}

fn config_file(app: &App) -> std::path::PathBuf {
    app.store.get(); // keep the borrow checker honest about immutability
    std::path::PathBuf::from(&app.model.info.config_file)
}

fn alt(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers {
            alt: true,
            ..Modifiers::default()
        },
        text: None,
        repeat: false,
    }
}

#[test]
fn choosing_an_accent_saves_applies_and_offers_undo() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    let refresh = app.handle_event(&action("set", Some("appearance.accent=#E8348A")), now);
    assert!(refresh.theme && refresh.main);
    assert_eq!(
        app.store.get().appearance.accent.as_deref(),
        Some("#E8348A")
    );
    assert_eq!(
        app.theme().palette.accent.to_hex().to_ascii_uppercase(),
        "#E8348A"
    );
    let toast = app.model().toast.clone().expect("toast");
    assert_eq!(toast.message, "Accent color set to Pink");
    assert!(toast.undo.is_some());
    assert_eq!(toast.expires, now + TOAST_LIFETIME);
}

#[test]
fn undo_goes_back_and_the_toast_says_so() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("set", Some("appearance.accent=#E8348A")), now);
    app.handle_event(&action("set", Some("appearance.accent=#4DA3FF")), now);
    let refresh = app.handle_event(&action("undo", None), now);
    assert!(refresh.theme);
    assert_eq!(
        app.store.get().appearance.accent.as_deref(),
        Some("#E8348A")
    );
    let toast = app.model().toast.clone().expect("toast");
    assert_eq!(toast.message, "Change undone");
    assert!(toast.undo.is_none());
}

#[test]
fn switching_to_light_re_themes_the_window() {
    let (_dir, mut app) = app_in();
    let refresh = app.handle_event(
        &action("set", Some("appearance.theme=desilium-light")),
        Instant::now(),
    );
    assert!(refresh.theme);
    assert_eq!(app.theme().mode, desilium_theme::Mode::Light);
}

#[test]
fn unknown_or_malformed_actions_do_nothing() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    for (name, arg) in [
        ("set", "appearance.accent"),
        ("set", "nope=1"),
        ("set", "appearance.cursor_size=big"),
        ("step", "appearance.accent=1"),
        ("nav", "elsewhere"),
        ("goto", "nope"),
        ("mystery", "x"),
    ] {
        let refresh = app.handle_event(&action(name, Some(arg)), now);
        assert!(refresh.is_empty(), "{name} {arg}");
    }
    assert_eq!(app.store.get(), &desilium_config::DesiliumConfig::default());
}

#[test]
fn invalid_hex_shows_an_inline_error_and_saves_nothing() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    let typed = UiEvent::TextChanged {
        element_id: None,
        key: Some("accent-hex".into()),
        value: "banana".into(),
    };
    let refresh = app.handle_event(&typed, now);
    assert_eq!(refresh.field, Some(("accent-hex".to_owned(), true)));
    let refresh = app.handle_event(&action("accent-apply", None), now);
    assert!(refresh.focus_hex && refresh.main);
    let editor = app.model().accent_editor.clone().expect("editor");
    assert!(editor.error.expect("error").contains("#7B61FF"));
    assert_eq!(app.store.get().appearance.accent, None);
}

#[test]
fn typed_hex_is_normalized_and_applied_on_enter() {
    let (_dir, mut app) = app_in();
    let submit = UiEvent::Submit {
        action: "accent-apply".into(),
        value: "e8348a".into(),
    };
    app.handle_event(&submit, Instant::now());
    assert_eq!(
        app.store.get().appearance.accent.as_deref(),
        Some("#E8348A")
    );
    assert!(app.model().accent_editor.is_none());
}

#[test]
fn clearing_the_accent_returns_to_the_theme_accent() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("set", Some("appearance.accent=#E8348A")), now);
    app.handle_event(&action("clear", Some("appearance.accent")), now);
    assert_eq!(app.store.get().appearance.accent, None);
    assert_eq!(app.theme(), &desilium_theme::Theme::dark());
}

#[test]
fn animation_speed_off_and_reduce_motion_map_to_the_config() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("set", Some("appearance.animation_speed=0")), now);
    assert!(!app.store.get().appearance.animations);
    app.handle_event(&action("set", Some("appearance.animation_speed=2")), now);
    assert!(app.store.get().appearance.animations);
    assert_eq!(app.store.get().appearance.animation_speed, 2.0);
    app.handle_event(&action("set", Some("appearance.reduce_motion=true")), now);
    assert!(app.store.get().appearance.reduce_motion);
}

#[test]
fn mouse_settings_are_stored_in_the_compositor_table_and_stay_in_range() {
    use desilium_config_types::AccelProfile;
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    for (setting, value) in [
        ("compositor.mouse.pointer_speed", "-35"),
        ("compositor.mouse.accel_profile", "flat"),
        ("compositor.mouse.natural_scroll", "true"),
        ("compositor.mouse.left_handed", "true"),
        ("compositor.mouse.scroll_speed", "9999"),
        ("compositor.mouse.double_click_ms", "10"),
    ] {
        app.handle_event(&action("set", Some(&format!("{setting}={value}"))), now);
    }
    let mouse = &app.store.get().compositor.mouse;
    assert_eq!(mouse.pointer_speed, -35);
    assert_eq!(mouse.accel_profile, AccelProfile::Flat);
    assert!(mouse.natural_scroll && mouse.left_handed);
    assert_eq!(mouse.scroll_speed, 400, "clamped to the slider's range");
    assert_eq!(mouse.double_click_ms, 200);
    app.handle_event(&action("reset-page", Some("mouse")), now);
    assert_eq!(
        app.store.get().compositor.mouse,
        desilium_config_types::MouseSettings::default()
    );
}

#[test]
fn stepper_changes_by_one_and_stops_at_the_limits() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("step", Some("compositor.desktops=1")), now);
    assert_eq!(app.store.get().compositor.desktops, 5);
    for _ in 0..40 {
        app.handle_event(&action("step", Some("compositor.desktops=1")), now);
    }
    assert_eq!(app.store.get().compositor.desktops, 32);
    for _ in 0..40 {
        app.handle_event(&action("step", Some("compositor.desktops=-1")), now);
    }
    assert_eq!(app.store.get().compositor.desktops, 1);
}

#[test]
fn dragging_a_slider_previews_and_stores_the_value_on_release() {
    use desilium_ui::DragPhase;
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    let before = app.store.get().shell.panel_radius;
    let refresh = app.range_drag("shell.panel_radius", DragPhase::Start, 0.0, now);
    assert_eq!(refresh.range, Some("shell.panel_radius"));
    assert_eq!(app.model().range_preview, Some(("shell.panel_radius", 0.0)));
    app.range_drag("shell.panel_radius", DragPhase::Move, 0.5, now);
    assert_eq!(
        app.store.get().shell.panel_radius,
        before,
        "nothing is stored while the button is down"
    );
    app.range_drag("shell.panel_radius", DragPhase::End, 1.7, now);
    assert_eq!(app.model().range_preview, None);
    assert_eq!(
        app.store.get().shell.panel_radius,
        36,
        "past the end is the maximum"
    );
    app.range_drag("appearance.corner_radius", DragPhase::End, -1.0, now);
    assert_eq!(app.store.get().appearance.corner_radius, 0);
    let unknown = app.range_drag("nope", DragPhase::End, 0.5, now);
    assert!(unknown.is_empty());
}

#[test]
fn terminal_text_field_applies_on_submit_and_empty_means_automatic() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    let submit = |text: &str| UiEvent::Submit {
        action: "text:compositor.terminal".into(),
        value: text.into(),
    };
    app.handle_event(&submit("  kitty "), now);
    assert_eq!(
        app.store.get().compositor.terminal.as_deref(),
        Some("kitty")
    );
    app.handle_event(&submit(""), now);
    assert_eq!(app.store.get().compositor.terminal, None);
}

#[test]
fn per_setting_section_and_page_resets() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("set", Some("appearance.cursor_size=64")), now);
    app.handle_event(&action("set", Some("appearance.reduce_motion=true")), now);
    app.handle_event(&action("reset", Some("appearance.cursor_size")), now);
    assert_eq!(app.store.get().appearance.cursor_size, 24);
    assert!(app.store.get().appearance.reduce_motion);
    app.handle_event(&action("reset-section", Some("appearance/Motion")), now);
    assert!(!app.store.get().appearance.reduce_motion);
    app.handle_event(&action("set", Some("appearance.theme=desilium-light")), now);
    app.handle_event(&action("reset-page", Some("appearance")), now);
    assert_eq!(app.store.get(), &desilium_config::DesiliumConfig::default());
    app.handle_event(&action("undo", None), now);
    assert_eq!(app.store.get().appearance.theme, "desilium-light");
}

#[test]
fn navigation_and_search_flow() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("nav", Some("keyboard")), now);
    assert_eq!(app.model().page, Page::Keyboard);
    let typed = UiEvent::TextChanged {
        element_id: None,
        key: Some("search".into()),
        value: "wallpaper".into(),
    };
    let refresh = app.handle_event(&typed, now);
    assert!(refresh.main && refresh.nav);
    assert!(app.model().searching());
    let submit = UiEvent::Submit {
        action: "search".into(),
        value: "wallpaper".into(),
    };
    let refresh = app.handle_event(&submit, now);
    assert!(refresh.clear_search);
    assert_eq!(app.model().page, Page::Appearance);
    assert!(!app.model().searching());
    assert_eq!(app.model().highlighted(now), Some("appearance.wallpaper"));
    assert_eq!(app.model().highlighted(now + HIGHLIGHT_LIFETIME), None);
}

#[test]
fn goto_from_a_result_opens_the_page_and_highlights_the_row() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("goto", Some("compositor.desktops")), now);
    assert_eq!(app.model().page, Page::Desktop);
    assert_eq!(app.model().highlighted(now), Some("compositor.desktops"));
}

#[test]
fn dropdowns_open_close_and_close_on_other_actions() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(
        &action("select-toggle", Some("appearance.wallpaper_mode")),
        now,
    );
    assert_eq!(app.model().open_select, Some("appearance.wallpaper_mode"));
    app.handle_event(
        &action("select-toggle", Some("appearance.wallpaper_mode")),
        now,
    );
    assert_eq!(app.model().open_select, None);
    app.handle_event(
        &action("select-toggle", Some("appearance.wallpaper_mode")),
        now,
    );
    app.handle_event(&action("set", Some("appearance.wallpaper_mode=fit")), now);
    assert_eq!(app.model().open_select, None);
    assert_eq!(app.store.get().appearance.wallpaper_mode, "fit");
}

#[test]
fn toast_and_highlight_expire_on_tick() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("set", Some("appearance.reduce_motion=true")), now);
    assert!(app.tick(now).is_empty());
    assert_eq!(app.next_deadline(), Some(now + TOAST_LIFETIME));
    let refresh = app.tick(now + TOAST_LIFETIME);
    assert!(refresh.toast && !refresh.main);
    assert!(app.model().toast.is_none());
}

#[test]
fn external_edit_is_picked_up_and_reported() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("set", Some("appearance.cursor_size=32")), now);
    fs::write(
        config_file(&app),
        "version = 1\n[appearance]\ncursor_size = 48\ntheme = \"desilium-light\"\n",
    )
    .expect("write");
    let refresh = app.reload_external(now);
    assert!(refresh.theme && refresh.main);
    assert_eq!(app.store.get().appearance.cursor_size, 48);
    assert_eq!(app.theme().mode, desilium_theme::Mode::Light);
    let toast = app.model().toast.clone().expect("toast");
    assert!(toast.undo.is_none());
}

#[test]
fn a_broken_external_edit_keeps_the_last_good_settings_and_says_so() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("set", Some("appearance.cursor_size=32")), now);
    fs::write(config_file(&app), "[appearance\nbroken").expect("write");
    app.reload_external(now);
    assert_eq!(app.store.get().appearance.cursor_size, 32);
    let notice = app.model().notice.clone().expect("notice");
    assert_eq!(notice.level, Level::Error);
    assert!(notice.text.contains("last good"));
    app.handle_event(&action("notice-dismiss", None), now);
    assert!(app.model().notice.is_none());
}

#[test]
fn saving_over_an_unseen_external_edit_offers_a_reload() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("set", Some("appearance.cursor_size=32")), now);
    fs::write(config_file(&app), "[appearance]\ncursor_size = 48\n").expect("write");
    app.handle_event(&action("set", Some("appearance.cursor_size=64")), now);
    let notice = app.model().notice.clone().expect("notice");
    assert_eq!(notice.action, Some(Recovery::Reload));
    app.handle_event(&action("notice-reload", None), now);
    assert_eq!(app.store.get().appearance.cursor_size, 48);
    app.handle_event(&action("set", Some("appearance.cursor_size=64")), now);
    assert_eq!(app.store.get().appearance.cursor_size, 64);
}

#[test]
fn an_unwritable_config_directory_is_a_friendly_error_with_retry() {
    let (dir, mut app) = app_in();
    let now = Instant::now();
    // A file where the settings directory should be makes every write fail.
    fs::write(dir.path().join("config"), b"in the way").expect("block");
    app.handle_event(&action("set", Some("appearance.cursor_size=64")), now);
    let notice = app.model().notice.clone().expect("notice");
    assert_eq!(notice.level, Level::Error);
    assert_eq!(notice.action, Some(Recovery::Retry));
    assert!(notice.text.contains("not saved"));
    assert_eq!(app.store.get().appearance.cursor_size, 24);
    fs::remove_file(dir.path().join("config")).expect("unblock");
    app.handle_event(&action("notice-retry", None), now);
    assert_eq!(app.store.get().appearance.cursor_size, 64);
    assert!(app.model().notice.is_none());
}

#[test]
fn rebinding_a_shortcut_by_pressing_keys() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("rebind", Some("0")), now);
    assert!(app.wants_key_capture());
    let bare = KeyEvent::character("w");
    app.on_key(&bare, false, now);
    assert!(
        app.model()
            .rebind
            .as_ref()
            .and_then(|r| r.message.clone())
            .is_some()
    );
    app.on_key(&alt(Key::Character("w".into())), false, now);
    assert!(!app.wants_key_capture());
    let first = &app.store.get().compositor.shortcuts[0];
    assert_eq!(first.key, "w");
    assert!(first.modifiers.alt);
    assert!(
        app.model()
            .toast
            .as_ref()
            .is_some_and(|t| t.message.contains("Alt+W"))
    );
}

#[test]
fn rebinding_onto_a_used_chord_asks_and_swaps() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    // Shortcut 0 is Alt+Q (close window); Alt+Enter belongs to "open a terminal" (index 1).
    app.handle_event(&action("rebind", Some("0")), now);
    app.on_key(&alt(Key::Enter), false, now);
    let pending = app
        .model()
        .rebind
        .clone()
        .and_then(|r| r.pending)
        .expect("pending");
    assert_eq!(pending.conflict, 1);
    assert_eq!(
        app.store.get().compositor.shortcuts[0].key,
        "q",
        "nothing changed yet"
    );
    app.handle_event(&action("rebind-swap", None), now);
    let list = &app.store.get().compositor.shortcuts;
    assert_eq!(list[0].key, "Return");
    assert_eq!(list[1].key, "q");
    assert!(app.model().rebind.is_none());
}

#[test]
fn rebinding_can_be_cancelled_and_shortcuts_can_be_reset() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("rebind", Some("0")), now);
    app.on_key(&KeyEvent::named(Key::Escape), false, now);
    assert!(!app.wants_key_capture());
    app.handle_event(&action("rebind", Some("0")), now);
    app.on_key(&alt(Key::Character("w".into())), false, now);
    assert_eq!(app.store.get().compositor.shortcuts[0].key, "w");
    app.handle_event(&action("shortcut-reset", Some("0")), now);
    assert_eq!(app.store.get().compositor.shortcuts[0].key, "q");
}

#[test]
fn rebinding_out_of_range_is_ignored() {
    let (_dir, mut app) = app_in();
    let refresh = app.handle_event(&action("rebind", Some("999")), Instant::now());
    assert!(refresh.is_empty());
    assert!(!app.wants_key_capture());
}

#[test]
fn wallpaper_choice_writes_the_path_and_builds_the_preview() {
    let (dir, mut app) = app_in();
    let walls = dir.path().join("wallpapers");
    fs::create_dir_all(&walls).expect("mkdir");
    let png = walls.join("night-sky.png");
    write_png(&png);
    // Re-open the app so discovery sees the file.
    let store = ConfigStore::open(dir.path().join("config2")).store;
    let mut options = AppOptions::from_environment(dir.path().join("config2"));
    options.cache_dir = dir.path().join("cache");
    options.wallpaper_env.override_dir = Some(walls.clone().into_os_string());
    drop(app);
    app = App::new(store, options, None);
    assert_eq!(app.model().wallpapers.len(), 1);
    assert_eq!(app.model().wallpapers[0].name, "Night sky");
    assert!(
        app.model().thumbs.contains_key(&png),
        "card thumbnail was rendered"
    );
    let arg = format!("appearance.wallpaper={}", png.display());
    app.handle_event(&action("set", Some(&arg)), Instant::now());
    assert_eq!(
        app.store.get().appearance.wallpaper.as_deref(),
        Some(png.to_string_lossy().as_ref())
    );
    assert!(app.model().preview.is_some());
    app.handle_event(
        &action("set", Some("appearance.wallpaper_mode=fit")),
        Instant::now(),
    );
    assert_eq!(
        app.model().preview.as_ref().map(|p| p.mode),
        Some(FitMode::Fit)
    );
    app.handle_event(
        &action("clear", Some("appearance.wallpaper")),
        Instant::now(),
    );
    assert!(app.model().preview.is_none());
}

fn write_png(path: &std::path::Path) {
    let file = fs::File::create(path).expect("create");
    let mut encoder = png::Encoder::new(file, 32, 18);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("header")
        .write_image_data(&vec![180u8; 32 * 18 * 4])
        .expect("data");
}

#[test]
fn every_page_renders_and_refreshes_against_a_real_document() {
    use crate::surface::build_surface;
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    let mut ui = build_surface(&mut app, (1100, 800), 1.0, now).expect("surface");
    for page in Page::ALL {
        app.handle_event(&action("nav", Some(page.slug())), now);
        Refresh::all()
            .apply(&app, &mut ui, now)
            .expect("apply page");
        app.handle_event(&action("set", Some("appearance.reduce_motion=true")), now);
        Refresh::toast().apply(&app, &mut ui, now).expect("toast");
    }
    let typed = UiEvent::TextChanged {
        element_id: None,
        key: Some("search".into()),
        value: "zebra".into(),
    };
    let refresh = app.handle_event(&typed, now);
    refresh.apply(&app, &mut ui, now).expect("search page");
    let refresh = app.handle_event(&action("search-clear", None), now);
    refresh.apply(&app, &mut ui, now).expect("clear");
}

#[test]
fn the_preview_thumbnail_does_not_depend_on_the_accent() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    let before = app.preview_spec();
    app.handle_event(&action("set", Some("appearance.accent=#E8348A")), now);
    assert_eq!(app.preview_spec(), before);
    app.handle_event(&action("set", Some("appearance.wallpaper_mode=fit")), now);
    let fit = app.preview_spec();
    assert_eq!(fit.mode, FitMode::Fit);
    app.handle_event(&action("set", Some("appearance.accent=#4DA3FF")), now);
    assert_eq!(
        app.preview_spec(),
        fit,
        "letterbox colour follows light/dark, not the accent"
    );
}

#[test]
fn resetting_the_keyboard_page_restores_every_default_shortcut() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("rebind", Some("0")), now);
    app.on_key(&alt(Key::Character("w".into())), false, now);
    assert_ne!(
        app.store.get().compositor.shortcuts,
        crate::shortcuts::defaults()
    );
    app.handle_event(&action("reset-page", Some("keyboard")), now);
    assert_eq!(
        app.store.get().compositor.shortcuts,
        crate::shortcuts::defaults()
    );
    app.handle_event(&action("undo", None), now);
    assert_eq!(app.store.get().compositor.shortcuts[0].key, "w");
}
