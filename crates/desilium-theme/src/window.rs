//! Window chrome and shadow tokens.

use serde::{Deserialize, Serialize};

use crate::check::MIN_TEXT_CONTRAST;
use crate::{Color, Mode, Palette};

/// Brand: window corner radius in logical px.
pub const WINDOW_CORNER_RADIUS: f32 = 12.0;
/// Brand: control (button, input) corner radius in logical px.
pub const CONTROL_RADIUS: f32 = 8.0;
/// Brand: title bar height in logical px.
pub const TITLE_BAR_HEIGHT: f32 = 36.0;

const BORDER_ACCENT_SHARE: f32 = 0.35;
const SHADOW_TINT: u32 = 0x0A0C1F;
const DEFAULT_BORDER_WIDTH: f32 = 1.0;
const DEFAULT_TITLE_FONT_SIZE: f32 = 13.0;
/// Medium titles, as in the approved mockups (13 px / 500).
const DEFAULT_TITLE_FONT_WEIGHT: u16 = 500;
const DEFAULT_BUTTON_WIDTH: f32 = 32.0;
const DEFAULT_BUTTON_HEIGHT: f32 = 28.0;
const DEFAULT_BUTTON_SPACING: f32 = 2.0;
const DEFAULT_BUTTON_SIDE_INSET: f32 = 4.0;
const DEFAULT_GLYPH_SIZE: f32 = 16.0;
const DEFAULT_GLYPH_STROKE: f32 = 1.5;
/// Hover and pressed tints are alpha over the palette's primary text colour.
const DARK_HOVER_ALPHA: f32 = 0.06;
const DARK_PRESSED_ALPHA: f32 = 0.10;
const LIGHT_HOVER_ALPHA: f32 = 0.05;
const LIGHT_PRESSED_ALPHA: f32 = 0.09;
/// Share of black mixed into the danger fill while the close button is pressed.
const CLOSE_PRESSED_DARKEN: f32 = 0.08;

/// Which edge of the title bar the window button group sits at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ButtonAlignment {
    /// Buttons sit at the right edge (the approved layout).
    #[default]
    Right,
    /// Buttons sit at the left edge.
    Left,
}

/// Minimize, maximize/restore and close buttons in the title bar.
///
/// Each button is a `width` x `height` hit area with a `corner_radius` fill.
/// Normal buttons are neutral; only close turns danger on hover or press.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowButtons {
    /// Hit area width in px.
    pub width: f32,
    /// Hit area height in px; must fit inside the title bar with a margin.
    pub height: f32,
    /// Corner radius of each button's hit area and fill in px.
    pub corner_radius: f32,
    /// Gap between neighbouring buttons in px.
    pub spacing: f32,
    /// Distance from the title bar edge the group hugs, in px.
    pub side_inset: f32,
    /// Edge of the title bar the group sits at.
    pub alignment: ButtonAlignment,
    /// Fill of normal buttons at rest (transparent by default).
    pub rest_fill: Color,
    /// Fill of normal buttons while hovered.
    pub hover_fill: Color,
    /// Fill of normal buttons while pressed.
    pub pressed_fill: Color,
    /// Fill of the close button while hovered (danger).
    pub close_hover_fill: Color,
    /// Fill of the close button while pressed.
    pub close_pressed_fill: Color,
    /// Glyph colour on the focused window's title bar.
    pub glyph_active: Color,
    /// Glyph colour on an unfocused window's title bar.
    pub glyph_inactive: Color,
    /// Glyph colour on the close button's danger fill.
    pub glyph_on_danger: Color,
    /// Edge length of the square glyph box in px; centred in the hit area.
    pub glyph_size: f32,
    /// Glyph stroke width in px.
    pub glyph_stroke: f32,
}

/// Window decoration tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Window {
    /// Window corner radius in px.
    pub corner_radius: f32,
    /// Radius of controls such as buttons and inputs in px.
    pub control_radius: f32,
    /// Outline width in px (0 disables the outline).
    pub border_width: f32,
    /// Outline colour of the focused window.
    pub border_active: Color,
    /// Outline colour of unfocused windows.
    pub border_inactive: Color,
    /// Title bar height in px.
    pub title_bar_height: f32,
    /// Title bar fill, focused (opaque; see `effects` for translucency).
    pub title_bar_active: Color,
    /// Title bar fill, unfocused.
    pub title_bar_inactive: Color,
    /// Title text colour, focused.
    pub title_text_active: Color,
    /// Title text colour, unfocused.
    pub title_text_inactive: Color,
    /// Title font size in px.
    pub title_font_size: f32,
    /// Title font weight, CSS numeric (100..=900). The text renderer snaps it
    /// to 400, 500 or 700.
    pub title_font_weight: u16,
    /// Window buttons.
    pub buttons: WindowButtons,
}

impl Window {
    /// Builds chrome tokens that harmonise with `palette`.
    pub fn for_palette(palette: &Palette, mode: Mode) -> Window {
        Window {
            corner_radius: WINDOW_CORNER_RADIUS,
            control_radius: CONTROL_RADIUS,
            border_width: DEFAULT_BORDER_WIDTH,
            border_active: palette
                .border
                .mix(palette.accent, BORDER_ACCENT_SHARE)
                .quantized(),
            border_inactive: palette.divider,
            title_bar_height: TITLE_BAR_HEIGHT,
            title_bar_active: palette.surface_raised,
            title_bar_inactive: palette.surface,
            title_text_active: palette.text_primary,
            title_text_inactive: palette.text_secondary,
            title_font_size: DEFAULT_TITLE_FONT_SIZE,
            title_font_weight: DEFAULT_TITLE_FONT_WEIGHT,
            buttons: WindowButtons::for_palette(palette, mode),
        }
    }
}

impl WindowButtons {
    /// Neutral buttons with a danger close, all tints derived from `palette`.
    pub fn for_palette(palette: &Palette, mode: Mode) -> WindowButtons {
        let (hover_alpha, pressed_alpha) = tint_alphas(mode);
        let tint = palette.text_primary;
        let danger = palette.danger;
        WindowButtons {
            width: DEFAULT_BUTTON_WIDTH,
            height: DEFAULT_BUTTON_HEIGHT,
            corner_radius: CONTROL_RADIUS,
            spacing: DEFAULT_BUTTON_SPACING,
            side_inset: DEFAULT_BUTTON_SIDE_INSET,
            alignment: ButtonAlignment::Right,
            rest_fill: Color::TRANSPARENT,
            hover_fill: tint.with_alpha(hover_alpha).quantized(),
            pressed_fill: tint.with_alpha(pressed_alpha).quantized(),
            close_hover_fill: danger,
            close_pressed_fill: danger.darken(CLOSE_PRESSED_DARKEN).quantized(),
            glyph_active: palette.text_secondary,
            glyph_inactive: palette.text_disabled,
            glyph_on_danger: ink_on(danger, palette),
            glyph_size: DEFAULT_GLYPH_SIZE,
            glyph_stroke: DEFAULT_GLYPH_STROKE,
        }
    }
}

/// Hover and pressed tint alphas for `mode`.
fn tint_alphas(mode: Mode) -> (f32, f32) {
    match mode {
        Mode::Dark => (DARK_HOVER_ALPHA, DARK_PRESSED_ALPHA),
        Mode::Light => (LIGHT_HOVER_ALPHA, LIGHT_PRESSED_ALPHA),
    }
}

/// The palette's on-accent ink if it reads on `fill`, else white, else black.
fn ink_on(fill: Color, palette: &Palette) -> Color {
    [palette.text_on_accent, Color::WHITE, Color::BLACK]
        .into_iter()
        .find(|ink| ink.contrast_ratio(fill) >= MIN_TEXT_CONTRAST)
        .unwrap_or(Color::BLACK)
}

impl Default for Window {
    fn default() -> Self {
        Window::for_palette(&Palette::dark(), Mode::Dark)
    }
}

impl Default for WindowButtons {
    fn default() -> Self {
        WindowButtons::for_palette(&Palette::dark(), Mode::Dark)
    }
}

/// One drop shadow; feeds the GPU shadow shader directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShadowStyle {
    /// Gaussian blur radius in px.
    pub blur: f32,
    /// Growth (positive) or shrink (negative) of the caster rect in px.
    pub spread: f32,
    /// Horizontal offset in px.
    pub offset_x: f32,
    /// Vertical offset in px (positive is down).
    pub offset_y: f32,
    /// Shadow colour including its opacity.
    pub color: Color,
}

impl ShadowStyle {
    /// No shadow at all (elevation level 0).
    pub const NONE: ShadowStyle = ShadowStyle {
        blur: 0.0,
        spread: 0.0,
        offset_x: 0.0,
        offset_y: 0.0,
        color: Color::TRANSPARENT,
    };

    fn tinted(blur: f32, spread: f32, offset_y: f32, alpha: f32) -> ShadowStyle {
        ShadowStyle {
            blur,
            spread,
            offset_x: 0.0,
            offset_y,
            color: Color::rgb(SHADOW_TINT).with_alpha(alpha).quantized(),
        }
    }

    /// Distance the shadow reaches beyond the caster, for damage tracking.
    pub fn extent(&self) -> f32 {
        let reach = self.blur + self.spread.max(0.0);
        reach + self.offset_x.abs().max(self.offset_y.abs())
    }
}

impl Default for ShadowStyle {
    fn default() -> Self {
        ShadowStyle::tinted(24.0, 0.0, 6.0, 0.35)
    }
}

/// Elevation shadows (levels 1-4; level 0 is no shadow).
///
/// These are the crate's named shadows. The mockups' five elevation levels map
/// onto them as documented in `docs/design.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shadow {
    /// Level 1: unfocused windows.
    pub unfocused: ShadowStyle,
    /// Level 2: focused window.
    pub focused: ShadowStyle,
    /// Level 3: menus, popovers, tooltips.
    pub popup: ShadowStyle,
    /// Level 4: modal dialogs and system overlays.
    pub modal: ShadowStyle,
}

impl Shadow {
    /// Highest elevation level.
    pub const MAX_LEVEL: u8 = 4;

    /// Shadows for `mode`: dark surfaces need stronger shadows to read.
    pub fn for_mode(mode: Mode) -> Shadow {
        let strength = match mode {
            Mode::Dark => 1.0,
            Mode::Light => 0.34,
        };
        Shadow {
            unfocused: ShadowStyle::tinted(24.0, 0.0, 6.0, 0.35 * strength),
            focused: ShadowStyle::tinted(48.0, -4.0, 16.0, 0.55 * strength),
            popup: ShadowStyle::tinted(56.0, -4.0, 20.0, 0.60 * strength),
            modal: ShadowStyle::tinted(96.0, -10.0, 32.0, 0.65 * strength),
        }
    }

    /// Style for an elevation level; levels above 4 are treated as 4.
    pub fn level(&self, level: u8) -> ShadowStyle {
        match level {
            0 => ShadowStyle::NONE,
            1 => self.unfocused.clone(),
            2 => self.focused.clone(),
            3 => self.popup.clone(),
            _ => self.modal.clone(),
        }
    }
}

impl Default for Shadow {
    fn default() -> Self {
        Shadow::for_mode(Mode::Dark)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevation_grows_monotonically() {
        for mode in [Mode::Dark, Mode::Light] {
            let shadow = Shadow::for_mode(mode);
            let blurs: Vec<f32> = (0..=Shadow::MAX_LEVEL)
                .map(|level| shadow.level(level).blur)
                .collect();
            assert!(blurs.windows(2).all(|pair| pair[0] < pair[1]), "{blurs:?}");
        }
    }

    #[test]
    fn level_zero_is_invisible_and_overflow_clamps() {
        let shadow = Shadow::default();
        assert_eq!(shadow.level(0).color.a, 0.0);
        assert_eq!(shadow.level(200), shadow.modal);
    }

    #[test]
    fn extent_covers_blur_and_offset() {
        let style = ShadowStyle::tinted(20.0, 4.0, 10.0, 0.5);
        assert_eq!(style.extent(), 34.0);
    }

    #[test]
    fn brand_constants_are_defaults() {
        let window = Window::default();
        assert_eq!(window.corner_radius, 12.0);
        assert_eq!(window.control_radius, 8.0);
        assert_eq!(window.title_bar_height, 36.0);
    }

    #[test]
    fn title_weight_is_medium_by_default() {
        for mode in [Mode::Dark, Mode::Light] {
            let window = Window::for_palette(&Palette::for_mode(mode), mode);
            assert_eq!(window.title_font_weight, 500);
        }
    }

    #[test]
    fn default_buttons_match_the_mockups() {
        let buttons = WindowButtons::default();
        assert_eq!(buttons.width, 32.0);
        assert_eq!(buttons.height, 28.0);
        assert_eq!(buttons.corner_radius, 8.0);
        assert_eq!(buttons.alignment, ButtonAlignment::Right);
        assert_eq!(buttons.rest_fill, Color::TRANSPARENT);
        assert_eq!(buttons.close_hover_fill, Palette::dark().danger);
    }

    #[test]
    fn button_tints_step_from_hover_to_pressed() {
        for mode in [Mode::Dark, Mode::Light] {
            let buttons = WindowButtons::for_palette(&Palette::for_mode(mode), mode);
            assert!(buttons.hover_fill.a > 0.0);
            assert!(buttons.hover_fill.a < buttons.pressed_fill.a);
            assert!(buttons.close_pressed_fill != buttons.close_hover_fill);
        }
    }

    #[test]
    fn glyph_on_danger_prefers_the_better_ink() {
        let dark = Palette::dark();
        assert_eq!(ink_on(Color::rgb(0xF2616F), &dark), dark.text_on_accent);
        assert_eq!(ink_on(Color::rgb(0xC42B41), &dark), Color::WHITE);
    }
}
