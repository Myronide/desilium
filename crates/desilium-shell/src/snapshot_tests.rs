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
            icon: demo_icon("code"),
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
        icon: demo_icon(id),
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

/// The icon files of `assets/demo-apps`, standing in for what installed applications ship; ids
/// without one show the neutral glyph, which is what an application without an icon gets.
fn demo_icon(id: &str) -> IconSpec {
    let file = match id.trim_end_matches(".desktop") {
        "nautilus" | "doria" => "demo-files",
        "firefox" => "demo-web",
        "code" => "demo-code",
        "music" => "demo-music",
        "mail" => "demo-mail",
        _ => return IconSpec::NEUTRAL,
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/demo-apps/icons/desilium-demo/scalable/apps")
        .join(format!("{file}.svg"));
    IconSpec::File(crate::view::icon::file_url(&path))
}

struct Tiles;

impl crate::launcher::RowIcons for Tiles {
    fn app_icon(&mut self, icon: &RowIcon, _: u32) -> IconSpec {
        match icon {
            RowIcon::App { desktop_id, .. } => demo_icon(desktop_id),
            _ => IconSpec::NEUTRAL,
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
            icon: demo_icon(id),
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
        let html =
            crate::dock::configured_html_for_tests(&view, &layout, &settings, &taskbar_status());
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

#[test]
fn icons_inserted_into_an_existing_document_are_drawn() {
    // The dock replaces its children on every update, so an icon file has to load into a
    // document that already exists, not only into one built around it.
    let wallpaper = [38, 30, 84];
    let html = crate::view::icon::icon_html(&demo_icon("firefox"), "", 64);
    let mut options = SurfaceOptions::new(90, 90);
    options.theme = Theme::dark();
    let mut ui = UiSurface::new("<div id=root></div>", &[BASE_CSS], options).unwrap();
    ui.replace_children("#root", &html).unwrap();
    let (rgba, _) = snapshot(&mut ui, "icon-inserted-late", wallpaper);
    let drawn = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .any(|px| px[..3] != wallpaper);
    assert!(drawn, "the icon file did not show up");
}

fn desktop_with_items() -> crate::desktop::Desktop {
    use crate::model::desktop::{DeskItem, Kind};
    let item = |name: &str, kind: Kind, icon: Option<&str>| DeskItem {
        path: PathBuf::from(format!("/home/u/Desktop/{name}")),
        file_name: name.to_owned(),
        name: name.trim_end_matches(".desktop").to_owned(),
        kind,
        icon: icon.map(str::to_owned),
    };
    let mut desktop = crate::desktop::Desktop::default();
    desktop.set_items(vec![
        item("Projects", Kind::Folder, None),
        item("Photos", Kind::Folder, None),
        item(
            "Browser.desktop",
            Kind::Launcher { trusted: true },
            Some("firefox"),
        ),
        item(
            "Downloaded.desktop",
            Kind::Launcher { trusted: false },
            Some("mail"),
        ),
        item("budget-2026-final-version.pdf", Kind::File, None),
        item("notes.txt", Kind::File, None),
    ]);
    desktop
}

fn desktop_icons(desktop: &crate::desktop::Desktop) -> Vec<IconSpec> {
    use crate::model::desktop::Kind;
    desktop
        .items
        .iter()
        .map(|item| match (item.icon.as_deref(), item.kind) {
            (Some(icon), _) => demo_icon(icon),
            (None, Kind::Folder) => demo_icon("nautilus"),
            (None, _) => IconSpec::NEUTRAL,
        })
        .collect()
}

/// The shell redraws the icons after every pointer event of a drag, which replaces the very
/// element the button went down on. The drag must go on and end properly.
#[test]
fn a_drag_survives_the_desktop_being_redrawn_under_the_pointer() {
    use desilium_ui::{DragPhase, PointerButton, UiEvent};
    let appearance = desilium_config_types::AppearanceSettings::default();
    let look = crate::wallpaper::Look::of(&appearance, &Theme::dark());
    let mut options = SurfaceOptions::new(1280, 720);
    options.theme = Theme::dark();
    let mut ui = UiSurface::new(&look.html(), &crate::wallpaper::STYLES, options).unwrap();
    let mut desktop = desktop_with_items();
    let icons = desktop_icons(&desktop);
    let grid = crate::model::grid::Grid::new((1280, 720), 32, 100);
    let draw = |ui: &mut UiSurface, desktop: &mut crate::desktop::Desktop| {
        let cells = desktop.cells(&grid);
        let html = crate::desktop::html(desktop, &icons, &grid, &cells, (1280, 720));
        ui.replace_children("#desk", &html).unwrap();
    };
    draw(&mut ui, &mut desktop);
    let first = ui.element_rect(".dicon").unwrap();
    let (x, y) = (first.x + first.width / 2.0, first.y + first.height / 2.0);

    let mut phases = Vec::new();
    ui.pointer_move(x, y);
    ui.pointer_down(x, y, PointerButton::Primary);
    for step in 1..=6 {
        let at = (x + 40.0 * f64::from(step), y + 20.0 * f64::from(step));
        ui.pointer_move(at.0, at.1);
        for event in ui.take_events() {
            let UiEvent::Drag(drag) = event else { continue };
            assert_eq!(drag.name, "desk-icon", "step {step}: {drag:?}");
            phases.push(drag.phase);
            match drag.phase {
                DragPhase::Start => {
                    desktop.begin_icon_drag(0, drag.origin, &grid);
                    desktop.drag_icons(drag.pos, &grid);
                }
                _ => desktop.drag_icons(drag.pos, &grid),
            }
        }
        draw(&mut ui, &mut desktop);
    }
    let end = (x + 240.0, y + 120.0);
    ui.pointer_up(end.0, end.1, PointerButton::Primary);
    for event in ui.take_events() {
        let UiEvent::Drag(drag) = event else { continue };
        assert_eq!(drag.name, "desk-icon", "{drag:?}");
        phases.push(drag.phase);
        desktop.drag_icons(drag.pos, &grid);
        desktop.drop_icons(&grid);
    }
    draw(&mut ui, &mut desktop);
    assert_eq!(phases.first(), Some(&DragPhase::Start), "{phases:?}");
    assert_eq!(phases.last(), Some(&DragPhase::End), "{phases:?}");
    assert!(phases.contains(&DragPhase::Move), "{phases:?}");
}

#[test]
fn the_desktop_draws_icons_and_a_menu_and_reports_clicks() {
    use desilium_ui::{PointerButton, UiEvent};
    let appearance = desilium_config_types::AppearanceSettings::default();
    let look = crate::wallpaper::Look::of(&appearance, &Theme::dark());
    let mut options = SurfaceOptions::new(1280, 720);
    options.theme = Theme::dark();
    let mut ui = UiSurface::new(&look.html(), &crate::wallpaper::STYLES, options).unwrap();
    let mut desktop = desktop_with_items();
    desktop.select_for_menu(2);
    let icons = desktop_icons(&desktop);
    let grid = crate::model::grid::Grid::new((1280, 720), 32, 100);
    let draw = |ui: &mut UiSurface, desktop: &mut crate::desktop::Desktop| {
        let cells = desktop.cells(&grid);
        let html = crate::desktop::html(desktop, &icons, &grid, &cells, (1280, 720));
        ui.replace_children("#desk", &html).unwrap();
    };
    draw(&mut ui, &mut desktop);
    let (rgba, _) = snapshot(&mut ui, "desktop-icons", [38, 30, 84]);
    assert!(rgba.iter().any(|byte| *byte != 0));

    // A click on the first icon, its right click, and a click on empty desktop.
    let first = ui.element_rect(".dicon").unwrap();
    let (x, y) = (first.x + first.width / 2.0, first.y + first.height / 2.0);

    // The pointer is a hand over an icon only; the empty desktop keeps the arrow.
    ui.pointer_move(x, y);
    assert_eq!(ui.cursor_hint(), desilium_ui::CursorHint::Pointer);
    ui.pointer_move(1000.0, 300.0);
    assert_eq!(ui.cursor_hint(), desilium_ui::CursorHint::Default);
    let click = |ui: &mut UiSurface, x: f64, y: f64, button: PointerButton| {
        ui.pointer_move(x, y);
        ui.pointer_down(x, y, button);
        ui.pointer_up(x, y, button);
        ui.take_events()
    };
    let action = |events: Vec<UiEvent>| match events.into_iter().next() {
        Some(UiEvent::Action(action)) => (action.action, action.arg),
        other => panic!("expected an action, got {other:?}"),
    };
    assert_eq!(
        action(click(&mut ui, x, y, PointerButton::Primary)),
        ("desk-item".to_owned(), Some("0".to_owned()))
    );
    assert_eq!(
        action(click(&mut ui, x, y, PointerButton::Secondary)),
        ("desk-item-menu".to_owned(), Some("0".to_owned()))
    );
    assert_eq!(
        action(click(&mut ui, 1000.0, 300.0, PointerButton::Primary)).0,
        "desk-bg"
    );
    assert_eq!(
        ui.pointer_position(),
        Some((1000.0, 300.0)),
        "menus open where the pointer is"
    );
    assert_eq!(
        action(click(&mut ui, 1000.0, 300.0, PointerButton::Secondary)).0,
        "desk-menu"
    );

    // Dragging from an icon starts an icon drag, from the empty desktop a selection rectangle.
    let drag_from = |ui: &mut UiSurface, x: f64, y: f64| {
        ui.pointer_move(x, y);
        ui.pointer_down(x, y, PointerButton::Primary);
        ui.pointer_move(x + 40.0, y + 40.0);
        ui.pointer_up(x + 40.0, y + 40.0, PointerButton::Primary);
        ui.take_events().into_iter().find_map(|event| match event {
            UiEvent::Drag(drag) => Some((drag.name, drag.arg)),
            _ => None,
        })
    };
    assert_eq!(
        drag_from(&mut ui, x, y),
        Some(("desk-icon".to_owned(), Some("0".to_owned())))
    );
    assert_eq!(
        drag_from(&mut ui, 1000.0, 300.0),
        Some(("desk-band".to_owned(), None))
    );

    // Carrying two icons and drawing a selection rectangle, as the shell draws them.
    desktop.selected = ["Projects", "Photos"].map(str::to_owned).into();
    desktop.begin_icon_drag(0, (x, y), &grid);
    desktop.drag_icons((x + 330.0, y + 180.0), &grid);
    draw(&mut ui, &mut desktop);
    snapshot(&mut ui, "desktop-drag", [38, 30, 84]);
    assert!(ui.element_rect(".dtarget").is_ok());
    desktop.end_gesture();
    desktop.begin_band((600.0, 120.0));
    desktop.drag_band((20.0, 330.0), &grid);
    draw(&mut ui, &mut desktop);
    snapshot(&mut ui, "desktop-band", [38, 30, 84]);
    assert!(ui.element_rect(".dband").is_ok());
    assert!(desktop.selected.len() >= 2, "{:?}", desktop.selected);
    desktop.clear();

    // The menu of an icon.
    desktop.menu = Some(crate::desktop::Menu {
        target: Some("Downloaded.desktop".into()),
        at: (700, 200),
    });
    draw(&mut ui, &mut desktop);
    snapshot(&mut ui, "desktop-menu", [38, 30, 84]);
    let menu = ui.element_rect(".dmenu").unwrap();
    assert!(menu.x >= 0.0 && menu.x + menu.width <= 1280.0 && menu.y + menu.height <= 720.0);
    let command = ui.element_rect(".dcmd").unwrap();
    let (cx, cy) = (command.x + 20.0, command.y + 10.0);
    assert_eq!(
        action(click(&mut ui, cx, cy, PointerButton::Primary)),
        ("desk-cmd".to_owned(), Some("open".to_owned())),
        "a menu button wins over the desktop behind it"
    );

    // Renaming shows an input holding the current name.
    desktop.menu = None;
    desktop.renaming = Some("notes.txt".into());
    draw(&mut ui, &mut desktop);
    ui.focus("#desk-rename").unwrap();
    assert_eq!(ui.input_value("#desk-rename").unwrap(), "notes.txt");
    snapshot(&mut ui, "desktop-rename", [38, 30, 84]);
}
