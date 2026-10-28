//! Real pointer input against the real document: find a control by its tooltip, click it,
//! and check that the configuration changes and the document follows.

use std::time::Instant;

use desilium_config::ConfigStore;
use desilium_settings::{
    app::{App, AppOptions},
    i18n::{Locale, set_locale},
    refresh::Refresh,
    registry::Page,
    surface::build_surface,
    wallpapers::Env,
};
use desilium_ui::{PointerButton, UiEvent, UiSurface};

const SIZE: (u32, u32) = (1280, 1000);
const SCAN_STEP: usize = 6;
/// Where each control lives in the 1280x1000 Appearance page; keeps the scan fast.
const ACCENT_ROW: (std::ops::Range<usize>, std::ops::Range<usize>) = (300..900, 230..420);
const MOTION_ROW: (std::ops::Range<usize>, std::ops::Range<usize>) = (500..900, 600..1000);

fn new_app(page: Page) -> (tempfile::TempDir, App) {
    // Controls are found by their English tooltips, whatever language the machine speaks.
    set_locale(Some(Locale::English));
    let dir = tempfile::tempdir().expect("dir");
    let config_dir = dir.path().join("config");
    let store = ConfigStore::open(&config_dir).store;
    let mut options = AppOptions::from_environment(config_dir);
    options.page = page;
    options.cache_dir = dir.path().join("cache");
    options.wallpaper_env = Env {
        override_dir: Some(dir.path().join("none").into_os_string()),
        ..Env::default()
    };
    (dir, App::new(store, options, None))
}

fn paint(ui: &mut UiSurface) {
    let stride = ui.min_stride();
    let mut buffer = vec![0; stride * ui.buffer_size().1 as usize];
    ui.render(&mut buffer, stride).expect("render");
}

/// Centre of the element whose tooltip is `tip`, found by hovering over a grid of points.
fn find_by_tooltip(
    ui: &mut UiSurface,
    tip: &str,
    area: &(std::ops::Range<usize>, std::ops::Range<usize>),
) -> Option<(f64, f64)> {
    for y in area.1.clone().step_by(SCAN_STEP) {
        for x in area.0.clone().step_by(SCAN_STEP) {
            ui.pointer_move(x as f64, y as f64);
            let Some(hovered) = ui.hover_target() else {
                continue;
            };
            if hovered.tooltip.as_deref() == Some(tip) && hovered.action.is_some() {
                let rect = hovered.rect;
                return Some((rect.x + rect.width / 2.0, rect.y + rect.height / 2.0));
            }
        }
    }
    None
}

fn click(ui: &mut UiSurface, at: (f64, f64)) -> Vec<UiEvent> {
    ui.pointer_move(at.0, at.1);
    ui.pointer_down(at.0, at.1, PointerButton::Primary);
    ui.pointer_up(at.0, at.1, PointerButton::Primary);
    ui.take_events()
}

fn click_tooltip(
    app: &mut App,
    ui: &mut UiSurface,
    tip: &str,
    area: &(std::ops::Range<usize>, std::ops::Range<usize>),
) {
    paint(ui);
    let at =
        find_by_tooltip(ui, tip, area).unwrap_or_else(|| panic!("no control with tooltip {tip}"));
    let now = Instant::now();
    let mut refresh = Refresh::default();
    for event in click(ui, at) {
        refresh = refresh.merge(app.handle_event(&event, now));
    }
    assert!(!refresh.is_empty(), "clicking {tip} did nothing");
    refresh.apply(app, ui, now).expect("apply");
    paint(ui);
}

#[test]
fn clicking_a_swatch_changes_the_accent_and_re_themes_the_window() {
    let (_dir, mut app) = new_app(Page::Appearance);
    let mut ui = build_surface(&mut app, SIZE, 1.0, Instant::now()).expect("surface");
    click_tooltip(&mut app, &mut ui, "Pink", &ACCENT_ROW);
    assert_eq!(
        app.ctx(Instant::now()).config.appearance.accent.as_deref(),
        Some("#E8348A")
    );
    assert_eq!(
        ui.theme().palette.accent.to_hex().to_ascii_uppercase(),
        "#E8348A"
    );
    assert!(app.model().toast.as_ref().is_some_and(|t| t.undo.is_some()));
}

#[test]
fn clicking_a_slider_position_sets_the_animation_speed() {
    let (_dir, mut app) = new_app(Page::Appearance);
    let mut ui = build_surface(&mut app, SIZE, 1.0, Instant::now()).expect("surface");
    click_tooltip(&mut app, &mut ui, "2x", &MOTION_ROW);
    let config = app.ctx(Instant::now()).config.appearance.clone();
    assert!(config.animations);
    assert_eq!(config.animation_speed, 2.0);
    click_tooltip(&mut app, &mut ui, "Off", &MOTION_ROW);
    assert!(!app.ctx(Instant::now()).config.appearance.animations);
}

#[test]
fn the_planned_sidebar_entry_has_a_tooltip_and_does_nothing_when_clicked() {
    let (_dir, mut app) = new_app(Page::Appearance);
    let mut ui = build_surface(&mut app, SIZE, 1.0, Instant::now()).expect("surface");
    paint(&mut ui);
    // "Notifications" is the first planned entry, below the first separator.
    let rect = ui.element_rect(".dis").expect("planned navigation entry");
    let at = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    ui.pointer_move(at.0, at.1);
    let hover = ui.hover_target().expect("hover target");
    assert_eq!(hover.tooltip.as_deref(), Some("Coming soon"));
    assert!(click(&mut ui, at).is_empty());
    assert_eq!(app.model().page, Page::Appearance);
}

#[test]
fn clicking_and_dragging_a_number_slider_on_the_desktop_page() {
    let (_dir, mut app) = new_app(Page::Desktop);
    let mut ui = build_surface(&mut app, SIZE, 1.0, Instant::now()).expect("surface");
    paint(&mut ui);
    let track = ui
        .element_rect("#trk-compositor-desktops")
        .expect("the desktop count slider");
    let y = track.y + track.height / 2.0;
    let now = Instant::now();

    // A click on the right end jumps to the largest value.
    let end = (track.x + track.width - 2.0, y);
    for event in click(&mut ui, end) {
        let refresh = app.handle_event(&event, now);
        refresh.apply(&app, &mut ui, now).expect("apply");
    }
    assert_eq!(app.ctx(now).config.compositor.desktops, 32);

    // A drag reports the track it started on, from the press to the release.
    paint(&mut ui);
    let track = ui
        .element_rect("#trk-compositor-desktops")
        .expect("the slider after the page was rebuilt");
    let start = (track.x + track.width - 2.0, y);
    let finish = (track.x + 2.0, y);
    ui.pointer_move(start.0, start.1);
    ui.pointer_down(start.0, start.1, PointerButton::Primary);
    ui.pointer_move(finish.0, finish.1);
    ui.pointer_up(finish.0, finish.1, PointerButton::Primary);
    let drags: Vec<_> = ui
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            UiEvent::Drag(drag) => Some(drag),
            _ => None,
        })
        .collect();
    assert!(
        drags
            .iter()
            .all(|drag| drag.name == "range" && drag.arg.as_deref() == Some("compositor.desktops")),
        "{drags:?}"
    );
    assert_eq!(
        drags.last().map(|drag| drag.phase),
        Some(desilium_ui::DragPhase::End)
    );
    let last = drags.last().expect("a drag");
    let fraction = (last.pos.0 - track.x) / track.width;
    app.range_drag("compositor.desktops", last.phase, fraction, now);
    assert_eq!(app.ctx(now).config.compositor.desktops, 1);
}

#[test]
fn the_wheel_scrolls_a_page_taller_than_the_window() {
    let (_dir, mut app) = new_app(Page::Appearance);
    let mut ui = build_surface(&mut app, (1280, 500), 1.0, Instant::now()).expect("surface");
    let stride = ui.min_stride();
    let mut before = vec![0; stride * ui.buffer_size().1 as usize];
    ui.render(&mut before, stride).expect("render");
    ui.pointer_move(700.0, 300.0);
    // The document engine scrolls content down for a negative delta (winit convention).
    ui.wheel(
        700.0,
        300.0,
        desilium_ui::WheelDelta::Lines { x: 0.0, y: -5.0 },
    );
    let mut after = vec![0; stride * ui.buffer_size().1 as usize];
    ui.render(&mut after, stride).expect("render");
    assert_ne!(before, after, "the content moved");
}
