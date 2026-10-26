//! Turns effect parameters into element geometry and shader uniforms.
//!
//! All shapes are evaluated in physical pixels. Positions are snapped with the
//! same rounding smithay applies to element geometry, so shader edges coincide
//! with the pixel grid of client surfaces at fractional scales.

use super::types::{
    Border, BorderPlacement, Corners, Fill, GradientBackground, MAX_GRADIENT_STOPS, RoundedRect,
    Shadow, sanitize_length, sanitize_unit,
};
use smithay::backend::renderer::gles::Uniform;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size};

/// Everything a `PixelShaderElement` needs besides its program.
pub(super) struct Prepared {
    pub area: Rectangle<i32, Logical>,
    pub opaque: Vec<Rectangle<i32, Logical>>,
    pub uniforms: Vec<Uniform<'static>>,
}

const SIGMA_PER_BLUR: f32 = 0.5;
const SIGMA_REACH: f32 = 3.0;
const MIN_SIGMA_PX: f32 = 0.35;
const MAX_BLUR: f32 = 512.0;
const MAX_SPREAD: f32 = 512.0;
const MAX_OFFSET: f32 = 4096.0;
const MAX_RADIUS: f32 = 4096.0;
const MAX_BORDER: f32 = 256.0;
const AA_MARGIN: i32 = 1;
const OPAQUE_INSET: i32 = 1;

fn xywh(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
    Rectangle::new(Point::from((x, y)), Size::from((w, h)))
}

fn valid_scale(scale: f64) -> Option<Scale<f64>> {
    (scale.is_finite() && scale > 0.0).then(|| Scale::from(scale))
}

fn clamp_length(value: f32, max: f32) -> f32 {
    sanitize_length(value).min(max)
}

fn clamp_signed(value: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-max, max)
    } else {
        0.0
    }
}

fn corner_px(radii: Corners, scale: f64) -> [f32; 4] {
    radii
        .sanitized()
        .map(|radius| radius.min(MAX_RADIUS) * scale as f32)
}

fn physical(rect: Rectangle<i32, Logical>, scale: Scale<f64>) -> Rectangle<i32, Physical> {
    rect.to_physical_precise_round(scale)
}

fn offset_in(
    rect: Rectangle<i32, Physical>,
    origin: Rectangle<i32, Physical>,
) -> Point<i32, Physical> {
    Point::from((
        rect.loc.x.saturating_sub(origin.loc.x),
        rect.loc.y.saturating_sub(origin.loc.y),
    ))
}

fn size_uniform(size: Size<i32, Physical>) -> Uniform<'static> {
    Uniform::new("u_phys", (size.w as f32, size.h as f32))
}

/// CSS gradient line: unit direction and the line length that touches the box corners.
fn gradient_line(angle_degrees: f32, width: f32, height: f32) -> [f32; 3] {
    let angle = if angle_degrees.is_finite() {
        angle_degrees
    } else {
        180.0
    }
    .to_radians();
    let (sin, cos) = angle.sin_cos();
    [sin, -cos, width * sin.abs() + height * cos.abs()]
}

/// Fully opaque cross-shaped region of a rounded rectangle, inset to survive edge rounding.
fn opaque_cross(
    width: i32,
    height: i32,
    radii: Corners,
    origin: i32,
) -> Vec<Rectangle<i32, Logical>> {
    let top = radii.top_left.max(radii.top_right);
    let bottom = radii.bottom_left.max(radii.bottom_right);
    let side = radii.max();
    let ceil = |value: f32| value.ceil() as i32;
    let (top, bottom, side) = (ceil(top), ceil(bottom), ceil(side));
    let inset = OPAQUE_INSET;
    let middle_top = top.max(inset);
    let middle_bottom = height.saturating_sub(bottom.max(inset));
    let mut regions = vec![xywh(
        origin + inset,
        origin + middle_top,
        width.saturating_sub(2 * inset),
        middle_bottom.saturating_sub(middle_top),
    )];
    let band_x = side.max(inset);
    let band_width = width.saturating_sub(2 * band_x);
    regions.push(xywh(
        origin + band_x,
        origin + inset,
        band_width,
        middle_top.saturating_sub(inset),
    ));
    regions.push(xywh(
        origin + band_x,
        origin + middle_bottom,
        band_width,
        height.saturating_sub(middle_bottom + inset),
    ));
    regions.retain(|region| region.size.w > 0 && region.size.h > 0);
    regions
}

fn border_metrics(border: Option<Border>, scale: Scale<f64>) -> (f32, bool, i32) {
    let Some(border) = border else {
        return (0.0, false, 0);
    };
    let width = clamp_length(border.width, MAX_BORDER);
    if width <= 0.0 {
        return (0.0, false, 0);
    }
    let physical = (f64::from(width) * scale.x).round().max(1.0) as f32;
    let outside = border.placement == BorderPlacement::Outside;
    let margin = if outside {
        width.ceil() as i32 + AA_MARGIN
    } else {
        0
    };
    (physical, outside, margin)
}

pub(super) fn prepare_rounded_rect(params: &RoundedRect) -> Option<Prepared> {
    let scale = valid_scale(params.scale)?;
    if params.rect.size.w <= 0 || params.rect.size.h <= 0 {
        return None;
    }
    let border = params.border.filter(|border| border.width > 0.0);
    let (border_px, outside, margin) = border_metrics(border, scale);
    let area = xywh(
        params.rect.loc.x - margin,
        params.rect.loc.y - margin,
        params.rect.size.w + 2 * margin,
        params.rect.size.h + 2 * margin,
    );
    let area_px = physical(area, scale);
    let rect_px = physical(params.rect, scale);
    let offset = offset_in(rect_px, area_px);

    let (from, to, line) = match params.fill {
        Fill::Solid(color) => (color, color, [0.0, 0.0, 0.0]),
        Fill::Linear {
            from,
            to,
            angle_degrees,
        } => (
            from,
            to,
            gradient_line(angle_degrees, rect_px.size.w as f32, rect_px.size.h as f32),
        ),
    };
    let dither = if line[2] > 0.0 { 1.0 } else { 0.0 };
    let border_color = border.map_or([0.0; 4], |border| border.color.sanitized());
    let uniforms = vec![
        size_uniform(area_px.size),
        Uniform::new(
            "u_rect",
            [
                offset.x as f32,
                offset.y as f32,
                rect_px.size.w as f32,
                rect_px.size.h as f32,
            ],
        ),
        Uniform::new("u_radii", corner_px(params.radii, scale.x)),
        Uniform::new("u_color_a", from.sanitized()),
        Uniform::new("u_color_b", to.sanitized()),
        Uniform::new("u_grad", line),
        Uniform::new("u_border", border_color),
        Uniform::new("u_border_w", border_px),
        Uniform::new("u_border_outer", if outside { 1.0f32 } else { 0.0 }),
        Uniform::new("u_alpha", sanitize_unit(params.alpha)),
        Uniform::new("u_dither", dither),
    ];
    Some(Prepared {
        area,
        opaque: rounded_rect_opaque(params, margin, border),
        uniforms,
    })
}

fn rounded_rect_opaque(
    params: &RoundedRect,
    margin: i32,
    border: Option<Border>,
) -> Vec<Rectangle<i32, Logical>> {
    let translucent = !params.fill.is_opaque() || params.alpha < 1.0;
    if translucent || !params.alpha.is_finite() {
        return Vec::new();
    }
    let ring = match border {
        Some(border) if border.placement == BorderPlacement::Inside => {
            clamp_length(border.width, MAX_BORDER)
        }
        _ => 0.0,
    };
    let radii = Corners {
        top_left: sanitize_length(params.radii.top_left),
        top_right: sanitize_length(params.radii.top_right),
        bottom_right: sanitize_length(params.radii.bottom_right),
        bottom_left: sanitize_length(params.radii.bottom_left),
    };
    if ring > 0.0 && border.is_some_and(|border| !border.color.is_opaque()) {
        // A translucent ring blends with the fill but the interior stays opaque.
        let ring = ring.ceil() as i32;
        let width = params.rect.size.w.saturating_sub(2 * ring);
        let height = params.rect.size.h.saturating_sub(2 * ring);
        return opaque_cross(width, height, radii, margin + ring);
    }
    opaque_cross(params.rect.size.w, params.rect.size.h, radii, margin)
}

pub(super) fn prepare_shadow(params: &Shadow) -> Option<Prepared> {
    let scale = valid_scale(params.scale)?;
    let spread = clamp_signed(params.spread, MAX_SPREAD);
    let width = params.rect.size.w as f32 + 2.0 * spread;
    let height = params.rect.size.h as f32 + 2.0 * spread;
    if params.rect.size.w <= 0 || params.rect.size.h <= 0 || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let sigma = clamp_length(params.blur, MAX_BLUR) * SIGMA_PER_BLUR;
    let reach = (sigma * SIGMA_REACH).ceil() + AA_MARGIN as f32;
    let left = params.rect.loc.x as f32 + clamp_signed(params.offset_x, MAX_OFFSET) - spread;
    let top = params.rect.loc.y as f32 + clamp_signed(params.offset_y, MAX_OFFSET) - spread;
    let area = Rectangle::from_extremities(
        ((left - reach).floor() as i32, (top - reach).floor() as i32),
        (
            (left + width + reach).ceil() as i32,
            (top + height + reach).ceil() as i32,
        ),
    );
    let area_px = physical(area, scale);
    let scale_f = scale.x as f32;
    let shape_x = left * scale_f - area_px.loc.x as f32;
    let shape_y = top * scale_f - area_px.loc.y as f32;
    let shape_radii = params.radii.sanitized().map(|radius| {
        let grown = if spread < 0.0 || radius > 0.0 {
            radius + spread
        } else {
            0.0
        };
        grown.clamp(0.0, MAX_RADIUS) * scale_f
    });
    let knock_px = physical(params.rect, scale);
    let knock_offset = offset_in(knock_px, area_px);
    let uniforms = vec![
        size_uniform(area_px.size),
        Uniform::new(
            "u_rect",
            [shape_x, shape_y, width * scale_f, height * scale_f],
        ),
        Uniform::new("u_radii", shape_radii),
        Uniform::new("u_sigma", (sigma * scale_f).max(MIN_SIGMA_PX)),
        Uniform::new("u_color", params.color.sanitized()),
        Uniform::new(
            "u_knock",
            [
                knock_offset.x as f32,
                knock_offset.y as f32,
                knock_px.size.w as f32,
                knock_px.size.h as f32,
            ],
        ),
        Uniform::new("u_knock_radii", corner_px(params.radii, scale.x)),
        Uniform::new("u_knock_on", if params.knockout { 1.0f32 } else { 0.0 }),
        Uniform::new("u_alpha", sanitize_unit(params.alpha)),
    ];
    Some(Prepared {
        area,
        opaque: Vec::new(),
        uniforms,
    })
}

pub(super) fn prepare_gradient(params: &GradientBackground) -> Option<Prepared> {
    let scale = valid_scale(params.scale)?;
    if params.area.size.w <= 0 || params.area.size.h <= 0 {
        return None;
    }
    let mut stops: Vec<_> = params.stops.iter().take(MAX_GRADIENT_STOPS).collect();
    if stops.len() < 2 {
        return None;
    }
    stops.sort_by(|a, b| a.position.total_cmp(&b.position));
    let area_px = physical(params.area, scale);
    let last = stops.last().map(|stop| stop.color.sanitized())?;
    let color_of = |index: usize| stops.get(index).map_or(last, |stop| stop.color.sanitized());
    let position_of = |index: usize| {
        stops
            .get(index)
            .map_or(1.0, |stop| sanitize_unit(stop.position))
    };
    let line = gradient_line(
        params.angle_degrees,
        area_px.size.w as f32,
        area_px.size.h as f32,
    );
    let opaque = stops.iter().all(|stop| stop.color.is_opaque()) && params.alpha >= 1.0;
    let uniforms = vec![
        size_uniform(area_px.size),
        Uniform::new("u_grad", line),
        Uniform::new("u_c0", color_of(0)),
        Uniform::new("u_c1", color_of(1)),
        Uniform::new("u_c2", color_of(2)),
        Uniform::new("u_c3", color_of(3)),
        Uniform::new(
            "u_pos",
            [
                position_of(0),
                position_of(1),
                position_of(2),
                position_of(3),
            ],
        ),
        Uniform::new("u_alpha", sanitize_unit(params.alpha)),
        Uniform::new("u_dither", clamp_length(params.dither, 4.0)),
    ];
    Some(Prepared {
        area: params.area,
        opaque: if opaque {
            vec![Rectangle::from_size(params.area.size)]
        } else {
            Vec::new()
        },
        uniforms,
    })
}
