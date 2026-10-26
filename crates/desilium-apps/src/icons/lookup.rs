//! Searches theme directories and the pixmap fallback for one named icon.

use std::path::{Path, PathBuf};

use super::IconSource;
use super::theme::{self, ThemeIndex};

/// Preferred file extensions, best first; SVG scales without a size bucket.
const EXTENSIONS: [&str; 3] = ["svg", "png", "xpm"];
const PIXMAPS_DIR: &str = "/usr/share/pixmaps";
const PIXMAPS_SUBDIR: &str = "pixmaps";

/// First icon whose directory matches the requested size exactly, in theme order.
pub(crate) fn exact(chain: &[ThemeIndex], name: &str, size: u32, scale: u32) -> Option<IconSource> {
    chain
        .iter()
        .flat_map(|theme| theme.directories.iter().map(move |dir| (theme, dir)))
        .filter(|(_, dir)| theme::distance(dir, size, scale) == 0)
        .find_map(|(theme, dir)| file_in_directory(&theme.dir.join(&dir.path), name, dir.size))
}

/// Icon from the directory closest to the requested size; ties keep theme order.
pub(crate) fn closest(
    chain: &[ThemeIndex],
    name: &str,
    size: u32,
    scale: u32,
) -> Option<IconSource> {
    let mut candidates: Vec<(u64, PathBuf, u32)> = chain
        .iter()
        .flat_map(|theme| theme.directories.iter().map(move |dir| (theme, dir)))
        .map(|(theme, dir)| {
            (
                theme::distance(dir, size, scale),
                theme.dir.join(&dir.path),
                dir.size,
            )
        })
        .collect();
    candidates.sort_by_key(|(distance, _, _)| *distance);
    candidates
        .iter()
        .find_map(|(_, directory, nominal)| file_in_directory(directory, name, *nominal))
}

/// Looks for `name` in the pixmap directories of each data dir and in `/usr/share/pixmaps`.
pub(crate) fn pixmaps(data_dirs: &[PathBuf], name: &str) -> Option<IconSource> {
    let mut dirs: Vec<PathBuf> = data_dirs
        .iter()
        .map(|dir| dir.join(PIXMAPS_SUBDIR))
        .collect();
    dirs.push(PathBuf::from(PIXMAPS_DIR));
    dirs.iter()
        .find_map(|directory| file_in_directory(directory, name, 0))
}

/// Resolves a name that is already an absolute path. Missing files yield `None`.
pub(crate) fn absolute(path: &Path) -> IconSource {
    if !path.is_file() {
        return IconSource::None;
    }
    if is_svg(path) {
        return IconSource::Svg(path.to_path_buf());
    }
    IconSource::Raster(path.to_path_buf(), 0)
}

fn file_in_directory(directory: &Path, name: &str, nominal_size: u32) -> Option<IconSource> {
    EXTENSIONS.iter().find_map(|extension| {
        let candidate = directory.join(format!("{name}.{extension}"));
        if !candidate.is_file() {
            return None;
        }
        Some(if *extension == "svg" {
            IconSource::Svg(candidate)
        } else {
            IconSource::Raster(candidate, nominal_size)
        })
    })
}

fn is_svg(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "svg")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, env_with_data_dirs};

    /// Loads the theme `t` from `<fixture>/icons/t` through the real index parser.
    fn chain_for(dir: &TempDir, index: &str) -> Vec<ThemeIndex> {
        dir.write("icons/t/index.theme", index);
        let env = env_with_data_dirs(&[dir.path()]);
        theme::resolve_chain(&env, "t")
    }

    #[test]
    fn exact_match_prefers_svg_then_png() {
        let dir = TempDir::new("icons-exact");
        dir.write("icons/t/48x48/apps/app.png", "x");
        let svg = dir.write("icons/t/48x48/apps/app.svg", "x");
        let chain = chain_for(
            &dir,
            "[Icon Theme]\nDirectories=48x48/apps\n[48x48/apps]\nSize=48\n",
        );
        assert_eq!(exact(&chain, "app", 48, 1), Some(IconSource::Svg(svg)));
    }

    #[test]
    fn closest_is_used_when_no_exact_size_exists() {
        let dir = TempDir::new("icons-closest");
        dir.write("icons/t/16x16/apps/app.png", "x");
        let big = dir.write("icons/t/64x64/apps/app.png", "x");
        let index = "[Icon Theme]\nDirectories=16x16/apps,64x64/apps\n[16x16/apps]\nSize=16\n[64x64/apps]\nSize=64\n";
        let chain = chain_for(&dir, index);
        assert!(exact(&chain, "app", 48, 1).is_none());
        assert_eq!(
            closest(&chain, "app", 48, 1),
            Some(IconSource::Raster(big, 64))
        );
    }

    #[test]
    fn pixmaps_are_the_last_resort() {
        let dir = TempDir::new("icons-pixmap");
        let file = dir.write("pixmaps/legacy.xpm", "x");
        assert_eq!(
            pixmaps(&[dir.path().to_path_buf()], "legacy"),
            Some(IconSource::Raster(file, 0))
        );
        assert_eq!(
            pixmaps(&[dir.path().to_path_buf()], "absent-icon-name"),
            None
        );
    }

    #[test]
    fn absolute_paths_are_used_directly() {
        let dir = TempDir::new("icons-abs");
        let file = dir.write("logo.svg", "<svg/>");
        assert_eq!(absolute(&file), IconSource::Svg(file.clone()));
        assert_eq!(absolute(&dir.path().join("missing.png")), IconSource::None);
    }
}
