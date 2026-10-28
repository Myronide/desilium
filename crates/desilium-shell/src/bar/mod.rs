//! The top bar surface: workspace pills, active window, clock and status cluster.

mod view;

use desilium_theme::Theme;
use desilium_ui::{
    SurfaceOptions, UiError, UiSurface, Updates,
    wayland::{Anchor, LayerConfig, LayerHost, SurfaceId},
};

use crate::{
    view::{BAR_CSS, BASE_CSS, note_failed},
    visibility::Presence,
};
pub(crate) use view::{ActiveApp, BAR_HEIGHT, BarView, bar_html};

/// Namespace of the bar's layer surface.
pub(crate) const NAMESPACE: &str = "desilium-bar";
/// Initial width before the compositor configures the real one.
const PLACEHOLDER_WIDTH: u32 = 1920;

/// The bar surface and what it currently shows.
pub(crate) struct Bar {
    id: SurfaceId,
    shown: BarView,
    /// Presence, reserved zone and full height as last applied.
    placed: (Presence, bool, u32),
}

impl Bar {
    /// Creates the surface showing `view`.
    pub(crate) fn new(host: &mut LayerHost, theme: &Theme, view: BarView) -> Result<Self, UiError> {
        let mut options = SurfaceOptions::new(PLACEHOLDER_WIDTH, BAR_HEIGHT);
        options.theme = theme.clone();
        let ui = UiSurface::new(&bar_html(&view), &[BASE_CSS, BAR_CSS], options)?;
        let mut config = LayerConfig::new(NAMESPACE, (0, BAR_HEIGHT));
        config.anchor = Anchor::TOP_BAR;
        config.exclusive_zone = i32::try_from(BAR_HEIGHT).unwrap_or(i32::MAX);
        let id = host.add_surface(config, ui)?;
        Ok(Self {
            id,
            shown: view,
            placed: (Presence::Shown, true, BAR_HEIGHT),
        })
    }

    /// The layer surface id.
    pub(crate) fn id(&self) -> SurfaceId {
        self.id
    }

    /// Moves the bar between full, edge strip and off; `reserve` keeps windows out from under it.
    /// `full` is the bar's height when shown.
    pub(crate) fn place(
        &mut self,
        host: &mut LayerHost,
        presence: Presence,
        reserve: bool,
        full: u32,
    ) -> Result<(), UiError> {
        if self.placed == (presence, reserve, full) {
            return Ok(());
        }
        self.placed = (presence, reserve, full);
        let height = presence.height(full);
        let width = host
            .surface(self.id)
            .map_or(PLACEHOLDER_WIDTH, |ui| ui.size().0);
        if let Some(ui) = host.surface_mut(self.id) {
            ui.resize(width, height)?;
        }
        host.reconfigure(self.id, |config| {
            config.size = (0, height);
            config.margins.top = if presence == Presence::Off { -1 } else { 0 };
            config.exclusive_zone = if reserve {
                i32::try_from(full).unwrap_or(i32::MAX)
            } else {
                0
            };
        })
    }

    pub(crate) fn configure(
        &mut self,
        host: &mut LayerHost,
        settings: &desilium_config_types::ShellSettings,
    ) {
        host.update(self.id, |ui| {
            let height = format!("height:{}px", settings.bar_height);
            note_failed(ui.set_attr(".bar", "style", &height), "bar height");
            for (selector, visible) in [
                (".ws-dots", settings.show_workspaces),
                (".bar-app", settings.bar_show_app),
                (".title", settings.bar_show_title),
                ("#date", settings.bar_show_date),
                (".bar-layout", settings.bar_show_layout),
                (".bar-power", settings.bar_show_power),
                ("#clock", settings.show_clock),
                (".bat", settings.show_battery),
                (".tb-group", settings.quick_settings),
            ] {
                let style = if visible { "" } else { "display:none" };
                note_failed(ui.set_attr(selector, "style", style), "bar item");
            }
        });
    }

    /// Updates only the parts of the bar that changed.
    pub(crate) fn show(&mut self, host: &mut LayerHost, view: BarView) {
        if view == self.shown {
            return;
        }
        let old = std::mem::replace(&mut self.shown, view);
        let new = &self.shown;
        let mut batch = Updates::new();
        let left_changed = old.workspaces != new.workspaces || old.app != new.app;
        if left_changed {
            batch = batch.children("#left", &view::left_html(new));
        }
        if old.clock != new.clock {
            batch = batch
                .text("#date", &new.clock.date)
                .text("#time", &new.clock.time);
        }
        if old.battery != new.battery || old.layout != new.layout {
            batch = batch.children("#right", &view::right_html(new));
        }
        if batch.is_empty() {
            return;
        }
        host.update(self.id, |ui| ui.apply(batch));
    }
}
