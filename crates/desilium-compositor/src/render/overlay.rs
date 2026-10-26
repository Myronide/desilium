//! Transient overlays drawn above all windows but below the cursor: the drag-to-snap preview.
//!
//! Development override for headless screenshots: `DESILIUM_DEBUG_SNAP=left|right|top|topleft|
//! topright|bottomleft|bottomright` forces that zone's preview. It is honoured only when the
//! compositor runs with `--headless` (the only output named `headless`); it has no effect otherwise.

use super::effects::{Border, EffectSlot, RoundedRect};
use super::elements::DesiliumElement;
use super::scene::{SceneContext, rgba};
use desilium_theme::{Color, Mode, Theme};
use desilium_wm::{Rect, SnapZone, snap_preview};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Rectangle};
use std::cell::RefCell;
use std::sync::OnceLock;

/// Gap between the preview and the zone edge, so it floats instead of touching the edge.
const INSET: i32 = 8;
const BORDER_WIDTH: f32 = 2.0;
const DEBUG_ENV: &str = "DESILIUM_DEBUG_SNAP";
/// Output name the headless backend gives its offscreen output.
const HEADLESS_OUTPUT: &str = "headless";

thread_local! {
    /// Stable element identity for the preview, so the damage tracker sees a moving rect, not a new element.
    static SLOT: RefCell<EffectSlot<RoundedRect>> = RefCell::new(EffectSlot::default());
}

pub fn push(out: &mut Vec<DesiliumElement>, _renderer: &mut GlesRenderer, ctx: &SceneContext<'_>) {
    let Some(rect) = active_rect(ctx) else {
        return;
    };
    let Some(effects) = ctx.resources.effects.as_ref() else {
        return;
    };
    let params = preview_params(ctx.state.theme(), rect, ctx.scale);
    SLOT.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return;
        };
        if let Some(element) = effects.rounded_rect_in(&mut slot, &params) {
            out.push(DesiliumElement::from(element));
        }
    });
}

/// Rect of the preview to draw this frame, if any.
fn active_rect(ctx: &SceneContext<'_>) -> Option<Rect> {
    if let Some(zone) = debug_zone().filter(|_| is_headless(ctx)) {
        return Some(snap_preview(ctx.state.wm.work_area(), zone));
    }
    ctx.state.snap_preview().map(|preview| preview.rect)
}

fn is_headless(ctx: &SceneContext<'_>) -> bool {
    ctx.state
        .space
        .outputs()
        .any(|output| output.name() == HEADLESS_OUTPUT)
}

/// The development zone override, read from the environment once.
fn debug_zone() -> Option<SnapZone> {
    static ZONE: OnceLock<Option<SnapZone>> = OnceLock::new();
    *ZONE.get_or_init(|| {
        let text = std::env::var(DEBUG_ENV).ok()?;
        parse_zone(&text)
    })
}

/// Maps the `DESILIUM_DEBUG_SNAP` spelling to a zone; `top` is the maximize zone.
fn parse_zone(text: &str) -> Option<SnapZone> {
    match text.trim().to_ascii_lowercase().as_str() {
        "left" => Some(SnapZone::Left),
        "right" => Some(SnapZone::Right),
        "top" => Some(SnapZone::Maximize),
        "topleft" => Some(SnapZone::TopLeft),
        "topright" => Some(SnapZone::TopRight),
        "bottomleft" => Some(SnapZone::BottomLeft),
        "bottomright" => Some(SnapZone::BottomRight),
        _ => None,
    }
}

/// Opacities of the accent wash and outline. Light surfaces need a firmer outline and a
/// lighter wash; on dark surfaces the wash stays soft.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tint {
    fill: f32,
    border: f32,
}

fn tint(mode: Mode) -> Tint {
    match mode {
        Mode::Dark => Tint {
            fill: 0.20,
            border: 0.60,
        },
        Mode::Light => Tint {
            fill: 0.18,
            border: 0.75,
        },
    }
}

/// Inset rounded rectangle in the accent colour, matching the window corner radius.
fn preview_params(theme: &Theme, zone_rect: Rect, scale: f64) -> RoundedRect {
    let accent: Color = theme.palette.accent;
    let tint = tint(theme.mode);
    RoundedRect::new(inset(zone_rect), rgba(accent.with_alpha(tint.fill)))
        .scale(scale)
        .radius(theme.window.corner_radius)
        .border(Border::inside(
            BORDER_WIDTH,
            rgba(accent.with_alpha(tint.border)),
        ))
}

fn inset(rect: Rect) -> Rectangle<i32, Logical> {
    let double = INSET.saturating_mul(2);
    Rectangle::new(
        (
            rect.loc.x.saturating_add(INSET),
            rect.loc.y.saturating_add(INSET),
        )
            .into(),
        (
            rect.size.w.saturating_sub(double).max(0),
            rect.size.h.saturating_sub(double).max(0),
        )
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_outline_is_firmer_than_dark_outline() {
        assert!(tint(Mode::Light).border > tint(Mode::Dark).border);
        assert!(tint(Mode::Light).fill < tint(Mode::Dark).fill);
    }

    #[test]
    fn debug_spellings_map_to_zones() {
        let cases = [
            ("left", SnapZone::Left),
            ("right", SnapZone::Right),
            ("top", SnapZone::Maximize),
            ("topleft", SnapZone::TopLeft),
            ("topright", SnapZone::TopRight),
            ("bottomleft", SnapZone::BottomLeft),
            ("bottomright", SnapZone::BottomRight),
            (" Left ", SnapZone::Left),
        ];
        for (text, zone) in cases {
            assert_eq!(parse_zone(text), Some(zone), "{text}");
        }
    }

    #[test]
    fn unknown_debug_spellings_are_ignored() {
        assert_eq!(parse_zone(""), None);
        assert_eq!(parse_zone("center"), None);
    }

    #[test]
    fn inset_shrinks_each_side_and_never_goes_negative() {
        let inset_rect = inset(Rect::new(0, 0, 500, 800));
        assert_eq!(inset_rect.loc.x, INSET);
        assert_eq!(inset_rect.loc.y, INSET);
        assert_eq!(inset_rect.size.w, 500 - 2 * INSET);
        assert_eq!(inset_rect.size.h, 800 - 2 * INSET);
        let tiny = inset(Rect::new(0, 0, 4, 4));
        assert_eq!((tiny.size.w, tiny.size.h), (0, 0));
    }
}
