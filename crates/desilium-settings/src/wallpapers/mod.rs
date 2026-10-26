//! Wallpaper discovery and thumbnails.
//!
//! Wallpapers are plain image files found in, in this order of precedence:
//! `$DESILIUM_WALLPAPER_DIR` (development override; when set it replaces the
//! other two), `$XDG_DATA_HOME/desilium/wallpapers` (or `~/.local/share/...`) and
//! `/usr/share/desilium/wallpapers`. Thumbnails are produced off the UI thread
//! and cached under `$XDG_CACHE_HOME/desilium/thumbs`.

mod thumbs;

use std::{
    collections::HashSet,
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

pub use desilium_wallpaper::FitMode;
pub use thumbs::{
    ThumbError, ThumbJob, ThumbResult, ThumbSpec, ThumbWorker, cache_dir, ensure_thumb, lookup,
};

const OVERRIDE_ENV: &str = "DESILIUM_WALLPAPER_DIR";
const SYSTEM_DIR: &str = "/usr/share/desilium/wallpapers";
const USER_SUBDIR: &str = "desilium/wallpapers";
const IMAGE_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

/// An installed wallpaper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallpaperEntry {
    /// Absolute path stored in `appearance.wallpaper`.
    pub path: PathBuf,
    /// Display name: "dusk-violet.jpg" becomes "Dusk violet".
    pub name: String,
}

/// Environment the search directories depend on, injectable for tests.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// `$DESILIUM_WALLPAPER_DIR`.
    pub override_dir: Option<OsString>,
    /// `$XDG_DATA_HOME`.
    pub xdg_data_home: Option<OsString>,
    /// `$HOME`.
    pub home: Option<OsString>,
}

impl Env {
    /// Reads the real environment.
    pub fn from_process() -> Self {
        Self {
            override_dir: env::var_os(OVERRIDE_ENV),
            xdg_data_home: env::var_os("XDG_DATA_HOME"),
            home: env::var_os("HOME"),
        }
    }
}

fn non_empty(value: &Option<OsString>) -> Option<&OsString> {
    value.as_ref().filter(|value| !value.is_empty())
}

/// Directories to scan, highest precedence first.
pub fn search_dirs(env: &Env) -> Vec<PathBuf> {
    if let Some(dir) = non_empty(&env.override_dir) {
        return vec![PathBuf::from(dir)];
    }
    let user_base = match (non_empty(&env.xdg_data_home), non_empty(&env.home)) {
        (Some(xdg), _) => Some(PathBuf::from(xdg)),
        (None, Some(home)) => Some(PathBuf::from(home).join(".local/share")),
        (None, None) => None,
    };
    user_base
        .into_iter()
        .map(|base| base.join(USER_SUBDIR))
        .chain([PathBuf::from(SYSTEM_DIR)])
        .collect()
}

/// "dusk-violet" to "Dusk violet".
pub fn display_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().replace(['-', '_'], " "))
        .unwrap_or_default();
    let mut chars = stem.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(ext))
        })
}

/// All wallpapers in `dirs`, sorted by name. A name found in an earlier directory hides later ones.
pub fn discover(dirs: &[PathBuf]) -> Vec<WallpaperEntry> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for dir in dirs {
        let Ok(read) = fs::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = read
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && is_image(path))
            .collect();
        files.sort();
        for path in files {
            let name = display_name(&path);
            if seen.insert(name.to_lowercase()) {
                entries.push(WallpaperEntry { path, name });
            }
        }
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        fs::write(dir.join(name), b"x").expect("write");
    }

    #[test]
    fn override_replaces_the_default_directories() {
        let env = Env {
            override_dir: Some("/dev/walls".into()),
            home: Some("/home/u".into()),
            ..Env::default()
        };
        assert_eq!(search_dirs(&env), [PathBuf::from("/dev/walls")]);
    }

    #[test]
    fn user_directory_comes_before_the_system_one() {
        let xdg = Env {
            xdg_data_home: Some("/data".into()),
            home: Some("/home/u".into()),
            ..Env::default()
        };
        assert_eq!(
            search_dirs(&xdg),
            [
                PathBuf::from("/data/desilium/wallpapers"),
                PathBuf::from(SYSTEM_DIR)
            ]
        );
        let home_only = Env {
            home: Some("/home/u".into()),
            ..Env::default()
        };
        assert_eq!(
            search_dirs(&home_only)[0],
            PathBuf::from("/home/u/.local/share/desilium/wallpapers")
        );
        assert_eq!(search_dirs(&Env::default()), [PathBuf::from(SYSTEM_DIR)]);
    }

    #[test]
    fn names_are_prettified() {
        assert_eq!(display_name(Path::new("/x/dusk-violet.jpg")), "Dusk violet");
        assert_eq!(display_name(Path::new("night_sky.PNG")), "Night sky");
        assert_eq!(display_name(Path::new("")), "");
    }

    #[test]
    fn discovery_filters_sorts_and_dedupes_by_name() {
        let user = tempfile::tempdir().expect("user");
        let system = tempfile::tempdir().expect("system");
        touch(user.path(), "Zen.jpg");
        touch(user.path(), "aurora.png");
        touch(user.path(), "notes.txt");
        touch(system.path(), "aurora.jpg");
        touch(system.path(), "ember.jpg");
        let found = discover(&[user.path().to_path_buf(), system.path().to_path_buf()]);
        let names: Vec<_> = found.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Aurora", "Ember", "Zen"]);
        assert_eq!(
            found[0].path,
            user.path().join("aurora.png"),
            "user copy wins"
        );
    }

    #[test]
    fn missing_directories_are_skipped() {
        assert!(discover(&[PathBuf::from("/definitely/not/here")]).is_empty());
    }
}
