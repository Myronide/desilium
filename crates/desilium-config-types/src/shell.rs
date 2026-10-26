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
    /// Show the dock or taskbar at all.
    pub panel_enabled: bool,
    pub panel_visibility: Visibility,
    /// Show the top bar at all.
    pub bar_enabled: bool,
    pub bar_visibility: Visibility,
    pub mode: PanelMode,
    pub edge: PanelEdge,
    pub icon_size: u16,
    pub spacing: u16,
    pub margin: u16,
    pub opacity: u16,
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
            panel_enabled: true,
            panel_visibility: Visibility::Always,
            bar_enabled: true,
            bar_visibility: Visibility::Always,
            mode: PanelMode::Dock,
            edge: PanelEdge::Bottom,
            icon_size: 48,
            spacing: 8,
            margin: 14,
            opacity: 92,
            show_labels: true,
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
