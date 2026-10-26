//! Saved window layout: which applications were open, where, and on which workspace.
//!
//! This restores the arrangement of the desktop, not the inside of the programs: the
//! applications are started again and their windows put back. A program that keeps its own
//! state (a browser's tabs, an editor's buffers) has to restore that itself.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::model::{
    dock::AppLookup,
    windows::{WindowInfo, WindowModel},
};

/// File name below `$XDG_STATE_HOME/desilium`.
const FILE_NAME: &str = "session.toml";
/// Most windows kept; a runaway session must not start hundreds of programs.
pub const MAX_WINDOWS: usize = 24;
/// Saved windows that did not show up this long after the start are given up on.
pub const RESTORE_PATIENCE: Duration = Duration::from_secs(60);
/// Quiet time after the last window change before the layout is written.
pub const SAVE_DELAY: Duration = Duration::from_secs(3);

/// One window of the saved layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedWindow {
    /// Wayland app id the window will have again.
    pub app_id: String,
    /// Desktop file that starts the application.
    pub desktop_id: String,
    /// Zero-based workspace.
    pub workspace: u32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub minimized: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    windows: Vec<SavedWindow>,
}

/// Where the layout is stored for the given state home.
pub fn path(state_home: &Path) -> PathBuf {
    state_home.join("desilium").join(FILE_NAME)
}

/// The saved layout; a missing or unreadable file gives an empty one.
pub fn load(path: &Path) -> Vec<SavedWindow> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    match toml::from_str::<File>(&text) {
        Ok(file) => file.windows.into_iter().take(MAX_WINDOWS).collect(),
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "ignoring an unreadable saved session");
            Vec::new()
        }
    }
}

/// Writes the layout atomically (temporary file, then rename).
pub fn save(path: &Path, windows: &[SavedWindow]) -> io::Result<()> {
    let text = toml::to_string_pretty(&File {
        windows: windows.to_vec(),
    })
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("toml.tmp");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    fs::rename(temporary, path)
}

/// The windows worth saving: those whose application can be started again.
pub fn capture(model: &WindowModel, lookup: &dyn AppLookup) -> Vec<SavedWindow> {
    model
        .windows()
        .iter()
        .filter(|window| !window.app_id.is_empty())
        .filter_map(|window| {
            let app = lookup.by_app_id(&window.app_id)?;
            Some(saved(window, app.desktop_id))
        })
        .take(MAX_WINDOWS)
        .collect()
}

fn saved(window: &WindowInfo, desktop_id: String) -> SavedWindow {
    let (x, y, width, height) = window.rect;
    SavedWindow {
        app_id: window.app_id.clone(),
        desktop_id,
        workspace: window.workspace,
        x,
        y,
        width,
        height,
        minimized: window.minimized,
    }
}

/// Saved windows waiting for their application to open a window.
#[derive(Debug)]
pub struct Restore {
    pending: Vec<SavedWindow>,
    started: Instant,
}

impl Restore {
    pub fn new(pending: Vec<SavedWindow>, now: Instant) -> Self {
        Self {
            pending,
            started: now,
        }
    }

    /// The saved window for a new window with this app id, removing it from the waiting list.
    pub fn claim(&mut self, app_id: &str) -> Option<SavedWindow> {
        let index = self
            .pending
            .iter()
            .position(|saved| saved.app_id.eq_ignore_ascii_case(app_id))?;
        Some(self.pending.remove(index))
    }

    /// True once everything was placed or given up on.
    pub fn is_done(&self, now: Instant) -> bool {
        self.pending.is_empty() || now.saturating_duration_since(self.started) > RESTORE_PATIENCE
    }

    /// Desktop ids to start, one per saved window.
    pub fn desktop_ids(&self) -> impl Iterator<Item = &str> {
        self.pending.iter().map(|saved| saved.desktop_id.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        dock::AppInfo,
        windows::{WindowId, WindowInfo},
    };

    struct Apps;

    impl AppLookup for Apps {
        fn by_desktop_id(&self, _: &str) -> Option<AppInfo> {
            None
        }

        fn by_app_id(&self, app_id: &str) -> Option<AppInfo> {
            (app_id == "foot").then(|| AppInfo {
                desktop_id: "foot.desktop".into(),
                name: "Foot".into(),
                icon: None,
                categories: Vec::new(),
            })
        }
    }

    fn window(id: u32, app_id: &str) -> WindowInfo {
        WindowInfo {
            app_id: app_id.into(),
            workspace: 2,
            rect: (10, 20, 300, 200),
            ..WindowInfo::new(WindowId(id))
        }
    }

    #[test]
    fn only_startable_applications_are_captured() {
        let mut model = WindowModel::default();
        model.replace(vec![window(1, "foot"), window(2, "unknown"), window(3, "")]);
        let windows = capture(&model, &Apps);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].desktop_id, "foot.desktop");
        assert_eq!((windows[0].workspace, windows[0].width), (2, 300));
    }

    #[test]
    fn layout_survives_a_round_trip_and_bad_files_are_ignored() {
        let dir = std::env::temp_dir().join(format!("desilium-session-{}", std::process::id()));
        let file = path(&dir);
        let mut model = WindowModel::default();
        model.replace(vec![window(1, "foot")]);
        let windows = capture(&model, &Apps);
        save(&file, &windows).expect("save");
        assert_eq!(load(&file), windows);
        fs::write(&file, "windows = [").expect("corrupt");
        assert!(load(&file).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_saved_window_is_claimed_once_in_order() {
        let mut first = SavedWindow {
            app_id: "foot".into(),
            desktop_id: "foot.desktop".into(),
            workspace: 0,
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            minimized: false,
        };
        let mut second = first.clone();
        second.x = 50;
        first.x = 10;
        let now = Instant::now();
        let mut restore = Restore::new(vec![first, second], now);
        assert_eq!(restore.claim("Foot").map(|saved| saved.x), Some(10));
        assert_eq!(restore.claim("foot").map(|saved| saved.x), Some(50));
        assert!(restore.claim("foot").is_none());
        assert!(restore.is_done(now));
    }

    #[test]
    fn waiting_windows_are_given_up_after_the_patience_runs_out() {
        let now = Instant::now();
        let saved = SavedWindow {
            app_id: "foot".into(),
            desktop_id: "foot.desktop".into(),
            workspace: 0,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            minimized: false,
        };
        let restore = Restore::new(vec![saved], now);
        assert!(!restore.is_done(now));
        assert!(restore.is_done(now + RESTORE_PATIENCE + Duration::from_secs(1)));
    }
}
