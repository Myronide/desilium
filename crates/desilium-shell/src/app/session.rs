//! Saving the window layout and putting it back at the next start.

use std::time::Instant;

use super::{Shell, timers::Due};
use crate::services::{
    apps::CatalogLookup,
    session::{self, Restore, SAVE_DELAY},
};

impl Shell {
    /// Runs once, with the first window list: decides whether a saved layout is restored.
    pub(super) fn session_start(&mut self) {
        if self.session_started {
            return;
        }
        self.session_started = true;
        // Windows that exist now belong to a desktop that is already running (the shell was
        // restarted): they are never moved, and nothing is started over them.
        self.session_seen
            .extend(self.windows.windows().iter().map(|window| window.id));
        let wanted = self.store.get().shell.restore_session;
        if wanted && self.proto.is_some() && self.windows.windows().is_empty() {
            self.begin_session_restore();
        }
    }

    fn begin_session_restore(&mut self) {
        let saved = session::load(&self.session_path);
        if saved.is_empty() {
            return;
        }
        let restore = Restore::new(saved, Instant::now());
        let ids: Vec<String> = restore.desktop_ids().map(str::to_owned).collect();
        tracing::info!(windows = ids.len(), "restoring the saved window layout");
        for desktop_id in ids {
            self.start_app(&desktop_id);
        }
        self.restore = Some(restore);
    }

    /// Windows changed: place new ones that a saved window is waiting for, and plan a save.
    pub(super) fn session_windows_changed(&mut self) {
        self.session_start();
        self.place_restored();
        if self.store.get().shell.restore_session && self.restore.is_none() {
            self.deadlines
                .set(Due::Session, Instant::now() + SAVE_DELAY);
        }
    }

    /// Moves newly opened windows to the workspace and rectangle their saved twin had.
    pub(super) fn place_restored(&mut self) {
        let Some(restore) = self.restore.as_mut() else {
            return;
        };
        let mut claimed = Vec::new();
        for window in self.windows.windows() {
            // The app id can arrive after the window; wait for it before judging the window.
            if window.app_id.is_empty() || !self.session_seen.insert(window.id) {
                continue;
            }
            if let Some(saved) = restore.claim(&window.app_id) {
                claimed.push((window.id, saved));
            }
        }
        if let Some(proto) = &self.proto {
            for (id, saved) in &claimed {
                proto.move_to_workspace(*id, saved.workspace);
                proto.set_geometry(*id, (saved.x, saved.y, saved.width, saved.height));
                if saved.minimized {
                    proto.set_minimized(*id, true);
                }
            }
        }
        if restore.is_done(Instant::now()) {
            self.restore = None;
            self.deadlines
                .set(Due::Session, Instant::now() + SAVE_DELAY);
        }
    }

    /// Writes the current layout when the option is on and a restore is not still running.
    pub(super) fn save_session(&mut self) {
        if !self.session_started
            || self.restore.is_some()
            || !self.store.get().shell.restore_session
        {
            return;
        }
        let windows = session::capture(&self.windows, &CatalogLookup(&self.catalog));
        if self.session_saved.as_ref() == Some(&windows) {
            return;
        }
        match session::save(&self.session_path, &windows) {
            Ok(()) => self.session_saved = Some(windows),
            Err(error) => tracing::warn!(%error, "cannot save the window layout"),
        }
    }
}
