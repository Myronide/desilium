//! Showing, hiding and dodging of the top bar and the dock.

use std::time::Instant;

use desilium_config_types::Visibility;
use desilium_ui::wayland::SurfaceId;

use super::{Shell, timers::Due};
use crate::{
    bar::BAR_HEIGHT,
    dock::layout::screen_rect,
    visibility::{self, HIDE_DELAY, Presence},
};

/// Which panel the pointer is over.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Hover {
    pub bar: bool,
    pub dock: bool,
}

impl Shell {
    /// The pointer entered or left a surface; panels react to their own surfaces only.
    pub(super) fn panel_pointer(&mut self, surface: SurfaceId, inside: bool) {
        if surface == self.bar.id() {
            self.hover.bar = inside;
        } else if surface == self.dock.id() {
            self.hover.dock = inside;
        } else {
            return;
        }
        if inside {
            self.update_panels();
        } else {
            self.deadlines
                .set(Due::Panels, Instant::now() + HIDE_DELAY);
        }
    }

    /// A popup opened from a panel keeps that panel on screen.
    fn popup_open(&self) -> bool {
        self.quick.is_open() || self.launcher.id().is_some()
    }

    /// Applies the visibility settings to both panels for the current windows and pointer.
    pub(super) fn update_panels(&mut self) {
        let settings = self.store.get().shell.clone();
        let output = self.output_size().unwrap_or((1280, 800));
        let workspace = self.workspaces.active;
        let popup = self.popup_open();

        let bar_area = (0, 0, output.0, BAR_HEIGHT as i32);
        let bar_hidden = visibility::hidden(
            settings.bar_visibility,
            bar_area,
            &self.windows,
            workspace,
            self.hover.bar || popup,
        );
        let bar_presence = Presence::of(settings.bar_enabled, bar_hidden);
        let reserve = bar_presence == Presence::Shown && settings.bar_visibility == Visibility::Always;
        if let Err(error) = self.bar.place(&mut self.host, bar_presence, reserve) {
            tracing::warn!(%error, "top bar placement failed");
        }

        let area = screen_rect(&settings, self.dock.layout(), output);
        let dock_area = (
            area.x as i32,
            area.y as i32,
            area.width as i32,
            area.height as i32,
        );
        let dock_hidden = visibility::hidden(
            settings.panel_visibility,
            dock_area,
            &self.windows,
            workspace,
            self.hover.dock || popup,
        );
        let dock_presence = Presence::of(settings.panel_enabled, dock_hidden);
        if let Err(error) = self.dock.place(&mut self.host, dock_presence) {
            tracing::warn!(%error, "dock placement failed");
        }

        // A popup can close without a pointer event; look again until the panel settles.
        let waiting = popup
            && (bar_presence == Presence::Shown && settings.bar_visibility != Visibility::Always
                || dock_presence == Presence::Shown
                    && settings.panel_visibility != Visibility::Always);
        if waiting {
            self.deadlines
                .set(Due::Panels, Instant::now() + HIDE_DELAY);
        }
    }
}
