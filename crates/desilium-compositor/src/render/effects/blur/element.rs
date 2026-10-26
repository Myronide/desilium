//! The render element and the bookkeeping that keeps its damage correct.

use super::params::BackdropBlurParams;
use super::plan::BlurPlan;
use super::resources::{DrawJob, SharedBlur};
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::utils::{Buffer, Physical, Point, Rectangle, Scale, Size, Transform};

/// Frosted glass: draws the blurred content below it clipped to a rounded rectangle.
///
/// # Contract
///
/// * The element reads the frame buffer when it is drawn, so it sees exactly what was
///   drawn below it in this frame (Smithay draws bottom to top).
/// * It never declares opaque regions: the elements below must be drawn for the grab.
/// * It reports its whole area as damaged whenever [`BackdropBlur::invalidate`] or
///   [`BackdropBlur::set_params`] moved the commit counter, and otherwise nothing. Keep the
///   counter honest with [`BackdropWatch`] (or [`super::BlurSlot`], which wraps it): any
///   change of any other element near the glass must invalidate it. Partial repaints of the
///   glass would re-blur already blurred pixels and show seams, hence "all or nothing".
/// * Keep the element (and its [`Id`]) alive between frames; clone it into the scene list.
#[derive(Debug, Clone)]
pub struct BackdropBlur {
    id: Id,
    commit: CommitCounter,
    params: BackdropBlurParams,
    shared: SharedBlur,
}

impl BackdropBlur {
    /// Creates a glass surface using the shared GL resources.
    pub fn new(shared: &SharedBlur, params: BackdropBlurParams) -> BackdropBlur {
        BackdropBlur {
            id: Id::new(),
            commit: CommitCounter::default(),
            params,
            shared: shared.clone(),
        }
    }

    /// The element's parameters.
    pub fn params(&self) -> &BackdropBlurParams {
        &self.params
    }

    /// Replaces the parameters; the area is damaged only if something changed.
    pub fn set_params(&mut self, params: BackdropBlurParams) {
        if self.params == params {
            return;
        }
        self.params = params;
        self.commit.increment();
    }

    /// Marks the glass dirty (the backdrop changed): the whole area is repainted next frame.
    pub fn invalidate(&mut self) {
        self.commit.increment();
    }

    fn physical_area(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.params.rect.to_physical_precise_round(scale)
    }
}

impl Element for BackdropBlur {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        self.physical_area(scale).loc
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        let size = self.params.rect.size;
        Rectangle::from_size(Size::from((f64::from(size.w), f64::from(size.h))))
    }

    fn transform(&self) -> Transform {
        Transform::Normal
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.physical_area(scale)
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        if commit == Some(self.commit) {
            return DamageSet::default();
        }
        DamageSet::from_slice(&[Rectangle::from_size(self.physical_area(scale).size)])
    }

    // Never opaque: the tracker would skip drawing the elements below, which we need.
    fn opaque_regions(&self, _scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        OpaqueRegions::default()
    }

    fn alpha(&self) -> f32 {
        1.0
    }

    fn kind(&self) -> Kind {
        Kind::Unspecified
    }
}

impl RenderElement<GlesRenderer> for BackdropBlur {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        _src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        if dst.is_empty() || damage.is_empty() {
            return Ok(());
        }
        let projection = *frame.projection();
        let outcome = frame.with_context(|gl| {
            let mut resources = self.shared.borrow_mut()?;
            let job = DrawJob {
                params: &self.params,
                projection,
                dst,
                damage,
            };
            resources.draw(gl, &job)
        })?;
        // Degrade gracefully: without blur the translucent fill drawn on top still renders.
        if let Err(error) = outcome {
            tracing::warn!(%error, "backdrop blur failed; falling back to plain translucency");
        }
        Ok(())
    }

    // Direct scanout would skip the shader.
    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}

/// What was last seen of one element near the glass.
#[derive(Debug, Clone, PartialEq)]
struct Seen {
    id: Id,
    commit: CommitCounter,
    geometry: Rectangle<i32, Physical>,
}

/// Detects when the backdrop of a [`BackdropBlur`] changed.
///
/// Feed it every other element of the scene (above AND below the glass: content above that
/// repaints inside the glass area also forces the glass to repaint as a whole) once per
/// frame. Elements are compared by id, commit counter and geometry, so no knowledge of the
/// damage tracker is needed. Cost: one pass over the scene list with an intersection test.
#[derive(Debug, Default)]
pub struct BackdropWatch {
    seen: Vec<Seen>,
    scratch: Vec<Seen>,
}

impl BackdropWatch {
    /// Returns true if anything within reach of `blur` changed since the previous call.
    pub fn changed<'a, E: Element + 'a>(
        &mut self,
        blur: &BackdropBlur,
        scale: Scale<f64>,
        scene: impl IntoIterator<Item = &'a E>,
    ) -> bool {
        let params = blur.params();
        let plan = BlurPlan::new(params.radius, params.scale);
        let margin = Point::from((plan.margin, plan.margin));
        let area = blur.physical_area(scale);
        let reach = Rectangle::new(
            area.loc - margin,
            area.size + margin.to_size() + margin.to_size(),
        );
        self.changed_in(blur.id(), reach, scale, scene)
    }

    fn changed_in<'a, E: Element + 'a>(
        &mut self,
        own_id: &Id,
        reach: Rectangle<i32, Physical>,
        scale: Scale<f64>,
        scene: impl IntoIterator<Item = &'a E>,
    ) -> bool {
        self.scratch.clear();
        for element in scene {
            let geometry = element.geometry(scale);
            if element.id() == own_id || !geometry.overlaps(reach) {
                continue;
            }
            self.scratch.push(Seen {
                id: element.id().clone(),
                commit: element.current_commit(),
                geometry,
            });
        }
        let changed = self.scratch != self.seen;
        std::mem::swap(&mut self.seen, &mut self.scratch);
        changed
    }
}

/// Persistent per-surface holder: keeps the element and its [`BackdropWatch`] together so
/// the element id stays stable and unchanged frames add no damage.
#[derive(Debug, Default)]
pub struct BlurSlot {
    blur: Option<BackdropBlur>,
    watch: BackdropWatch,
}

impl BlurSlot {
    /// Updates the element with `params` and `scene` (all other scene elements) and returns
    /// a clone to put into the element list at the glass's z position.
    pub fn update<'a, E: Element + 'a>(
        &mut self,
        shared: &SharedBlur,
        params: BackdropBlurParams,
        scale: Scale<f64>,
        scene: impl IntoIterator<Item = &'a E>,
    ) -> BackdropBlur {
        let blur = self
            .blur
            .get_or_insert_with(|| BackdropBlur::new(shared, params.clone()));
        blur.set_params(params);
        if self.watch.changed(blur, scale, scene) {
            blur.invalidate();
        }
        blur.clone()
    }

    /// Forgets the element (e.g. the surface closed); the next update creates a new id.
    pub fn clear(&mut self) {
        self.blur = None;
        self.watch = BackdropWatch::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::backend::renderer::element::solid::SolidColorRenderElement;

    fn solid(id: &Id, commit: CommitCounter, x: i32) -> SolidColorRenderElement {
        let area = Rectangle::new((x, 0).into(), (20, 20).into());
        SolidColorRenderElement::new(id.clone(), area, commit, [1.0; 4], Kind::Unspecified)
    }

    fn reach() -> Rectangle<i32, Physical> {
        Rectangle::new((0, 0).into(), (100, 100).into())
    }

    fn scale() -> Scale<f64> {
        Scale::from(1.0)
    }

    #[test]
    fn first_sight_counts_as_change_then_idle_is_quiet() {
        let (own, id) = (Id::new(), Id::new());
        let mut watch = BackdropWatch::default();
        let scene = [solid(&id, CommitCounter::default(), 10)];
        assert!(watch.changed_in(&own, reach(), scale(), scene.iter()));
        assert!(!watch.changed_in(&own, reach(), scale(), scene.iter()));
    }

    #[test]
    fn commit_move_removal_and_addition_are_changes() {
        let (own, id) = (Id::new(), Id::new());
        let mut watch = BackdropWatch::default();
        let mut commit = CommitCounter::default();
        let base = [solid(&id, commit, 10)];
        watch.changed_in(&own, reach(), scale(), base.iter());
        commit.increment();
        let committed = [solid(&id, commit, 10)];
        assert!(watch.changed_in(&own, reach(), scale(), committed.iter()));
        let moved = [solid(&id, commit, 30)];
        assert!(watch.changed_in(&own, reach(), scale(), moved.iter()));
        let none: [SolidColorRenderElement; 0] = [];
        assert!(watch.changed_in(&own, reach(), scale(), none.iter()));
        assert!(!watch.changed_in(&own, reach(), scale(), none.iter()));
        assert!(watch.changed_in(&own, reach(), scale(), moved.iter()));
    }

    #[test]
    fn far_away_and_own_elements_are_ignored() {
        let (own, id) = (Id::new(), Id::new());
        let mut watch = BackdropWatch::default();
        let mut commit = CommitCounter::default();
        let scene = |commit| [solid(&id, commit, 500), solid(&own, commit, 10)];
        watch.changed_in(&own, reach(), scale(), scene(commit).iter());
        commit.increment();
        assert!(!watch.changed_in(&own, reach(), scale(), scene(commit).iter()));
    }
}
