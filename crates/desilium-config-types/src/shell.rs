//! Persisted, GUI-editable panel and dock preferences.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PanelMode {
    #[default]
    Dock,
    Taskbar,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PanelEdge {
    Top,
    #[default]
    Bottom,
}
/// When a panel is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Visibility {
    /// Always shown.
    #[default]
    Always,
    /// Hidden until the pointer touches its screen edge.
    AutoHide,
    /// Hidden while a window overlaps it.
    AvoidWindows,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellSettings {
    /// Reopen the applications and window layout of the last session at startup.
    pub restore_session: bool,
    /// Show the contents of the desktop folder as icons on the desktop.
    pub desktop_icons: bool,
    /// Show the dock or taskbar at all.
    pub panel_enabled: bool,
    pub panel_visibility: Visibility,
    /// Show the top bar at all.
    pub bar_enabled: bool,
    pub bar_visibility: Visibility,
    /// Height of the top bar in logical pixels.
    pub bar_height: u16,
    /// Name and icon of the focused application in the top bar.
    pub bar_show_app: bool,
    /// Title of the focused window in the top bar.
    pub bar_show_title: bool,
    /// Date next to the time in the top bar.
    pub bar_show_date: bool,
    /// Keyboard layout button in the top bar.
    pub bar_show_layout: bool,
    /// Power button in the top bar.
    pub bar_show_power: bool,
    /// Keep windows clear of an always visible dock or taskbar instead of lying under it.
    pub panel_reserve: bool,
    pub mode: PanelMode,
    pub edge: PanelEdge,
    pub icon_size: u16,
    pub spacing: u16,
    pub margin: u16,
    pub opacity: u16,
    /// Corner radius of the dock or taskbar in logical pixels.
    pub panel_radius: u16,
    /// Corner radius of the launcher and quick settings popups in logical pixels.
    pub popup_radius: u16,
    pub show_labels: bool,
    pub show_running: bool,
    pub show_indicators: bool,
    pub show_workspaces: bool,
    pub show_clock: bool,
    pub clock_24h: bool,
    pub show_battery: bool,
    pub quick_settings: bool,
}
impl Default for ShellSettings {
    fn default() -> Self {
        Self {
            restore_session: false,
            desktop_icons: true,
            panel_enabled: true,
            panel_visibility: Visibility::Always,
            bar_enabled: true,
            bar_visibility: Visibility::Always,
            bar_height: 32,
            bar_show_app: true,
            bar_show_title: true,
            bar_show_date: true,
            bar_show_layout: true,
            bar_show_power: true,
            panel_reserve: true,
            mode: PanelMode::Dock,
            edge: PanelEdge::Bottom,
            icon_size: 48,
            spacing: 8,
            margin: 14,
            opacity: 92,
            panel_radius: 22,
            popup_radius: 20,
            show_labels: false,
            show_running: true,
            show_indicators: true,
            show_workspaces: true,
            show_clock: true,
            clock_24h: true,
            show_battery: true,
            quick_settings: true,
        }
    }
}
