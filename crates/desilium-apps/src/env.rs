//! The XDG directories, search path, desktop names and locale that every lookup reads.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::desktop::locale::locale_chain;

/// Desktop name this shell advertises; matched against `OnlyShowIn` and `NotShowIn`.
pub const DESKTOP_NAME: &str = "Desilium";
const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";
const EXECUTABLE_BITS: u32 = 0o111;

/// Snapshot of the XDG directories, executable search path, desktop names and locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    /// Home directory, used for `~/.icons`.
    pub home: PathBuf,
    /// User data directory first, then `XDG_DATA_DIRS` in order.
    pub data_dirs: Vec<PathBuf>,
    /// Executable search path from `PATH`.
    pub path_dirs: Vec<PathBuf>,
    /// Desktop names matched against `OnlyShowIn` and `NotShowIn`.
    pub desktops: Vec<String>,
    /// Locale candidates, most specific first (for example `de_DE`, then `de`).
    pub locales: Vec<String>,
    /// `$XDG_STATE_HOME`, or `~/.local/state`.
    pub state_home: PathBuf,
}

impl Environment {
    /// Reads the environment of the running process.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Builds the environment from an injectable variable lookup; tests use this.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let lookup: &dyn Fn(&str) -> Option<String> = &lookup;
        let home = non_empty(lookup, "HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let data_home =
            absolute_var(lookup, "XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
        let state_home =
            absolute_var(lookup, "XDG_STATE_HOME").unwrap_or_else(|| home.join(".local/state"));
        let system_dirs =
            non_empty(lookup, "XDG_DATA_DIRS").unwrap_or_else(|| DEFAULT_DATA_DIRS.to_owned());

        let mut data_dirs = vec![data_home];
        data_dirs.extend(split_paths(&system_dirs));
        let path_dirs = split_paths(&non_empty(lookup, "PATH").unwrap_or_default());

        let mut desktops: Vec<String> = non_empty(lookup, "XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if !desktops.iter().any(|name| name == DESKTOP_NAME) {
            desktops.push(DESKTOP_NAME.to_owned());
        }

        let locales = locale_chain(
            non_empty(lookup, "LANGUAGE").as_deref(),
            non_empty(lookup, "LC_MESSAGES").as_deref(),
            non_empty(lookup, "LANG").as_deref(),
        );

        Self {
            home,
            data_dirs: dedup_absolute(data_dirs),
            path_dirs,
            desktops,
            locales,
            state_home,
        }
    }

    /// Directories that contain icon themes, user theme directory last.
    pub fn icon_roots(&self) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = self.data_dirs.iter().map(|dir| dir.join("icons")).collect();
        roots.push(self.home.join(".icons"));
        dedup_absolute(roots)
    }

    /// Resolves a program name or absolute path to an executable file.
    pub fn find_program(&self, name: &str) -> Option<PathBuf> {
        if name.is_empty() {
            return None;
        }
        let direct = Path::new(name);
        if direct.is_absolute() {
            return is_executable(direct).then(|| direct.to_path_buf());
        }
        self.path_dirs
            .iter()
            .map(|dir| dir.join(name))
            .find(|candidate| is_executable(candidate))
    }
}

/// Whether `path` is a regular file with at least one execute bit set.
pub fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & EXECUTABLE_BITS != 0)
}

/// Modification time of one watched directory; `None` when it does not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirStamp {
    path: PathBuf,
    modified: Option<SystemTime>,
}

impl DirStamp {
    /// Records the current modification time of `path`.
    pub(crate) fn of(path: &Path) -> Self {
        let modified = fs::metadata(path).and_then(|meta| meta.modified()).ok();
        Self {
            path: path.to_path_buf(),
            modified,
        }
    }

    /// Whether the directory changed (or appeared or vanished) since this stamp was taken.
    pub(crate) fn is_stale(&self) -> bool {
        Self::of(&self.path).modified != self.modified
    }
}

fn non_empty(lookup: &dyn Fn(&str) -> Option<String>, key: &str) -> Option<String> {
    lookup(key).filter(|value| !value.is_empty())
}

fn absolute_var(lookup: &dyn Fn(&str) -> Option<String>, key: &str) -> Option<PathBuf> {
    non_empty(lookup, key)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn split_paths(value: &str) -> Vec<PathBuf> {
    value
        .split(':')
        .filter(|part| !part.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Drops relative paths and repeats while keeping the first occurrence order.
fn dedup_absolute(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique: Vec<PathBuf> = Vec::with_capacity(paths.len());
    for path in paths {
        if path.is_absolute() && !unique.contains(&path) {
            unique.push(path);
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup_from(
        pairs: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> Option<String> {
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn data_dirs_put_user_directory_first() {
        let env = Environment::from_lookup(lookup_from(&[
            ("HOME", "/home/a"),
            ("XDG_DATA_DIRS", "/opt/share:/usr/share"),
        ]));
        assert_eq!(env.data_dirs[0], PathBuf::from("/home/a/.local/share"));
        assert_eq!(env.data_dirs[1], PathBuf::from("/opt/share"));
        assert_eq!(env.data_dirs[2], PathBuf::from("/usr/share"));
        assert_eq!(env.data_dirs.len(), 3);
    }

    #[test]
    fn relative_xdg_overrides_are_ignored() {
        let env = Environment::from_lookup(lookup_from(&[
            ("HOME", "/home/a"),
            ("XDG_DATA_HOME", "relative/share"),
            ("XDG_STATE_HOME", "/state"),
        ]));
        assert_eq!(env.data_dirs[0], PathBuf::from("/home/a/.local/share"));
        assert_eq!(env.state_home, PathBuf::from("/state"));
    }

    #[test]
    fn desktop_names_include_the_shell_name_once() {
        let env =
            Environment::from_lookup(lookup_from(&[("XDG_CURRENT_DESKTOP", "GNOME:Desilium")]));
        assert_eq!(
            env.desktops,
            vec!["GNOME".to_owned(), "Desilium".to_owned()]
        );
    }

    #[test]
    fn missing_variables_yield_defaults() {
        let env = Environment::from_lookup(lookup_from(&[]));
        assert_eq!(env.desktops, vec!["Desilium".to_owned()]);
        assert!(env.path_dirs.is_empty());
        assert!(
            env.data_dirs
                .iter()
                .any(|dir| dir == Path::new("/usr/share"))
        );
    }

    #[test]
    fn find_program_checks_executable_bit() {
        let dir = crate::testutil::TempDir::new("env-find");
        let plain = dir.write("plain", "data");
        let tool = dir.write("tool", "data");
        let _ = fs::set_permissions(&tool, fs::Permissions::from_mode(0o755));
        let _ = fs::set_permissions(&plain, fs::Permissions::from_mode(0o644));
        let mut env = Environment::from_lookup(lookup_from(&[]));
        env.path_dirs = vec![dir.path().to_path_buf()];
        assert_eq!(env.find_program("tool"), Some(tool));
        assert_eq!(env.find_program("plain"), None);
        assert_eq!(env.find_program("missing"), None);
        assert_eq!(env.find_program(""), None);
    }

    #[test]
    fn dir_stamp_detects_new_entries() {
        let dir = crate::testutil::TempDir::new("env-stamp");
        crate::testutil::age_dir(dir.path());
        let stamp = DirStamp::of(dir.path());
        assert!(!stamp.is_stale());
        dir.write("new.desktop", "x");
        assert!(stamp.is_stale());
    }
}
