//! Mouse settings (the `[compositor.mouse]` table): applied to KWin's input devices.

use serde::{Deserialize, Serialize};

/// Pointer speed range of the settings; `-100` and `100` are KWin's `-1.0` and `1.0`.
pub const POINTER_SPEED_RANGE: (i16, i16) = (-100, 100);
/// Scroll speed range in percent of the normal speed.
pub const SCROLL_SPEED_RANGE: (u16, u16) = (25, 400);
/// Double-click interval range in milliseconds.
pub const DOUBLE_CLICK_RANGE: (u16, u16) = (200, 1000);

/// How the pointer speeds up with the speed of the hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccelProfile {
    /// The pointer follows the hand at a constant ratio.
    Flat,
    /// The pointer moves further the faster the hand moves.
    #[default]
    Adaptive,
}

impl AccelProfile {
    /// Value of KWin's `pointerAccelerationProfileFlat` property.
    pub fn is_flat(self) -> bool {
        self == Self::Flat
    }
}

/// Settings of every mouse (pointing devices that are not touchpads).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseSettings {
    /// Pointer speed, `-100..=100`; `0` is the device's normal speed.
    pub pointer_speed: i16,
    pub accel_profile: AccelProfile,
    /// Scrolling moves the content the way the fingers move (as on a phone).
    pub natural_scroll: bool,
    /// Swaps the primary and the secondary button.
    pub left_handed: bool,
    /// Scroll speed in percent of the normal speed, `25..=400`.
    pub scroll_speed: u16,
    /// Longest time between the two clicks of a double click in milliseconds, `200..=1000`.
    pub double_click_ms: u16,
}

impl Default for MouseSettings {
    fn default() -> Self {
        Self {
            pointer_speed: 0,
            accel_profile: AccelProfile::Adaptive,
            natural_scroll: false,
            left_handed: false,
            scroll_speed: 100,
            double_click_ms: 400,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_neutral_settings() {
        let mouse = MouseSettings::default();
        assert_eq!(mouse.pointer_speed, 0);
        assert!(!mouse.accel_profile.is_flat());
        assert!(!mouse.natural_scroll && !mouse.left_handed);
        assert_eq!((mouse.scroll_speed, mouse.double_click_ms), (100, 400));
    }

    #[test]
    fn a_partial_table_keeps_the_other_defaults() {
        let parsed: MouseSettings =
            toml::from_str("accel_profile = \"flat\"\nleft_handed = true").expect("parse");
        assert!(parsed.accel_profile.is_flat() && parsed.left_handed);
        assert_eq!(parsed.scroll_speed, 100);
    }
}
