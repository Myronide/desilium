//! The dock surface: a floating glass tile that is exactly as wide as its content.

pub(crate) mod layout;
mod view;

use desilium_theme::Theme;
use desilium_ui::{
    Rect, SurfaceOptions, UiError, UiSurface,
    wayland::{Anchor, LayerConfig, LayerHost, SurfaceId},
};

use crate::view::{BASE_CSS, DOCK_CSS};
pub use layout::{BOTTOM_MARGIN, DockLayout, HEIGHT, RADIUS};
pub use view::{DockCell, DockView};

/// Namespace of the dock's layer surface.
pub const NAMESPACE: &str = "desilium-dock";

/// The dock surface and what it shows.
pub struct Dock {
    id: SurfaceId,
    view: DockView,
    layout: DockLayout,
}

fn layout_for(view: &DockView) -> DockLayout {
    layout::layout(view.cells.len(), view.separator)
}

impl Dock {
    /// Creates the surface showing `view`.
    pub fn new(host: &mut LayerHost, theme: &Theme, view: DockView) -> Result<Self, UiError> {
        let layout = layout_for(&view);
        let html = format!("<div id=root>{}</div>", view::dock_html(&view, &layout));
        let mut options = SurfaceOptions::new(layout.width, HEIGHT);
        options.theme = theme.clone();
        let ui = UiSurface::new(&html, &[BASE_CSS, DOCK_CSS], options)?;
        let mut config = LayerConfig::new(NAMESPACE, (layout.width, HEIGHT));
        config.anchor = Anchor::BOTTOM_CENTER;
        config.margins.bottom = BOTTOM_MARGIN;
        let id = host.add_surface(config, ui)?;
        Ok(Self { id, view, layout })
    }

    /// The layer surface id.
    pub fn id(&self) -> SurfaceId {
        self.id
    }

    /// Current geometry (surface-local pixels).
    pub fn layout(&self) -> &DockLayout {
        &self.layout
    }

    /// Rect of the launcher button or app cell `index`.
    pub fn cell_rect(&self, index: usize) -> Option<Rect> {
        self.layout.items.get(index).copied()
    }

    /// Shows `view`; returns true when the geometry changed (minimize rectangles need an update).
    pub fn show(&mut self, host: &mut LayerHost, view: DockView) -> Result<bool, UiError> {
        if view == self.view {
            return Ok(false);
        }
        let layout = layout_for(&view);
        let html = view::dock_html(&view, &layout);
        let resized = layout.width != self.layout.width;
        let moved = layout != self.layout;
        if resized {
            host.surface_mut(self.id)
                .map(|ui| ui.resize(layout.width, HEIGHT))
                .transpose()?;
            host.reconfigure(self.id, |config| config.size = (layout.width, HEIGHT))?;
        }
        if let Some(Err(error)) = host.update(self.id, |ui| ui.replace_children("#root", &html)) {
            tracing::warn!(%error, "dock update failed");
        }
        self.view = view;
        self.layout = layout;
        Ok(moved)
    }
}

/// Dock markup for snapshot tests.
#[cfg(test)]
pub fn dock_html_for_tests(view: &DockView, layout: &DockLayout) -> String {
    view::dock_html(view, layout)
}
