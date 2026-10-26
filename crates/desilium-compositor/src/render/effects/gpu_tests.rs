//! Offscreen GPU tests. Every test skips (with a note on stderr) without a render node.

use super::*;
use crate::backend::headless::{HeadlessGpu, Image};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Element;
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::{ImportMem, Renderer};
use smithay::utils::{Logical, Rectangle, Transform};
use std::path::PathBuf;

const CLEAR_TRANSPARENT: [f32; 4] = [0.0; 4];
const SCREENSHOT_DIR: &str = "../../docs/screenshots";

fn setup() -> Option<(HeadlessGpu, Effects)> {
    let mut gpu = match HeadlessGpu::new() {
        Ok(gpu) => gpu,
        Err(error) => {
            eprintln!("skipping GPU test: {error}");
            return None;
        }
    };
    match Effects::compile(gpu.renderer()) {
        Ok(effects) => Some((gpu, effects)),
        Err(error) => panic!("effects must compile: {error}"),
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
    Rectangle::new((x, y).into(), (w, h).into())
}

fn render(
    gpu: &mut HeadlessGpu,
    w: i32,
    h: i32,
    scale: f64,
    elements: &[PixelShaderElement],
    clear: [f32; 4],
) -> Image {
    let image = gpu
        .render_to_image((w, h).into(), scale, elements, clear)
        .unwrap();
    if headless_is_flipped(gpu) {
        return flip_vertically(image);
    }
    image
}

/// Probes whether `HeadlessGpu` returns rows bottom-up, so these tests stay valid
/// independently of the backend's readback orientation handling.
fn headless_is_flipped(gpu: &mut HeadlessGpu) -> bool {
    use smithay::backend::renderer::element::Kind;
    use smithay::backend::renderer::element::solid::SolidColorRenderElement;
    use smithay::backend::renderer::utils::CommitCounter;
    let probe = SolidColorRenderElement::new(
        smithay::backend::renderer::element::Id::new(),
        Rectangle::new((0, 0).into(), (4, 4).into()),
        CommitCounter::default(),
        [1.0; 4],
        Kind::Unspecified,
    );
    let image = gpu
        .render_to_image((16, 16).into(), 1.0, &[probe], [0.0, 0.0, 0.0, 0.0])
        .unwrap();
    alpha_at(&image, 1, 1) == 0
}

fn flip_vertically(mut image: Image) -> Image {
    let row = image.width as usize * 4;
    let rows: Vec<Vec<u8>> = image.rgba.chunks(row).map(<[u8]>::to_vec).collect();
    image.rgba = rows.into_iter().rev().flatten().collect();
    image
}

fn save(image: &Image, name: &str) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SCREENSHOT_DIR);
    std::fs::create_dir_all(&dir).unwrap();
    image.save_png(&dir.join(name)).unwrap();
}

fn alpha_at(image: &Image, x: u32, y: u32) -> u8 {
    image.pixel(x, y).unwrap()[3]
}

#[test]
fn all_effects_compile() {
    let Some((_gpu, effects)) = setup() else {
        return;
    };
    assert!(effects.failures().is_empty(), "{:?}", effects.failures());
    for kind in [
        EffectKind::RoundedRect,
        EffectKind::Shadow,
        EffectKind::GradientBackground,
        EffectKind::RoundedSurface,
    ] {
        assert!(effects.is_available(kind), "{kind:?}");
    }
}

#[test]
fn rounded_rect_corners_are_transparent_and_interior_opaque() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let params = RoundedRect::new(rect(10, 10, 100, 100), Rgba::WHITE).radius(20.0);
    let element = effects.rounded_rect(&params).unwrap();
    let image = render(&mut gpu, 120, 120, 1.0, &[element], CLEAR_TRANSPARENT);
    assert_eq!(alpha_at(&image, 10, 10), 0, "outer corner pixel");
    assert_eq!(alpha_at(&image, 11, 11), 0);
    assert_eq!(alpha_at(&image, 60, 60), 255);
    assert_eq!(alpha_at(&image, 60, 10), 255, "top edge midpoint");
    assert_eq!(alpha_at(&image, 10, 60), 255, "left edge midpoint");
    assert_eq!(alpha_at(&image, 9, 60), 0, "outside the rect");
}

#[test]
fn rounded_rect_is_symmetric() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let params = RoundedRect::new(rect(8, 8, 64, 64), Rgba::WHITE).radius(16.0);
    let image = render(
        &mut gpu,
        80,
        80,
        1.0,
        &[effects.rounded_rect(&params).unwrap()],
        CLEAR_TRANSPARENT,
    );
    for y in 0..80 {
        for x in 0..80 {
            let mirror_x = alpha_at(&image, 79 - x, y);
            let mirror_y = alpha_at(&image, x, 79 - y);
            assert_eq!(alpha_at(&image, x, y), mirror_x, "x mirror at {x},{y}");
            assert_eq!(alpha_at(&image, x, y), mirror_y, "y mirror at {x},{y}");
        }
    }
}

#[test]
fn edges_have_no_dark_fringe() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let params = RoundedRect::new(rect(10, 10, 100, 100), Rgba::WHITE).radius(24.0);
    let image = render(
        &mut gpu,
        120,
        120,
        1.0,
        &[effects.rounded_rect(&params).unwrap()],
        CLEAR_TRANSPARENT,
    );
    let mut partial = 0;
    for pixel in image.rgba.as_chunks::<4>().0.iter().filter(|p| p[3] > 16) {
        assert!(
            pixel[0] >= 248 && pixel[1] >= 248 && pixel[2] >= 248,
            "fringe {pixel:?}"
        );
        partial += usize::from(pixel[3] < 255);
    }
    assert!(
        partial > 20,
        "corner must be antialiased, got {partial} partial pixels"
    );
}

#[test]
fn top_only_corners_keep_bottom_square() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let params = RoundedRect::new(rect(10, 10, 100, 40), Rgba::WHITE).corners(Corners::top(14.0));
    let image = render(
        &mut gpu,
        120,
        60,
        1.0,
        &[effects.rounded_rect(&params).unwrap()],
        CLEAR_TRANSPARENT,
    );
    assert_eq!(alpha_at(&image, 10, 10), 0);
    assert_eq!(alpha_at(&image, 109, 10), 0);
    assert_eq!(alpha_at(&image, 10, 49), 255);
    assert_eq!(alpha_at(&image, 109, 49), 255);
}

#[test]
fn fractional_scale_straight_edges_stay_crisp() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    for scale in [1.25, 1.5] {
        let params = RoundedRect::new(rect(7, 5, 40, 30), Rgba::WHITE).scale(scale);
        let size = (80.0 * scale) as i32;
        let image = render(
            &mut gpu,
            size,
            size,
            scale,
            &[effects.rounded_rect(&params).unwrap()],
            CLEAR_TRANSPARENT,
        );
        let row = (15.0 * scale) as u32;
        let soft = (0..size as u32)
            .filter(|x| !matches!(alpha_at(&image, *x, row), 0 | 255))
            .count();
        assert_eq!(soft, 0, "scale {scale}: edges must land on whole pixels");
    }
}

#[test]
fn inner_and_outer_borders_have_requested_width() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let red = Rgba::from_rgba8(255, 0, 0, 255);
    let base = RoundedRect::new(rect(10, 10, 60, 60), Rgba::WHITE).radius(10.0);
    let inner = effects
        .rounded_rect(&base.clone().border(Border::inside(3.0, red)))
        .unwrap();
    let image = render(&mut gpu, 80, 80, 1.0, &[inner], CLEAR_TRANSPARENT);
    assert_eq!(image.pixel(10, 40), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(12, 40), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(13, 40), Some([255, 255, 255, 255]));
    assert_eq!(alpha_at(&image, 9, 40), 0);
    let outer = effects
        .rounded_rect(&base.border(Border::outside(3.0, red)))
        .unwrap();
    let image = render(&mut gpu, 80, 80, 1.0, &[outer], CLEAR_TRANSPARENT);
    assert_eq!(image.pixel(7, 40), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(10, 40), Some([255, 255, 255, 255]));
    assert_eq!(alpha_at(&image, 6, 40), 0);
}

#[test]
fn shadow_decays_monotonically_and_is_knocked_out_below_caster() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let caster = rect(60, 60, 80, 80);
    let shadow = Shadow::new(caster)
        .corners(Corners::all(12.0))
        .offset(0.0, 0.0)
        .blur(30.0, 0.0)
        .color(Rgba::BLACK);
    let image = render(
        &mut gpu,
        200,
        200,
        1.0,
        &[effects.shadow(&shadow).unwrap()],
        CLEAR_TRANSPARENT,
    );
    assert_eq!(alpha_at(&image, 100, 100), 0, "knockout below the caster");
    let falloff: Vec<u8> = (140..200).map(|x| alpha_at(&image, x, 100)).collect();
    assert!(
        falloff.windows(2).all(|pair| pair[0] >= pair[1]),
        "{falloff:?}"
    );
    assert!(falloff[0] > 100, "peak at the edge: {}", falloff[0]);
    assert!(
        *falloff.last().unwrap() <= 2,
        "tail must vanish: {falloff:?}"
    );
    let left: Vec<u8> = (0..60).rev().map(|x| alpha_at(&image, x, 100)).collect();
    let right: Vec<u8> = (140..200).map(|x| alpha_at(&image, x, 100)).collect();
    assert_eq!(left.len(), right.len());
    for (a, b) in left.iter().zip(&right) {
        assert!(a.abs_diff(*b) <= 2, "horizontal symmetry {a} vs {b}");
    }
}

#[test]
fn shadow_offset_moves_the_darkest_side() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let shadow = Shadow::new(rect(60, 60, 80, 80))
        .blur(20.0, 0.0)
        .offset(0.0, 16.0)
        .color(Rgba::BLACK);
    let image = render(
        &mut gpu,
        200,
        220,
        1.0,
        &[effects.shadow(&shadow).unwrap()],
        CLEAR_TRANSPARENT,
    );
    assert!(alpha_at(&image, 100, 150) > alpha_at(&image, 100, 52) + 60);
}

#[test]
fn gradient_runs_from_first_to_last_stop_without_banding_jumps() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let area = rect(0, 0, 64, 512);
    let background = GradientBackground::two_stop(area, Rgba::BLACK, Rgba::WHITE, 180.0);
    let image = render(
        &mut gpu,
        64,
        512,
        1.0,
        &[effects.gradient_background(&background).unwrap()],
        CLEAR_TRANSPARENT,
    );
    assert!(image.pixel(32, 0).unwrap()[0] <= 2);
    assert!(image.pixel(32, 511).unwrap()[0] >= 253);
    let column: Vec<u8> = (0..512).map(|y| image.pixel(32, y).unwrap()[0]).collect();
    assert!(
        column.windows(2).all(|pair| pair[1].abs_diff(pair[0]) <= 2),
        "no jumps"
    );
    let averaged = |from: usize| {
        column[from..from + 32]
            .iter()
            .map(|v| u32::from(*v))
            .sum::<u32>()
            / 32
    };
    assert!(averaged(0) < averaged(240) && averaged(240) < averaged(480));
}

#[test]
fn gradient_dither_breaks_up_flat_steps() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let from = Rgba::from_rgba8(30, 30, 40, 255);
    let to = Rgba::from_rgba8(34, 34, 44, 255);
    let background = GradientBackground::two_stop(rect(0, 0, 256, 256), from, to, 90.0);
    let image = render(
        &mut gpu,
        256,
        256,
        1.0,
        &[effects.gradient_background(&background).unwrap()],
        CLEAR_TRANSPARENT,
    );
    let mixed_columns = (0..256u32)
        .filter(|x| {
            (0..256u32)
                .map(|y| image.pixel(*x, y).unwrap()[0])
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 1
        })
        .count();
    assert!(
        mixed_columns > 100,
        "dither should vary pixels inside one gradient step, got {mixed_columns}"
    );
}

#[test]
fn slot_only_bumps_commit_counter_when_parameters_change() {
    let Some((_gpu, effects)) = setup() else {
        return;
    };
    let mut slot = EffectSlot::default();
    let params = RoundedRect::new(rect(0, 0, 50, 50), Rgba::WHITE).radius(8.0);
    let first = effects.rounded_rect_in(&mut slot, &params).unwrap();
    let again = effects.rounded_rect_in(&mut slot, &params).unwrap();
    assert_eq!(first.id(), again.id());
    assert_eq!(first.current_commit(), again.current_commit());
    let changed = effects
        .rounded_rect_in(&mut slot, &params.clone().radius(9.0))
        .unwrap();
    assert_eq!(first.id(), changed.id());
    assert_ne!(first.current_commit(), changed.current_commit());
}

#[test]
fn rounded_surface_clips_client_content() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let program = effects.rounded_surface_program().unwrap().clone();
    let pixels = vec![[0u8, 200, 0, 255]; 64 * 64].concat();
    let texture = gpu
        .renderer()
        .import_memory(&pixels, Fourcc::Abgr8888, (64, 64).into(), false)
        .unwrap();
    let context_id = gpu.renderer().context_id();
    let plain = TextureRenderElement::from_static_texture(
        smithay::backend::renderer::element::Id::new(),
        context_id,
        (8.0, 8.0),
        texture,
        1,
        Transform::Normal,
        None,
        None,
        None,
        None,
        smithay::backend::renderer::element::Kind::Unspecified,
    );
    let clip = rect(8, 8, 64, 64);
    let element = RoundedSurfaceElement::new(plain, program, clip, Corners::all(16.0), 1.0);
    let image = gpu
        .render_to_image((80, 80).into(), 1.0, &[element], CLEAR_TRANSPARENT)
        .unwrap();
    assert_eq!(alpha_at(&image, 8, 8), 0, "corner clipped");
    assert_eq!(image.pixel(40, 40), Some([0, 200, 0, 255]));
    assert_eq!(
        image.pixel(40, 8),
        Some([0, 200, 0, 255]),
        "edge midpoint kept"
    );
    let partial = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] > 0 && p[3] < 255)
        .count();
    assert!(partial > 10, "antialiased clip edge");
}

struct Palette {
    background: (Rgba, Rgba),
    title: (Rgba, Rgba),
    body: Rgba,
    border: Rgba,
    shadow: Rgba,
}

fn light() -> Palette {
    Palette {
        background: (
            Rgba::from_rgba8(138, 180, 248, 255),
            Rgba::from_rgba8(240, 196, 222, 255),
        ),
        title: (
            Rgba::from_rgba8(250, 250, 252, 255),
            Rgba::from_rgba8(232, 233, 238, 255),
        ),
        body: Rgba::from_rgba8(255, 255, 255, 255),
        border: Rgba::from_rgba8(0, 0, 0, 40),
        shadow: Rgba::from_rgba8(20, 20, 50, 255).with_opacity(0.38),
    }
}

fn dark() -> Palette {
    Palette {
        background: (
            Rgba::from_rgba8(24, 28, 52, 255),
            Rgba::from_rgba8(68, 36, 84, 255),
        ),
        title: (
            Rgba::from_rgba8(52, 54, 66, 255),
            Rgba::from_rgba8(40, 42, 52, 255),
        ),
        body: Rgba::from_rgba8(30, 31, 40, 255),
        border: Rgba::from_rgba8(255, 255, 255, 36),
        shadow: Rgba::BLACK.with_opacity(0.55),
    }
}

/// One window made of shadow, body, title bar and hairline border; topmost first.
fn window(
    effects: &Effects,
    frame: Rectangle<i32, Logical>,
    scale: f64,
    palette: &Palette,
) -> Vec<PixelShaderElement> {
    const RADIUS: f32 = 12.0;
    const TITLE_HEIGHT: i32 = 36;
    let outline = RoundedRect::new(frame, Rgba::TRANSPARENT)
        .scale(scale)
        .radius(RADIUS)
        .border(Border::inside(1.0, palette.border));
    let title_rect = rect(frame.loc.x, frame.loc.y, frame.size.w, TITLE_HEIGHT);
    let title = RoundedRect::new(title_rect, Rgba::WHITE)
        .scale(scale)
        .corners(Corners::top(RADIUS))
        .fill(Fill::vertical(palette.title.0, palette.title.1));
    let body = RoundedRect::new(frame, palette.body)
        .scale(scale)
        .radius(RADIUS);
    let shadow = Shadow::new(frame)
        .scale(scale)
        .corners(Corners::all(RADIUS))
        .blur(40.0, -4.0)
        .offset(0.0, 14.0)
        .color(palette.shadow);
    let tight = Shadow::new(frame)
        .scale(scale)
        .corners(Corners::all(RADIUS))
        .blur(8.0, 0.0)
        .offset(0.0, 2.0)
        .color(palette.shadow.with_opacity(0.6));
    [
        outline_element(effects, &outline),
        effects.rounded_rect(&title),
        effects.rounded_rect(&body),
        effects.shadow(&tight),
        effects.shadow(&shadow),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn outline_element(effects: &Effects, outline: &RoundedRect) -> Option<PixelShaderElement> {
    effects.rounded_rect(outline)
}

fn scene(effects: &Effects, scale: f64, palette: &Palette) -> Vec<PixelShaderElement> {
    let area = rect(0, 0, 760, 480);
    let mut elements = window(effects, rect(70, 150, 420, 270), scale, palette);
    let mut front = window(effects, rect(300, 60, 400, 260), scale, palette);
    front.append(&mut elements);
    let background =
        GradientBackground::two_stop(area, palette.background.0, palette.background.1, 135.0)
            .scale(scale);
    front.extend(effects.gradient_background(&background));
    front
}

fn screenshot(name: &str, scale: f64, palette: &Palette) {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let elements = scene(&effects, scale, palette);
    let (w, h) = (
        (760.0 * scale).round() as i32,
        (480.0 * scale).round() as i32,
    );
    let image = render(&mut gpu, w, h, scale, &elements, [0.0, 0.0, 0.0, 1.0]);
    save(&image, name);
}

#[test]
fn render_screenshots() {
    screenshot("effects-light-1x.png", 1.0, &light());
    screenshot("effects-dark-1x.png", 1.0, &dark());
    screenshot("effects-light-1.25x.png", 1.25, &light());
    screenshot("effects-dark-1.5x.png", 1.5, &dark());
}

#[test]
fn shapes_are_valid_for_degenerate_input() {
    let Some((_gpu, effects)) = setup() else {
        return;
    };
    assert!(
        effects
            .rounded_rect(&RoundedRect::new(rect(0, 0, 0, 10), Rgba::WHITE))
            .is_none()
    );
    assert!(
        effects
            .rounded_rect(&RoundedRect::new(rect(0, 0, 10, 10), Rgba::WHITE).scale(f64::NAN))
            .is_none()
    );
    assert!(
        effects
            .shadow(&Shadow::new(rect(0, 0, 10, 10)).blur(f32::NAN, f32::INFINITY))
            .is_some()
    );
    let single = GradientBackground::two_stop(rect(0, 0, 10, 10), Rgba::BLACK, Rgba::WHITE, 0.0)
        .stops(vec![]);
    assert!(effects.gradient_background(&single).is_none());
}
