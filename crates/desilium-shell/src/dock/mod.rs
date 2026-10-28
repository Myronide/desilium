//! The dock surface: a floating glass tile that is exactly as wide as its content.

pub(crate) mod layout;
mod view;

use desilium_theme::Theme;
use desilium_ui::{
    Rect, SurfaceOptions, UiError, UiSurface,
    wayland::{Anchor, LayerConfig, LayerHost, SurfaceId},
};

use crate::{
    view::{BASE_CSS, DOCK_CSS},
    visibility::{Presence, bar_inset, panel_zone, reserves_space},
};
pub(crate) use layout::{BOTTOM_MARGIN, DockLayout, HEIGHT};
pub(crate) use view::{DockCell, DockView, Status, panel_radius};

/// Namespace of the dock's layer surface.
pub(crate) const NAMESPACE: &str = "desilium-dock";

/// The dock surface and what it shows.
pub(crate) struct Dock {
    id: SurfaceId,
    view: DockView,
    layout: DockLayout,
    settings: desilium_config_types::ShellSettings,
    output_width: u32,
    status: Status,
    dirty: bool,
    presence: Presence,
}

fn layout_for(view: &DockView) -> DockLayout {
    layout::layout(view.cells.len(), view.separator)
}

impl Dock {
    /// Creates the surface showing `view`.
    pub(crate) fn new(
        host: &mut LayerHost,
        theme: &Theme,
        view: DockView,
    ) -> Result<Self, UiError> {
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
            status: Status::default(),
            dirty: false,
            presence: Presence::Shown,
        })
    }

    /// The layer surface id.
    pub(crate) fn id(&self) -> SurfaceId {
        self.id
    }

    /// Current geometry (surface-local pixels).
    pub(crate) fn layout(&self) -> &DockLayout {
        &self.layout
    }

    /// Rect of the launcher button or app cell `index`.
    pub(crate) fn cell_rect(&self, index: usize) -> Option<Rect> {
        self.layout.items.get(index).copied()
    }

    /// Makes the next [`show`](Self::show) draw the dock again even if its content is unchanged.
    pub(crate) fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Read the current scrolled client rectangle and clip it to the app strip.
    pub(crate) fn visible_cell_rect(&self, host: &mut LayerHost, index: usize) -> Option<Rect> {
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

    pub(crate) fn configure(
        &mut self,
        host: &mut LayerHost,
        settings: &desilium_config_types::ShellSettings,
        width: u32,
        status: Status,
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
        self.apply_placement(host)
    }

    /// Moves the dock between full, edge strip and off.
    pub(crate) fn place(
        &mut self,
        host: &mut LayerHost,
        presence: Presence,
    ) -> Result<(), UiError> {
        if presence == self.presence {
            return Ok(());
        }
        self.presence = presence;
        self.apply_placement(host)
    }

    /// Exclusive zone for the current settings, presence and height: reserved space for an always
    /// visible panel, 0 for a taskbar (stacked behind the bar, nothing reserved), -1 for a dock.
    fn zone(&self, full_height: u32) -> i32 {
        let settings = &self.settings;
        if self.presence == Presence::Shown && reserves_space(settings) {
            panel_zone(settings, full_height)
        } else if settings.mode == desilium_config_types::PanelMode::Taskbar {
            0
        } else {
            -1
        }
    }

    /// Anchor, margins, reserved space and height for the current settings and presence.
    fn apply_placement(&mut self, host: &mut LayerHost) -> Result<(), UiError> {
        let settings = &self.settings;
        let top = settings.edge == desilium_config_types::PanelEdge::Top;
        let presence = self.presence;
        // Margin at the attached edge: normal, none for the pointer strip, outside when off.
        let edge_margin = match presence {
            Presence::Shown => i32::from(settings.margin),
            Presence::Peek => 0,
            Presence::Off => -1,
        };
        // A surface with a zone of -1 ignores the bar and is placed against the screen edge, so
        // it keeps clear of the bar by margin; any other zone is stacked behind the bar already.
        let zone = self.zone(self.layout.height);
        let below_bar = if zone < 0 && presence != Presence::Off {
            bar_inset(settings)
        } else {
            0
        };
        let height = presence.height(self.layout.height);
        let width = self.layout.width;
        if let Some(ui) = host.surface_mut(self.id) {
            ui.resize(width, height)?;
        }
        host.reconfigure(self.id, |config| {
            config.anchor = Anchor {
                top,
                bottom: !top,
                left: false,
                right: false,
            };
            config.margins.top = if top { edge_margin + below_bar } else { 0 };
            config.margins.bottom = if top { 0 } else { edge_margin };
            config.exclusive_zone = zone;
            config.size = (width, height);
        })
    }

    /// Shows `view`; returns true when the geometry changed (minimize rectangles need an update).
    pub(crate) fn show(&mut self, host: &mut LayerHost, view: DockView) -> Result<bool, UiError> {
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
            let height = self.presence.height(layout.height);
            host.surface_mut(self.id)
                .map(|ui| ui.resize(layout.width, height))
                .transpose()?;
            let zone = self.zone(layout.height);
            host.reconfigure(self.id, |config| {
                config.size = (layout.width, height);
                config.exclusive_zone = zone;
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
pub(crate) fn dock_html_for_tests(view: &DockView, layout: &DockLayout) -> String {
    view::dock_html(view, layout)
}

#[cfg(test)]
pub(crate) fn configured_html_for_tests(
    view: &DockView,
    layout: &DockLayout,
    settings: &desilium_config_types::ShellSettings,
    status: &Status,
) -> String {
    view::configured_html(view, layout, settings, status)
}
