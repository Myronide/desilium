//! Procedurally drawn fallback cursors. Shapes are authored on a 24-unit grid and
//! rasterized from signed distances, so every edge is anti-aliased at any size.

use std::f32::consts::{FRAC_PI_4, PI};
use std::ops::{Add, Mul, Sub};

use crate::{CursorImage, CursorKind};

const DESIGN_UNITS: f32 = 24.0;
const DEFAULT_SIZE: u32 = 24;
const MIN_SIZE: u32 = 8;
const MAX_SIZE: u32 = 256;
const BYTES_PER_PIXEL: usize = 4;
/// Pixel centres sit at half-integer offsets; coverage ramps over one pixel around the edge.
const HALF: f32 = 0.5;
/// Outline width in design units; a white rim surrounds the dark body.
const OUTLINE_UNITS: f32 = 1.0;
const BODY_LEVEL: f32 = 0.1;
const SHADOW_OFFSET: Point = point(0.6, 0.9);
const SHADOW_BLUR_UNITS: f32 = 1.2;
const SHADOW_ALPHA: f32 = 0.45;
const CENTER: Point = point(12.0, 12.0);
const ARROW_TIP: Point = point(0.0, 0.0);
const HAND_TIP: Point = point(8.0, 2.4);
/// Spinner gap (radians, screen space) where the ring is open.
const SPINNER_GAP: (f32, f32) = (-FRAC_PI_4, PI / 12.0);
/// Distance returned inside the spinner gap so nothing is drawn there.
const GAP_DISTANCE: f32 = 1000.0;

const ARROW: [Point; 7] = [
    point(0.0, 0.0),
    point(0.0, 17.5),
    point(4.2, 13.7),
    point(6.9, 20.2),
    point(10.3, 18.7),
    point(7.6, 12.4),
    point(12.8, 12.4),
];

/// Double-headed arrow centred on the origin, pointing along +x.
const DOUBLE_ARROW: [Point; 10] = [
    point(-10.0, 0.0),
    point(-4.5, -5.0),
    point(-4.5, -2.0),
    point(4.5, -2.0),
    point(4.5, -5.0),
    point(10.0, 0.0),
    point(4.5, 5.0),
    point(4.5, 2.0),
    point(-4.5, 2.0),
    point(-4.5, 5.0),
];

/// Hand parts as capsules (start, end, radius): index, middle, ring, palm, thumb.
const HAND_CAPSULES: [(Point, Point, f32); 5] = [
    (point(8.0, 11.5), point(8.0, 4.2), 1.8),
    (point(11.2, 10.5), point(11.2, 4.6), 1.8),
    (point(14.2, 11.0), point(14.2, 5.8), 1.7),
    (point(7.5, 14.5), point(13.5, 14.5), 4.8),
    (point(4.6, 12.6), point(6.2, 9.5), 1.6),
];

/// Arrowheads of the move cursor, one triangle per direction.
const MOVE_HEADS: [[Point; 3]; 4] = [
    [point(12.0, 1.0), point(8.6, 5.2), point(15.4, 5.2)],
    [point(12.0, 23.0), point(8.6, 18.8), point(15.4, 18.8)],
    [point(1.0, 12.0), point(5.2, 8.6), point(5.2, 15.4)],
    [point(23.0, 12.0), point(18.8, 8.6), point(18.8, 15.4)],
];

/// A point or vector on the 24-unit design grid.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Point {
    x: f32,
    y: f32,
}

const fn point(x: f32, y: f32) -> Point {
    Point { x, y }
}

impl Point {
    fn dot(self, other: Point) -> f32 {
        self.x * other.x + self.y * other.y
    }

    fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
}

impl Add for Point {
    type Output = Point;
    fn add(self, other: Point) -> Point {
        point(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Point {
    type Output = Point;
    fn sub(self, other: Point) -> Point {
        point(self.x - other.x, self.y - other.y)
    }
}

impl Mul<f32> for Point {
    type Output = Point;
    fn mul(self, factor: f32) -> Point {
        point(self.x * factor, self.y * factor)
    }
}

/// Which procedural outline a kind is drawn with.
#[derive(Clone, Copy)]
enum Shape {
    Arrow,
    ArrowWithSpinner,
    IBeam,
    Hand,
    Spinner,
    NotAllowed,
    Crosshair,
    Move,
    DoubleArrow { degrees: f32 },
}

impl Shape {
    fn of(kind: CursorKind) -> Shape {
        match kind {
            CursorKind::Default => Shape::Arrow,
            CursorKind::Text => Shape::IBeam,
            CursorKind::Pointer | CursorKind::Grab | CursorKind::Grabbing => Shape::Hand,
            CursorKind::Wait => Shape::Spinner,
            CursorKind::Progress => Shape::ArrowWithSpinner,
            CursorKind::NotAllowed => Shape::NotAllowed,
            CursorKind::Crosshair => Shape::Crosshair,
            CursorKind::Move => Shape::Move,
            CursorKind::ResizeN | CursorKind::ResizeS | CursorKind::ResizeNs => {
                Shape::DoubleArrow { degrees: 90.0 }
            }
            CursorKind::ResizeE | CursorKind::ResizeW | CursorKind::ResizeEw => {
                Shape::DoubleArrow { degrees: 0.0 }
            }
            CursorKind::ResizeNe | CursorKind::ResizeSw => Shape::DoubleArrow { degrees: -45.0 },
            CursorKind::ResizeNw | CursorKind::ResizeSe => Shape::DoubleArrow { degrees: 45.0 },
        }
    }

    /// Hotspot in design units: the tip of an arrow, the centre of symmetric shapes.
    fn hotspot(self) -> Point {
        match self {
            Shape::Arrow | Shape::ArrowWithSpinner => ARROW_TIP,
            Shape::Hand => HAND_TIP,
            Shape::IBeam
            | Shape::Spinner
            | Shape::NotAllowed
            | Shape::Crosshair
            | Shape::Move
            | Shape::DoubleArrow { .. } => CENTER,
        }
    }

    /// Signed distance to the shape in design units; negative inside.
    fn distance(self, p: Point) -> f32 {
        match self {
            Shape::Arrow => polygon(p, &ARROW),
            Shape::ArrowWithSpinner => polygon(p, &ARROW).min(ring(p, point(19.0, 19.0), 3.4, 1.6)),
            Shape::IBeam => ibeam(p),
            Shape::Hand => hand(p),
            Shape::Spinner => spinner(p),
            Shape::NotAllowed => not_allowed(p),
            Shape::Crosshair => crosshair(p),
            Shape::Move => move_cross(p),
            Shape::DoubleArrow { degrees } => double_arrow(p, degrees),
        }
    }
}

/// Builds the cursor image for `kind` at `size` pixels (clamped; 0 means the default size).
///
/// Anti-aliased and crisp at any size; the hotspot is always inside the image.
pub fn builtin_cursor(kind: CursorKind, size: u32) -> CursorImage {
    let size = normalized_size(size);
    let shape = Shape::of(kind);
    let scale = size as f32 / DESIGN_UNITS;
    let hotspot = shape.hotspot() * scale;
    CursorImage {
        width: size,
        height: size,
        hotspot_x: pixel_within(hotspot.x, size),
        hotspot_y: pixel_within(hotspot.y, size),
        rgba_premultiplied: rasterize(shape, size, scale),
        delay_ms: 0,
    }
}

/// Pixel size actually rendered for a requested size.
pub(crate) fn normalized_size(size: u32) -> u32 {
    if size == 0 {
        return DEFAULT_SIZE;
    }
    size.clamp(MIN_SIZE, MAX_SIZE)
}

fn pixel_within(value: f32, size: u32) -> u32 {
    (value.round().max(0.0) as u32).min(size - 1)
}

fn rasterize(shape: Shape, size: u32, scale: f32) -> Vec<u8> {
    let row = size as usize;
    let mut pixels = vec![0u8; row * row * BYTES_PER_PIXEL];
    for (index, out) in pixels
        .as_chunks_mut::<BYTES_PER_PIXEL>()
        .0
        .iter_mut()
        .enumerate()
    {
        let x = (index % row) as f32;
        let y = (index / row) as f32;
        let point = point((x + HALF) / scale, (y + HALF) / scale);
        write_rgba(out, shade(shape, point, scale));
    }
    pixels
}

/// Premultiplied colour and alpha for one pixel: white rim, dark body, soft shadow below.
fn shade(shape: Shape, point: Point, scale: f32) -> [f32; 4] {
    let distance = shape.distance(point) * scale;
    let shadow_distance = shape.distance(point - SHADOW_OFFSET) * scale;
    let outline = (OUTLINE_UNITS * scale).max(1.0);
    let outer = (HALF - distance).clamp(0.0, 1.0);
    let inner = (HALF - (distance + outline)).clamp(0.0, 1.0);
    let rim = outer - inner;
    let shadow =
        SHADOW_ALPHA * (HALF - shadow_distance / (SHADOW_BLUR_UNITS * scale)).clamp(0.0, 1.0);
    let level = rim + BODY_LEVEL * inner;
    [level, level, level, outer + shadow * (1.0 - outer)]
}

fn write_rgba(out: &mut [u8; BYTES_PER_PIXEL], premultiplied: [f32; 4]) {
    let [red, green, blue, alpha] = out;
    *red = to_byte(premultiplied[0]);
    *green = to_byte(premultiplied[1]);
    *blue = to_byte(premultiplied[2]);
    *alpha = to_byte(premultiplied[3]);
}

fn to_byte(level: f32) -> u8 {
    (level.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let edge = b - a;
    let edge_len_sq = edge.dot(edge);
    let along = if edge_len_sq > 0.0 {
        ((p - a).dot(edge) / edge_len_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p - (a + edge * along)).length()
}

fn capsule(p: Point, a: Point, b: Point, radius: f32) -> f32 {
    segment_distance(p, a, b) - radius
}

fn circle(p: Point, center: Point, radius: f32) -> f32 {
    (p - center).length() - radius
}

fn ring(p: Point, center: Point, radius: f32, width: f32) -> f32 {
    ((p - center).length() - radius).abs() - width * HALF
}

/// Signed distance to a closed polygon: exact edge distance, sign from even-odd crossings.
fn polygon(p: Point, points: &[Point]) -> f32 {
    let mut nearest_sq = f32::INFINITY;
    let mut inside = false;
    for (a, b) in edges(points) {
        let distance = segment_distance(p, a, b);
        nearest_sq = nearest_sq.min(distance * distance);
        if (a.y > p.y) != (b.y > p.y) {
            let crossing_x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < crossing_x {
                inside = !inside;
            }
        }
    }
    let distance = nearest_sq.sqrt();
    if inside { -distance } else { distance }
}

fn edges(points: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| (*a, *b))
}

fn ibeam(p: Point) -> f32 {
    let stem = capsule(p, point(12.0, 3.5), point(12.0, 20.5), 1.1);
    let top = capsule(p, point(8.5, 3.2), point(15.5, 3.2), 1.1);
    let bottom = capsule(p, point(8.5, 20.8), point(15.5, 20.8), 1.1);
    stem.min(top).min(bottom)
}

fn hand(p: Point) -> f32 {
    HAND_CAPSULES
        .iter()
        .map(|&(a, b, radius)| capsule(p, a, b, radius))
        .fold(f32::INFINITY, f32::min)
}

/// Open ring: the gap is an angular wedge where no distance is reported.
fn spinner(p: Point) -> f32 {
    let offset = p - CENTER;
    let (gap_start, gap_end) = SPINNER_GAP;
    if (gap_start..=gap_end).contains(&offset.y.atan2(offset.x)) {
        return GAP_DISTANCE;
    }
    ring(p, CENTER, 7.5, 2.6)
}

fn not_allowed(p: Point) -> f32 {
    let outline = ring(p, CENTER, 8.0, 2.4);
    let bar = capsule(p, point(6.0, 6.0), point(18.0, 18.0), 1.2);
    outline.min(bar.max(circle(p, CENTER, 6.8)))
}

fn crosshair(p: Point) -> f32 {
    let bars = [
        (point(1.0, 12.0), point(10.0, 12.0)),
        (point(14.0, 12.0), point(23.0, 12.0)),
        (point(12.0, 1.0), point(12.0, 10.0)),
        (point(12.0, 14.0), point(12.0, 23.0)),
    ];
    bars.iter()
        .map(|&(a, b)| capsule(p, a, b, 0.8))
        .fold(f32::INFINITY, f32::min)
}

fn move_cross(p: Point) -> f32 {
    let bars = capsule(p, point(12.0, 5.0), point(12.0, 19.0), 1.0).min(capsule(
        p,
        point(5.0, 12.0),
        point(19.0, 12.0),
        1.0,
    ));
    MOVE_HEADS
        .iter()
        .fold(bars, |best, head| best.min(polygon(p, head)))
}

/// Double-headed arrow rotated so its axis points at `degrees` (screen space, y down).
fn double_arrow(p: Point, degrees: f32) -> f32 {
    let offset = p - CENTER;
    let (sin, cos) = degrees.to_radians().sin_cos();
    let local = point(
        offset.x * cos + offset.y * sin,
        -offset.x * sin + offset.y * cos,
    );
    polygon(local, &DOUBLE_ARROW)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_SIZES: [u32; 4] = [8, 24, 32, 48];

    fn pixel_alpha(image: &CursorImage, x: u32, y: u32) -> u8 {
        let index = (y as usize * image.width as usize + x as usize) * BYTES_PER_PIXEL + 3;
        image.rgba_premultiplied[index]
    }

    #[test]
    fn buffer_matches_dimensions_for_every_kind() {
        for kind in CursorKind::ALL {
            for size in ALL_SIZES {
                let image = builtin_cursor(kind, size);
                let expected = image.width as usize * image.height as usize * BYTES_PER_PIXEL;
                assert_eq!(
                    image.rgba_premultiplied.len(),
                    expected,
                    "{kind:?} at {size}"
                );
            }
        }
    }

    #[test]
    fn hotspot_is_inside_bounds_for_every_kind() {
        for kind in CursorKind::ALL {
            for size in ALL_SIZES {
                let image = builtin_cursor(kind, size);
                assert!(image.hotspot_x < image.width, "{kind:?} at {size}");
                assert!(image.hotspot_y < image.height, "{kind:?} at {size}");
            }
        }
    }

    #[test]
    fn every_kind_draws_visible_pixels() {
        for kind in CursorKind::ALL {
            let image = builtin_cursor(kind, 24);
            let visible = image
                .rgba_premultiplied
                .as_chunks::<4>()
                .0
                .iter()
                .any(|px| px[3] > 0);
            assert!(visible, "{kind:?} is fully transparent");
        }
    }

    #[test]
    fn premultiplied_invariant_holds_for_every_kind() {
        for kind in CursorKind::ALL {
            let image = builtin_cursor(kind, 32);
            for px in image.rgba_premultiplied.as_chunks::<4>().0 {
                assert!(
                    px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3],
                    "{kind:?}"
                );
            }
        }
    }

    #[test]
    fn arrow_body_is_opaque_and_corner_is_transparent() {
        let image = builtin_cursor(CursorKind::Default, 24);
        assert_eq!(pixel_alpha(&image, 1, 6), 255);
        assert_eq!(pixel_alpha(&image, 20, 1), 0);
    }

    #[test]
    fn zero_size_uses_default_size() {
        let image = builtin_cursor(CursorKind::Default, 0);
        assert_eq!(image.width, DEFAULT_SIZE);
        assert_eq!(image.height, DEFAULT_SIZE);
    }

    #[test]
    fn huge_size_is_clamped() {
        let image = builtin_cursor(CursorKind::Text, u32::MAX);
        assert_eq!(image.width, MAX_SIZE);
    }

    #[test]
    fn text_hotspot_is_centred() {
        let image = builtin_cursor(CursorKind::Text, 24);
        assert_eq!((image.hotspot_x, image.hotspot_y), (12, 12));
    }
}
