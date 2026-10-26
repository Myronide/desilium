//! Builds the document for the app.

use std::time::Instant;

use desilium_ui::{SurfaceOptions, UiError, UiSurface};

use crate::{
    app::App,
    model::is_compact,
    view::{SETTINGS_CSS, SHARED_CSS, document},
};

/// Creates the document at `size` (logical pixels) in the app's current theme.
pub fn build_surface(
    app: &mut App,
    size: (u32, u32),
    scale: f64,
    now: Instant,
) -> Result<UiSurface, UiError> {
    app.model_mut().compact = is_compact(size.0);
    let html = document(&app.ctx(now));
    let mut options = SurfaceOptions::new(size.0, size.1);
    options.theme = app.theme().clone();
    options.scale = scale;
    UiSurface::new(&html, &[SHARED_CSS, SETTINGS_CSS], options)
}
