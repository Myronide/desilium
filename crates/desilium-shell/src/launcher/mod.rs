//! The launcher surface: a centred overlay panel with exclusive keyboard focus.
//!
//! It is created once, kept unmapped while closed and keeps its document in memory,
//! so opening only fills the fields and maps the surface.

mod results;
mod view;

use desilium_theme::Theme;
use desilium_ui::{
    Key, KeyEvent, SurfaceOptions, UiError, UiSurface,
    wayland::{Anchor, KeyboardMode, Layer, LayerConfig, LayerHost, SurfaceId},
};

use crate::{
    model::launcher::{LauncherModel, Results},
    slot::Slot,
    view::{BASE_CSS, LAUNCHER_CSS},
};
pub use results::{Context, compute};
pub use view::{PinView, RowIcons};

/// Namespace of the launcher's layer surface.
pub const NAMESPACE: &str = "desilium-launcher";
/// Panel width in logical pixels.
pub const WIDTH: u32 = 800;
/// Panel height before the first measurement.
const PLACEHOLDER_HEIGHT: u32 = 600;
/// Layer-shell value that places a surface regardless of other surfaces' exclusive zones.
const IGNORE_EXCLUSIVE_ZONES: i32 = -1;
/// Vertical position of the panel's top edge, as a share of the output height.
const TOP_SHARE_PERCENT: i32 = 15;
/// Top margin when the output size is unknown.
const FALLBACK_TOP: i32 = 120;

/// Inputs the launcher needs to draw itself.
pub struct Parts<'a> {
    /// Desktop ids behind `Alt+1..6`.
    pub pinned: &'a [String],
    /// The pinned strip.
    pub pins: &'a [PinView],
    /// Icon resolution for result rows.
    pub icons: &'a mut dyn RowIcons,
}

/// Launcher surface plus its state machine.
pub struct Launcher {
    slot: Slot,
    /// Search text, category, results and selection.
    pub model: LauncherModel,
    height: u32,
    top: i32,
}

fn log_failure(result: Option<Result<(), UiError>>) {
    if let Some(Err(error)) = result {
        tracing::warn!(%error, "launcher update failed");
    }
}

impl Launcher {
    /// Builds the document; the surface is created on the compositor when opened.
    pub fn new(theme: &Theme) -> Result<Self, UiError> {
        let mut options = SurfaceOptions::new(WIDTH, PLACEHOLDER_HEIGHT);
        options.theme = theme.clone();
        let ui = UiSurface::new("<div id=root></div>", &[BASE_CSS, LAUNCHER_CSS], options)?;
        let mut config = LayerConfig::new(NAMESPACE, (WIDTH, PLACEHOLDER_HEIGHT));
        config.layer = Layer::Overlay;
        // The top margin is measured from the screen edge, not below the bar.
        config.exclusive_zone = IGNORE_EXCLUSIVE_ZONES;
        config.anchor = Anchor {
            top: true,
            ..Anchor::CENTER
        };
        config.keyboard = KeyboardMode::Exclusive;
        config.report_keys = true;
        Ok(Self {
            slot: Slot::parked(config, ui),
            model: LauncherModel::default(),
            height: PLACEHOLDER_HEIGHT,
            top: FALLBACK_TOP,
        })
    }

    /// The layer surface id while open.
    pub fn id(&self) -> Option<SurfaceId> {
        self.slot.id()
    }

    /// True when `id` is the open launcher's surface.
    pub fn owns(&self, id: SurfaceId) -> bool {
        self.slot.owns(id)
    }

    /// True while the panel is on screen.
    pub fn is_open(&self) -> bool {
        self.slot.is_shown()
    }

    /// Distance of the panel from the top of the screen and its height.
    pub fn top_and_height(&self) -> (i32, u32) {
        (self.top, self.height)
    }

    /// Fills the panel with `query` and `results`, then puts it on screen.
    pub fn open(
        &mut self,
        host: &mut LayerHost,
        output_height: Option<i32>,
        query: &str,
        results: Results,
        parts: Parts<'_>,
    ) -> Result<(), UiError> {
        self.model.reset();
        self.model.set_query(query);
        self.model.set_results(results);
        self.top = output_height.map_or(FALLBACK_TOP, |h| h * TOP_SHARE_PERCENT / 100);
        let html = view::launcher_html(&self.model, parts.pins, parts.pinned, parts.icons);
        if let Some(ui) = self.slot.ui_mut(host) {
            ui.replace_children("#root", &html)?;
            // The text editor can only move its caret once layout exists.
            ui.element_rect("#panel")?;
            ui.focus("#q")?;
            // A prefilled query is edited at its end, like in any search field.
            ui.key_down(&KeyEvent::named(Key::End));
        }
        self.fit(host)?;
        self.slot.show(host)?;
        Ok(())
    }

    /// Takes the panel off the screen and forgets the query.
    pub fn close(&mut self, host: &mut LayerHost) {
        self.model.reset();
        self.slot.hide(host);
    }

    /// Replaces the results and chips after a query or category change.
    pub fn show_results(
        &mut self,
        host: &mut LayerHost,
        results: Results,
        parts: Parts<'_>,
    ) -> Result<(), UiError> {
        self.model.set_results(results);
        self.redraw(host, parts)
    }

    /// Redraws chips and results from the current model (selection change).
    pub fn redraw(&mut self, host: &mut LayerHost, parts: Parts<'_>) -> Result<(), UiError> {
        let Some(id) = self.slot.id() else {
            return Ok(());
        };
        let chips = view::chips_html(self.model.category());
        let results = view::results_html(&self.model, parts.pinned, parts.icons);
        log_failure(host.update(id, |ui| {
            ui.replace_children("#chips", &chips)?;
            ui.replace_children("#results", &results)
        }));
        self.fit(host)
    }

    /// Applies a new theme, shown or not.
    pub fn set_theme(&mut self, host: &mut LayerHost, theme: &Theme) {
        if let Some(ui) = self.slot.ui_mut(host) {
            ui.set_theme(theme);
        }
        if let Err(error) = host.render_pending() {
            tracing::warn!(%error, "launcher theme redraw failed");
        }
    }

    /// Resizes the surface to the panel's natural height.
    fn fit(&mut self, host: &mut LayerHost) -> Result<(), UiError> {
        let measured = match self.slot.ui_mut(host) {
            Some(ui) => Some(ui.element_rect("#panel")?),
            None => None,
        };
        let height = measured.map_or(self.height, |rect| rect.height.ceil() as u32);
        if let Some(ui) = self.slot.ui_mut(host) {
            ui.resize(WIDTH, height)?;
        }
        let top = self.top;
        self.slot.reconfigure(host, |config| {
            config.size = (WIDTH, height);
            config.margins.top = top;
        })?;
        self.height = height;
        Ok(())
    }
}

/// Launcher markup for snapshot tests.
#[cfg(test)]
pub fn launcher_html_for_tests(
    model: &LauncherModel,
    pins: &[PinView],
    icons: &mut dyn RowIcons,
) -> String {
    let pinned: Vec<String> = Vec::new();
    view::launcher_html(model, pins, &pinned, icons)
}
