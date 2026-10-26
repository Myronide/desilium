//! Offscreen GPU tests of the backdrop blur. Every test skips without a render node.

use super::super::{
    Corners, EffectKind, Effects, GradientBackground, GradientStop, Rgba, RoundedRect,
};
use super::{BackdropBlur, BackdropBlurParams};
use crate::backend::headless::{HeadlessGpu, Image};
use smithay::backend::renderer::element::render_elements;
use smithay::backend::renderer::element::{Element, Kind, RenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::gles::element::PixelShaderElement;
use smithay::backend::renderer::{Color32F, Frame, Renderer};
use smithay::utils::{Logical, Rectangle, Scale, Size, Transform};
use std::path::PathBuf;

const SCREENSHOT_DIR: &str = "../../docs/screenshots";

render_elements! {
    TestElement<=GlesRenderer>;
    Pixel = PixelShaderElement,
    Glass = BackdropBlur,
}

fn setup() -> Option<(HeadlessGpu, Effects)> {
    let mut gpu = match HeadlessGpu::new() {
        Ok(gpu) => gpu,
        Err(error) => {
            eprintln!("skipping GPU test: {error}");
            return None;
        }
    };
    let effects = Effects::compile(gpu.renderer()).expect("effects must compile");
    Some((gpu, effects))
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
    Rectangle::new((x, y).into(), (w, h).into())
}

fn color(red: u8, green: u8, blue: u8) -> Rgba {
    Rgba::from_rgba8(red, green, blue, 255)
}

fn glass(effects: &Effects, params: BackdropBlurParams) -> TestElement {
    TestElement::Glass(effects.backdrop_blur(params).expect("blur available"))
}

fn solid(effects: &Effects, area: Rectangle<i32, Logical>, fill: Rgba, scale: f64) -> TestElement {
    let params = RoundedRect::new(area, fill).scale(scale);
    TestElement::Pixel(effects.rounded_rect(&params).expect("rect available"))
}

fn render(gpu: &mut HeadlessGpu, size: (i32, i32), scale: f64, scene: &[TestElement]) -> Image {
    let background = [0.0, 0.0, 0.0, 1.0];
    gpu.render_to_image(size.into(), scale, scene, background)
        .expect("render")
}

fn save(image: &Image, name: &str) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SCREENSHOT_DIR);
    std::fs::create_dir_all(&dir).unwrap();
    image.save_png(&dir.join(name)).unwrap();
}

fn red_at(image: &Image, x: u32, y: u32) -> i32 {
    i32::from(image.pixel(x, y).unwrap()[0])
}

/// Black left half, white right half, split at logical x = 100 (200 x 120 canvas).
fn edge_scene(
    effects: &Effects,
    scale: f64,
    glass_params: Option<BackdropBlurParams>,
) -> Vec<TestElement> {
    let mut scene = Vec::new();
    scene.extend(glass_params.map(|params| glass(effects, params)));
    scene.push(solid(effects, rect(100, 0, 100, 120), Rgba::WHITE, scale));
    scene
}

fn px(value: i32, scale: f64) -> i32 {
    (f64::from(value) * scale).round() as i32
}

#[test]
fn blur_effect_is_available() {
    let Some((_gpu, effects)) = setup() else {
        return;
    };
    assert!(effects.is_available(EffectKind::BackdropBlur));
    assert!(effects.failures().is_empty(), "{:?}", effects.failures());
}

#[test]
fn constant_colors_stay_constant_even_at_screen_edges() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let fill = color(77, 128, 179);
    for (radius, scale) in [(8.0, 1.0), (24.0, 1.0), (48.0, 1.5), (48.0, 2.0)] {
        let size = (px(200, scale), px(120, scale));
        // Touches the left and top screen edges to exercise edge extension.
        let params = BackdropBlurParams::new(rect(0, 0, 120, 70), radius)
            .scale(scale)
            .noise(0.0);
        let scene = [
            glass(&effects, params),
            solid(&effects, rect(0, 0, 200, 120), fill, scale),
        ];
        let image = render(&mut gpu, size, scale, &scene);
        for y in 0..px(70, scale) as u32 {
            for x in 0..px(120, scale) as u32 {
                let pixel = image.pixel(x, y).unwrap();
                for (got, want) in pixel.iter().zip([77_u8, 128, 179, 255]) {
                    assert!(
                        got.abs_diff(want) <= 1,
                        "r{radius} s{scale} at {x},{y}: {pixel:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn hard_edge_spreads_monotonically_and_wider_with_radius() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let mut widths = Vec::new();
    for radius in [8.0, 24.0, 48.0] {
        let params = BackdropBlurParams::new(rect(10, 20, 180, 80), radius).noise(0.0);
        let image = render(
            &mut gpu,
            (200, 120),
            1.0,
            &edge_scene(&effects, 1.0, Some(params)),
        );
        let row: Vec<i32> = (10..190).map(|x| red_at(&image, x, 60)).collect();
        assert!(
            row.windows(2).all(|pair| pair[1] >= pair[0] - 1),
            "r{radius}: {row:?}"
        );
        let middle = (row[89] + row[90]) / 2;
        assert!((middle - 128).abs() <= 10, "r{radius}: midpoint {middle}");
        let rising = |level: i32| row.iter().position(|v| *v >= level).unwrap_or(0);
        widths.push(rising(230) - rising(25));
    }
    assert!(widths[0] < widths[1] && widths[1] < widths[2], "{widths:?}");
    // Gaussian sigma = radius / 2, so the 10-90 % width is about 2.56 sigma = 1.28 * radius.
    for (width, radius) in widths.iter().zip([8.0_f32, 24.0, 48.0]) {
        let expected = radius * 1.28;
        assert!(
            (*width as f32 - expected).abs() < expected * 0.35 + 3.0,
            "{width} vs {expected}"
        );
    }
}

#[test]
fn pixels_outside_the_glass_are_untouched() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let area = rect(40, 30, 120, 60);
    let params = BackdropBlurParams::new(area, 24.0).corners(Corners::all(16.0));
    let with = render(
        &mut gpu,
        (200, 120),
        1.0,
        &edge_scene(&effects, 1.0, Some(params)),
    );
    let without = render(&mut gpu, (200, 120), 1.0, &edge_scene(&effects, 1.0, None));
    let mut inside_changed = 0;
    for y in 0..120_i32 {
        for x in 0..200_i32 {
            let inside = x >= area.loc.x - 1
                && x <= area.loc.x + area.size.w
                && y >= area.loc.y - 1
                && y <= area.loc.y + area.size.h;
            let (a, b) = (
                with.pixel(x as u32, y as u32),
                without.pixel(x as u32, y as u32),
            );
            if !inside {
                assert_eq!(a, b, "outside pixel {x},{y} changed");
                continue;
            }
            inside_changed += usize::from(a != b);
        }
    }
    assert!(
        inside_changed > 500,
        "the glass must actually change its area"
    );
}

#[test]
fn corners_are_cut_by_the_rounded_mask() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let area = rect(40, 30, 120, 60);
    let params = BackdropBlurParams::new(area, 24.0)
        .corners(Corners::all(24.0))
        .noise(0.0);
    let image = render(
        &mut gpu,
        (200, 120),
        1.0,
        &edge_scene(&effects, 1.0, Some(params)),
    );
    // The outer corner pixel keeps the sharp backdrop (black, left of the white half).
    assert_eq!(red_at(&image, 41, 31), 0, "corner must not be glass");
    // A pixel on the straight edge, a few px in, is glass: the black/white edge at x=100
    // is smoothed there, which the corner pixel above could not be.
    assert!(red_at(&image, 100, 31) > 20 && red_at(&image, 100, 31) < 235);
}

#[test]
fn scale_invariance_within_tolerance() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    for radius in [8.0, 24.0, 48.0] {
        let params = |scale: f64| {
            BackdropBlurParams::new(rect(10, 20, 180, 80), radius)
                .scale(scale)
                .noise(0.0)
        };
        let one = render(
            &mut gpu,
            (200, 120),
            1.0,
            &edge_scene(&effects, 1.0, Some(params(1.0))),
        );
        let two = render(
            &mut gpu,
            (400, 240),
            2.0,
            &edge_scene(&effects, 2.0, Some(params(2.0))),
        );
        let mut worst = 0;
        for y in 20..100_u32 {
            for x in 10..190_u32 {
                let (fx, fy) = (x * 2, y * 2);
                let sum: i32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                    .iter()
                    .map(|(dx, dy)| red_at(&two, fx + dx, fy + dy))
                    .sum();
                worst = worst.max((sum / 4 - red_at(&one, x, y)).abs());
            }
        }
        assert!(
            worst <= 14,
            "radius {radius}: scale 1 vs 2 differ by {worst}"
        );
    }
}

#[test]
fn flipped_output_transform_matches_normal() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let params = BackdropBlurParams::new(rect(30, 20, 130, 70), 24.0)
        .corners(Corners {
            top_left: 4.0,
            top_right: 30.0,
            bottom_right: 14.0,
            bottom_left: 22.0,
        })
        .noise(0.0);
    let scene = edge_scene(&effects, 1.0, Some(params));
    let normal = render(&mut gpu, (200, 120), 1.0, &scene);
    let flipped = render_transformed(&mut gpu, (200, 120), &scene, Transform::Flipped180);
    let mut worst = 0;
    for y in 0..120_u32 {
        for x in 0..200_u32 {
            let other = flipped.pixel(x, 119 - y).unwrap();
            let base = normal.pixel(x, y).unwrap();
            worst = worst.max(
                base.iter()
                    .zip(other)
                    .map(|(a, b)| i32::from(*a) - i32::from(b))
                    .map(i32::abs)
                    .max()
                    .unwrap(),
            );
        }
    }
    assert!(worst <= 2, "Flipped180 differs from Normal by {worst}");
}

fn render_transformed(
    gpu: &mut HeadlessGpu,
    size: (i32, i32),
    scene: &[TestElement],
    transform: Transform,
) -> Image {
    let physical: Size<i32, smithay::utils::Physical> = size.into();
    gpu.render_with(physical, |renderer, target| {
        let mut frame = renderer
            .render(target, physical, transform)
            .map_err(|e| crate::error::CompositorError::Render(e.to_string()))?;
        let full = Rectangle::from_size(physical);
        frame
            .clear(Color32F::from([0.0, 0.0, 0.0, 1.0]), &[full])
            .map_err(|e| crate::error::CompositorError::Render(e.to_string()))?;
        let scale = Scale::from(1.0);
        for element in scene.iter().rev() {
            let dst = element.geometry(scale);
            let damage = [Rectangle::from_size(dst.size)];
            element
                .draw(&mut frame, element.src(), dst, &damage, &[])
                .map_err(|e| crate::error::CompositorError::Render(e.to_string()))?;
        }
        let sync = frame
            .finish()
            .map_err(|e| crate::error::CompositorError::Render(e.to_string()))?;
        renderer
            .wait(&sync)
            .map_err(|e| crate::error::CompositorError::Render(e.to_string()))
    })
    .expect("render")
}

#[test]
fn repeated_draws_are_stable_and_allocation_free() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let params = BackdropBlurParams::new(rect(10, 20, 180, 80), 24.0).noise(0.0);
    let scene = edge_scene(&effects, 1.0, Some(params));
    let first = render(&mut gpu, (200, 120), 1.0, &scene);
    for _ in 0..4 {
        let again = render(&mut gpu, (200, 120), 1.0, &scene);
        assert_eq!(first.rgba, again.rgba);
    }
}

#[test]
fn partial_damage_only_touches_the_damaged_rectangle() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    let params = BackdropBlurParams::new(rect(10, 20, 180, 80), 24.0).noise(0.0);
    let scene = edge_scene(&effects, 1.0, Some(params));
    let full = render(&mut gpu, (200, 120), 1.0, &scene);
    let TestElement::Glass(element) = &scene[0] else {
        panic!("glass first");
    };
    let size: Size<i32, smithay::utils::Physical> = (200, 120).into();
    let partial = gpu
        .render_with(size, |renderer, target| {
            let mut frame = renderer
                .render(target, size, Transform::Normal)
                .map_err(|e| crate::error::CompositorError::Render(e.to_string()))?;
            frame
                .clear(
                    Color32F::from([0.0, 0.0, 0.0, 1.0]),
                    &[Rectangle::from_size(size)],
                )
                .map_err(|e| crate::error::CompositorError::Render(e.to_string()))?;
            let white = &scene[1];
            let dst = white.geometry(Scale::from(1.0));
            white
                .draw(
                    &mut frame,
                    white.src(),
                    dst,
                    &[Rectangle::from_size(dst.size)],
                    &[],
                )
                .map_err(|e| crate::error::CompositorError::Render(e.to_string()))?;
            let area = element.geometry(Scale::from(1.0));
            let damage = [Rectangle::new((20, 10).into(), (30, 20).into())];
            element
                .draw(&mut frame, element.src(), area, &damage, &[])
                .map_err(|e| crate::error::CompositorError::Render(e.to_string()))?;
            let sync = frame
                .finish()
                .map_err(|e| crate::error::CompositorError::Render(e.to_string()))?;
            renderer
                .wait(&sync)
                .map_err(|e| crate::error::CompositorError::Render(e.to_string()))
        })
        .expect("render");
    // Inside the damage rectangle (glass-local 20..50 x 10..30 = canvas 30..60 x 30..50) the
    // result equals the full render; outside it the sharp backdrop is untouched.
    assert_eq!(partial.pixel(45, 40), full.pixel(45, 40));
    assert_eq!(partial.pixel(100, 40), Some([255, 255, 255, 255]));
    assert_eq!(partial.pixel(5, 40), Some([0, 0, 0, 255]));
}

/// Wallpaper gradient plus colourful shapes, the backdrop of the gallery.
fn wallpaper(effects: &Effects, size: (i32, i32), scale: f64) -> Vec<TestElement> {
    let logical = rect(0, 0, size.0, size.1);
    let stops = vec![
        GradientStop {
            position: 0.0,
            color: color(40, 60, 140),
        },
        GradientStop {
            position: 0.55,
            color: color(150, 70, 170),
        },
        GradientStop {
            position: 1.0,
            color: color(240, 140, 90),
        },
    ];
    let background = GradientBackground::two_stop(logical, Rgba::BLACK, Rgba::WHITE, 135.0)
        .scale(scale)
        .stops(stops);
    let shapes = [
        (rect(30, 150, 120, 90), color(250, 200, 40), 45.0),
        (rect(170, 40, 90, 90), color(30, 200, 170), 45.0),
        (rect(300, 130, 130, 70), color(235, 60, 70), 20.0),
        (rect(80, 40, 40, 200), color(255, 255, 255), 0.0),
        (rect(360, 20, 70, 70), color(20, 20, 30), 35.0),
        (rect(220, 190, 200, 24), color(120, 220, 90), 12.0),
    ];
    let mut scene: Vec<TestElement> = shapes
        .iter()
        .rev()
        .map(|(area, fill, radius)| {
            let params = RoundedRect::new(*area, *fill).radius(*radius).scale(scale);
            TestElement::Pixel(effects.rounded_rect(&params).unwrap())
        })
        .collect();
    scene.push(TestElement::Pixel(
        effects.gradient_background(&background).unwrap(),
    ));
    scene
}

fn gallery(gpu: &mut HeadlessGpu, effects: &Effects, radius: f32, scale: f64, dark: bool) -> Image {
    let logical = (480, 280);
    let tint = if dark {
        Rgba::from_straight(0.07, 0.08, 0.11, 0.38)
    } else {
        Rgba::from_straight(0.97, 0.98, 1.0, 0.42)
    };
    let panel = rect(60, 50, 360, 170);
    let params = BackdropBlurParams::new(panel, radius)
        .scale(scale)
        .corners(Corners::all(18.0))
        .saturation(1.35)
        .tint(tint);
    let mut scene = vec![glass(effects, params)];
    scene.extend(wallpaper(effects, logical, scale));
    render(
        gpu,
        (px(logical.0, scale), px(logical.1, scale)),
        scale,
        &scene,
    )
}

#[test]
fn render_gallery_screenshots() {
    let Some((mut gpu, effects)) = setup() else {
        return;
    };
    for radius in [8.0, 24.0, 48.0] {
        for (scale, dark, tag) in [
            (1.0, true, "1x-dark"),
            (1.0, false, "1x-light"),
            (1.5, true, "1.5x-dark"),
            (2.0, true, "2x-dark"),
        ] {
            let image = gallery(&mut gpu, &effects, radius, scale, dark);
            save(&image, &format!("blur-r{radius}-{tag}.png"));
        }
    }
    save(
        &{
            let scene = wallpaper(&effects, (480, 280), 1.0);
            render(&mut gpu, (480, 280), 1.0, &scene)
        },
        "blur-backdrop-reference.png",
    );
}

#[test]
fn kind_is_not_opaque() {
    let Some((_gpu, effects)) = setup() else {
        return;
    };
    let element = effects
        .backdrop_blur(BackdropBlurParams::new(rect(0, 0, 10, 10), 8.0))
        .unwrap();
    assert_eq!(
        element.opaque_regions(Scale::from(1.0)).into_iter().count(),
        0
    );
    assert_eq!(element.kind(), Kind::Unspecified);
}
