//! Everything the view needs besides the configuration itself.

use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::{
    changes::{Recovery, UndoTarget},
    registry::Page,
    wallpapers::{FitMode, WallpaperEntry},
};

/// How long the "Undo" toast stays.
pub const TOAST_LIFETIME: Duration = Duration::from_secs(12);
/// How long a search result stays highlighted after jumping to it.
pub const HIGHLIGHT_LIFETIME: Duration = Duration::from_secs(3);
/// Windows narrower than this switch to the single-column layout.
pub const COMPACT_BELOW: u32 = 1180;

/// The last change, with the way back.
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    /// Sentence shown ("Accent set to Violet").
    pub message: String,
    /// Reverts the change; `None` for messages without undo.
    pub undo: Option<UndoTarget>,
    /// When it disappears.
    pub expires: Instant,
}

/// Severity of a [`Notice`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Something was adjusted for you.
    Info,
    /// A change did not happen.
    Error,
}

/// A banner above the page content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// Severity.
    pub level: Level,
    /// What happened.
    pub text: String,
    /// Offered way out, if any.
    pub action: Option<Recovery>,
}

/// State of the custom hex colour field.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccentEditor {
    /// What the user typed so far.
    pub text: String,
    /// Why it cannot be applied yet.
    pub error: Option<String>,
}

/// The wallpaper preview thumbnail currently available.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewThumb {
    /// Wallpaper it shows.
    pub source: PathBuf,
    /// Fit mode it was rendered with.
    pub mode: FitMode,
    /// Rendered PNG.
    pub png: PathBuf,
}

/// A shortcut being re-bound (see `rebind.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rebind {
    /// Index into `compositor.shortcuts`.
    pub index: usize,
    /// Message shown in the row while waiting (conflicts, hints).
    pub message: Option<String>,
    /// A captured combination waiting for confirmation because it collides.
    pub pending: Option<PendingRebind>,
}

/// A captured shortcut that steals a combination from another action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRebind {
    /// Modifier flags in order alt, ctrl, shift, logo.
    pub modifiers: [bool; 4],
    /// Key name as written to the config.
    pub key: String,
    /// Index of the shortcut that currently owns the combination.
    pub conflict: usize,
}

/// Read-only facts shown on the About page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemInfo {
    /// Application version.
    pub version: String,
    /// Configuration file path.
    pub config_file: String,
    /// Where wallpapers are looked up.
    pub wallpaper_dirs: Vec<String>,
}

/// View state of the app.
#[derive(Debug, Clone)]
pub struct Model {
    /// Page shown when no search is active.
    pub page: Page,
    /// Search field content.
    pub query: String,
    /// Setting to outline after jumping to it.
    pub highlight: Option<(&'static str, Instant)>,
    /// Last change with Undo.
    pub toast: Option<Toast>,
    /// Banner with a repair or an error.
    pub notice: Option<Notice>,
    /// Setting whose dropdown is open.
    pub open_select: Option<&'static str>,
    /// Custom accent field, when open.
    pub accent_editor: Option<AccentEditor>,
    /// Installed wallpapers.
    pub wallpapers: Vec<WallpaperEntry>,
    /// Card thumbnails by wallpaper path.
    pub thumbs: HashMap<PathBuf, PathBuf>,
    /// Large preview thumbnail.
    pub preview: Option<PreviewThumb>,
    /// Installed cursor themes.
    pub cursor_themes: Vec<String>,
    /// Shortcut being re-bound.
    pub rebind: Option<Rebind>,
    /// About page facts.
    pub info: SystemInfo,
    /// Single-column layout.
    pub compact: bool,
}

impl Model {
    /// A fresh model showing `page`.
    pub fn new(page: Page) -> Self {
        Self {
            page,
            query: String::new(),
            highlight: None,
            toast: None,
            notice: None,
            open_select: None,
            accent_editor: None,
            wallpapers: Vec::new(),
            thumbs: HashMap::new(),
            preview: None,
            cursor_themes: Vec::new(),
            rebind: None,
            info: SystemInfo::default(),
            compact: false,
        }
    }

    /// True while the search results replace the page.
    pub fn searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// Id of the highlighted setting if its highlight is still running.
    pub fn highlighted(&self, now: Instant) -> Option<&'static str> {
        self.highlight
            .filter(|(_, until)| *until > now)
            .map(|(id, _)| id)
    }

    /// The earliest moment something in the model expires.
    pub fn next_deadline(&self) -> Option<Instant> {
        let toast = self.toast.as_ref().map(|toast| toast.expires);
        let highlight = self.highlight.map(|(_, until)| until);
        [toast, highlight].into_iter().flatten().min()
    }

    /// Drops everything that has expired by `now`; true if anything changed.
    pub fn expire(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if self
            .toast
            .as_ref()
            .is_some_and(|toast| toast.expires <= now)
        {
            self.toast = None;
            changed = true;
        }
        if self.highlight.is_some_and(|(_, until)| until <= now) {
            self.highlight = None;
            changed = true;
        }
        changed
    }

    /// Closes transient UI (dropdown, colour field) that must not survive a navigation or change.
    pub fn close_popups(&mut self) {
        self.open_select = None;
    }
}

/// Layout class for a window width.
pub fn is_compact(width: u32) -> bool {
    width < COMPACT_BELOW
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_and_highlight_expire_at_their_deadline() {
        let t0 = Instant::now();
        let mut model = Model::new(Page::Appearance);
        model.toast = Some(Toast {
            message: "x".into(),
            undo: None,
            expires: t0 + TOAST_LIFETIME,
        });
        model.highlight = Some(("appearance.accent", t0 + HIGHLIGHT_LIFETIME));
        assert_eq!(model.next_deadline(), Some(t0 + HIGHLIGHT_LIFETIME));
        assert_eq!(model.highlighted(t0), Some("appearance.accent"));
        assert!(!model.expire(t0));
        assert!(model.expire(t0 + HIGHLIGHT_LIFETIME));
        assert!(model.highlight.is_none());
        assert!(model.toast.is_some());
        assert!(model.expire(t0 + TOAST_LIFETIME));
        assert_eq!(model.next_deadline(), None);
    }

    #[test]
    fn blank_query_is_not_searching() {
        let mut model = Model::new(Page::About);
        assert!(!model.searching());
        model.query = "  ".into();
        assert!(!model.searching());
        model.query = "a".into();
        assert!(model.searching());
    }

    #[test]
    fn compact_layout_below_the_threshold() {
        assert!(is_compact(COMPACT_BELOW - 1));
        assert!(!is_compact(COMPACT_BELOW));
    }
}
