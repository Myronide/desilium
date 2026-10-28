//! Tooltip bubble: one small overlay surface that never takes pointer input.
//!
//! Surfaces report hover targets with a `data-tooltip`; this places a bubble next to
//! them. It is unmapped while nothing is hovered, so it costs nothing at rest.

use desilium_theme::Theme;
use desilium_ui::{
    Rect, SurfaceOptions, UiError, UiSurface,
    wayland::{Anchor, Layer, LayerConfig, LayerHost, SurfaceId},
};

use crate::{
    slot::Slot,
    view::{BASE_CSS, TOOLTIP_CSS, esc},
};

/// Namespace of the tooltip's layer surface.
pub(crate) const NAMESPACE: &str = "desilium-tooltip";
/// Transparent margin around the bubble for its shadow (matches `.tip-wrap` padding).
const SHADOW_PAD: i32 = 10;
/// Space between target and bubble.
const TARGET_GAP: i32 = 6;
/// Layer-shell value that places a surface regardless of other surfaces' exclusive zones.
const IGNORE_EXCLUSIVE_ZONES: i32 = -1;
const PLACEHOLDER: (u32, u32) = (200, 46);
/// Screen edge clearance.
const EDGE: i32 = 4;

/// Where the bubble goes relative to its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    /// Under the target (top bar, launcher).
    Below,
    /// Over the target (dock).
    Above,
}

/// Top-left corner of the surface so the bubble of `bubble` size sits next to `target`.
///
/// Pure so the placement rules are testable; `screen_width` clamps horizontally.
pub(crate) fn position(
    target: Rect,
    bubble: (i32, i32),
    placement: Placement,
    screen_width: Option<i32>,
) -> (i32, i32) {
    let surface_w = bubble.0 + 2 * SHADOW_PAD;
    let centre = (target.x + target.width / 2.0).round() as i32;
    let mut x = centre - surface_w / 2;
    if let Some(width) = screen_width {
        x = x.min(width - surface_w - EDGE);
    }
    x = x.max(EDGE - SHADOW_PAD);
    let bubble_top = match placement {
        Placement::Below => (target.y + target.height).round() as i32 + TARGET_GAP,
        Placement::Above => target.y.round() as i32 - TARGET_GAP - bubble.1,
    };
    (x, (bubble_top - SHADOW_PAD).max(0))
}

/// The tooltip surface.
pub(crate) struct Tooltip {
    slot: Slot,
    text: String,
}

impl Tooltip {
    /// Builds the document; the surface is created when a tooltip is shown.
    pub(crate) fn new(theme: &Theme) -> Result<Self, UiError> {
        let mut options = SurfaceOptions::new(PLACEHOLDER.0, PLACEHOLDER.1);
        options.theme = theme.clone();
        let html = "<div class=tip-wrap><div class=tooltip id=tip></div></div>";
        let ui = UiSurface::new(html, &[BASE_CSS, TOOLTIP_CSS], options)?;
        let mut config = LayerConfig::new(NAMESPACE, PLACEHOLDER);
        config.layer = Layer::Overlay;
        // Margins are absolute screen coordinates: ignore the bar's exclusive zone.
        config.exclusive_zone = IGNORE_EXCLUSIVE_ZONES;
        config.anchor = Anchor {
            top: true,
            left: true,
            ..Anchor::CENTER
        };
        Ok(Self {
            slot: Slot::parked(config, ui),
            text: String::new(),
        })
    }

    /// True when `id` is the visible bubble's surface.
    pub(crate) fn owns(&self, id: SurfaceId) -> bool {
        self.slot.owns(id)
    }

    /// True while on screen.
    pub(crate) fn is_visible(&self) -> bool {
        self.slot.is_shown()
    }

    /// Shows `text` next to `target` (screen coordinates).
    pub(crate) fn show(
        &mut self,
        host: &mut LayerHost,
        text: &str,
        target: Rect,
        placement: Placement,
        screen_width: Option<i32>,
    ) -> Result<(), UiError> {
        let bubble = self.prepare(host, text)?;
        let (x, y) = position(target, bubble, placement, screen_width);
        let size = (
            u32::try_from(bubble.0 + 2 * SHADOW_PAD).unwrap_or(1),
            u32::try_from(bubble.1 + 2 * SHADOW_PAD).unwrap_or(1),
        );
        if let Some(ui) = self.slot.ui_mut(host) {
            ui.resize(size.0, size.1)?;
        }
        self.slot.reconfigure(host, |config| {
            config.size = size;
            config.margins.left = x;
            config.margins.top = y;
        })?;
        let fresh = !self.slot.is_shown();
        if let (true, Some(id)) = (fresh, self.slot.show(host)?) {
            host.set_input_region(id, None);
        }
        Ok(())
    }

    /// Sets the text and measures the bubble.
    fn prepare(&mut self, host: &mut LayerHost, text: &str) -> Result<(i32, i32), UiError> {
        let Some(ui) = self.slot.ui_mut(host) else {
            return Ok((0, 0));
        };
        if self.text != text {
            self.text = text.to_owned();
            ui.replace_children("#tip", &esc(text))?;
        }
        let bubble = ui.element_rect("#tip")?;
        Ok((bubble.width.ceil() as i32, bubble.height.ceil() as i32))
    }

    /// Removes the bubble from the screen.
    pub(crate) fn hide(&mut self, host: &mut LayerHost) {
        self.slot.hide(host);
    }

    /// Applies a new theme, shown or not.
    pub(crate) fn set_theme(&mut self, host: &mut LayerHost, theme: &Theme) {
        if let Some(ui) = self.slot.ui_mut(host) {
            ui.set_theme(theme);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn below_centres_under_the_target() {
        let (x, y) = position(
            target(100.0, 4.0, 40.0, 24.0),
            (60, 26),
            Placement::Below,
            None,
        );
        assert_eq!(x, 120 - (60 + 20) / 2);
        assert_eq!(y, 28 + TARGET_GAP - SHADOW_PAD);
    }

    #[test]
    fn above_puts_the_bubble_over_the_target() {
        let (_, y) = position(
            target(500.0, 1000.0, 48.0, 48.0),
            (60, 26),
            Placement::Above,
            None,
        );
        assert_eq!(y, 1000 - TARGET_GAP - 26 - SHADOW_PAD);
    }

    #[test]
    fn clamps_to_both_screen_edges() {
        let left = position(
            target(0.0, 0.0, 10.0, 10.0),
            (100, 26),
            Placement::Below,
            Some(1920),
        );
        assert_eq!(left.0, EDGE - SHADOW_PAD);
        let right = position(
            target(1915.0, 0.0, 10.0, 10.0),
            (100, 26),
            Placement::Below,
            Some(1920),
        );
        assert_eq!(right.0, 1920 - 120 - EDGE);
    }

    #[test]
    fn never_goes_above_the_screen() {
        let (_, y) = position(
            target(0.0, 2.0, 10.0, 10.0),
            (60, 26),
            Placement::Above,
            None,
        );
        assert_eq!(y, 0);
    }
}
