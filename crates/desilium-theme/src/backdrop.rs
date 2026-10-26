//! Wallpaper gradient and visual effect tokens.

use serde::{Deserialize, Serialize};

use crate::{Color, Mode};

const DARK_STOPS: [(f32, f32, f32, f32); 3] = [
    (0.0, 0.50, 0.06, 0.0),
    (0.55, 0.48, 0.12, 0.0),
    (1.0, 0.52, 0.20, 18.0),
];
const LIGHT_STOPS: [(f32, f32, f32, f32); 3] = [
    (0.0, 0.70, 0.97, 0.0),
    (0.55, 0.75, 0.93, 0.0),
    (1.0, 0.80, 0.88, -18.0),
];
const DEFAULT_ANGLE: f32 = 160.0;
const DEFAULT_NOISE: f32 = 0.025;
/// Highest sensible film-grain strength.
pub const MAX_NOISE: f32 = 0.25;

/// One colour stop of a gradient.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Position along the gradient, `0..=1`.
    pub position: f32,
    /// Colour at that position.
    pub color: Color,
}

/// Linear gradient; angle follows CSS (0 = to top, 90 = to right).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gradient {
    /// Direction in degrees.
    pub angle_deg: f32,
    /// Two or more stops sorted by position.
    pub stops: Vec<GradientStop>,
}

impl Gradient {
    /// Colour at `t` in `0..=1`; assumes sorted stops.
    pub fn color_at(&self, t: f32) -> Color {
        let Some(first) = self.stops.first() else {
            return Color::BLACK;
        };
        if t <= first.position {
            return first.color;
        }
        for pair in self.stops.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            if t <= to.position {
                let span = (to.position - from.position).max(f32::EPSILON);
                return from.color.mix(to.color, (t - from.position) / span);
            }
        }
        self.stops.last().map_or(Color::BLACK, |stop| stop.color)
    }
}

impl Default for Gradient {
    fn default() -> Self {
        Background::for_accent(Color::rgb(crate::BRAND_ACCENT), Mode::Dark).gradient
    }
}

/// Default wallpaper: a calm gradient with optional grain against banding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Background {
    /// Base gradient.
    pub gradient: Gradient,
    /// Film-grain strength, `0..=0.25`; hides 8-bit banding.
    pub noise: f32,
}

impl Background {
    /// Wallpaper whose hue follows `accent`.
    pub fn for_accent(accent: Color, mode: Mode) -> Background {
        let (hue, _, _) = accent.to_hsl();
        let table = match mode {
            Mode::Dark => &DARK_STOPS,
            Mode::Light => &LIGHT_STOPS,
        };
        let stops = table
            .iter()
            .map(|&(position, sat, light, shift)| GradientStop {
                position,
                color: Color::from_hsl(hue + shift, sat, light, 1.0).quantized(),
            })
            .collect();
        Background {
            gradient: Gradient {
                angle_deg: DEFAULT_ANGLE,
                stops,
            },
            noise: DEFAULT_NOISE,
        }
    }
}

impl Default for Background {
    fn default() -> Self {
        Background::for_accent(Color::rgb(crate::BRAND_ACCENT), Mode::Dark)
    }
}

/// Blur, saturation and translucency. Translucency: 0 is opaque, 1 is fully see-through.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Effects {
    /// Backdrop blur radius in px.
    pub blur_radius: f32,
    /// Backdrop saturation multiplier (1 = unchanged).
    pub backdrop_saturation: f32,
    /// Translucency of panels and docks.
    pub panel_translucency: f32,
    /// Translucency of title bars.
    pub titlebar_translucency: f32,
    /// Translucency of menus and popovers.
    pub popup_translucency: f32,
}

impl Effects {
    /// Effects tuned per mode: light surfaces tolerate less see-through.
    pub fn for_mode(mode: Mode) -> Effects {
        let panel = match mode {
            Mode::Dark => 0.28,
            Mode::Light => 0.22,
        };
        Effects {
            blur_radius: 24.0,
            backdrop_saturation: 1.35,
            panel_translucency: panel,
            titlebar_translucency: 0.12,
            popup_translucency: 0.18,
        }
    }
}

impl Default for Effects {
    fn default() -> Self {
        Effects::for_mode(Mode::Dark)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_wallpaper_is_dark_and_light_is_light() {
        let accent = Color::rgb(crate::BRAND_ACCENT);
        let dark = Background::for_accent(accent, Mode::Dark);
        let light = Background::for_accent(accent, Mode::Light);
        assert!(
            dark.gradient
                .stops
                .iter()
                .all(|s| s.color.to_hsl().2 < 0.25)
        );
        assert!(
            light
                .gradient
                .stops
                .iter()
                .all(|s| s.color.to_hsl().2 > 0.85)
        );
    }

    #[test]
    fn color_at_clamps_and_interpolates() {
        let gradient = Gradient {
            angle_deg: 0.0,
            stops: vec![
                GradientStop {
                    position: 0.0,
                    color: Color::BLACK,
                },
                GradientStop {
                    position: 1.0,
                    color: Color::WHITE,
                },
            ],
        };
        assert_eq!(gradient.color_at(-1.0), Color::BLACK);
        assert_eq!(gradient.color_at(2.0), Color::WHITE);
        assert_eq!(gradient.color_at(0.5).r, 0.5);
    }

    #[test]
    fn empty_gradient_is_black_not_a_panic() {
        let gradient = Gradient {
            angle_deg: 0.0,
            stops: vec![],
        };
        assert_eq!(gradient.color_at(0.5), Color::BLACK);
    }
}
