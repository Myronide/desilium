//! Directory walk, desktop file id derivation and XDG precedence for the catalog.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use tracing::{debug, warn};

use super::Item;
use crate::desktop::ini;
use crate::desktop::{DesktopEntry, DesktopId};
use crate::env::{DirStamp, Environment};

const APPLICATIONS_DIR: &str = "applications";
/// Sub-directories are followed this deep below an `applications` root.
const MAX_WALK_DEPTH: usize = 4;
/// Upper bound on desktop files considered per scan, against pathological trees.
const MAX_DESKTOP_FILES: usize = 4096;

/// Result of one full scan.
pub(crate) struct Scanned {
    pub(crate) items: BTreeMap<String, Item>,
    pub(crate) stamps: Vec<DirStamp>,
}

/// Walks every data directory in precedence order. The first file that yields a given
/// id shadows all later ones, even when it is hidden or fails to parse.
pub(crate) fn scan(env: &Environment) -> Scanned {
    let mut stamps = Vec::new();
    let mut items = BTreeMap::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for data_dir in &env.data_dirs {
        stamps.push(DirStamp::of(data_dir));
        let root = data_dir.join(APPLICATIONS_DIR);
        let mut found = Vec::new();
        walk(&root, 0, &mut found, &mut stamps);
        for path in found {
            let Some(relative) = path.strip_prefix(&root).ok() else {
                continue;
            };
            let Some(id) = desktop_file_id(relative) else {
                debug!(path = %path.display(), "skipping desktop file with an unusable name");
                continue;
            };
            if !seen.insert(id.as_str().to_owned()) {
                continue;
            }
            admit(env, id, &path, &mut items);
        }
    }
    Scanned { items, stamps }
}

/// Derives the desktop file id from a path relative to an `applications` root:
/// `kde/foo.desktop` becomes `kde-foo.desktop`.
pub(crate) fn desktop_file_id(relative: &Path) -> Option<DesktopId> {
    let parts: Option<Vec<&str>> = relative
        .components()
        .map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect();
    DesktopId::new(&parts?.join("-"))
}

/// Recursively collects `.desktop` files, sorted within each directory, and stamps every
/// directory visited so that changes can be detected later.
fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>, stamps: &mut Vec<DirStamp>) {
    stamps.push(DirStamp::of(dir));
    let read = match fs::read_dir(dir) {
        Ok(read) => read,
        Err(err) => {
            debug!(dir = %dir.display(), %err, "application directory not readable");
            return;
        }
    };
    let mut children: Vec<PathBuf> = read
        .filter_map(|child| match child {
            Ok(child) => Some(child.path()),
            Err(err) => {
                debug!(dir = %dir.display(), %err, "skipping unreadable directory entry");
                None
            }
        })
        .collect();
    children.sort();
    for child in children {
        if found.len() >= MAX_DESKTOP_FILES {
            warn!("application scan stopped at {MAX_DESKTOP_FILES} desktop files");
            return;
        }
        if child.is_dir() {
            if depth < MAX_WALK_DEPTH {
                walk(&child, depth + 1, found, stamps);
            }
        } else if child.extension().is_some_and(|ext| ext == "desktop") {
            found.push(child);
        }
    }
}

/// Parses one winning file and records it unless it is hidden or not an application.
fn admit(env: &Environment, id: DesktopId, path: &Path, items: &mut BTreeMap<String, Item>) {
    let Some(entry) = load_entry(env, &id, path) else {
        return;
    };
    if entry.hidden {
        return;
    }
    let listed = !entry.no_display
        && entry.shown_in(&env.desktops)
        && entry
            .try_exec
            .as_deref()
            .is_none_or(|program| env.find_program(program).is_some());
    items.insert(id.as_str().to_owned(), Item { entry, listed });
}

fn load_entry(env: &Environment, id: &DesktopId, path: &Path) -> Option<DesktopEntry> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            warn!(path = %path.display(), %err, "cannot read desktop file");
            return None;
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    let file = match ini::parse(&text) {
        Ok(file) => file,
        Err(err) => {
            warn!(path = %path.display(), %err, "invalid desktop file");
            return None;
        }
    };
    match DesktopEntry::from_ini(id.clone(), path.to_path_buf(), &file, &env.locales) {
        Ok(entry) => Some(entry),
        Err(err) => {
            debug!(path = %path.display(), %err, "desktop file is not an application");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, env_with_data_dirs};

    #[test]
    fn ids_join_subdirectories_with_dashes() {
        let id = desktop_file_id(Path::new("kde/konsole.desktop")).expect("valid id");
        assert_eq!(id.as_str(), "kde-konsole.desktop");
        assert!(desktop_file_id(Path::new("plain.desktop")).is_some());
        assert!(desktop_file_id(Path::new("notes.txt")).is_none());
        assert!(desktop_file_id(Path::new("../escape.desktop")).is_none());
    }

    const APP: &str = "[Desktop Entry]\nType=Application\nName=X\nExec=x\n";

    #[test]
    fn user_directory_shadows_system_directory() {
        let user = TempDir::new("scan-user");
        let system = TempDir::new("scan-system");
        user.write(
            "applications/app.desktop",
            "[Desktop Entry]\nType=Application\nName=User\nExec=u\n",
        );
        system.write("applications/app.desktop", APP);
        system.write("applications/other.desktop", APP);
        let env = env_with_data_dirs(&[user.path(), system.path()]);
        let scanned = scan(&env);
        let app = scanned.items.get("app.desktop").expect("app present");
        assert_eq!(app.entry.name, "User");
        assert!(scanned.items.contains_key("other.desktop"));
    }

    #[test]
    fn hidden_user_file_deletes_the_system_entry() {
        let user = TempDir::new("scan-hide-user");
        let system = TempDir::new("scan-hide-system");
        user.write(
            "applications/app.desktop",
            "[Desktop Entry]\nType=Application\nName=U\nExec=u\nHidden=true\n",
        );
        system.write("applications/app.desktop", APP);
        let env = env_with_data_dirs(&[user.path(), system.path()]);
        assert!(scan(&env).items.is_empty());
    }

    #[test]
    fn invalid_user_file_still_shadows_system_file() {
        let user = TempDir::new("scan-broken-user");
        let system = TempDir::new("scan-broken-system");
        user.write("applications/app.desktop", "garbage without a group\n");
        system.write("applications/app.desktop", APP);
        let env = env_with_data_dirs(&[user.path(), system.path()]);
        assert!(scan(&env).items.is_empty());
    }

    #[test]
    fn try_exec_and_only_show_in_decide_listing() {
        let dir = TempDir::new("scan-listing");
        dir.write(
            "applications/missing.desktop",
            "[Desktop Entry]\nType=Application\nName=M\nExec=m\nTryExec=no-such-program-xyz\n",
        );
        dir.write(
            "applications/gnome.desktop",
            "[Desktop Entry]\nType=Application\nName=G\nExec=g\nOnlyShowIn=GNOME;\n",
        );
        let env = env_with_data_dirs(&[dir.path()]);
        let scanned = scan(&env);
        assert!(!scanned.items["missing.desktop"].listed);
        assert!(!scanned.items["gnome.desktop"].listed);
    }

    #[test]
    fn subdirectory_files_get_prefixed_ids() {
        let dir = TempDir::new("scan-sub");
        dir.write("applications/kde/konsole.desktop", APP);
        let env = env_with_data_dirs(&[dir.path()]);
        assert!(scan(&env).items.contains_key("kde-konsole.desktop"));
    }
}
