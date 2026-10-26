#![allow(clippy::single_range_in_vec_init)] // a highlight list with one range is exactly what the rows need
//! Renders the real documents with the real UI engine and checks pixels.
//!
//! PNGs go to `target/shell-snapshots/` for eyeballing; no display or GPU is needed.

use std::{fs, path::PathBuf};

use desilium_theme::Theme;
use desilium_ui::{SurfaceOptions, UiSurface};

use crate::{
    bar::{ActiveApp, BarView, bar_html},
    dock::{DockCell, DockView},
    model::{
        battery::{Battery, BatteryStatus},
        clock::ClockText,
        dock::Dot,
        icons::tile_for,
        launcher::{Activation, CalcCard, Category, LauncherModel, Results, Row, RowIcon, Section},
        windows::Workspaces,
    },
    view::{BAR_CSS, BASE_CSS, DOCK_CSS, LAUNCHER_CSS, TOOLTIP_CSS, icon::IconSpec},
};

fn out_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/shell-snapshots");
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Draws `ui` over a flat wallpaper colour and writes `name.png`; returns RGBA pixels.
fn snapshot(ui: &mut UiSurface, name: &str, wallpaper: [u8; 3]) -> (Vec<u8>, u32) {
    let stride = ui.min_stride();
    let (width, height) = ui.buffer_size();
    let mut buffer = vec![0; stride * height as usize];
    ui.render(&mut buffer, stride).unwrap();
    let mut rgba = Vec::with_capacity(buffer.len());
    for px in buffer.as_chunks::<4>().0 {
        let inverse = 255 - u32::from(px[3]);
        let blend =
            |top: u8, back: u8| (u32::from(top) + u32::from(back) * inverse / 255).min(255) as u8;
        rgba.extend([
            blend(px[2], wallpaper[0]),
            blend(px[1], wallpaper[1]),
            blend(px[0], wallpaper[2]),
            255,
        ]);
    }
    let file = fs::File::create(out_dir().join(format!("{name}.png"))).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&rgba)
        .unwrap();
    (rgba, width)
}

fn at(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * width + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2]]
}

fn themes() -> [(&'static str, Theme, [u8; 3]); 2] {
    [
        ("dark", Theme::dark(), [38, 30, 84]),
        ("light", Theme::light(), [214, 218, 246]),
    ]
}

fn bar_view() -> BarView {
    BarView {
        workspaces: Workspaces {
            count: 4,
            active: 1,
        },
        app: ActiveApp {
            name: "Code".into(),
            glyph: "code".into(),
            title: "main.rs - desilium".into(),
        },
        clock: ClockText {
            date: "Mon 26 Oct".into(),
            time: "14:32".into(),
        },
        battery: Some(Battery {
            percent: 82,
            status: BatteryStatus::Discharging,
        }),
        layout: "EN".into(),
    }
}

#[test]
fn bar_renders_accent_pill_and_text() {
    for (name, theme, wallpaper) in themes() {
        let mut options = SurfaceOptions::new(1920, 32);
        options.theme = theme.clone();
        let mut ui = UiSurface::new(&bar_html(&bar_view()), &[BASE_CSS, BAR_CSS], options).unwrap();
        let (rgba, width) = snapshot(&mut ui, &format!("bar-{name}"), wallpaper);
        let accent = theme.palette.accent.to_bytes();
        // The active workspace pill is the only accent-coloured element in the first 90 px.
        let found = (0..90).any(|x| at(&rgba, width, x, 16) == [accent[0], accent[1], accent[2]]);
        assert!(found, "{name}: no accent pill in the workspace area");
        let clock = ui.element_rect("#clock").unwrap();
        let centre = clock.x + clock.width / 2.0;
        assert!(
            (centre - 960.0).abs() < 2.0,
            "{name}: clock centre {centre}"
        );
    }
}

fn dock_view(with_files: bool) -> DockView {
    let cell = |name: &str, id: &str, dots: Vec<Dot>| DockCell {
        key: format!("{id}.desktop"),
        name: name.into(),
        icon: IconSpec::Tile(tile_for(&format!("{id}.desktop"), &[])),
        dots,
    };
    let mut cells = vec![
        cell("Files", "nautilus", vec![Dot::Open]),
        cell("Firefox", "firefox", vec![]),
        cell("Terminal", "foot", vec![Dot::Open, Dot::Focused]),
        cell("Settings", "desilium-settings", vec![]),
    ];
    if with_files {
        cells.push(cell("Mystery", "mystery", vec![Dot::Focused]));
    }
    DockView {
        cells,
        separator: with_files.then_some(4),
    }
}

#[test]
fn dock_dom_matches_the_computed_layout() {
    use crate::dock::{HEIGHT, layout};
    let view = dock_view(true);
    let geometry = layout::layout(view.cells.len(), view.separator);
    let html = format!(
        "<div id=root>{}</div>",
        crate::dock::dock_html_for_tests(&view, &geometry)
    );
    let mut ui = UiSurface::new(
        &html,
        &[BASE_CSS, DOCK_CSS],
        SurfaceOptions::new(geometry.width, HEIGHT),
    )
    .unwrap();
    let launcher = ui.element_rect("#dock-launcher").unwrap();
    assert_eq!(
        (launcher.x, launcher.y),
        (geometry.launcher.x, geometry.launcher.y)
    );
    for (index, expected) in geometry.items.iter().enumerate() {
        let rect = ui.element_rect(&format!("#dock-{index}")).unwrap();
        assert_eq!(
            (rect.x, rect.y, rect.width, rect.height),
            (expected.x, expected.y, 48.0, 48.0),
            "cell {index}"
        );
    }
    let dock = ui.element_rect("#dock").unwrap();
    assert_eq!(
        (dock.width, dock.height),
        (f64::from(geometry.width), f64::from(HEIGHT))
    );
}

#[test]
fn dock_renders_glass_and_tiles() {
    use crate::dock::{HEIGHT, layout};
    for (name, theme, wallpaper) in themes() {
        let view = dock_view(true);
        let geometry = layout::layout(view.cells.len(), view.separator);
        let html = format!(
            "<div id=root>{}</div>",
            crate::dock::dock_html_for_tests(&view, &geometry)
        );
        let mut options = SurfaceOptions::new(geometry.width, HEIGHT);
        options.theme = theme;
        let mut ui = UiSurface::new(&html, &[BASE_CSS, DOCK_CSS], options).unwrap();
        let (rgba, width) = snapshot(&mut ui, &format!("dock-{name}"), wallpaper);
        let outside_corner = at(&rgba, width, 1, 1);
        assert_eq!(
            outside_corner, wallpaper,
            "{name}: rounded corner must stay transparent"
        );
        let cell = geometry.items[2];
        let inside = at(&rgba, width, (cell.x + 24.0) as u32, (cell.y + 24.0) as u32);
        assert_ne!(inside, wallpaper, "{name}: icon tile missing");
    }
}

fn launcher_model() -> LauncherModel {
    let mut model = LauncherModel::default();
    model.set_query("24*7");
    let app = |title: &str, id: &str| Row {
        section: Section::Applications,
        activation: Activation::LaunchApp {
            desktop_id: format!("{id}.desktop"),
        },
        title: title.into(),
        subtitle: "Scientific and programmer modes".into(),
        highlights: vec![0..2],
        icon: RowIcon::App {
            desktop_id: format!("{id}.desktop"),
            icon: None,
            categories: vec!["Utility".into()],
        },
    };
    let rows = vec![
        app("Calculator", "calculator"),
        Row {
            section: Section::Commands,
            activation: Activation::RunInTerminal("expr 24".into()),
            title: "Run expr 24 \\* 7 in Terminal".into(),
            subtitle: "Command".into(),
            highlights: vec![4..15],
            icon: RowIcon::Command,
        },
        Row {
            section: Section::Web,
            activation: Activation::WebSearch("24*7".into()),
            title: "Search the web for \u{201c}24 * 7\u{201d}".into(),
            subtitle: "Default search engine".into(),
            highlights: vec![],
            icon: RowIcon::Web,
        },
    ];
    model.set_results(Results {
        calc: Some(CalcCard {
            expression: "24 \u{d7} 7".into(),
            result: "168".into(),
        }),
        rows,
    });
    assert!(model.set_category(Category::All) || model.category() == Category::All);
    model
}

struct Tiles;

impl crate::launcher::RowIcons for Tiles {
    fn app_icon(&mut self, icon: &RowIcon, _: u32) -> IconSpec {
        match icon {
            RowIcon::App {
                desktop_id,
                categories,
                ..
            } => IconSpec::Tile(tile_for(desktop_id, categories)),
            _ => IconSpec::Tile(crate::model::icons::GENERIC_TILE),
        }
    }
}

#[test]
fn launcher_renders_with_natural_height() {
    use crate::launcher::{PinView, launcher_html_for_tests};
    let pins: Vec<PinView> = ["Files", "Web", "Code", "Terminal", "Music", "Settings"]
        .iter()
        .zip([
            "nautilus",
            "firefox",
            "code",
            "foot",
            "music",
            "desilium-settings",
        ])
        .map(|(name, id)| PinView {
            name: (*name).into(),
            icon: IconSpec::Tile(tile_for(&format!("{id}.desktop"), &[])),
        })
        .collect();
    for (name, theme, wallpaper) in themes() {
        let model = launcher_model();
        let html = launcher_html_for_tests(&model, &pins, &mut Tiles);
        let mut options = SurfaceOptions::new(800, 900);
        options.theme = theme;
        let mut ui = UiSurface::new(
            &format!("<div id=root>{html}</div>"),
            &[BASE_CSS, LAUNCHER_CSS],
            options,
        )
        .unwrap();
        let panel = ui.element_rect("#panel").unwrap();
        assert!(
            panel.height > 600.0 && panel.height < 800.0,
            "{name}: height {}",
            panel.height
        );
        snapshot(&mut ui, &format!("launcher-{name}"), wallpaper);
    }
}

#[test]
fn tooltip_renders_a_bubble() {
    for (name, theme, wallpaper) in themes() {
        let mut options = SurfaceOptions::new(120, 46);
        options.theme = theme;
        let html = "<div class=tip-wrap><div class=tooltip id=tip>Terminal</div></div>";
        let mut ui = UiSurface::new(html, &[BASE_CSS, TOOLTIP_CSS], options).unwrap();
        let bubble = ui.element_rect("#tip").unwrap();
        assert_eq!(bubble.height, 26.0);
        assert!(
            bubble.width > 40.0 && bubble.width < 100.0,
            "width {}",
            bubble.width
        );
        snapshot(&mut ui, &format!("tooltip-{name}"), wallpaper);
    }
}

#[test]
fn typing_into_a_prefilled_search_field_appends_after_the_query() {
    use desilium_ui::{Key, KeyEvent};
    let mut model = launcher_model();
    model.set_query("co");
    let html = crate::launcher::launcher_html_for_tests(&model, &[], &mut Tiles);
    let mut ui = UiSurface::new(
        &format!("<div id=root>{html}</div>"),
        &[BASE_CSS, LAUNCHER_CSS],
        SurfaceOptions::new(800, 900),
    )
    .unwrap();
    ui.element_rect("#panel").unwrap();
    ui.focus("#q").unwrap();
    ui.key_down(&KeyEvent::named(Key::End));
    snapshot(&mut ui, "launcher-caret-end", [38, 30, 84]);
    ui.key_down(&KeyEvent::character("x"));
    assert_eq!(ui.input_value("#q").unwrap(), "cox");
}

fn click(ui: &mut UiSurface, x: f64, y: f64, button: desilium_ui::PointerButton) {
    ui.pointer_move(x, y);
    ui.pointer_down(x, y, button);
    ui.pointer_up(x, y, button);
}

#[test]
fn dock_clicks_emit_the_documented_actions() {
    use crate::dock::{HEIGHT, layout};
    use desilium_ui::PointerButton;
    let view = dock_view(true);
    let geometry = layout::layout(view.cells.len(), view.separator);
    let html = format!(
        "<div id=root>{}</div>",
        crate::dock::dock_html_for_tests(&view, &geometry)
    );
    let mut ui = UiSurface::new(
        &html,
        &[BASE_CSS, DOCK_CSS],
        SurfaceOptions::new(geometry.width, HEIGHT),
    )
    .unwrap();
    let centre = |r: desilium_ui::Rect| (r.x + r.width / 2.0, r.y + r.height / 2.0);

    let (x, y) = centre(geometry.launcher);
    click(&mut ui, x, y, PointerButton::Primary);
    assert_eq!(ui.take_actions()[0].action, "launcher");

    let (x, y) = centre(geometry.items[1]);
    click(&mut ui, x, y, PointerButton::Primary);
    let actions = ui.take_actions();
    assert_eq!(
        (actions[0].action.as_str(), actions[0].arg.as_deref()),
        ("app", Some("firefox.desktop"))
    );

    click(&mut ui, x, y, PointerButton::Middle);
    let actions = ui.take_actions();
    assert_eq!(
        (actions[0].action.as_str(), actions[0].arg.as_deref()),
        ("app-new", Some("firefox.desktop"))
    );

    ui.pointer_move(x, y);
    let hover = ui.hover_target().unwrap();
    assert_eq!(hover.tooltip.as_deref(), Some("Firefox"));
}

#[test]
fn bar_clicks_emit_workspace_and_status_actions() {
    use desilium_ui::PointerButton;
    let mut ui = UiSurface::new(
        &bar_html(&bar_view()),
        &[BASE_CSS, BAR_CSS],
        SurfaceOptions::new(1920, 32),
    )
    .unwrap();
    let pill = ui.element_rect(".ws-pill").unwrap();
    click(
        &mut ui,
        pill.x + pill.width / 2.0,
        16.0,
        PointerButton::Primary,
    );
    let action = ui.take_actions().remove(0);
    assert_eq!(
        (action.action.as_str(), action.arg.as_deref()),
        ("workspace", Some("0"))
    );
    let group = ui.element_rect(".tb-group").unwrap();
    click(&mut ui, group.x + 4.0, 16.0, PointerButton::Primary);
    assert_eq!(ui.take_actions()[0].action, "quick-settings");
}

#[test]
fn surfaces_render_at_fractional_and_double_scale() {
    use crate::dock::{HEIGHT, layout};
    let view = dock_view(false);
    let geometry = layout::layout(view.cells.len(), view.separator);
    for scale in [1.5, 2.0] {
        let html = format!(
            "<div id=root>{}</div>",
            crate::dock::dock_html_for_tests(&view, &geometry)
        );
        let mut options = SurfaceOptions::new(geometry.width, HEIGHT);
        options.scale = scale;
        let mut ui = UiSurface::new(&html, &[BASE_CSS, DOCK_CSS], options).unwrap();
        let (width, height) = ui.buffer_size();
        assert_eq!(f64::from(height), (f64::from(HEIGHT) * scale).round());
        let (rgba, stride) = snapshot(&mut ui, &format!("dock-scale-{scale}"), [38, 30, 84]);
        assert_eq!(stride, width);
        let cell = geometry.items[0];
        let inside = at(
            &rgba,
            width,
            ((cell.x + 24.0) * scale) as u32,
            ((cell.y + 24.0) * scale) as u32,
        );
        assert_ne!(inside, [38, 30, 84], "scale {scale}: tile missing");
        let rect = ui.element_rect("#dock-0").unwrap();
        assert_eq!(
            rect.width, 48.0,
            "layout is in logical pixels at scale {scale}"
        );
    }
}

fn taskbar_status() -> crate::dock::Status {
    crate::dock::Status {
        time: "14:32".into(),
        date: "26.10.2026".into(),
        battery: Some(80),
    }
}

#[test]
fn quick_settings_and_taskbar_render_in_both_themes() {
    use crate::services::controls::{Link, LinkKind, State, WifiNetwork};
    let network = |ssid: &str, signal, secured, active| WifiNetwork {
        ssid: ssid.into(),
        signal,
        secured,
        active,
    };
    let state = State {
        links: Some(vec![Link {
            kind: LinkKind::Wifi,
            name: "Office".into(),
        }]),
        networks: vec![
            network("Office", 82, true, true),
            network("Guest", 61, false, false),
        ],
        volume: Some(42),
        microphone: Some(false),
        wifi: Some(true),
        bluetooth: Some(false),
        brightness: Some(75),
        profile: Some("balanced".into()),
        ..Default::default()
    };
    for (name, theme, wallpaper) in themes() {
        let mut options = SurfaceOptions::new(410, 640);
        options.theme = theme.clone();
        let html = crate::quick::html_for_tests(&state, &Default::default());
        let mut ui =
            UiSurface::new(&html, &[BASE_CSS, include_str!("quick/style.css")], options).unwrap();
        let (rgba, _) = snapshot(&mut ui, &format!("quick-{name}"), wallpaper);
        assert!(rgba.iter().any(|byte| *byte != 0));
        let settings = desilium_config_types::ShellSettings {
            mode: desilium_config_types::PanelMode::Taskbar,
            show_labels: true,
            ..Default::default()
        };
        let view = dock_view(false);
        let layout = crate::dock::layout::configured(view.cells.len(), None, &settings, 1280);
        let html = crate::dock::configured_html_for_tests(&view, &layout, &settings, &taskbar_status());
        assert!(html.contains("task-label"));
        let mut options = SurfaceOptions::new(layout.width, layout.height);
        options.theme = theme.clone();
        let mut ui = UiSurface::new(&html, &[BASE_CSS, DOCK_CSS], options).unwrap();
        snapshot(&mut ui, &format!("taskbar-{name}"), wallpaper);

        // The default look: centred icons without labels, tray, clock and settings on the right.
        let settings = desilium_config_types::ShellSettings {
            mode: desilium_config_types::PanelMode::Taskbar,
            ..Default::default()
        };
        let view = dock_view(true);
        let layout =
            crate::dock::layout::configured(view.cells.len(), view.separator, &settings, 1280);
        let html =
            crate::dock::configured_html_for_tests(&view, &layout, &settings, &taskbar_status());
        assert!(!html.contains("<span class=task-label>"));
        assert!(html.contains("data-action=show-desktop"));
        assert!(html.contains("data-action=settings"));
        let mut options = SurfaceOptions::new(layout.width, layout.height);
        options.theme = theme;
        let mut ui = UiSurface::new(&html, &[BASE_CSS, DOCK_CSS], options).unwrap();
        snapshot(&mut ui, &format!("taskbar-default-{name}"), wallpaper);
    }
}

#[test]
fn configured_dock_hit_rectangles_match_rendered_cells_and_keep_status_separate() {
    use desilium_config_types::{PanelMode, ShellSettings};
    for mode in [PanelMode::Dock, PanelMode::Taskbar] {
        for icon_size in [32, 48, 72] {
            let settings = ShellSettings {
                mode,
                icon_size,
                spacing: 12,
                ..Default::default()
            };
            let view = dock_view(true);
            let layout =
                crate::dock::layout::configured(view.cells.len(), view.separator, &settings, 1280);
            let html = crate::dock::configured_html_for_tests(
                &view,
                &layout,
                &settings,
                &taskbar_status(),
            );
            let mut ui = UiSurface::new(
                &html,
                &[BASE_CSS, DOCK_CSS],
                SurfaceOptions::new(layout.width, layout.height),
            )
            .unwrap();
            for (index, expected) in layout.items.iter().enumerate() {
                let rect = ui.element_rect(&format!("#dock-{index}")).unwrap();
                assert!(
                    (rect.x - expected.x).abs() < 0.1,
                    "{mode:?} {icon_size}: {rect:?} != {expected:?}"
                );
                assert!((rect.y - expected.y).abs() < 0.1);
                assert_eq!(rect.width, expected.width);
                assert_eq!(rect.height, expected.height);
            }
            if let Some(expected) = layout.system {
                let system = ui.element_rect(".task-system").unwrap();
                let strip = ui.element_rect(".dock-strip").unwrap();
                assert_eq!(system.x, expected.x);
                assert!(strip.x + strip.width <= system.x);
                assert!(system.x + system.width <= f64::from(layout.width));
            }
        }
    }
}
