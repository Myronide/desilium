use super::effects::Effects;
use super::glass::Glass;
use super::snapshot::Snapshots;
use super::{background, cursor, decoration, popups};
use smithay::backend::renderer::gles::GlesRenderer;
use tracing::{info, warn};

/// GPU-side resources bound to one GL context; create exactly once per renderer.
pub struct RenderResources {
    /// `None` when no effect shader compiled; the scene then falls back to plain rendering.
    pub effects: Option<Effects>,
    #[allow(dead_code)] // filled in by the decoration work
    pub decoration: decoration::Resources,
    #[allow(dead_code)] // filled in by the cursor work
    pub cursor: cursor::Resources,
    #[allow(dead_code)] // filled in by the wallpaper work
    pub background: background::Resources,
    /// Last-frame window snapshots for the close animation.
    pub snapshots: Snapshots,
    /// Backdrop-blur requests of the frame being built and their persistent slots.
    pub glass: Glass,
    /// Cached shadow and border elements of popups.
    pub popups: popups::Resources,
}

impl RenderResources {
    fn with_effects(renderer: &mut GlesRenderer, effects: Option<Effects>) -> Self {
        Self {
            effects,
            decoration: decoration::Resources::new(renderer),
            cursor: cursor::Resources::new(renderer),
            background: background::Resources::new(renderer),
            snapshots: Snapshots::default(),
            glass: Glass::default(),
            popups: popups::Resources::default(),
        }
    }

    pub fn new(renderer: &mut GlesRenderer) -> Self {
        match Effects::compile(renderer) {
            Ok(effects) => {
                for failure in effects.failures() {
                    warn!(kind = ?failure.kind, source = %failure.source, "effect unavailable");
                }
                info!("effects ready");
                Self::with_effects(renderer, Some(effects))
            }
            Err(error) => {
                warn!(%error, "effects unavailable; using plain rendering");
                Self::with_effects(renderer, None)
            }
        }
    }
}
