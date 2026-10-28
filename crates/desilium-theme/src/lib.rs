//! Desilium visual language as typed, validated settings.
//!
//! A [`Theme`] holds every design token (colour, geometry, shadow, type,
//! motion, wallpaper, effects). Built-ins come from [`Theme::dark`] and
//! [`Theme::light`]; any accent can be expanded with
//! [`Theme::derive_from_accent`]; user files go through
//! [`Theme::from_toml_str_validated`], which can never yield an unusable theme.

mod backdrop;
mod check;
mod color;
pub mod kde;
mod motion;
mod palette;
mod scale;
mod theme;
mod validate;
mod window;

pub use backdrop::{Background, Effects, Gradient, GradientStop, MAX_NOISE};
pub use check::{MIN_DISABLED_CONTRAST, MIN_TEXT_CONTRAST, ThemeIssue};
pub use color::{Color, ColorParseError};
pub use motion::{Easing, Motion, MotionKind, Spring};
pub use palette::{Mode, Palette};
pub use scale::{FontSizes, FontWeights, GRID_UNIT, LineHeights, Spacing, Typography};
pub use theme::{Theme, ThemeError};
pub use window::{
    ButtonAlignment, CONTROL_RADIUS, Shadow, ShadowStyle, TITLE_BAR_HEIGHT, WINDOW_CORNER_RADIUS,
    Window, WindowButtons,
};

/// Brand accent `#7B61FF` as `0xRRGGBB`.
pub const BRAND_ACCENT: u32 = 0x7B61FF;

/// A theme shipped with Desilium, as its TOML source.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinTheme {
    /// Stable identifier, equal to the file stem.
    pub id: &'static str,
    /// Full TOML document; always equal to the matching Rust built-in.
    pub toml: &'static str,
}

/// All built-in themes (`desilium-dark` first: it is the default).
pub const BUILTIN_THEMES: [BuiltinTheme; 2] = [
    BuiltinTheme {
        id: "desilium-dark",
        toml: include_str!("../themes/desilium-dark.toml"),
    },
    BuiltinTheme {
        id: "desilium-light",
        toml: include_str!("../themes/desilium-light.toml"),
    },
];
