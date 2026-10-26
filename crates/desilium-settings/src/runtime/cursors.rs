//! Installed cursor themes: directories under the icon paths that contain a `cursors` folder.

use std::{
    collections::BTreeSet,
    env,
    path::{Path, PathBuf},
};

/// Where icon themes live, most specific first.
pub fn icon_dirs() -> Vec<PathBuf> {
    let home = env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let data_home = env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".local/share")));
    let mut dirs: Vec<PathBuf> = data_home.into_iter().map(|dir| dir.join("icons")).collect();
    dirs.extend(home.map(|home| home.join(".icons")));
    dirs.push(PathBuf::from("/usr/share/icons"));
    dirs
}

/// Names of the cursor themes found in `dirs`, sorted and unique.
pub fn find_themes(dirs: &[PathBuf]) -> Vec<String> {
    let mut names = BTreeSet::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            if has_cursors(&entry.path()) {
                names.insert(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    names.into_iter().collect()
}

fn has_cursors(theme_dir: &Path) -> bool {
    theme_dir.join("cursors").is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_directories_with_cursors_count() {
        let dir = tempfile::tempdir().expect("dir");
        for (theme, cursors) in [("Adwaita", true), ("hicolor", false), ("Bibata", true)] {
            let path = dir.path().join(theme);
            std::fs::create_dir_all(path.join(if cursors { "cursors" } else { "apps" }))
                .expect("mkdir");
        }
        std::fs::write(dir.path().join("loose-file"), b"x").expect("file");
        let found = find_themes(&[dir.path().to_path_buf(), PathBuf::from("/not/there")]);
        assert_eq!(found, ["Adwaita", "Bibata"]);
    }

    #[test]
    fn duplicates_across_directories_collapse() {
        let a = tempfile::tempdir().expect("a");
        let b = tempfile::tempdir().expect("b");
        for dir in [&a, &b] {
            std::fs::create_dir_all(dir.path().join("Adwaita/cursors")).expect("mkdir");
        }
        assert_eq!(
            find_themes(&[a.path().into(), b.path().into()]),
            ["Adwaita"]
        );
    }
}
