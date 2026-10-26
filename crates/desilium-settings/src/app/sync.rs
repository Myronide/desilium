//! Keeping derived state (theme, thumbnails) in step with the configuration.

use super::App;
use crate::{refresh::Refresh, theme::resolve_theme};

impl App {
    /// Recomputes everything derived from the active configuration.
    ///
    /// Called after every change, undo and external reload. Returns the extra
    /// refresh the new state needs (a changed theme must re-derive the tokens).
    pub(super) fn sync(&mut self) -> Refresh {
        let theme = resolve_theme(&self.store.get().appearance);
        let theme_changed = theme != self.theme;
        self.theme = theme;
        self.request_thumbs();
        Refresh {
            theme: theme_changed,
            ..Refresh::default()
        }
    }
}
