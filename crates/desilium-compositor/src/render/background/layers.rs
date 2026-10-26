//! The wallpaper layers on screen and the transition between them.

use super::fade::Fade;
use super::request::CacheKey;
use super::worker::Rendered;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::{Buffer, Rectangle, Transform};
use std::time::Duration;

const BUFFER_SCALE: i32 = 1;

/// A rendered wallpaper uploaded (lazily) as a texture, with the key it was made for.
pub struct Layer {
    pub key: CacheKey,
    pub buffer: MemoryRenderBuffer,
}

impl Layer {
    /// Wraps a finished render; returns `None` when its size does not fit the buffer API.
    pub fn new(key: CacheKey, rendered: Rendered) -> Option<Layer> {
        let size = (
            i32::try_from(rendered.size.w).ok()?,
            i32::try_from(rendered.size.h).ok()?,
        );
        // Letterbox bars are opaque, so the whole texture can be declared opaque.
        let opaque = Rectangle::<i32, Buffer>::from_size(size.into());
        let buffer = MemoryRenderBuffer::from_slice(
            &rendered.rgba,
            Fourcc::Abgr8888,
            size,
            BUFFER_SCALE,
            Transform::Normal,
            Some(vec![opaque]),
        );
        Some(Layer { key, buffer })
    }
}

/// A crossfade from one layer (or the gradient) to another (or the gradient).
pub struct Transition {
    pub from: Option<Layer>,
    pub to: Option<Layer>,
    pub fade: Fade,
}

/// Opacity of each layer for one frame; `None` means the layer is not drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alphas {
    pub from: Option<f32>,
    pub to: Option<f32>,
}

impl Transition {
    /// Opacities at `now`. The outgoing wallpaper stays fully opaque under an incoming one, and
    /// fades itself only when the target is the bare gradient.
    pub fn alphas(&self, now: Duration) -> Alphas {
        let progress = self.fade.progress(now);
        Alphas {
            from: self.from.as_ref().map(|_| match self.to {
                Some(_) => 1.0,
                None => 1.0 - progress,
            }),
            to: self.to.as_ref().map(|_| progress),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::background::fade::FADE_DURATION;
    use desilium_wallpaper::{FitMode, Size};
    use std::path::PathBuf;

    const START: Duration = Duration::from_secs(1);

    fn layer() -> Layer {
        let key = CacheKey {
            path: PathBuf::from("/x"),
            mtime: None,
            mode: FitMode::Fill,
            size: Size::new(2, 2),
            scale_milli: 1000,
        };
        let rendered = Rendered {
            size: Size::new(2, 2),
            rgba: vec![255; 16],
        };
        Layer::new(key, rendered).unwrap()
    }

    fn transition(from: bool, to: bool) -> Transition {
        Transition {
            from: from.then(layer),
            to: to.then(layer),
            fade: Fade::start(START, true),
        }
    }

    #[test]
    fn gradient_to_wallpaper_fades_the_wallpaper_in() {
        let t = transition(false, true);
        assert_eq!(
            t.alphas(START),
            Alphas {
                from: None,
                to: Some(0.0)
            }
        );
        assert_eq!(t.alphas(START + FADE_DURATION).to, Some(1.0));
    }

    #[test]
    fn wallpaper_to_wallpaper_keeps_the_old_one_opaque_underneath() {
        let alphas = transition(true, true).alphas(START + FADE_DURATION / 2);
        assert_eq!(alphas.from, Some(1.0));
        assert!(alphas.to.is_some_and(|alpha| alpha > 0.4 && alpha < 0.6));
    }

    #[test]
    fn wallpaper_to_gradient_fades_the_old_one_out() {
        let t = transition(true, false);
        assert_eq!(t.alphas(START).from, Some(1.0));
        assert_eq!(t.alphas(START + FADE_DURATION).from, Some(0.0));
    }
}
