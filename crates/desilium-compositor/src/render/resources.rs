use super::effects::Effects;
use super::snapshot::Snapshots;
use super::{background, cursor, decoration};
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
}

impl RenderResources {
    fn with_effects(renderer: &mut GlesRenderer, effects: Option<Effects>) -> Self {
        Self {
            effects,
            decoration: decoration::Resources::new(renderer),
            cursor: cursor::Resources::new(renderer),
            background: background::Resources::new(renderer),
            snapshots: Snapshots::default(),
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
