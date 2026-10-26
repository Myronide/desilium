//! The shell's view of the compositor as plain data, so that diffing needs no Wayland objects.

use desilium_wm::{Rect, WindowId, WindowState};

/// Longest title or app id (in bytes) that is sent to the shell.
pub const MAX_TEXT_BYTES: usize = 1024;

/// Wire scale of 1.0 (the 120ths of `wp_fractional_scale`).
pub const SCALE_UNIT_120: f64 = 120.0;

/// Which window states the shell is told about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowFlags {
    pub activated: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub fullscreen: bool,
}

impl WindowFlags {
    /// Derives the flags; a minimized window is never reported as activated.
    pub fn new(state: WindowState, focused: bool) -> Self {
        Self {
            activated: focused && state != WindowState::Minimized,
            minimized: state == WindowState::Minimized,
            maximized: state == WindowState::Maximized,
            fullscreen: state == WindowState::Fullscreen,
        }
    }
}

/// Everything the shell shows about one window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: WindowId,
    pub title: String,
    pub app_id: String,
    pub flags: WindowFlags,
    pub workspace: usize,
}

/// Workspace count and the one in view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub count: usize,
    pub active: usize,
}

/// The output and its usable area in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputInfo {
    pub area: Rect,
    pub work_area: Rect,
    pub scale_120ths: u32,
}

/// The complete state the shell can observe at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSnapshot {
    /// Sorted by id, which is creation order.
    pub windows: Vec<WindowInfo>,
    pub workspaces: WorkspaceInfo,
    /// `None` while the compositor has no output.
    pub output: Option<OutputInfo>,
}

impl ShellSnapshot {
    /// Builds a snapshot; sorts the windows so that equal states compare equal.
    pub fn new(
        mut windows: Vec<WindowInfo>,
        workspaces: WorkspaceInfo,
        output: Option<OutputInfo>,
    ) -> Self {
        windows.sort_by_key(|window| window.id);
        Self {
            windows,
            workspaces,
            output,
        }
    }
}

/// Cuts `text` to at most [`MAX_TEXT_BYTES`] without splitting a character.
pub fn clamp_text(text: &str) -> String {
    if text.len() <= MAX_TEXT_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_TEXT_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default().to_owned()
}

/// Converts a fractional scale into 120ths, rounded and kept at least 1.
pub fn scale_to_120ths(scale: f64) -> u32 {
    let scaled = (scale * SCALE_UNIT_120).round();
    if !scaled.is_finite() || scaled < 1.0 {
        return 1;
    }
    if scaled >= f64::from(u32::MAX) {
        return u32::MAX;
    }
    // Truncation is exact here: the value is a rounded integer inside the u32 range.
    scaled as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimized_window_is_never_activated() {
        let flags = WindowFlags::new(WindowState::Minimized, true);
        assert!(flags.minimized);
        assert!(!flags.activated);
    }

    #[test]
    fn focused_maximized_window_reports_both() {
        let flags = WindowFlags::new(WindowState::Maximized, true);
        assert!(flags.activated && flags.maximized);
        assert!(!flags.fullscreen && !flags.minimized);
    }

    #[test]
    fn unfocused_normal_window_has_no_flags() {
        assert_eq!(
            WindowFlags::new(WindowState::Normal, false),
            WindowFlags::default()
        );
    }

    #[test]
    fn short_text_is_kept() {
        assert_eq!(clamp_text("Terminal"), "Terminal");
        assert_eq!(clamp_text(""), "");
    }

    #[test]
    fn long_text_is_cut_on_a_char_boundary() {
        let text = "ä".repeat(MAX_TEXT_BYTES);
        let cut = clamp_text(&text);
        assert!(cut.len() <= MAX_TEXT_BYTES);
        assert!(cut.chars().all(|ch| ch == 'ä'));
        assert_eq!(cut.len(), MAX_TEXT_BYTES);
    }

    #[test]
    fn long_text_with_odd_boundary_loses_one_byte() {
        let text = format!("a{}", "ä".repeat(MAX_TEXT_BYTES));
        assert_eq!(clamp_text(&text).len(), MAX_TEXT_BYTES - 1);
    }

    #[test]
    fn scale_converts_to_120ths() {
        assert_eq!(scale_to_120ths(1.0), 120);
        assert_eq!(scale_to_120ths(1.5), 180);
        assert_eq!(scale_to_120ths(2.0), 240);
    }

    #[test]
    fn scale_conversion_is_total() {
        assert_eq!(scale_to_120ths(f64::NAN), 1);
        assert_eq!(scale_to_120ths(-1.0), 1);
        assert_eq!(scale_to_120ths(f64::INFINITY), 1);
        assert_eq!(scale_to_120ths(1.0e20), u32::MAX);
    }
}
