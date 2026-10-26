//! Element wrappers used while a window is being transformed by an animation.

use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::utils::{Buffer, Physical, Point, Rectangle, Scale, Transform};

/// Reports no opaque regions so the damage tracker never skips what lies below.
///
/// The rounded-surface wrapper computes its translucent corner squares in the coordinates it is
/// drawn in; once a scale wrapper moves it, those squares no longer line up with the opaque
/// regions it reports, so those are dropped while the window animates.
#[derive(Debug)]
pub struct NoOpaque<E>(pub E);

impl<E: Element> Element for NoOpaque<E> {
    fn id(&self) -> &Id {
        self.0.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.0.current_commit()
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        self.0.location(scale)
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.0.src()
    }

    fn transform(&self) -> Transform {
        self.0.transform()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.0.geometry(scale)
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.0.damage_since(scale, commit)
    }

    fn opaque_regions(&self, _scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        OpaqueRegions::default()
    }

    fn alpha(&self) -> f32 {
        self.0.alpha()
    }

    fn kind(&self) -> Kind {
        self.0.kind()
    }
}

impl<E: RenderElement<GlesRenderer>> RenderElement<GlesRenderer> for NoOpaque<E> {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        self.0.draw(frame, src, dst, damage, opaque_regions)
    }

    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        self.0.underlying_storage(renderer)
    }
}
