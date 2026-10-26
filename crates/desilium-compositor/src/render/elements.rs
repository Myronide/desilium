use super::effects::{BackdropBlur, RoundedSurfaceElement};
use super::transform::NoOpaque;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::render_elements;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::utils::{RelocateRenderElement, RescaleRenderElement};
use smithay::backend::renderer::gles::element::PixelShaderElement;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};

render_elements! {
    /// Everything the compositor can draw; the scene is a list of these, topmost first.
    pub DesiliumElement<=GlesRenderer>;
    /// Client window content clipped to the window shape.
    Surface = RoundedSurfaceElement<WaylandSurfaceRenderElement<GlesRenderer>>,
    /// Client content that is not clipped (popups, windows without the rounded program).
    Plain = WaylandSurfaceRenderElement<GlesRenderer>,
    /// Rounded client content of a window that an animation is scaling and moving.
    Moved = RelocateRenderElement<RescaleRenderElement<NoOpaque<RoundedSurfaceElement<WaylandSurfaceRenderElement<GlesRenderer>>>>>,
    /// Unclipped client content of an animated window.
    MovedPlain = RelocateRenderElement<RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>>,
    /// A window's last frame, fading out while it closes.
    Snapshot = TextureRenderElement<GlesTexture>,
    /// Procedural quads: shadows, frames, backgrounds.
    Shader = PixelShaderElement,
    /// Frosted glass: blurs whatever was drawn below it.
    Glass = BackdropBlur,
    /// CPU-rendered bitmaps uploaded as textures: title text, cursors.
    Memory = MemoryRenderBufferRenderElement<GlesRenderer>,
}
