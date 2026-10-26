//! Colour palette, light/dark mode and accent-driven palette generation.

use serde::{Deserialize, Serialize};

use crate::{BRAND_ACCENT, Color};

/// Hue the neutral surfaces lean toward; keeps every theme "slightly cool".
const COOL_HUE: f32 = 232.0;
const COOL_HUE_PULL: f32 = 0.4;
const HALF_TURN: f32 = 180.0;
const FULL_TURN: f32 = 360.0;
const INK: u32 = 0x0C0A1E;

/// Colour scheme of a theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// Deep surfaces, light text (default).
    #[default]
    Dark,
    /// Bright surfaces, dark text.
    Light,
}

/// All semantic colours of a theme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Palette {
    /// Desktop base behind everything.
    pub background: Color,
    /// Window content and panels.
    pub surface: Color,
    /// Cards, title bars, inputs.
    pub surface_raised: Color,
    /// Menus, popovers, dialogs.
    pub overlay: Color,
    /// Main text.
    pub text_primary: Color,
    /// Supporting text.
    pub text_secondary: Color,
    /// Disabled text and placeholders (large-text contrast, 3:1).
    pub text_disabled: Color,
    /// Text on accent-filled controls.
    pub text_on_accent: Color,
    /// Brand accent.
    pub accent: Color,
    /// Accent while hovered.
    pub accent_hover: Color,
    /// Accent while pressed (transient state, 3:1 only).
    pub accent_pressed: Color,
    /// Translucent accent wash for selected rows and focus halos.
    pub accent_subtle: Color,
    /// Positive state.
    pub success: Color,
    /// Cautionary state.
    pub warning: Color,
    /// Destructive or error state.
    pub danger: Color,
    /// Outlines of controls and windows.
    pub border: Color,
    /// Hairlines between content.
    pub divider: Color,
    /// Text selection background.
    pub selection: Color,
}

impl Palette {
    /// Hand-tuned dark palette around the brand accent.
    pub fn dark() -> Palette {
        Palette {
            background: Color::rgb(0x0E1016),
            surface: Color::rgb(0x151821),
            surface_raised: Color::rgb(0x1D212D),
            overlay: Color::rgb(0x262B3A),
            text_primary: Color::rgb(0xECEEF5),
            text_secondary: Color::rgb(0xA3A9BC),
            text_disabled: Color::rgb(0x7A8199),
            text_on_accent: Color::rgb(INK),
            accent: Color::rgb(BRAND_ACCENT),
            accent_hover: Color::rgb(0x8D77FF),
            accent_pressed: Color::rgb(0x6F54F5),
            accent_subtle: Color::rgba(0x7B61FF29),
            success: Color::rgb(0x4CC38A),
            warning: Color::rgb(0xF0B54A),
            danger: Color::rgb(0xF2616F),
            border: Color::rgb(0x2C3244),
            divider: Color::rgb(0x1F2430),
            selection: Color::rgba(0x7B61FF52),
        }
    }

    /// Hand-tuned light palette around the brand accent.
    pub fn light() -> Palette {
        Palette {
            background: Color::rgb(0xF1F2F8),
            surface: Color::rgb(0xF8F9FC),
            surface_raised: Color::rgb(0xFFFFFF),
            overlay: Color::rgb(0xFFFFFF),
            text_primary: Color::rgb(0x14161F),
            text_secondary: Color::rgb(0x545B70),
            text_disabled: Color::rgb(0x7C8398),
            text_on_accent: Color::rgb(INK),
            accent: Color::rgb(BRAND_ACCENT),
            accent_hover: Color::rgb(0x8D77FF),
            accent_pressed: Color::rgb(0x6F54F5),
            accent_subtle: Color::rgba(0x7B61FF1F),
            success: Color::rgb(0x167A4E),
            warning: Color::rgb(0x8F5A0B),
            danger: Color::rgb(0xC42B41),
            border: Color::rgb(0xD8DBE6),
            divider: Color::rgb(0xE4E6EE),
            selection: Color::rgba(0x7B61FF38),
        }
    }

    /// Palette for `mode`.
    pub fn for_mode(mode: Mode) -> Palette {
        match mode {
            Mode::Dark => Palette::dark(),
            Mode::Light => Palette::light(),
        }
    }

    /// Generates a complete palette from any accent; text is guaranteed WCAG AA.
    pub fn derive(accent: Color, mode: Mode) -> Palette {
        let accent = Color {
            a: 1.0,
            ..accent.clamped()
        };
        let (hue, _, _) = accent.to_hsl();
        let tint = cool_hue(hue);
        let mut palette = match mode {
            Mode::Dark => derived_dark(tint),
            Mode::Light => derived_light(tint),
        };
        let semantic = Palette::for_mode(mode);
        palette.success = semantic.success;
        palette.warning = semantic.warning;
        palette.danger = semantic.danger;
        palette.border = palette.surface.mix(palette.text_primary, 0.14);
        palette.divider = palette.surface.mix(palette.text_primary, 0.08);
        palette.apply_accent(accent, mode);
        crate::validate::repair_palette_contrast(&mut palette, &mut Vec::new());
        palette
    }

    /// Sets the accent family; hover moves toward more contrast with its text.
    fn apply_accent(&mut self, accent: Color, mode: Mode) {
        let (subtle, selection) = match mode {
            Mode::Dark => (0.16, 0.32),
            Mode::Light => (0.12, 0.22),
        };
        self.accent = accent;
        self.text_on_accent = best_on_accent(accent);
        let ink_text = self.text_on_accent.relative_luminance() < 0.5;
        (self.accent_hover, self.accent_pressed) = match ink_text {
            true => (accent.lighten(0.12), accent.darken(0.10)),
            false => (accent.darken(0.08), accent.darken(0.16)),
        };
        self.accent_subtle = accent.with_alpha(subtle);
        self.selection = accent.with_alpha(selection);
    }
}

impl Default for Palette {
    fn default() -> Self {
        Palette::dark()
    }
}

/// Best readable text colour on `accent`: white, brand ink or pure black.
pub(crate) fn best_on_accent(accent: Color) -> Color {
    [Color::WHITE, Color::rgb(INK), Color::BLACK]
        .into_iter()
        .max_by(|a, b| {
            let (left, right) = (a.contrast_ratio(accent), b.contrast_ratio(accent));
            left.total_cmp(&right)
        })
        .unwrap_or(Color::BLACK)
}

fn cool_hue(hue: f32) -> f32 {
    let delta = (COOL_HUE - hue + HALF_TURN + FULL_TURN).rem_euclid(FULL_TURN) - HALF_TURN;
    hue + delta * COOL_HUE_PULL
}

fn derived_dark(tint: f32) -> Palette {
    let neutral = |sat, light| Color::from_hsl(tint, sat, light, 1.0);
    Palette {
        background: neutral(0.20, 0.065),
        surface: neutral(0.19, 0.095),
        surface_raised: neutral(0.18, 0.135),
        overlay: neutral(0.17, 0.175),
        text_primary: neutral(0.30, 0.94),
        text_secondary: neutral(0.14, 0.72),
        text_disabled: neutral(0.10, 0.52),
        ..Palette::dark()
    }
}

fn derived_light(tint: f32) -> Palette {
    let neutral = |sat, light| Color::from_hsl(tint, sat, light, 1.0);
    Palette {
        background: neutral(0.28, 0.955),
        surface: neutral(0.30, 0.985),
        surface_raised: Color::WHITE,
        overlay: Color::WHITE,
        text_primary: neutral(0.30, 0.09),
        text_secondary: neutral(0.14, 0.33),
        text_disabled: neutral(0.10, 0.48),
        ..Palette::light()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cool_hue_pulls_along_the_short_arc() {
        assert!((cool_hue(232.0) - 232.0).abs() < 1e-3);
        let warm = cool_hue(10.0);
        assert!(
            warm < 10.0 || warm > 300.0,
            "red should lean toward blue via magenta: {warm}"
        );
        assert!((cool_hue(252.0) - 244.0).abs() < 0.1);
    }

    #[test]
    fn on_accent_picks_the_more_readable_candidate() {
        assert_eq!(best_on_accent(Color::rgb(0x1A1A66)), Color::WHITE);
        assert_ne!(best_on_accent(Color::rgb(0xF5E663)), Color::WHITE);
    }

    #[test]
    fn derive_keeps_the_given_accent() {
        let accent = Color::rgb(0x2E9CFF);
        assert_eq!(Palette::derive(accent, Mode::Dark).accent, accent);
        assert_eq!(Palette::derive(accent, Mode::Light).accent, accent);
    }

    #[test]
    fn derive_strips_accent_alpha() {
        let derived = Palette::derive(Color::rgb(0x2E9CFF).with_alpha(0.3), Mode::Dark);
        assert_eq!(derived.accent.a, 1.0);
    }
}
