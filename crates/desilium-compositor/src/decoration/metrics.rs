//! Chrome dimensions in whole logical pixels, derived from the theme.

use super::geometry::Insets;
use desilium_theme::{ButtonAlignment, Theme};
use desilium_wm::Size;

/// Visible title bar text starts this far from the window's left edge (design: 12 px inset).
pub const TITLE_INSET: i32 = 12;
/// Gap between the title text and the button group (design: 8 px gaps).
pub const TITLE_GAP: i32 = 8;
/// Resize zone that reaches into the visible frame.
pub const RESIZE_INSIDE: i32 = 2;
/// Resize zone that reaches outside the visible frame, over the shadow area.
pub const RESIZE_OUTSIDE: i32 = 4;
/// Length along an edge that counts as a corner grab.
pub const CORNER_ZONE: i32 = 14;
/// Floor for the frame width so the button group never overlaps the left edge.
const MIN_TITLE_TEXT: i32 = 48;
const MAX_METRIC_PX: f32 = 4096.0;
const BUTTON_COUNT: i32 = 3;

/// How much chrome a window currently wears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Chrome {
    /// No server-side chrome (client-side decorated or fullscreen).
    #[default]
    None,
    /// Rounded frame with border, title bar and resize zones.
    Windowed,
    /// Title bar only: no border, no rounding, no resize zones.
    Maximized,
}

/// Chrome sizes in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// Title bar height.
    pub title_height: i32,
    /// Border width of a windowed frame.
    pub border: i32,
    /// Outer corner radius of a windowed frame.
    pub corner_radius: f32,
    /// Button hit area width.
    pub button_width: i32,
    /// Button hit area height.
    pub button_height: i32,
    /// Gap between buttons.
    pub button_spacing: i32,
    /// Distance of the button group from the title bar edge.
    pub button_side_inset: i32,
    /// True when the button group sits at the left edge.
    pub buttons_left: bool,
    /// Edge length of the glyph box centred in a button.
    pub glyph_size: i32,
    /// Glyph stroke width.
    pub glyph_stroke: f32,
}

fn px(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(0.0, MAX_METRIC_PX) as i32
}

impl Metrics {
    /// Reads the chrome tokens of `theme`.
    pub fn from_theme(theme: &Theme) -> Metrics {
        let window = &theme.window;
        let buttons = &window.buttons;
        Metrics {
            title_height: px(window.title_bar_height).max(1),
            border: px(window.border_width),
            corner_radius: window.corner_radius.max(0.0),
            button_width: px(buttons.width),
            button_height: px(buttons.height),
            button_spacing: px(buttons.spacing),
            button_side_inset: px(buttons.side_inset),
            buttons_left: buttons.alignment == ButtonAlignment::Left,
            glyph_size: px(buttons.glyph_size).max(1),
            glyph_stroke: buttons.glyph_stroke.max(0.0),
        }
    }

    /// Space between a frame's outer edge and its client surface.
    pub fn insets(&self, chrome: Chrome) -> Insets {
        match chrome {
            Chrome::None => Insets::ZERO,
            Chrome::Windowed => Insets {
                left: self.border,
                top: self.title_height,
                right: self.border,
                bottom: self.border,
            },
            Chrome::Maximized => Insets {
                left: 0,
                top: self.title_height,
                right: 0,
                bottom: 0,
            },
        }
    }

    /// Width of the three-button group.
    pub fn button_group_width(&self) -> i32 {
        self.button_width * BUTTON_COUNT + self.button_spacing * (BUTTON_COUNT - 1)
    }

    /// Smallest frame that still shows the buttons and a sliver of title.
    pub fn min_frame_size(&self, chrome: Chrome) -> Size {
        if chrome == Chrome::None {
            return Size { w: 1, h: 1 };
        }
        let width = self.button_group_width() + self.button_side_inset * 2 + MIN_TITLE_TEXT;
        Size {
            w: width,
            h: self.title_height + 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_matches_the_design() {
        let metrics = Metrics::from_theme(&Theme::dark());
        assert_eq!(metrics.title_height, 36);
        assert_eq!(metrics.border, 1);
        assert_eq!(metrics.button_width, 32);
        assert_eq!(metrics.button_height, 28);
        assert_eq!(metrics.button_group_width(), 100);
        assert!(!metrics.buttons_left);
    }

    #[test]
    fn insets_per_chrome_kind() {
        let metrics = Metrics::from_theme(&Theme::dark());
        assert_eq!(metrics.insets(Chrome::None), Insets::ZERO);
        let windowed = metrics.insets(Chrome::Windowed);
        assert_eq!(
            (windowed.left, windowed.top, windowed.right, windowed.bottom),
            (1, 36, 1, 1)
        );
        let maximized = metrics.insets(Chrome::Maximized);
        assert_eq!(
            (
                maximized.left,
                maximized.top,
                maximized.right,
                maximized.bottom
            ),
            (0, 36, 0, 0)
        );
    }

    #[test]
    fn hostile_tokens_stay_finite() {
        let mut theme = Theme::dark();
        theme.window.title_bar_height = f32::NAN;
        theme.window.border_width = -4.0;
        theme.window.buttons.width = f32::INFINITY;
        let metrics = Metrics::from_theme(&theme);
        assert_eq!(metrics.title_height, 1);
        assert_eq!(metrics.border, 0);
        assert_eq!(metrics.button_width, 0);
    }

    #[test]
    fn min_frame_covers_buttons() {
        let metrics = Metrics::from_theme(&Theme::dark());
        let min = metrics.min_frame_size(Chrome::Windowed);
        assert!(min.w > metrics.button_group_width());
        assert_eq!(metrics.min_frame_size(Chrome::None), Size { w: 1, h: 1 });
    }
}
