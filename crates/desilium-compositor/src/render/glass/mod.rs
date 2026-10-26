//! Frosted-glass bookkeeping: collects glass requests while the scene is built, then inserts one
//! [`BackdropBlur`](super::effects::BackdropBlur) per request at its z position.
//!
//! The scene is a list of elements, topmost first, that is only ever appended to while it is built.
//! A surface that wants glass therefore records `out.len()` after pushing everything that belongs
//! above the glass (the surface itself, its border) and before pushing anything below it. Once the
//! list is complete, [`Glass::resolve`] walks the requests from the bottom up so earlier insertions
//! never shift later indices, and lets every slot compare the final element list with the previous
//! frame. An unchanged desktop therefore keeps its glass at zero damage.

pub mod debug;

#[cfg(test)]
mod gpu_tests;

use super::effects::{BackdropBlurParams, BlurSlot, Effects};
use super::elements::DesiliumElement;
use desilium_wm::WindowId;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::backend::ObjectId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Scale, Size};
use std::cell::RefCell;
use std::collections::HashMap;

/// Identity of one glass surface; keeps its [`BlurSlot`] (and so its element id) between frames.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GlassKey {
    /// A layer surface or popup, by its `wl_surface`.
    Surface(ObjectId),
    /// The title bar of a window.
    TitleBar(WindowId),
    /// Synthetic glass drawn by a development override.
    Debug(u8),
}

impl GlassKey {
    /// Key of the glass behind `surface`.
    pub fn surface(surface: &WlSurface) -> GlassKey {
        GlassKey::Surface(surface.id())
    }
}

/// One glass surface wanted in the current frame.
#[derive(Debug, Clone)]
struct Request {
    key: GlassKey,
    /// Insert position in the topmost-first list: the glass goes right before this element.
    index: usize,
    params: BackdropBlurParams,
}

#[derive(Default)]
struct Inner {
    /// Output size and scale of the previous frame, to notice resizes.
    output: Option<(Size<i32, Logical>, f64)>,
    requests: Vec<Request>,
    slots: HashMap<GlassKey, BlurSlot>,
}

/// Glass requests of the frame being built and the slots that outlive it.
///
/// Interior mutability because the scene is built from a shared `SceneContext`; rendering is
/// single threaded.
#[derive(Default)]
pub struct Glass {
    inner: RefCell<Inner>,
}

impl Glass {
    /// Starts a scene build.
    pub fn begin_frame(&self) {
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.requests.clear();
        }
    }

    /// Frees the blur textures sized for the old output when the output size or scale changed.
    pub fn track_output(
        &self,
        renderer: &mut GlesRenderer,
        effects: Option<&Effects>,
        size: Size<i32, Logical>,
        scale: f64,
    ) {
        let Ok(mut inner) = self.inner.try_borrow_mut() else {
            return;
        };
        let now = Some((size, scale));
        if inner.output == now {
            return;
        }
        let first_frame = inner.output.is_none();
        inner.output = now;
        let shared = effects.and_then(Effects::backdrop_blur_resources);
        if let Some(shared) = shared.filter(|_| !first_frame) {
            shared.trim(renderer);
        }
    }

    /// Asks for glass at list position `index`; call it while the scene is still being appended to.
    pub fn request(&self, key: GlassKey, index: usize, params: BackdropBlurParams) {
        let Ok(mut inner) = self.inner.try_borrow_mut() else {
            return;
        };
        inner.requests.push(Request { key, index, params });
    }

    /// Inserts the glass elements into the finished scene and drops slots nobody asked for.
    ///
    /// Without blur support nothing is inserted; the surfaces keep their plain translucency.
    pub fn resolve(&self, effects: Option<&Effects>, scale: f64, out: &mut Vec<DesiliumElement>) {
        let Ok(mut inner) = self.inner.try_borrow_mut() else {
            return;
        };
        let Inner {
            requests, slots, ..
        } = &mut *inner;
        let requests = std::mem::take(requests);
        let Some(shared) = effects.and_then(Effects::backdrop_blur_resources) else {
            slots.clear();
            return;
        };
        slots.retain(|key, _| requests.iter().any(|request| &request.key == key));
        for position in insertion_order(requests.iter().map(|request| request.index)) {
            let Some(request) = requests.get(position) else {
                continue;
            };
            let slot = slots.entry(request.key.clone()).or_default();
            let glass = slot.update(
                shared,
                request.params.clone(),
                Scale::from(scale),
                out.iter(),
            );
            let index = request.index.min(out.len());
            out.insert(index, DesiliumElement::from(glass));
        }
    }

    /// Number of live slots (for tests and logging).
    #[cfg(test)]
    fn slot_count(&self) -> usize {
        self.inner.borrow().slots.len()
    }
}

/// The order in which requests must be inserted: highest index first, and among requests at the
/// same index the one recorded last first, so the one recorded first ends up topmost.
fn insertion_order(indices: impl Iterator<Item = usize>) -> Vec<usize> {
    let mut order: Vec<(usize, usize)> = indices.enumerate().map(|(rank, at)| (at, rank)).collect();
    order.sort_unstable_by(|left, right| right.cmp(left));
    order.into_iter().map(|(_, rank)| rank).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertion_runs_from_the_bottom_of_the_scene_up() {
        let order = insertion_order([2, 9, 5].into_iter());
        assert_eq!(order, vec![1, 2, 0]);
    }

    #[test]
    fn same_index_keeps_the_first_recorded_glass_on_top() {
        // Recorded 0 then 1 at the same spot: 1 is inserted first, 0 lands before it (above).
        let order = insertion_order([4, 4].into_iter());
        assert_eq!(order, vec![1, 0]);
        let mut list = vec!["a", "b", "c", "d", "e"];
        for rank in order {
            list.insert(4, ["glass0", "glass1"][rank]);
        }
        assert_eq!(list, ["a", "b", "c", "d", "glass0", "glass1", "e"]);
    }

    #[test]
    fn no_requests_means_no_work() {
        assert!(insertion_order(std::iter::empty()).is_empty());
    }

    #[test]
    fn unsupported_blur_drops_requests_and_slots() {
        let glass = Glass::default();
        glass.begin_frame();
        let rect = smithay::utils::Rectangle::new((0, 0).into(), (10, 10).into());
        glass.request(GlassKey::Debug(0), 0, BackdropBlurParams::new(rect, 8.0));
        let mut out = Vec::new();
        glass.resolve(None, 1.0, &mut out);
        assert!(out.is_empty());
        assert_eq!(glass.slot_count(), 0);
    }
}
