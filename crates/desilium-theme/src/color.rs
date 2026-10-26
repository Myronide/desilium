//! RGBA colour in sRGB space with the helpers a theme engine needs.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

const BYTE_MAX: f32 = 255.0;
const SRGB_LINEAR_CUTOFF: f32 = 0.04045;
const SRGB_LINEAR_SLOPE: f32 = 12.92;
const SRGB_OFFSET: f32 = 0.055;
const SRGB_GAMMA: f32 = 2.4;
const LUMA_R: f32 = 0.2126;
const LUMA_G: f32 = 0.7152;
const LUMA_B: f32 = 0.0722;
const CONTRAST_FLARE: f32 = 0.05;
const HEX_RGB_LEN: usize = 6;
const HEX_RGBA_LEN: usize = 8;
const HUE_SECTOR_DEGREES: f32 = 60.0;
const FULL_TURN: f32 = 360.0;

/// Why a hex colour string could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ColorParseError {
    /// The string does not start with `#`.
    #[error("colour must start with '#'")]
    MissingHash,
    /// The digit count is neither 6 nor 8.
    #[error("colour must have 6 or 8 hex digits, got {0}")]
    BadLength(usize),
    /// A character is not a hexadecimal digit.
    #[error("colour contains a non-hex digit")]
    BadDigit,
}

/// Straight-alpha colour; channels are sRGB-encoded floats in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Red channel.
    pub r: f32,
    /// Green channel.
    pub g: f32,
    /// Blue channel.
    pub b: f32,
    /// Opacity (1.0 is opaque).
    pub a: f32,
}

impl Color {
    /// Opaque black.
    pub const BLACK: Color = Color::rgb(0x000000);
    /// Opaque white.
    pub const WHITE: Color = Color::rgb(0xFFFFFF);
    /// Fully transparent black.
    pub const TRANSPARENT: Color = Color::rgba(0x00000000);

    /// Opaque colour from `0xRRGGBB`.
    pub const fn rgb(hex: u32) -> Color {
        Color::from_bytes(
            ((hex >> 16) & 0xFF) as u8,
            ((hex >> 8) & 0xFF) as u8,
            (hex & 0xFF) as u8,
            0xFF,
        )
    }

    /// Colour from `0xRRGGBBAA`.
    pub const fn rgba(hex: u32) -> Color {
        Color::from_bytes(
            ((hex >> 24) & 0xFF) as u8,
            ((hex >> 16) & 0xFF) as u8,
            ((hex >> 8) & 0xFF) as u8,
            (hex & 0xFF) as u8,
        )
    }

    /// Colour from 8-bit channels.
    pub const fn from_bytes(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color {
            r: r as f32 / BYTE_MAX,
            g: g as f32 / BYTE_MAX,
            b: b as f32 / BYTE_MAX,
            a: a as f32 / BYTE_MAX,
        }
    }

    /// Colour from float channels; values are clamped into range.
    pub fn from_rgba_f32(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r, g, b, a }.clamped()
    }

    /// Parses `#RRGGBB` or `#RRGGBBAA` (case-insensitive).
    pub fn from_hex(text: &str) -> Result<Color, ColorParseError> {
        let digits = text.strip_prefix('#').ok_or(ColorParseError::MissingHash)?;
        if digits.len() != HEX_RGB_LEN && digits.len() != HEX_RGBA_LEN {
            return Err(ColorParseError::BadLength(digits.len()));
        }
        if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ColorParseError::BadDigit);
        }
        let value = u32::from_str_radix(digits, 16).map_err(|_| ColorParseError::BadDigit)?;
        if digits.len() == HEX_RGB_LEN {
            return Ok(Color::rgb(value));
        }
        Ok(Color::rgba(value))
    }

    /// Formats as `#RRGGBB`, or `#RRGGBBAA` when not fully opaque.
    pub fn to_hex(self) -> String {
        let [r, g, b, a] = self.to_bytes();
        if a == u8::MAX {
            return format!("#{r:02X}{g:02X}{b:02X}");
        }
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }

    /// Quantises to 8-bit channels.
    pub fn to_bytes(self) -> [u8; 4] {
        let c = self.clamped();
        [c.r, c.g, c.b, c.a].map(|v| (v * BYTE_MAX).round() as u8)
    }

    /// Rounds to 8-bit precision so the colour survives a hex round trip unchanged.
    pub fn quantized(self) -> Color {
        let [r, g, b, a] = self.to_bytes();
        Color::from_bytes(r, g, b, a)
    }

    /// Returns the colour with every channel forced into `0..=1` (NaN becomes 0).
    pub fn clamped(self) -> Color {
        let fix = |v: f32| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
        Color {
            r: fix(self.r),
            g: fix(self.g),
            b: fix(self.b),
            a: fix(self.a),
        }
    }

    /// True when all channels are finite and inside `0..=1`.
    pub fn is_valid(self) -> bool {
        self == self.clamped()
    }

    /// Same colour with a different opacity.
    pub fn with_alpha(self, alpha: f32) -> Color {
        Color { a: alpha, ..self }.clamped()
    }

    /// Moves RGB toward white by `amount` (0..=1); alpha is kept.
    pub fn lighten(self, amount: f32) -> Color {
        self.toward_rgb(Color::WHITE, amount)
    }

    /// Moves RGB toward black by `amount` (0..=1); alpha is kept.
    pub fn darken(self, amount: f32) -> Color {
        self.toward_rgb(Color::BLACK, amount)
    }

    /// Linear interpolation of all four channels in sRGB space (`t` clamped).
    pub fn mix(self, other: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let lerp = |from: f32, to: f32| from + (to - from) * t;
        Color {
            r: lerp(self.r, other.r),
            g: lerp(self.g, other.g),
            b: lerp(self.b, other.b),
            a: lerp(self.a, other.a),
        }
    }

    fn toward_rgb(self, target: Color, amount: f32) -> Color {
        Color {
            a: self.a,
            ..self.mix(target, amount)
        }
    }

    /// `[r, g, b, a]` as sRGB-encoded floats.
    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Premultiplied-alpha array in sRGB encoding (for non-sRGB framebuffers).
    pub fn premultiplied(self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
    }

    /// Straight-alpha array with RGB converted to linear light.
    pub fn to_linear(self) -> [f32; 4] {
        [
            srgb_to_linear(self.r),
            srgb_to_linear(self.g),
            srgb_to_linear(self.b),
            self.a,
        ]
    }

    /// Premultiplied-alpha array in linear light (for sRGB framebuffers).
    pub fn premultiplied_linear(self) -> [f32; 4] {
        let [r, g, b, a] = self.to_linear();
        [r * a, g * a, b * a, a]
    }

    /// Composites `self` over `backdrop`, treating the backdrop as opaque.
    pub fn over(self, backdrop: Color) -> Color {
        Color {
            a: 1.0,
            ..backdrop.mix(
                Color {
                    a: backdrop.a,
                    ..self
                },
                self.a,
            )
        }
    }

    /// WCAG relative luminance (alpha ignored).
    pub fn relative_luminance(self) -> f32 {
        let [r, g, b, _] = self.to_linear();
        LUMA_R * r + LUMA_G * g + LUMA_B * b
    }

    /// WCAG contrast ratio (1..=21); `self`'s alpha is composited over `other`.
    pub fn contrast_ratio(self, other: Color) -> f32 {
        let fg = self.over(other).relative_luminance();
        let bg = other.relative_luminance();
        let (light, dark) = if fg >= bg { (fg, bg) } else { (bg, fg) };
        (light + CONTRAST_FLARE) / (dark + CONTRAST_FLARE)
    }

    /// Hue in degrees (0..360), saturation and lightness in 0..=1.
    pub fn to_hsl(self) -> (f32, f32, f32) {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        let lightness = (max + min) / 2.0;
        let delta = max - min;
        if delta <= f32::EPSILON {
            return (0.0, 0.0, lightness);
        }
        let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
        let sector = if max == self.r {
            ((self.g - self.b) / delta).rem_euclid(6.0)
        } else if max == self.g {
            (self.b - self.r) / delta + 2.0
        } else {
            (self.r - self.g) / delta + 4.0
        };
        (sector * HUE_SECTOR_DEGREES, saturation, lightness)
    }

    /// Builds a colour from HSL (hue in degrees, wraps) and an opacity.
    pub fn from_hsl(hue: f32, saturation: f32, lightness: f32, alpha: f32) -> Color {
        let s = saturation.clamp(0.0, 1.0);
        let l = lightness.clamp(0.0, 1.0);
        let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let sector = hue.rem_euclid(FULL_TURN) / HUE_SECTOR_DEGREES;
        let second = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
        let (r, g, b) = match sector as u8 {
            0 => (chroma, second, 0.0),
            1 => (second, chroma, 0.0),
            2 => (0.0, chroma, second),
            3 => (0.0, second, chroma),
            4 => (second, 0.0, chroma),
            _ => (chroma, 0.0, second),
        };
        let offset = l - chroma / 2.0;
        Color::from_rgba_f32(r + offset, g + offset, b + offset, alpha)
    }
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= SRGB_LINEAR_CUTOFF {
        return value / SRGB_LINEAR_SLOPE;
    }
    ((value + SRGB_OFFSET) / (1.0 + SRGB_OFFSET)).powf(SRGB_GAMMA)
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Default for Color {
    fn default() -> Self {
        Color::BLACK
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Color::from_hex(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_for_every_byte_value() {
        for value in 0..=u8::MAX {
            let color = Color::from_bytes(value, 255 - value, value / 2, value);
            assert_eq!(Color::from_hex(&color.to_hex()).unwrap(), color);
        }
    }

    #[test]
    fn parses_rgb_and_rgba_case_insensitively() {
        assert_eq!(Color::from_hex("#7b61ff").unwrap(), Color::rgb(0x7B61FF));
        let translucent = Color::from_hex("#00000080").unwrap();
        assert!((translucent.a - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(translucent.to_hex(), "#00000080");
        assert_eq!(Color::rgb(0x7B61FF).to_hex(), "#7B61FF");
    }

    #[test]
    fn rejects_malformed_hex() {
        assert_eq!(Color::from_hex("7B61FF"), Err(ColorParseError::MissingHash));
        assert_eq!(
            Color::from_hex("#7B61F"),
            Err(ColorParseError::BadLength(5))
        );
        assert_eq!(Color::from_hex("#7B61GG"), Err(ColorParseError::BadDigit));
        assert_eq!(Color::from_hex("#+B61FF"), Err(ColorParseError::BadDigit));
        assert_eq!(Color::from_hex("#ÄÖÜ"), Err(ColorParseError::BadDigit));
    }

    #[test]
    fn contrast_matches_wcag_reference_values() {
        let ratio = Color::BLACK.contrast_ratio(Color::WHITE);
        assert!((ratio - 21.0).abs() < 0.01);
        assert!((Color::WHITE.contrast_ratio(Color::WHITE) - 1.0).abs() < 1e-5);
        let grey = Color::rgb(0x777777).contrast_ratio(Color::WHITE);
        assert!((grey - 4.48).abs() < 0.02);
    }

    #[test]
    fn translucent_foreground_is_flattened_before_contrast() {
        let faded = Color::BLACK.with_alpha(0.0);
        assert!((faded.contrast_ratio(Color::WHITE) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn lighten_darken_keep_alpha_and_limits() {
        let base = Color::rgb(0x336699).with_alpha(0.5);
        assert_eq!(base.lighten(0.2).a, 0.5);
        assert_eq!(base.lighten(1.0).to_bytes()[..3], [255, 255, 255]);
        assert_eq!(base.darken(1.0).to_bytes()[..3], [0, 0, 0]);
        assert_eq!(base.lighten(-3.0), base);
    }

    #[test]
    fn mix_interpolates_all_channels() {
        let mid = Color::BLACK.mix(Color::WHITE, 0.5);
        assert_eq!(
            mid,
            Color {
                r: 0.5,
                g: 0.5,
                b: 0.5,
                a: 1.0
            }
        );
        assert_eq!(Color::BLACK.mix(Color::WHITE, 9.0), Color::WHITE);
    }

    #[test]
    fn premultiplied_scales_rgb_only() {
        let color = Color::WHITE.with_alpha(0.25);
        assert_eq!(color.premultiplied(), [0.25, 0.25, 0.25, 0.25]);
        assert_eq!(color.to_array(), [1.0, 1.0, 1.0, 0.25]);
        let linear = Color::rgb(0x808080).to_linear();
        assert!((linear[0] - 0.2158).abs() < 0.001);
    }

    #[test]
    fn hsl_round_trips() {
        for hex in [0x7B61FF, 0xFF0000, 0x00FF80, 0x123456, 0x808080, 0xFFFFFF] {
            let color = Color::rgb(hex);
            let (h, s, l) = color.to_hsl();
            let back = Color::from_hsl(h, s, l, 1.0);
            assert_eq!(back.to_bytes(), color.to_bytes(), "{hex:06X}");
        }
    }

    #[test]
    fn clamping_repairs_nan_and_range() {
        let broken = Color {
            r: f32::NAN,
            g: 2.0,
            b: -1.0,
            a: 0.5,
        };
        assert!(!broken.is_valid());
        assert_eq!(
            broken.clamped(),
            Color {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 0.5
            }
        );
    }

    #[test]
    fn serde_uses_hex_strings() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Holder {
            c: Color,
        }
        let text = toml::to_string(&Holder {
            c: Color::rgb(0x7B61FF),
        })
        .unwrap();
        assert_eq!(text.trim(), "c = \"#7B61FF\"");
        assert!(toml::from_str::<Holder>("c = \"nope\"").is_err());
    }
}
