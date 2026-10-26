//! The top bar surface: workspace pills, active window, clock and status cluster.

mod view;

use desilium_theme::Theme;
use desilium_ui::{
    SurfaceOptions, UiError, UiSurface, Updates,
    wayland::{Anchor, LayerConfig, LayerHost, SurfaceId},
};

use crate::view::{BAR_CSS, BASE_CSS};
pub use view::{ActiveApp, BAR_HEIGHT, BarView, bar_html};

/// Namespace of the bar's layer surface.
pub const NAMESPACE: &str = "desilium-bar";
/// Initial width before the compositor configures the real one.
const PLACEHOLDER_WIDTH: u32 = 1920;

/// The bar surface and what it currently shows.
pub struct Bar {
    id: SurfaceId,
    shown: BarView,
}

impl Bar {
    /// Creates the surface showing `view`.
    pub fn new(host: &mut LayerHost, theme: &Theme, view: BarView) -> Result<Self, UiError> {
        let mut options = SurfaceOptions::new(PLACEHOLDER_WIDTH, BAR_HEIGHT);
        options.theme = theme.clone();
        let ui = UiSurface::new(&bar_html(&view), &[BASE_CSS, BAR_CSS], options)?;
        let mut config = LayerConfig::new(NAMESPACE, (0, BAR_HEIGHT));
        config.anchor = Anchor::TOP_BAR;
        config.exclusive_zone = i32::try_from(BAR_HEIGHT).unwrap_or(i32::MAX);
        let id = host.add_surface(config, ui)?;
        Ok(Self { id, shown: view })
    }

    /// The layer surface id.
    pub fn id(&self) -> SurfaceId {
        self.id
    }

    pub fn configure(
        &mut self,
        host: &mut LayerHost,
        settings: &desilium_config_types::ShellSettings,
    ) {
        host.update(self.id, |ui| {
            for (selector, visible) in [
                (".ws-dots", settings.show_workspaces),
                ("#clock", settings.show_clock),
                (".bat", settings.show_battery),
                (".tb-group", settings.quick_settings),
            ] {
                let _ = ui.set_attr(selector, "style", if visible { "" } else { "display:none" });
            }
        });
    }

    /// Updates only the parts of the bar that changed.
    pub fn show(&mut self, host: &mut LayerHost, view: BarView) {
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
