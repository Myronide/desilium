//! The components a Desilium session starts by default and how their binaries are found.
//!
//! Discovery order: next to the `desilium-session` executable first (installed prefix or
//! `target/` directory), then `PATH`. An explicit flag always wins and must exist.

use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::component::{Criticality, WaylandRole};

/// One component the session knows how to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultComponent {
    /// Name used in logs, plans and command-line flags.
    pub name: &'static str,
    /// Executable file name.
    pub binary: &'static str,
    /// How the session reacts when this component exits.
    pub criticality: Criticality,
    /// How this component relates to the Wayland socket.
    pub wayland: WaylandRole,
    /// A missing required component is a plan error; a missing optional one is skipped.
    pub required: bool,
}

/// Start order: the compositor first, then the clients that need its socket.
pub const DEFAULT_COMPONENTS: [DefaultComponent; 3] = [
    DefaultComponent {
        name: "compositor",
        binary: "desilium-compositor",
        criticality: Criticality::Critical,
        wayland: WaylandRole::Provider,
        required: true,
    },
    DefaultComponent {
        name: "shell",
        binary: "desilium-shell",
        criticality: Criticality::Restartable,
        wayland: WaylandRole::Client,
        required: false,
    },
    DefaultComponent {
        name: "notifications",
        binary: "desilium-notifications",
        criticality: Criticality::Restartable,
        wayland: WaylandRole::Client,
        required: false,
    },
];

/// Where a resolved binary was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundVia {
    /// Given explicitly on the command line.
    Flag,
    /// Next to the session executable.
    Sibling,
    /// Found through `PATH`.
    Path,
}

impl FoundVia {
    /// Lower-case name used in plans and machine-readable output.
    pub fn label(self) -> &'static str {
        match self {
            Self::Flag => "flag",
            Self::Sibling => "sibling",
            Self::Path => "path",
        }
    }
}

/// Looks up a binary by name: next to `exe_dir` first, then in `search_path`.
pub fn find_default(
    binary: &str,
    exe_dir: Option<&Path>,
    search_path: Option<&OsStr>,
) -> Option<(PathBuf, FoundVia)> {
    let sibling = exe_dir
        .map(|dir| dir.join(binary))
        .filter(|candidate| candidate.is_file());
    if let Some(path) = sibling {
        return Some((path, FoundVia::Sibling));
    }
    find_in_path(Path::new(binary), search_path).map(|path| (path, FoundVia::Path))
}

/// Resolves an explicit path: bare names are searched in `search_path`, anything with a directory must exist.
pub fn resolve_explicit(path: &Path, search_path: Option<&OsStr>) -> Option<PathBuf> {
    if path.is_absolute() || path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }
    find_in_path(path, search_path)
}

/// Finds `name` in the directories of `search_path`, in order.
pub fn find_in_path(name: &Path, search_path: Option<&OsStr>) -> Option<PathBuf> {
    search_path
        .into_iter()
        .flat_map(env::split_paths)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::startup::scratch_tests::Scratch;

    #[test]
    fn sibling_wins_over_path() {
        let scratch = Scratch::new("sibling-wins");
        let exe_dir = scratch.path().join("bin");
        let sibling = scratch.touch("bin/desilium-shell");
        let elsewhere = scratch.touch("other/desilium-shell");
        let search = env::join_paths([scratch.path().join("other")]).expect("joinable");

        let found = find_default("desilium-shell", Some(&exe_dir), Some(&search));

        assert_eq!(found, Some((sibling, FoundVia::Sibling)));
        assert!(elsewhere.is_file());
    }

    #[test]
    fn path_is_used_when_no_sibling_exists() {
        let scratch = Scratch::new("path-fallback");
        let exe_dir = scratch.path().join("bin");
        let on_path = scratch.touch("other/desilium-notifications");
        let search = env::join_paths([scratch.path().join("other")]).expect("joinable");

        let found = find_default("desilium-notifications", Some(&exe_dir), Some(&search));

        assert_eq!(found, Some((on_path, FoundVia::Path)));
    }

    #[test]
    fn missing_binary_is_none_without_touching_the_real_path() {
        let scratch = Scratch::new("missing");
        let search = env::join_paths([scratch.path()]).expect("joinable");
        assert_eq!(find_default("desilium-nothing", None, Some(&search)), None);
        assert_eq!(find_default("desilium-nothing", None, None), None);
    }

    #[test]
    fn explicit_bare_name_is_searched_in_the_injected_path() {
        let scratch = Scratch::new("explicit-bare");
        let shell = scratch.touch("tools/my-shell");
        let search = env::join_paths([scratch.path().join("tools")]).expect("joinable");
        assert_eq!(
            resolve_explicit(Path::new("my-shell"), Some(&search)),
            Some(shell)
        );
    }

    #[test]
    fn explicit_path_with_directory_must_exist() {
        let scratch = Scratch::new("explicit-path");
        let existing = scratch.touch("custom/comp");
        assert_eq!(resolve_explicit(&existing, None), Some(existing.clone()));
        assert_eq!(
            resolve_explicit(&scratch.path().join("gone/comp"), None),
            None
        );
    }

    #[test]
    fn default_table_starts_the_compositor_first_and_only_it_is_required() {
        assert_eq!(DEFAULT_COMPONENTS[0].name, "compositor");
        assert_eq!(DEFAULT_COMPONENTS[0].criticality, Criticality::Critical);
        assert_eq!(DEFAULT_COMPONENTS[0].wayland, WaylandRole::Provider);
        assert!(DEFAULT_COMPONENTS[0].required);
        assert!(DEFAULT_COMPONENTS[1..].iter().all(|c| !c.required));
        assert!(
            DEFAULT_COMPONENTS[1..]
                .iter()
                .all(|c| c.wayland == WaylandRole::Client)
        );
    }
}
