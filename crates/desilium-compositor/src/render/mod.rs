mod background;
mod cursor;
mod decoration;
pub mod effects;
mod elements;
mod layers;
mod overlay;
mod resources;
mod scene;
mod snapshot;
mod transform;

pub use resources::RenderResources;
pub use scene::SceneContext;

use crate::error::CompositorError;
use smithay::backend::renderer::damage::{OutputDamageTracker, RenderOutputResult};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTarget};
use smithay::backend::winit::WinitGraphicsBackend;
use std::fmt::Display;

/// Shown only where no background element covers the output.
pub const CLEAR_COLOR: [f32; 4] = [0.08, 0.09, 0.12, 1.0];
const FULL_REPAINT: usize = 0;

/// Draws the whole scene into `framebuffer`; the single path shared by every backend.
///
/// `age` is the buffer age (0 repaints everything), so offscreen targets pass 0.
pub fn render_scene<'d>(
    renderer: &mut GlesRenderer,
    framebuffer: &mut GlesTarget<'_>,
    damage_tracker: &'d mut OutputDamageTracker,
    ctx: &SceneContext<'_>,
    age: usize,
) -> Result<RenderOutputResult<'d>, CompositorError> {
    let elements = scene::build(renderer, ctx);
    damage_tracker
        .render_output(renderer, framebuffer, age, &elements, CLEAR_COLOR)
        .map_err(render_error)
}

/// Repaints a full offscreen target (no damage reuse, no presentation).
pub fn render_full(
    renderer: &mut GlesRenderer,
    framebuffer: &mut GlesTarget<'_>,
    damage_tracker: &mut OutputDamageTracker,
    ctx: &SceneContext<'_>,
) -> Result<(), CompositorError> {
    render_scene(renderer, framebuffer, damage_tracker, ctx, FULL_REPAINT).map(drop)
}

/// Renders into the winit window and presents only the damaged area.
pub fn draw(
    backend: &mut WinitGraphicsBackend<GlesRenderer>,
    damage_tracker: &mut OutputDamageTracker,
    ctx: &SceneContext<'_>,
) -> Result<(), CompositorError> {
    let damage = {
        let (renderer, mut framebuffer) = backend.bind().map_err(render_error)?;
        let result = render_scene(
            renderer,
            &mut framebuffer,
            damage_tracker,
            ctx,
            FULL_REPAINT,
        )?;
        result.damage.cloned()
    };
    backend.submit(damage.as_deref()).map_err(render_error)
}

fn render_error(error: impl Display) -> CompositorError {
    CompositorError::Render(error.to_string())
}
