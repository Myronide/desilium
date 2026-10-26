//! The application controller: turns document events into configuration changes and view updates.
//!
//! No Wayland in here. `App` owns the store and the view state; `Refresh` values
//! tell the caller which parts of the document to rebuild. That keeps the whole
//! behaviour testable without a display.

#[cfg(test)]
mod behaviour_tests;
mod dispatch;
mod keys;
mod rebind;
mod sync;
mod thumbs;

use std::{path::PathBuf, time::Instant};

use desilium_config::ConfigStore;
use desilium_theme::Theme;

use crate::{
    changes::{ChangeError, Recovery},
    model::{Level, Model, Notice},
    registry::{Page, Value},
    theme::resolve_theme,
    view::Ctx,
    wallpapers::{Env, ThumbWorker, cache_dir, discover, search_dirs},
};

/// How to start the app.
#[derive(Debug, Clone)]
pub struct AppOptions {
    /// Page to show first.
    pub page: Page,
    /// Search text to show first.
    pub query: Option<String>,
    /// Directory of the configuration store (for the About page).
    pub config_dir: PathBuf,
    /// Wallpaper search environment.
    pub wallpaper_env: Env,
    /// Thumbnail cache directory.
    pub cache_dir: PathBuf,
    /// Cursor themes to offer.
    pub cursor_themes: Vec<String>,
}

impl AppOptions {
    /// Options derived from the process environment.
    pub fn from_environment(config_dir: PathBuf) -> Self {
        Self {
            page: Page::Appearance,
            query: None,
            config_dir,
            wallpaper_env: Env::from_process(),
            cache_dir: cache_dir(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME")),
            cursor_themes: Vec::new(),
        }
    }
}

/// A failed change that "Try again" can repeat.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Retry {
    pub(crate) setting: &'static str,
    pub(crate) value: Value,
}

/// Controller state.
pub struct App {
    store: ConfigStore,
    model: Model,
    theme: Theme,
    cache: PathBuf,
    worker: Option<ThumbWorker>,
    pending_thumbs: Vec<String>,
    retry: Option<Retry>,
}

impl App {
    /// Builds the app around an opened store. `worker` renders thumbnails in the background;
    /// without one they are rendered synchronously (tests, screenshots).
    pub fn new(store: ConfigStore, options: AppOptions, worker: Option<ThumbWorker>) -> Self {
        let dirs = search_dirs(&options.wallpaper_env);
        let mut model = Model::new(options.page);
        model.query = options.query.unwrap_or_default();
        model.wallpapers = discover(&dirs);
        model.cursor_themes = options.cursor_themes;
        model.info.version = env!("CARGO_PKG_VERSION").to_owned();
        model.info.config_file = options.config_dir.join("config.toml").display().to_string();
        model.info.wallpaper_dirs = dirs.iter().map(|dir| dir.display().to_string()).collect();
        let theme = resolve_theme(&store.get().appearance);
        let mut app = Self {
            store,
            model,
            theme,
            cache: options.cache_dir,
            worker,
            pending_thumbs: Vec::new(),
            retry: None,
        };
        app.request_thumbs();
        app
    }

    /// The store, for the event loop's file watcher.
    pub fn store_mut(&mut self) -> &mut ConfigStore {
        &mut self.store
    }

    /// The theme the window wears.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// View state (read-only).
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// View state, for the window layer (size class, search text).
    pub fn model_mut(&mut self) -> &mut Model {
        &mut self.model
    }

    /// Everything the view reads.
    pub fn ctx(&self, now: Instant) -> Ctx<'_> {
        Ctx {
            model: &self.model,
            config: self.store.get(),
            theme: &self.theme,
            now,
        }
    }

    /// True while a shortcut is being captured: keys must reach the app, not the document.
    pub fn wants_key_capture(&self) -> bool {
        self.model.rebind.is_some()
    }

    /// The earliest moment [`tick`](Self::tick) has work to do.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.model.next_deadline()
    }

    /// Shows a banner above the page (start-up repairs, for example).
    pub fn show_notice(&mut self, level: Level, text: String) {
        self.notice(level, text, Some(Recovery::Dismiss));
    }

    pub(crate) fn notice(&mut self, level: Level, text: String, action: Option<Recovery>) {
        self.model.notice = Some(Notice {
            level,
            text,
            action,
        });
    }

    pub(crate) fn fail(&mut self, error: &ChangeError, retry: Option<Retry>) {
        let failure = crate::changes::friendly(error);
        self.retry = retry.filter(|_| failure.recovery == Recovery::Retry);
        self.notice(Level::Error, failure.message, Some(failure.recovery));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An app over an empty temporary config directory (no wallpapers, no worker).
    pub(crate) fn app_in() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ConfigStore::open(dir.path().join("config")).store;
        let mut options = AppOptions::from_environment(dir.path().join("config"));
        options.cache_dir = dir.path().join("cache");
        options.wallpaper_env = Env {
            override_dir: Some(dir.path().join("wallpapers").into_os_string()),
            ..Env::default()
        };
        let app = App::new(store, options, None);
        (dir, app)
    }
}
