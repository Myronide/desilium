use super::image::Image;
use crate::error::CompositorError;
use smithay::backend::allocator::Fourcc;
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTarget, GlesTexture};
use smithay::backend::renderer::{
    Bind, Color32F, ExportMem, Frame, Offscreen, Renderer, TextureMapping,
};
use smithay::reexports::gbm::Device as GbmDevice;
use smithay::utils::{Buffer, DeviceFd, Physical, Rectangle, Scale, Size, Transform};
use std::fmt::Display;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

const RENDER_NODE_DIR: &str = "/dev/dri";
const RENDER_NODE_PREFIX: &str = "renderD";
/// Little-endian ABGR8888 is the byte order R, G, B, A in memory.
const READBACK_FORMAT: Fourcc = Fourcc::Abgr8888;

/// A display-less GLES renderer on a DRM render node, for offscreen rendering.
pub struct HeadlessGpu {
    renderer: GlesRenderer,
}

impl HeadlessGpu {
    /// Opens the first render node and creates an EGL/GLES renderer on it.
    pub fn new() -> Result<Self, CompositorError> {
        let node = first_render_node()?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&node)
            .map_err(|e| gpu_error(format!("{}: {e}", node.display())))?;
        let gbm =
            GbmDevice::new(DeviceFd::from(std::os::fd::OwnedFd::from(file))).map_err(gpu_error)?;
        // SAFETY: the GBM device outlives the display (the display keeps a clone).
        let display = unsafe { EGLDisplay::new(gbm) }.map_err(gpu_error)?;
        let context = EGLContext::new(&display).map_err(gpu_error)?;
        // SAFETY: the context is fresh and only ever used from this renderer.
        let renderer = unsafe { GlesRenderer::new(context) }.map_err(gpu_error)?;
        Ok(Self { renderer })
    }

    /// The underlying renderer, e.g. to import textures or compile shaders.
    pub fn renderer(&mut self) -> &mut GlesRenderer {
        &mut self.renderer
    }

    /// Renders `elements` (topmost first) over `clear` and returns the picture.
    pub fn render_to_image<E: RenderElement<GlesRenderer>>(
        &mut self,
        size: Size<i32, Physical>,
        scale: f64,
        elements: &[E],
        clear: [f32; 4],
    ) -> Result<Image, CompositorError> {
        self.render_with(size, |renderer, target| {
            draw_elements(renderer, target, size, scale, elements, clear)
        })
    }

    /// Runs `draw` against a fresh offscreen target and reads the result back.
    pub fn render_with<F>(
        &mut self,
        size: Size<i32, Physical>,
        draw: F,
    ) -> Result<Image, CompositorError>
    where
        F: FnOnce(&mut GlesRenderer, &mut GlesTarget<'_>) -> Result<(), CompositorError>,
    {
        let buffer_size: Size<i32, Buffer> = (size.w, size.h).into();
        let mut texture: GlesTexture = Offscreen::<GlesTexture>::create_buffer(
            &mut self.renderer,
            READBACK_FORMAT,
            buffer_size,
        )
        .map_err(render_error)?;
        let mut target = self.renderer.bind(&mut texture).map_err(render_error)?;
        draw(&mut self.renderer, &mut target)?;
        let region = Rectangle::from_size(buffer_size);
        let mapping = self
            .renderer
            .copy_framebuffer(&target, region, READBACK_FORMAT)
            .map_err(render_error)?;
        // GL reads rows bottom-up unless the mapping says otherwise; images are top-down.
        let needs_flip = !mapping.flipped();
        let bytes = self.renderer.map_texture(&mapping).map_err(render_error)?;
        let width = u32::try_from(size.w).map_err(render_error)?;
        let height = u32::try_from(size.h).map_err(render_error)?;
        Ok(Image::from_premultiplied(
            width,
            height,
            bytes.to_vec(),
            needs_flip,
        ))
    }
}

fn draw_elements<E: RenderElement<GlesRenderer>>(
    renderer: &mut GlesRenderer,
    target: &mut GlesTarget<'_>,
    size: Size<i32, Physical>,
    scale: f64,
    elements: &[E],
    clear: [f32; 4],
) -> Result<(), CompositorError> {
    let scale = Scale::from(scale);
    let full = Rectangle::from_size(size);
    let mut frame = renderer
        .render(target, size, Transform::Normal)
        .map_err(render_error)?;
    frame
        .clear(Color32F::from(clear), &[full])
        .map_err(render_error)?;
    for element in elements.iter().rev() {
        let dst = element.geometry(scale);
        let damage = [Rectangle::from_size(dst.size)];
        element
            .draw(&mut frame, element.src(), dst, &damage, &[])
            .map_err(render_error)?;
    }
    let sync = frame.finish().map_err(render_error)?;
    renderer.wait(&sync).map_err(render_error)
}

fn first_render_node() -> Result<PathBuf, CompositorError> {
    let entries = std::fs::read_dir(RENDER_NODE_DIR)
        .map_err(|e| gpu_error(format!("{RENDER_NODE_DIR}: {e}")))?;
    let mut nodes: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_render_node(path))
        .collect();
    nodes.sort();
    nodes
        .into_iter()
        .next()
        .ok_or_else(|| gpu_error(format!("no {RENDER_NODE_DIR}/{RENDER_NODE_PREFIX}* node")))
}

fn is_render_node(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(RENDER_NODE_PREFIX))
}

fn gpu_error(error: impl Display) -> CompositorError {
    CompositorError::Gpu(error.to_string())
}

fn render_error(error: impl Display) -> CompositorError {
    CompositorError::Render(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::backend::renderer::element::Id;
    use smithay::backend::renderer::element::solid::SolidColorRenderElement;
    use smithay::backend::renderer::utils::CommitCounter;

    fn gpu_or_skip() -> Option<HeadlessGpu> {
        match HeadlessGpu::new() {
            Ok(gpu) => Some(gpu),
            Err(error) => {
                eprintln!("skipping GPU test: {error}");
                None
            }
        }
    }

    #[test]
    fn clear_color_fills_the_image() {
        let Some(mut gpu) = gpu_or_skip() else {
            return;
        };
        let elements: [SolidColorRenderElement; 0] = [];
        let image = gpu
            .render_to_image((8, 4).into(), 1.0, &elements, [1.0, 0.0, 0.0, 1.0])
            .unwrap();
        assert_eq!((image.width, image.height), (8, 4));
        assert_eq!(image.pixel(3, 2), Some([255, 0, 0, 255]));
    }

    #[test]
    fn element_lands_top_left_without_vertical_flip() {
        let Some(mut gpu) = gpu_or_skip() else {
            return;
        };
        let geometry = Rectangle::new((0, 0).into(), (4, 4).into());
        let element = SolidColorRenderElement::new(
            Id::new(),
            geometry,
            CommitCounter::default(),
            [0.0, 1.0, 0.0, 1.0],
            smithay::backend::renderer::element::Kind::Unspecified,
        );
        let image = gpu
            .render_to_image((16, 16).into(), 1.0, &[element], [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        assert_eq!(image.pixel(1, 1), Some([0, 255, 0, 255]));
        assert_eq!(image.pixel(1, 14), Some([0, 0, 0, 255]));
        assert_eq!(image.pixel(14, 1), Some([0, 0, 0, 255]));
    }
}
