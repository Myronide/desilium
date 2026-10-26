//! Offscreen checks of the glass bookkeeping on a real GL context; skipped without a render node.

use super::{Glass, GlassKey};
use crate::backend::headless::HeadlessGpu;
use crate::render::effects::{BackdropBlurParams, EffectSlot, Effects, Rgba, RoundedRect};
use crate::render::elements::DesiliumElement;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::utils::{Logical, Rectangle, Transform};

const SIZE: (i32, i32) = (200, 120);
const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
    Rectangle::new((x, y).into(), (w, h).into())
}

/// A frame like the scene builds it: panel on top, glass requested right below it, backdrop below.
fn build(
    effects: &Effects,
    glass: &Glass,
    slots: &mut [EffectSlot<RoundedRect>; 2],
    backdrop_x: i32,
) -> Vec<DesiliumElement> {
    glass.begin_frame();
    let mut out = Vec::new();
    let [panel_slot, backdrop_slot] = slots;
    let panel = RoundedRect::new(rect(40, 30, 100, 50), Rgba::from_rgba8(20, 20, 30, 120));
    out.extend(
        effects
            .rounded_rect_in(panel_slot, &panel)
            .map(DesiliumElement::from),
    );
    let params = BackdropBlurParams::new(rect(40, 30, 100, 50), 12.0);
    glass.request(GlassKey::Debug(7), out.len(), params);
    let backdrop = RoundedRect::new(rect(backdrop_x, 0, 100, 120), Rgba::WHITE);
    out.extend(
        effects
            .rounded_rect_in(backdrop_slot, &backdrop)
            .map(DesiliumElement::from),
    );
    glass.resolve(Some(effects), 1.0, &mut out);
    out
}

fn is_glass(element: &DesiliumElement) -> bool {
    matches!(element, DesiliumElement::Glass(_))
}

#[test]
fn glass_sits_between_panel_and_backdrop_and_idles_without_damage() {
    let Ok(mut gpu) = HeadlessGpu::new() else {
        eprintln!("skipping GPU test: no render node");
        return;
    };
    let effects = Effects::compile(gpu.renderer()).expect("effects compile");
    let glass = Glass::default();
    let mut slots = [EffectSlot::default(), EffectSlot::default()];
    let mut tracker = OutputDamageTracker::new(SIZE, 1.0, Transform::Normal);
    let mut idle_damage = None;
    let mut moved_damage = None;
    gpu.render_with(SIZE.into(), |renderer, target| {
        let first = build(&effects, &glass, &mut slots, 20);
        assert_eq!(first.iter().position(is_glass), Some(1));
        let drawn = tracker.render_output(renderer, target, 0, &first, CLEAR);
        assert!(drawn.is_ok());
        let second = build(&effects, &glass, &mut slots, 20);
        let result = tracker.render_output(renderer, target, 1, &second, CLEAR);
        idle_damage = result.ok().map(|result| result.damage.map(|d| d.len()));
        let third = build(&effects, &glass, &mut slots, 60);
        let result = tracker.render_output(renderer, target, 1, &third, CLEAR);
        moved_damage = result.ok().map(|result| result.damage.map(|d| d.len()));
        Ok(())
    })
    .expect("render");
    let idle = idle_damage.expect("second frame rendered");
    assert!(
        idle.is_none_or(|count| count == 0),
        "idle frame damaged: {idle:?}"
    );
    let moved = moved_damage.expect("third frame rendered");
    assert!(
        moved.is_some_and(|count| count > 0),
        "moving the backdrop must repaint"
    );
}

#[test]
fn slots_of_vanished_surfaces_are_released() {
    let Ok(mut gpu) = HeadlessGpu::new() else {
        eprintln!("skipping GPU test: no render node");
        return;
    };
    let effects = Effects::compile(gpu.renderer()).expect("effects compile");
    let glass = Glass::default();
    let mut slots = [EffectSlot::default(), EffectSlot::default()];
    let with_glass = build(&effects, &glass, &mut slots, 0);
    assert_eq!(with_glass.iter().filter(|e| is_glass(e)).count(), 1);
    assert_eq!(glass.slot_count(), 1);
    glass.begin_frame();
    let mut empty = Vec::new();
    glass.resolve(Some(&effects), 1.0, &mut empty);
    assert!(empty.is_empty());
    assert_eq!(glass.slot_count(), 0);
}
