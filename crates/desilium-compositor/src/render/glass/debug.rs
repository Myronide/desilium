//! Development overrides that make glass visible in headless screenshots.
//!
//! Real menus and layer surfaces with transparency are hard to produce without input injection,
//! so three environment variables force the effects. They are honoured only when the compositor
//! runs with `--headless` (the only output named `headless`) and are ignored everywhere else:
//!
//! * `DESILIUM_DEBUG_POPUP=1|<0..1>` draws a synthetic popup-styled menu over the windows; a
//!   fraction replaces the theme's popup translucency.
//! * `DESILIUM_DEBUG_GLASS=on|<0..1>` forces that title bar translucency (`on` means 0.45).
//! * `DESILIUM_DEBUG_LAYER_BLUR=<radius>[,<corner radius>]` puts blur behind every layer surface.

use super::super::SceneContext;
use super::super::effects::{BackdropBlurParams, Border, Corners, EffectSlot, RoundedRect, Shadow};
use super::super::elements::DesiliumElement;
use super::super::overlay::is_headless;
use super::super::scene::rgba;
use super::GlassKey;
use crate::state::layers::SurfaceEffects;
use desilium_theme::Theme;
use smithay::utils::{Logical, Rectangle};
use std::cell::RefCell;
use std::sync::OnceLock;

const POPUP_ENV: &str = "DESILIUM_DEBUG_POPUP";
const GLASS_ENV: &str = "DESILIUM_DEBUG_GLASS";
const LAYER_ENV: &str = "DESILIUM_DEBUG_LAYER_BLUR";
/// Translucency used when the override only says "on".
const PRESET_TRANSLUCENCY: f32 = 0.45;
/// Corner radius of the layer blur when the override gives none.
const DEFAULT_LAYER_CORNER: f32 = 14.0;
const DEBUG_POPUP_KEY: u8 = 0;

/// Synthetic menu geometry as shares of the output and fixed size.
const MENU_LEFT_SHARE: f64 = 0.17;
const MENU_TOP_SHARE: f64 = 0.30;
const MENU_SIZE: (i32, i32) = (264, 176);
const MENU_PADDING: i32 = 8;
const ROW_HEIGHT: i32 = 32;
const ROW_COUNT: i32 = 5;
const HIGHLIGHTED_ROW: i32 = 1;
const LABEL_HEIGHT: i32 = 8;
const LABEL_INSET: i32 = 14;
const LABEL_WIDTHS: [i32; ROW_COUNT as usize] = [120, 168, 96, 140, 80];
const LABEL_ALPHA: f32 = 0.62;
const HIGHLIGHT_ALPHA: f32 = 0.30;

/// What the environment asks for.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DebugGlass {
    /// Draw the synthetic popup, optionally with a forced translucency.
    pub popup: Option<PopupDebug>,
    /// Forced title bar translucency.
    pub titlebar: Option<f32>,
    /// Forced layer surface blur as (radius, corner radius).
    pub layer: Option<(f32, f32)>,
}

/// How the synthetic popup is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PopupDebug {
    /// Replaces the theme's `popup_translucency` when set.
    pub translucency: Option<f32>,
}

impl DebugGlass {
    /// Parses the three override values.
    pub fn parse(popup: Option<&str>, glass: Option<&str>, layer: Option<&str>) -> DebugGlass {
        DebugGlass {
            popup: popup.and_then(parse_popup),
            titlebar: glass.and_then(parse_translucency),
            layer: layer.and_then(parse_layer),
        }
    }

    fn from_env() -> DebugGlass {
        let read = |name: &str| std::env::var(name).ok();
        DebugGlass::parse(
            read(POPUP_ENV).as_deref(),
            read(GLASS_ENV).as_deref(),
            read(LAYER_ENV).as_deref(),
        )
    }
}

fn is_truthy(text: &str) -> bool {
    matches!(
        text.trim().to_ascii_lowercase().as_str(),
        "1" | "on" | "true"
    )
}

fn parse_translucency(text: &str) -> Option<f32> {
    if is_truthy(text) {
        return Some(PRESET_TRANSLUCENCY);
    }
    let value: f32 = text.trim().parse().ok()?;
    (value.is_finite() && value > 0.0 && value < 1.0).then_some(value)
}

fn parse_popup(text: &str) -> Option<PopupDebug> {
    if is_truthy(text) {
        return Some(PopupDebug { translucency: None });
    }
    let translucency = parse_translucency(text)?;
    Some(PopupDebug {
        translucency: Some(translucency),
    })
}

fn parse_layer(text: &str) -> Option<(f32, f32)> {
    let mut parts = text.split(',').map(|part| part.trim().parse::<f32>().ok());
    let radius = parts.next()??;
    let corner = parts.next().flatten().unwrap_or(DEFAULT_LAYER_CORNER);
    let valid = radius.is_finite() && radius > 0.0 && corner.is_finite() && corner >= 0.0;
    valid.then_some((radius, corner))
}

/// The overrides in force: none unless the compositor runs headless.
fn active(ctx: &SceneContext<'_>) -> DebugGlass {
    static SETTINGS: OnceLock<DebugGlass> = OnceLock::new();
    if !is_headless(ctx) {
        return DebugGlass::default();
    }
    *SETTINGS.get_or_init(DebugGlass::from_env)
}

/// Forced title bar translucency, if any.
pub fn titlebar_translucency(ctx: &SceneContext<'_>) -> Option<f32> {
    active(ctx).titlebar
}

/// Forced blur for every layer surface, if any.
pub fn layer_effects(ctx: &SceneContext<'_>) -> Option<SurfaceEffects> {
    let (radius, corner) = active(ctx).layer?;
    Some(SurfaceEffects {
        blur_radius: radius,
        corner_radii: [corner; 4],
    })
}

thread_local! {
    /// Stable element identities of the synthetic menu: shadow, panel, highlight, labels.
    static SLOTS: RefCell<MenuSlots> = RefCell::new(MenuSlots::default());
}

#[derive(Default)]
struct MenuSlots {
    shadow: EffectSlot<Shadow>,
    panel: EffectSlot<RoundedRect>,
    highlight: EffectSlot<RoundedRect>,
    labels: [EffectSlot<RoundedRect>; ROW_COUNT as usize],
}

/// Draws the synthetic menu (topmost first) when `DESILIUM_DEBUG_POPUP` asks for it.
pub fn push_popup(out: &mut Vec<DesiliumElement>, ctx: &SceneContext<'_>) {
    let Some(debug) = active(ctx).popup else {
        return;
    };
    let Some(effects) = ctx.resources.effects.as_ref() else {
        return;
    };
    SLOTS.with(|slots| {
        let Ok(mut slots) = slots.try_borrow_mut() else {
            return;
        };
        let theme = ctx.state.theme();
        let rect = menu_rect(ctx.area);
        let radius = super::super::popups::popup_radius(theme, rect.size);
        push_rows(out, ctx, &mut slots, rect);
        let translucency = debug
            .translucency
            .unwrap_or(theme.effects.popup_translucency);
        let panel = panel_params(theme, rect, ctx.scale, radius, translucency);
        out.extend(
            effects
                .rounded_rect_in(&mut slots.panel, &panel)
                .map(DesiliumElement::from),
        );
        ctx.resources.glass.request(
            GlassKey::Debug(DEBUG_POPUP_KEY),
            out.len(),
            BackdropBlurParams::new(rect, theme.effects.blur_radius)
                .scale(ctx.scale)
                .corners(Corners::all(radius))
                .saturation(theme.effects.backdrop_saturation),
        );
        let shadow = super::super::popups::shadow_params(theme, rect, ctx.scale, radius);
        out.extend(
            effects
                .shadow_in(&mut slots.shadow, &shadow)
                .map(DesiliumElement::from),
        );
    });
}

fn menu_rect(area: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let left = (f64::from(area.size.w) * MENU_LEFT_SHARE).round() as i32;
    let top = (f64::from(area.size.h) * MENU_TOP_SHARE).round() as i32;
    Rectangle::new(
        (
            area.loc.x.saturating_add(left),
            area.loc.y.saturating_add(top),
        )
            .into(),
        MENU_SIZE.into(),
    )
}

/// The fill is the theme's popup surface with the popup translucency; the glass shows through.
fn panel_params(
    theme: &Theme,
    rect: Rectangle<i32, Logical>,
    scale: f64,
    radius: f32,
    translucency: f32,
) -> RoundedRect {
    let fill = theme.palette.overlay.with_alpha(1.0 - translucency);
    RoundedRect::new(rect, rgba(fill))
        .scale(scale)
        .radius(radius)
        .border(Border::inside(
            1.0,
            rgba(super::super::popups::border_color(theme)),
        ))
}

fn push_rows(
    out: &mut Vec<DesiliumElement>,
    ctx: &SceneContext<'_>,
    slots: &mut MenuSlots,
    rect: Rectangle<i32, Logical>,
) {
    let Some(effects) = ctx.resources.effects.as_ref() else {
        return;
    };
    let theme = ctx.state.theme();
    let row_at = |row: i32| rect.loc.y + MENU_PADDING + row * ROW_HEIGHT;
    let label_color = theme.palette.text_primary.with_alpha(LABEL_ALPHA);
    for (row, slot) in (0..).zip(slots.labels.iter_mut()) {
        let width = LABEL_WIDTHS.get(row as usize).copied().unwrap_or_default();
        let label = Rectangle::new(
            (
                rect.loc.x + MENU_PADDING + LABEL_INSET,
                row_at(row) + (ROW_HEIGHT - LABEL_HEIGHT) / 2,
            )
                .into(),
            (width, LABEL_HEIGHT).into(),
        );
        let params = RoundedRect::new(label, rgba(label_color))
            .scale(ctx.scale)
            .radius(LABEL_HEIGHT as f32 / 2.0);
        out.extend(
            effects
                .rounded_rect_in(slot, &params)
                .map(DesiliumElement::from),
        );
    }
    let row = Rectangle::new(
        (rect.loc.x + MENU_PADDING, row_at(HIGHLIGHTED_ROW)).into(),
        (rect.size.w - 2 * MENU_PADDING, ROW_HEIGHT).into(),
    );
    let accent = theme.palette.accent.with_alpha(HIGHLIGHT_ALPHA);
    let params = RoundedRect::new(row, rgba(accent))
        .scale(ctx.scale)
        .radius(theme.window.control_radius);
    out.extend(
        effects
            .rounded_rect_in(&mut slots.highlight, &params)
            .map(DesiliumElement::from),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_forced_by_default() {
        assert_eq!(DebugGlass::parse(None, None, None), DebugGlass::default());
    }

    #[test]
    fn popup_accepts_truthy_spellings_and_a_translucency() {
        for text in ["1", "on", "TRUE", " 1 "] {
            let popup = DebugGlass::parse(Some(text), None, None).popup;
            assert_eq!(popup, Some(PopupDebug { translucency: None }), "{text}");
        }
        let forced = DebugGlass::parse(Some("0.5"), None, None).popup;
        assert_eq!(forced.and_then(|popup| popup.translucency), Some(0.5));
        assert_eq!(DebugGlass::parse(Some("0"), None, None).popup, None);
        assert_eq!(DebugGlass::parse(Some(""), None, None).popup, None);
    }

    #[test]
    fn title_translucency_takes_a_preset_or_a_fraction() {
        assert_eq!(parse_translucency("on"), Some(PRESET_TRANSLUCENCY));
        assert_eq!(parse_translucency("0.3"), Some(0.3));
        assert_eq!(parse_translucency("0"), None);
        assert_eq!(parse_translucency("1.5"), None);
        assert_eq!(parse_translucency("nan"), None);
        assert_eq!(parse_translucency("glass"), None);
    }

    #[test]
    fn layer_blur_takes_radius_and_optional_corner() {
        assert_eq!(parse_layer("24"), Some((24.0, DEFAULT_LAYER_CORNER)));
        assert_eq!(parse_layer("24, 8"), Some((24.0, 8.0)));
        assert_eq!(parse_layer("0"), None);
        assert_eq!(parse_layer("-4"), None);
        assert_eq!(parse_layer("big"), None);
        assert_eq!(parse_layer(""), None);
    }

    #[test]
    fn menu_sits_inside_a_normal_output() {
        let area = Rectangle::new((0, 0).into(), (1280, 800).into());
        let rect = menu_rect(area);
        assert!(area.contains_rect(rect));
    }
}
