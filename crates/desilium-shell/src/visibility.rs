//! When the top bar and the dock are on screen: always, on hover only, or out of the windows' way.

use std::time::Duration;

use desilium_config_types::Visibility;

use crate::model::windows::WindowModel;

/// Thickness of the strip an auto-hidden panel leaves at the screen edge to catch the pointer.
pub const PEEK: u32 = 2;
/// Time the pointer must have left a panel before it hides again.
pub const HIDE_DELAY: Duration = Duration::from_millis(350);

/// Global logical rectangle as `(x, y, width, height)`.
pub type Rect = (i32, i32, i32, i32);

/// What a panel surface currently does on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Fully visible.
    Shown,
    /// Reduced to a thin strip at the edge that reveals it on hover.
    Peek,
    /// Switched off: one pixel outside the screen, unreachable.
    Off,
}

impl Presence {
    /// Presence of a panel that is `enabled` and wanted hidden or not.
    pub fn of(enabled: bool, hidden: bool) -> Self {
        match (enabled, hidden) {
            (false, _) => Self::Off,
            (true, true) => Self::Peek,
            (true, false) => Self::Shown,
        }
    }

    /// Height of a layer surface whose full height is `full`.
    pub fn height(self, full: u32) -> u32 {
        match self {
            Self::Shown => full,
            Self::Peek => PEEK.min(full),
            Self::Off => 1,
        }
    }
}

/// Height the top bar takes from the top edge for other surfaces to stay clear of.
pub fn bar_inset(settings: &desilium_config_types::ShellSettings) -> i32 {
    if settings.bar_enabled && settings.bar_visibility == Visibility::Always {
        i32::from(settings.bar_height)
    } else {
        0
    }
}

/// Whether the dock or taskbar keeps windows out of the space it covers.
pub fn reserves_space(settings: &desilium_config_types::ShellSettings) -> bool {
    settings.panel_enabled
        && settings.panel_visibility == Visibility::Always
        && settings.panel_reserve
}

/// Exclusive zone of a dock or taskbar of `height` shown in full: its own height plus the edge
/// margin, so the gap to the screen edge reappears between the panel and the windows.
///
/// The compositor adds the margin of the anchored edge to a positive zone itself, which is the
/// gap on the screen side; the margin in the zone is the matching gap on the window side.
pub fn panel_zone(settings: &desilium_config_types::ShellSettings, height: u32) -> i32 {
    i32::try_from(height)
        .unwrap_or(i32::MAX)
        .saturating_add(i32::from(settings.margin))
}

/// Whether a panel in `mode`, occupying `panel` when shown, should currently be hidden.
pub fn hidden(
    mode: Visibility,
    panel: Rect,
    windows: &WindowModel,
    workspace: u32,
    hovered: bool,
) -> bool {
    match mode {
        Visibility::Always => false,
        Visibility::AutoHide => !hovered,
        Visibility::AvoidWindows => !hovered && overlapped(panel, windows, workspace),
    }
}

/// Whether a visible window of `workspace` overlaps `panel`.
pub fn overlapped(panel: Rect, windows: &WindowModel, workspace: u32) -> bool {
    windows
        .windows()
        .iter()
        .filter(|window| !window.minimized && window.on_workspace(workspace))
        .any(|window| intersects(panel, window.rect))
}

fn intersects(a: Rect, b: Rect) -> bool {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    aw > 0
        && ah > 0
        && bw > 0
        && bh > 0
        && i64::from(ax) < i64::from(bx) + i64::from(bw)
        && i64::from(bx) < i64::from(ax) + i64::from(aw)
        && i64::from(ay) < i64::from(by) + i64::from(bh)
        && i64::from(by) < i64::from(ay) + i64::from(ah)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::windows::{WindowId, WindowInfo};

    fn model(windows: Vec<WindowInfo>) -> WindowModel {
        let mut model = WindowModel::default();
        model.replace(windows);
        model
    }

    fn window(id: u32, rect: Rect) -> WindowInfo {
        WindowInfo {
            rect,
            ..WindowInfo::new(WindowId(id))
        }
    }

    const BAR: Rect = (0, 0, 1000, 36);

    #[test]
    fn always_is_never_hidden() {
        let windows = model(vec![window(1, (0, 0, 1000, 800))]);
        assert!(!hidden(Visibility::Always, BAR, &windows, 0, false));
    }

    #[test]
    fn auto_hide_follows_the_pointer_only() {
        let windows = model(vec![]);
        assert!(hidden(Visibility::AutoHide, BAR, &windows, 0, false));
        assert!(!hidden(Visibility::AutoHide, BAR, &windows, 0, true));
    }

    #[test]
    fn dodging_hides_only_under_an_overlapping_window() {
        let touching = model(vec![window(1, (100, 30, 300, 300))]);
        let below = model(vec![window(1, (100, 36, 300, 300))]);
        assert!(hidden(Visibility::AvoidWindows, BAR, &touching, 0, false));
        assert!(!hidden(Visibility::AvoidWindows, BAR, &below, 0, false));
        assert!(!hidden(Visibility::AvoidWindows, BAR, &touching, 0, true));
    }

    #[test]
    fn minimized_and_other_workspace_windows_do_not_count() {
        let mut minimized = window(1, (0, 0, 500, 500));
        minimized.minimized = true;
        let mut elsewhere = window(2, (0, 0, 500, 500));
        elsewhere.workspace = 1;
        let windows = model(vec![minimized, elsewhere]);
        assert!(!hidden(Visibility::AvoidWindows, BAR, &windows, 0, false));
    }

    #[test]
    fn windows_on_all_workspaces_count_everywhere() {
        let mut sticky = window(1, (0, 0, 500, 500));
        sticky.workspace = 2;
        sticky.on_all_workspaces = true;
        assert!(hidden(
            Visibility::AvoidWindows,
            BAR,
            &model(vec![sticky]),
            0,
            false
        ));
    }

    #[test]
    fn an_always_visible_panel_reserves_its_height_plus_the_edge_margin() {
        let mut settings = desilium_config_types::ShellSettings::default();
        assert!(reserves_space(&settings));
        assert_eq!(
            panel_zone(&settings, 56),
            56 + i32::from(settings.margin),
            "the gap above the panel mirrors the gap below it"
        );
        settings.panel_reserve = false;
        assert!(!reserves_space(&settings));
        settings.panel_reserve = true;
        settings.panel_visibility = Visibility::AutoHide;
        assert!(!reserves_space(&settings));
        settings.panel_visibility = Visibility::Always;
        settings.panel_enabled = false;
        assert!(!reserves_space(&settings));
    }

    #[test]
    fn the_bar_inset_follows_the_configured_height() {
        let mut settings = desilium_config_types::ShellSettings {
            bar_height: 40,
            ..Default::default()
        };
        assert_eq!(bar_inset(&settings), 40);
        settings.bar_visibility = Visibility::AutoHide;
        assert_eq!(bar_inset(&settings), 0);
    }

    #[test]
    fn presence_heights_are_bounded() {
        assert_eq!(Presence::Shown.height(36), 36);
        assert_eq!(Presence::Peek.height(36), PEEK);
        assert_eq!(Presence::Peek.height(1), 1);
        assert_eq!(Presence::Off.height(36), 1);
        assert_eq!(Presence::of(false, true), Presence::Off);
    }
}
