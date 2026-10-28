//! Geometry of the dock tile. The CSS in `assets/dock.css` uses the same numbers.

use desilium_ui::Rect;

/// Icon cell edge.
pub(crate) const ITEM: u32 = 48;
/// Space between cells.
pub(crate) const GAP: u32 = 8;
/// Horizontal padding inside the tile.
pub(crate) const PAD_X: u32 = 10;
/// Vertical padding inside the tile.
pub(crate) const PAD_Y: u32 = 7;
/// Tile border width.
pub(crate) const BORDER: u32 = 1;
/// Separator width including its 2 px side margins.
pub(crate) const SEPARATOR: u32 = 5;
/// Tile height.
pub(crate) const HEIGHT: u32 = ITEM + 2 * (PAD_Y + BORDER);
/// Distance between the tile and the screen edge.
pub(crate) const BOTTOM_MARGIN: i32 = 14;
/// Quick buttons a taskbar shows next to the launcher.
pub(crate) const TASKBAR_TOOLS: usize = 2;
/// Width of the taskbar's right cluster: tray icons, two-line clock and the settings button.
pub(crate) const SYSTEM_WIDTH: u32 = 236;

/// Where everything sits inside the dock surface.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DockLayout {
    /// Surface width.
    pub width: u32,
    pub height: u32,
    /// Reserved system cluster; application scrolling never overlaps it.
    pub system: Option<Rect>,
    /// The launcher button.
    pub launcher: Rect,
    /// Quick buttons after the launcher (taskbar only): show desktop, next workspace.
    pub tools: Vec<Rect>,
    /// App cells in order.
    pub items: Vec<Rect>,
}

fn cell(x: u32) -> Rect {
    Rect {
        x: f64::from(x),
        y: f64::from(BORDER + PAD_Y),
        width: f64::from(ITEM),
        height: f64::from(ITEM),
    }
}

/// Lays out the launcher button, `count` app cells and an optional separator before cell `separator`.
pub(crate) fn layout(count: usize, separator: Option<usize>) -> DockLayout {
    let mut x = BORDER + PAD_X;
    let launcher = cell(x);
    x += ITEM;
    let mut items = Vec::with_capacity(count);
    for index in 0..count {
        x += GAP;
        if separator == Some(index) {
            x += SEPARATOR + GAP;
        }
        items.push(cell(x));
        x += ITEM;
    }
    if separator == Some(count) {
        x += GAP + SEPARATOR;
    }
    DockLayout {
        width: x + PAD_X + BORDER,
        height: HEIGHT,
        system: None,
        launcher,
        tools: Vec::new(),
        items,
    }
}

/// Runtime layout shares its hit rectangles with minimize animations.
pub(crate) fn configured(
    count: usize,
    separator: Option<usize>,
    settings: &desilium_config_types::ShellSettings,
    output_width: u32,
) -> DockLayout {
    use desilium_config_types::PanelMode;
    let taskbar = settings.mode == PanelMode::Taskbar;
    let icon = u32::from(settings.icon_size);
    let cell_width = icon
        + if taskbar && settings.show_labels {
            100
        } else {
            0
        };
    let gap = u32::from(settings.spacing);
    let available = output_width
        .saturating_sub(2 * u32::from(settings.margin))
        .max(1);
    let mut x = BORDER + PAD_X;
    let rectangle = |x| Rect {
        x: f64::from(x),
        y: f64::from(BORDER + PAD_Y),
        width: f64::from(cell_width),
        height: f64::from(icon),
    };
    let launcher = Rect {
        width: f64::from(icon),
        ..rectangle(x)
    };
    x += icon;
    // A taskbar has two quick buttons (show desktop, next workspace) next to the launcher.
    let mut tools = Vec::new();
    if taskbar {
        for _ in 0..TASKBAR_TOOLS {
            x += gap;
            tools.push(Rect {
                width: f64::from(icon),
                ..rectangle(x)
            });
            x += icon;
        }
    }
    let first_item = x + gap;
    let mut items = Vec::with_capacity(count);
    for index in 0..count {
        x += gap;
        if separator == Some(index) {
            x += SEPARATOR + gap;
        }
        items.push(rectangle(x));
        x += cell_width;
    }
    if separator == Some(count) {
        x += gap + SEPARATOR;
    }
    let width = if taskbar {
        available
    } else {
        (x + PAD_X + BORDER).min(available)
    };
    let system = taskbar
        .then(|| {
            let left = first_item.saturating_sub(gap) + 2 * (PAD_X + BORDER) + gap;
            let reserved = SYSTEM_WIDTH.min(width.saturating_sub(left));
            Rect {
                x: f64::from(width.saturating_sub(PAD_X + BORDER + reserved)),
                y: f64::from(PAD_Y + BORDER),
                width: f64::from(reserved),
                height: f64::from(icon),
            }
        })
        .filter(|rect| rect.width > 0.0);
    if let (true, Some(system)) = (taskbar && count > 0, system) {
        // Centre the applications on the bar while they fit between the quick buttons and the
        // system cluster; once they do not, they start next to the buttons and scroll.
        let content = x.saturating_sub(first_item);
        let wanted = i64::from(width.saturating_sub(content) / 2);
        let latest = system.x as i64 - i64::from(gap) - i64::from(content);
        let start = wanted.clamp(i64::from(first_item), latest.max(i64::from(first_item)));
        let shift = (start - i64::from(first_item)) as f64;
        for item in &mut items {
            item.x += shift;
        }
    }
    DockLayout {
        width,
        height: icon + 2 * (PAD_Y + BORDER),
        system,
        launcher,
        tools,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_dock_is_just_the_launcher() {
        let l = layout(0, None);
        assert_eq!(l.width, 1 + 10 + 48 + 10 + 1);
        assert!(l.items.is_empty());
    }

    #[test]
    fn cells_are_spaced_by_the_gap() {
        let l = layout(3, None);
        assert_eq!(l.items[0].x, 11.0 + 48.0 + 8.0);
        assert_eq!(l.items[1].x - l.items[0].x, 56.0);
        assert_eq!(l.width, 11 + 4 * 48 + 3 * 8 + 11);
    }

    #[test]
    fn separator_adds_its_width_and_gap() {
        let plain = layout(3, None);
        let split = layout(3, Some(1));
        assert_eq!(split.width, plain.width + SEPARATOR + GAP);
        assert_eq!(split.items[0].x, plain.items[0].x);
        assert_eq!(
            split.items[1].x,
            plain.items[1].x + f64::from(SEPARATOR + GAP)
        );
    }

    #[test]
    fn height_matches_the_mockup() {
        assert_eq!(HEIGHT, 64);
    }
}

#[cfg(test)]
mod configured_tests {
    use super::*;
    #[test]
    fn taskbar_uses_available_width_and_named_cells() {
        let settings = desilium_config_types::ShellSettings {
            mode: desilium_config_types::PanelMode::Taskbar,
            icon_size: 40,
            margin: 10,
            show_labels: true,
            ..Default::default()
        };
        let layout = configured(3, None, &settings, 1280);
        assert_eq!(layout.width, 1260);
        assert_eq!(layout.height, 56);
        assert_eq!(layout.items[0].width, 140.0);
    }
    #[test]
    fn taskbar_centres_its_apps_between_the_quick_buttons_and_the_system_cluster() {
        let settings = desilium_config_types::ShellSettings {
            mode: desilium_config_types::PanelMode::Taskbar,
            ..Default::default()
        };
        let layout = configured(5, None, &settings, 1280);
        assert_eq!(layout.tools.len(), TASKBAR_TOOLS);
        let first = layout.items.first().expect("first");
        let last = layout.items.last().expect("last");
        let middle = (first.x + last.x + last.width) / 2.0;
        assert!((middle - f64::from(layout.width) / 2.0).abs() <= 1.0);
        let system = layout.system.expect("system cluster");
        assert!(last.x + last.width < system.x);
        assert!(first.x > layout.tools[1].x + layout.tools[1].width);
    }
    #[test]
    fn a_crowded_taskbar_starts_its_apps_next_to_the_quick_buttons() {
        let settings = desilium_config_types::ShellSettings {
            mode: desilium_config_types::PanelMode::Taskbar,
            ..Default::default()
        };
        let layout = configured(40, None, &settings, 1280);
        let tools_end = layout.tools[1].x + layout.tools[1].width;
        assert_eq!(layout.items[0].x, tools_end + f64::from(settings.spacing));
    }
    #[test]
    fn dock_geometry_follows_icon_size_spacing_and_output_limit() {
        let small = desilium_config_types::ShellSettings {
            icon_size: 32,
            spacing: 4,
            ..Default::default()
        };
        let large = desilium_config_types::ShellSettings {
            icon_size: 64,
            spacing: 12,
            ..small.clone()
        };
        assert!(configured(4, None, &large, 1920).width > configured(4, None, &small, 1920).width);
        assert!(configured(40, None, &large, 800).width <= 800);
    }
}

/// Output-local surface rectangle, matching the layer-shell anchor contract.
pub(crate) fn screen_rect(
    settings: &desilium_config_types::ShellSettings,
    layout: &DockLayout,
    output: (i32, i32),
) -> Rect {
    let y = if settings.edge == desilium_config_types::PanelEdge::Top {
        crate::visibility::bar_inset(settings) + i32::from(settings.margin)
    } else {
        output.1 - i32::from(settings.margin) - layout.height as i32
    };
    Rect {
        x: f64::from((output.0 - layout.width as i32) / 2),
        y: f64::from(y.max(0)),
        width: f64::from(layout.width),
        height: f64::from(layout.height),
    }
}
