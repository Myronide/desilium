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
    settings: desilium_config_types::ShellSettings,
    output_width: u32,
    status: String,
    dirty: bool,
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
        Ok(Self {
            id,
            view,
            layout,
            settings: Default::default(),
            output_width: 1920,
            status: String::new(),
            dirty: false,
        })
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

    /// Read the current scrolled client rectangle and clip it to the app strip.
    pub fn visible_cell_rect(&self, host: &mut LayerHost, index: usize) -> Option<Rect> {
        let rect = host
            .surface_mut(self.id)?
            .element_rect(&format!("#dock-{index}"))
            .ok()?;
        let end = self
            .layout
            .system
            .map_or(f64::from(self.layout.width), |system| {
                (system.x - f64::from(self.settings.spacing)).max(0.0)
            });
        let x = rect.x.max(0.0);
        let right = (rect.x + rect.width).min(end);
        let y = rect.y.max(0.0);
        let bottom = (rect.y + rect.height).min(f64::from(self.layout.height));
        (right > x && bottom > y).then_some(Rect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }

    pub fn configure(
        &mut self,
        host: &mut LayerHost,
        settings: &desilium_config_types::ShellSettings,
        width: u32,
        status: String,
    ) -> Result<(), UiError> {
        let layout_changed = self.settings != *settings || self.output_width != width;
        self.dirty |= layout_changed || self.status != status;
        if !layout_changed {
            self.status = status;
            return Ok(());
        }
        self.settings = settings.clone();
        self.output_width = width;
        self.status = status;
        let top = settings.edge == desilium_config_types::PanelEdge::Top;
        host.reconfigure(self.id, |config| {
            config.anchor = Anchor {
                top,
                bottom: !top,
                left: false,
                right: false,
            };
            config.margins.top = if top {
                i32::from(settings.margin)
                    + if settings.mode == desilium_config_types::PanelMode::Dock {
                        crate::bar::BAR_HEIGHT as i32
                    } else {
                        0
                    }
            } else {
                0
            };
            config.margins.bottom = if top { 0 } else { i32::from(settings.margin) };
            config.exclusive_zone = if settings.mode == desilium_config_types::PanelMode::Taskbar {
                i32::from(settings.icon_size) + 16
            } else {
                -1
            };
        })
    }

    /// Shows `view`; returns true when the geometry changed (minimize rectangles need an update).
    pub fn show(&mut self, host: &mut LayerHost, view: DockView) -> Result<bool, UiError> {
        if view == self.view && !self.dirty {
            return Ok(false);
        }
        let layout = layout::configured(
            view.cells.len(),
            view.separator,
            &self.settings,
            self.output_width,
        );
        let html = view::configured_html(&view, &layout, &self.settings, &self.status);
        let resized = layout.width != self.layout.width || layout.height != self.layout.height;
        self.dirty = false;
        let moved = layout != self.layout;
        if resized {
            host.surface_mut(self.id)
                .map(|ui| ui.resize(layout.width, layout.height))
                .transpose()?;
            host.reconfigure(self.id, |config| {
                config.size = (layout.width, layout.height)
            })?;
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

#[cfg(test)]
pub fn configured_html_for_tests(
    view: &DockView,
    layout: &DockLayout,
    settings: &desilium_config_types::ShellSettings,
    status: &str,
) -> String {
    view::configured_html(view, layout, settings, status)
}
