//! Backdrop blur behind the shell's surfaces through `org_kde_kwin_blur`.
//!
//! The blur protocol takes a `wl_region`, which is a union of rectangles, so rounded corners are
//! approximated with one rectangle per pixel row of each corner (neighbouring rows with the same
//! inset are merged).

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    protocol::{wl_compositor::WlCompositor, wl_region::WlRegion},
};
use wayland_protocols_plasma::blur::client::{
    org_kde_kwin_blur::OrgKdeKwinBlur, org_kde_kwin_blur_manager::OrgKdeKwinBlurManager,
};

use super::state::ProtoState;

/// A rectangle `(x, y, width, height)` in surface-local logical pixels.
pub type Rect = (i32, i32, i32, i32);

/// The area to blur for a surface of `size` with the corner radii `[top-left, top-right,
/// bottom-right, bottom-left]`, as non-overlapping rectangles.
pub fn blur_rects(size: (u32, u32), radii: [u32; 4]) -> Vec<Rect> {
    let width = i32::try_from(size.0).unwrap_or(i32::MAX);
    let height = i32::try_from(size.1).unwrap_or(i32::MAX);
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let limit = u32::try_from(width.min(height) / 2).unwrap_or(0);
    let [top_left, top_right, bottom_right, bottom_left] = radii.map(|radius| radius.min(limit));
    let top_band = top_left.max(top_right) as i32;
    let bottom_band = bottom_left.max(bottom_right) as i32;

    let mut runs = Runs::default();
    for y in 0..top_band {
        runs.push(
            y,
            y + 1,
            corner_inset(top_left, y),
            corner_inset(top_right, y),
        );
    }
    if top_band < height - bottom_band {
        runs.push(top_band, height - bottom_band, 0, 0);
    }
    for y in (height - bottom_band).max(top_band)..height {
        let from_bottom = height - 1 - y;
        runs.push(
            y,
            y + 1,
            corner_inset(bottom_left, from_bottom),
            corner_inset(bottom_right, from_bottom),
        );
    }
    runs.rects(width)
}

/// Rows `y0..y1` that share the same left and right inset.
#[derive(Default)]
struct Runs(Vec<(i32, i32, i32, i32)>);

impl Runs {
    /// Adds rows, extending the previous run when it is adjacent and has the same insets.
    fn push(&mut self, y0: i32, y1: i32, left: i32, right: i32) {
        if let Some(last) = self.0.last_mut()
            && last.1 == y0
            && last.2 == left
            && last.3 == right
        {
            last.1 = y1;
            return;
        }
        self.0.push((y0, y1, left, right));
    }

    fn rects(&self, width: i32) -> Vec<Rect> {
        self.0
            .iter()
            .filter(|(_, _, left, right)| width - left - right > 0)
            .map(|&(y0, y1, left, right)| (left, y0, width - left - right, y1 - y0))
            .collect()
    }
}

/// Pixels the circle of `radius` is inset from the edge at pixel row `distance` from that edge.
fn corner_inset(radius: u32, distance: i32) -> i32 {
    let radius = f64::from(radius);
    let from_edge = f64::from(distance) + 0.5;
    if radius <= 0.0 || from_edge >= radius {
        return 0;
    }
    let dy = radius - from_edge;
    (radius - (radius * radius - dy * dy).sqrt()).ceil() as i32
}

/// What a surface currently has applied, to skip identical requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    /// Logical surface size.
    pub size: (u32, u32),
    /// Blur strength requested by the shell; only zero/non-zero matters to KWin.
    pub blur: u32,
    /// Corner radii, clockwise from top-left.
    pub radii: [u32; 4],
}

/// A surface with a blur object attached.
pub struct BlurEntry {
    pub surface: wayland_client::protocol::wl_surface::WlSurface,
    pub blur: OrgKdeKwinBlur,
    pub applied: Effect,
}

impl BlurEntry {
    /// The surface is still usable.
    pub fn alive(&self) -> bool {
        self.surface.is_alive()
    }
}

/// Region and blur objects need dispatch targets even though they never send events.
macro_rules! silent_dispatch {
    ($($interface:ty),+ $(,)?) => {$(
        impl Dispatch<$interface, ()> for ProtoState {
            fn event(
                _: &mut Self,
                _: &$interface,
                _: <$interface as Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    )+};
}

silent_dispatch!(
    OrgKdeKwinBlurManager,
    OrgKdeKwinBlur,
    WlCompositor,
    WlRegion
);

#[cfg(test)]
mod tests {
    use super::*;

    fn area(rects: &[Rect]) -> i64 {
        rects
            .iter()
            .map(|&(_, _, w, h)| i64::from(w) * i64::from(h))
            .sum()
    }

    #[test]
    fn square_corners_are_one_rectangle() {
        assert_eq!(blur_rects((200, 100), [0; 4]), vec![(0, 0, 200, 100)]);
    }

    #[test]
    fn empty_surface_has_no_region() {
        assert!(blur_rects((0, 100), [8; 4]).is_empty());
        assert!(blur_rects((100, 0), [8; 4]).is_empty());
    }

    #[test]
    fn rounded_corners_cut_the_top_and_bottom_rows() {
        let rects = blur_rects((200, 100), [16; 4]);
        let first = rects.first().copied().expect("rows");
        assert!(first.0 > 0, "the first row is inset: {first:?}");
        assert_eq!(first.1, 0);
        let last = rects.last().copied().expect("rows");
        assert_eq!(
            last.1 + last.3,
            100,
            "the last rect ends at the bottom edge"
        );
        assert!(last.0 > 0, "the last row is inset: {last:?}");
        let full = 200 * 100;
        let cut = full - area(&rects);
        let ideal = 4.0 * (1.0 - std::f64::consts::FRAC_PI_4) * 16.0 * 16.0;
        assert!(
            (cut as f64) > ideal * 0.8 && (cut as f64) < ideal * 1.8,
            "cut {cut} px, circle would cut {ideal:.0}"
        );
    }

    #[test]
    fn rectangles_never_overlap_and_stay_inside() {
        let rects = blur_rects((120, 60), [12, 4, 20, 0]);
        let mut covered = vec![false; 120 * 60];
        for (x, y, w, h) in rects {
            assert!(x >= 0 && y >= 0 && w > 0 && h > 0 && x + w <= 120 && y + h <= 60);
            for yy in y..y + h {
                for xx in x..x + w {
                    let cell = &mut covered[(yy * 120 + xx) as usize];
                    assert!(!*cell, "pixel {xx},{yy} covered twice");
                    *cell = true;
                }
            }
        }
    }

    #[test]
    fn different_radii_per_corner_are_honoured() {
        let rects = blur_rects((100, 100), [20, 0, 0, 0]);
        let first = rects.first().copied().expect("rows");
        assert!(first.0 > 0, "top-left is rounded");
        assert_eq!(first.0 + first.2, 100, "top-right is square");
        let last = rects.last().copied().expect("rows");
        assert_eq!((last.0, last.2), (0, 100), "bottom corners are square");
    }

    #[test]
    fn oversized_radius_is_limited_to_half_the_short_side() {
        let rects = blur_rects((40, 20), [500; 4]);
        assert!(area(&rects) > 0);
        assert!(rects.iter().all(|&(x, _, w, _)| x >= 0 && x + w <= 40));
    }

    #[test]
    fn row_inset_follows_the_circle() {
        assert_eq!(corner_inset(10, 0), 7);
        assert_eq!(
            corner_inset(10, 9),
            1,
            "rounded up: never blur outside the shape"
        );
        assert_eq!(corner_inset(10, 10), 0);
        assert_eq!(corner_inset(0, 0), 0);
        assert!(corner_inset(10, 1) < corner_inset(10, 0));
    }
}
