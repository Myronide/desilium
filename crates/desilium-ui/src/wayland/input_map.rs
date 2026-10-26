//! Pure mappings from Wayland input values to [`UiSurface`](crate::UiSurface) input types.
//!
//! Shared by the layer host and the xdg window host so both interpret buttons,
//! scrolling, modifiers and the cursor shape identically.

use smithay_client_toolkit::{
    reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape,
    seat::{keyboard::Modifiers as SctkModifiers, pointer::AxisScroll},
};

use crate::{CursorHint, Modifiers, PointerButton, WheelDelta};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

/// Linux evdev button code to [`PointerButton`]; other buttons are ignored.
pub(super) fn to_button(code: u32) -> Option<PointerButton> {
    match code {
        BTN_LEFT => Some(PointerButton::Primary),
        BTN_RIGHT => Some(PointerButton::Secondary),
        BTN_MIDDLE => Some(PointerButton::Middle),
        _ => None,
    }
}

/// sctk modifier state to ours.
pub(super) fn to_modifiers(modifiers: &SctkModifiers) -> Modifiers {
    Modifiers {
        shift: modifiers.shift,
        ctrl: modifiers.ctrl,
        alt: modifiers.alt,
        logo: modifiers.logo,
    }
}

/// Wheel notches (discrete) win over smooth pixel scrolling when both are present.
pub(super) fn wheel_delta(horizontal: &AxisScroll, vertical: &AxisScroll) -> WheelDelta {
    if horizontal.discrete != 0 || vertical.discrete != 0 {
        return WheelDelta::Lines {
            x: f64::from(horizontal.discrete),
            y: f64::from(vertical.discrete),
        };
    }
    WheelDelta::Pixels {
        x: horizontal.absolute,
        y: vertical.absolute,
    }
}

/// Wayland axis values to what `UiSurface::wheel` needs to scroll the content the same way.
///
/// Wayland reports a positive value when the user scrolls down (reveals lower content), but the
/// document engine follows the winit convention where positive moves the content down. Without
/// the flip the page scrolls against the user's hand. The layer host currently passes values
/// through unflipped (see the report of the settings app).
pub(super) fn content_scroll(delta: WheelDelta) -> WheelDelta {
    match delta {
        WheelDelta::Lines { x, y } => WheelDelta::Lines { x: -x, y: -y },
        WheelDelta::Pixels { x, y } => WheelDelta::Pixels { x: -x, y: -y },
    }
}

/// `wp_cursor_shape_v1` shape for a CSS cursor hint.
pub(super) fn cursor_shape(hint: CursorHint) -> Shape {
    match hint {
        CursorHint::Default => Shape::Default,
        CursorHint::Pointer => Shape::Pointer,
        CursorHint::Text => Shape::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scroll(discrete: i32, absolute: f64) -> AxisScroll {
        AxisScroll {
            absolute,
            discrete,
            stop: false,
        }
    }

    #[test]
    fn evdev_buttons_map_and_unknown_ones_are_ignored() {
        assert_eq!(to_button(BTN_LEFT), Some(PointerButton::Primary));
        assert_eq!(to_button(BTN_RIGHT), Some(PointerButton::Secondary));
        assert_eq!(to_button(BTN_MIDDLE), Some(PointerButton::Middle));
        assert_eq!(to_button(0x113), None);
    }

    #[test]
    fn discrete_scroll_wins_over_pixels() {
        let delta = wheel_delta(&scroll(0, 0.0), &scroll(-1, 15.0));
        assert_eq!(delta, WheelDelta::Lines { x: 0.0, y: -1.0 });
    }

    #[test]
    fn content_scroll_flips_both_axes() {
        assert_eq!(
            content_scroll(WheelDelta::Lines { x: 1.0, y: 2.0 }),
            WheelDelta::Lines { x: -1.0, y: -2.0 }
        );
        assert_eq!(
            content_scroll(WheelDelta::Pixels { x: -3.0, y: 4.0 }),
            WheelDelta::Pixels { x: 3.0, y: -4.0 }
        );
    }

    #[test]
    fn smooth_scroll_uses_pixels() {
        let delta = wheel_delta(&scroll(0, 2.5), &scroll(0, 7.5));
        assert_eq!(delta, WheelDelta::Pixels { x: 2.5, y: 7.5 });
    }

    #[test]
    fn cursor_hints_map_to_shapes() {
        assert_eq!(cursor_shape(CursorHint::Text), Shape::Text);
        assert_eq!(cursor_shape(CursorHint::Pointer), Shape::Pointer);
    }
}
