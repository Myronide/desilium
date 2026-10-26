//! Icon theme index parsing, inheritance and directory size matching (icon theme spec 0.13).

use std::path::PathBuf;

use tracing::debug;

use crate::desktop::ini::{self, Group};
use crate::env::Environment;

pub(crate) const HICOLOR: &str = "hicolor";
/// Largest icon size a theme directory may declare.
const MAX_ICON_SIZE: u32 = 1024;
const MAX_SCALE: u32 = 8;
const DEFAULT_THRESHOLD: u32 = 2;
/// Inheritance is followed this deep; real chains are two or three themes long.
const MAX_THEMES: usize = 16;
const ICON_THEME_GROUP: &str = "Icon Theme";

/// How a theme directory's size range is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DirKind {
    /// Only icons of exactly `size` match.
    Fixed,
    /// Any size between `min_size` and `max_size` matches (vector art).
    Scalable,
    /// Sizes within `threshold` of `size` match.
    Threshold,
}

/// One `[directory]` section of an `index.theme`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IconDirectory {
    /// Path relative to the theme root, e.g. `48x48/apps`.
    pub(crate) path: String,
    pub(crate) size: u32,
    pub(crate) scale: u32,
    pub(crate) kind: DirKind,
    pub(crate) min_size: u32,
    pub(crate) max_size: u32,
    pub(crate) threshold: u32,
}

/// A theme found on disk, with its own directories and inheritance list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ThemeIndex {
    pub(crate) name: String,
    pub(crate) dir: PathBuf,
    pub(crate) directories: Vec<IconDirectory>,
    inherits: Vec<String>,
}

impl ThemeIndex {
    /// Directories whose modification times decide whether this theme must be reloaded.
    pub(crate) fn watch_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.dir.clone()];
        paths.extend(self.directories.iter().map(|dir| self.dir.join(&dir.path)));
        paths
    }
}

/// Builds the lookup chain for `theme`: the theme itself, then its inherited themes
/// depth first in `Inherits=` order, then `hicolor` as the implicit last resort.
/// Cycles and missing themes are skipped.
pub(crate) fn resolve_chain(env: &Environment, theme: &str) -> Vec<ThemeIndex> {
    let mut chain = Vec::new();
    collect(env, theme, &mut chain);
    if !chain.iter().any(|loaded| loaded.name == HICOLOR) {
        collect(env, HICOLOR, &mut chain);
    }
    chain
}

fn collect(env: &Environment, name: &str, chain: &mut Vec<ThemeIndex>) {
    if chain.len() >= MAX_THEMES || chain.iter().any(|loaded| loaded.name == name) {
        return;
    }
    let Some(index) = load_theme(env, name) else {
        return;
    };
    let parents = index.inherits.clone();
    chain.push(index);
    for parent in &parents {
        collect(env, parent, chain);
    }
}

fn load_theme(env: &Environment, name: &str) -> Option<ThemeIndex> {
    if name.is_empty() || name.contains(['/', '\0']) || name == ".." {
        return None;
    }
    let roots = env.icon_roots();
    let dir = roots
        .iter()
        .map(|root| root.join(name))
        .find(|candidate| candidate.join("index.theme").is_file())?;
    let text = match std::fs::read_to_string(dir.join("index.theme")) {
        Ok(text) => text,
        Err(err) => {
            debug!(theme = name, %err, "cannot read icon theme index");
            return None;
        }
    };
    let file = match ini::parse(&text) {
        Ok(file) => file,
        Err(err) => {
            debug!(theme = name, %err, "invalid icon theme index");
            return None;
        }
    };
    let header = file.group(ICON_THEME_GROUP)?;
    let directories = header
        .raw("Directories")
        .map(ini::split_csv)
        .unwrap_or_default()
        .iter()
        .filter_map(|path| file.group(path).and_then(directory_from))
        .collect();
    Some(ThemeIndex {
        name: name.to_owned(),
        dir,
        directories,
        inherits: header
            .raw("Inherits")
            .map(ini::split_csv)
            .unwrap_or_default(),
    })
}

fn directory_from(group: &Group) -> Option<IconDirectory> {
    let size = number(group.raw("Size"))?;
    if size == 0 || size > MAX_ICON_SIZE {
        return None;
    }
    let scale = number(group.raw("Scale")).unwrap_or(1).clamp(1, MAX_SCALE);
    let kind = match group.raw("Type").map(str::trim) {
        Some("Fixed") => DirKind::Fixed,
        Some("Scalable") => DirKind::Scalable,
        _ => DirKind::Threshold,
    };
    Some(IconDirectory {
        path: group.name.clone(),
        size,
        scale,
        kind,
        min_size: number(group.raw("MinSize")).unwrap_or(size),
        max_size: number(group.raw("MaxSize")).unwrap_or(size),
        threshold: number(group.raw("Threshold")).unwrap_or(DEFAULT_THRESHOLD),
    })
}

fn number(value: Option<&str>) -> Option<u32> {
    value?.trim().parse().ok()
}

/// Inclusive size range, in logical pixels, that a directory serves without scaling.
fn allowed_range(dir: &IconDirectory) -> (u32, u32) {
    match dir.kind {
        DirKind::Fixed => (dir.size, dir.size),
        DirKind::Scalable => (dir.min_size, dir.max_size),
        DirKind::Threshold => (
            dir.size.saturating_sub(dir.threshold),
            dir.size.saturating_add(dir.threshold),
        ),
    }
}

/// How far a directory is from serving `size` at `scale`, in device pixels. Zero means
/// the directory matches exactly; larger values are worse matches.
pub(crate) fn distance(dir: &IconDirectory, size: u32, scale: u32) -> u64 {
    let wanted = u64::from(size) * u64::from(scale);
    let dir_scale = u64::from(dir.scale);
    if dir.kind == DirKind::Fixed {
        return (u64::from(dir.size) * dir_scale).abs_diff(wanted);
    }
    let (low, high) = allowed_range(dir);
    let low = u64::from(low) * dir_scale;
    let high = u64::from(high) * dir_scale;
    if wanted < low {
        low - wanted
    } else {
        wanted.saturating_sub(high)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(
        kind: DirKind,
        size: u32,
        min: u32,
        max: u32,
        threshold: u32,
        scale: u32,
    ) -> IconDirectory {
        IconDirectory {
            path: String::new(),
            size,
            scale,
            kind,
            min_size: min,
            max_size: max,
            threshold,
        }
    }

    #[test]
    fn fixed_distance_is_absolute_difference() {
        let d = dir(DirKind::Fixed, 48, 48, 48, 2, 1);
        assert_eq!(distance(&d, 48, 1), 0);
        assert_eq!(distance(&d, 24, 1), 24);
        assert_eq!(distance(&d, 24, 2), 0);
    }

    #[test]
    fn threshold_matches_within_two_pixels() {
        let d = dir(DirKind::Threshold, 48, 46, 50, 2, 1);
        assert_eq!(distance(&d, 47, 1), 0);
        assert_eq!(distance(&d, 50, 1), 0);
        assert_eq!(distance(&d, 56, 1), 6);
        assert_eq!(distance(&d, 40, 1), 6);
    }

    #[test]
    fn scalable_matches_its_whole_range() {
        let d = dir(DirKind::Scalable, 64, 8, 512, 2, 1);
        assert_eq!(distance(&d, 256, 1), 0);
        assert_eq!(distance(&d, 4, 1), 4);
        assert_eq!(distance(&d, 1000, 1), 488);
    }

    #[test]
    fn index_parses_directories_and_inheritance() {
        let text = "[Icon Theme]\nName=Demo\nInherits=base,hicolor\nDirectories=48x48/apps,scalable/apps\n\
                    [48x48/apps]\nSize=48\n\
                    [scalable/apps]\nSize=64\nType=Scalable\nMinSize=8\nMaxSize=512\n\
                    [missing]\nNotListed=1\n";
        let file = ini::parse(text).expect("valid index");
        let header = file.group(ICON_THEME_GROUP).expect("header");
        assert_eq!(
            header.raw("Inherits").map(ini::split_csv),
            Some(vec!["base".to_owned(), "hicolor".to_owned()])
        );
        let dirs: Vec<IconDirectory> = ["48x48/apps", "scalable/apps"]
            .iter()
            .filter_map(|path| file.group(path).and_then(directory_from))
            .collect();
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0].kind, DirKind::Threshold);
        assert_eq!(dirs[1].kind, DirKind::Scalable);
        assert_eq!(dirs[1].max_size, 512);
    }

    #[test]
    fn directory_without_size_or_with_absurd_size_is_rejected() {
        let file = ini::parse("[a]\nType=Fixed\n[b]\nSize=99999\n[c]\nSize=0\n").expect("valid");
        assert!(file.group("a").and_then(directory_from).is_none());
        assert!(file.group("b").and_then(directory_from).is_none());
        assert!(file.group("c").and_then(directory_from).is_none());
    }
}
