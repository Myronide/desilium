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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellSettings {
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
