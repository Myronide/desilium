//! Freedesktop icon lookup (icon theme spec 0.13) with a bounded, mtime-validated cache.

mod lookup;
mod theme;

use std::collections::HashMap;
use std::path::PathBuf;

use crate::desktop::DesktopEntry;
use crate::env::{DirStamp, Environment};

/// Icon used when a name cannot be resolved.
pub const FALLBACK_ICON: &str = "application-x-executable";
/// Cached lookups kept before the cache is cleared wholesale.
const CACHE_LIMIT: usize = 512;

/// Where a resolved icon lives. Decoding is left to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconSource {
    /// Scalable vector icon.
    Svg(PathBuf),
    /// Raster icon and the nominal size of its directory (`0` when unknown, e.g. pixmaps).
    Raster(PathBuf, u32),
    /// No icon was found.
    None,
}

/// Resolves icon names for one theme, caching results until the theme changes on disk.
#[derive(Debug)]
pub struct IconResolver {
    env: Environment,
    theme: String,
    chain: Vec<theme::ThemeIndex>,
    stamps: Vec<DirStamp>,
    cache: HashMap<(String, u32, u32), IconSource>,
}

impl IconResolver {
    /// Creates a resolver for the configured `theme` (for example `hicolor` or `Papirus`).
    pub fn new(theme: &str, env: &Environment) -> Self {
        let mut resolver = Self {
            env: env.clone(),
            theme: theme.to_owned(),
            chain: Vec::new(),
            stamps: Vec::new(),
            cache: HashMap::new(),
        };
        resolver.reload();
        resolver
    }

    /// Re-reads theme indexes and drops cached results when a theme directory changed.
    /// Returns true when the cache was reset.
    pub fn revalidate(&mut self) -> bool {
        if !self.stamps.iter().any(DirStamp::is_stale) {
            return false;
        }
        self.reload();
        true
    }

    /// Resolves `name` at a logical `size` and integer `scale`, without a fallback.
    /// Absolute paths are accepted as they are.
    pub fn lookup(&mut self, name: &str, size: u32, scale: u32) -> IconSource {
        let key = (name.to_owned(), size, scale);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let found = self.resolve(name, size, scale);
        if self.cache.len() >= CACHE_LIMIT {
            self.cache.clear();
        }
        self.cache.insert(key, found.clone());
        found
    }

    /// Like [`IconResolver::lookup`], but falls back to [`FALLBACK_ICON`] when missing.
    pub fn lookup_or_fallback(&mut self, name: &str, size: u32, scale: u32) -> IconSource {
        match self.lookup(name, size, scale) {
            IconSource::None => self.lookup(FALLBACK_ICON, size, scale),
            found => found,
        }
    }

    /// Icon for a desktop entry; entries without an icon name get the fallback icon.
    pub fn icon_for(&mut self, entry: &DesktopEntry, size: u32, scale: u32) -> IconSource {
        match entry.icon.as_deref() {
            Some(name) => self.lookup_or_fallback(name, size, scale),
            None => self.lookup(FALLBACK_ICON, size, scale),
        }
    }

    fn reload(&mut self) {
        self.chain = theme::resolve_chain(&self.env, &self.theme);
        self.stamps = self
            .chain
            .iter()
            .flat_map(theme::ThemeIndex::watch_paths)
            .map(|path| DirStamp::of(&path))
            .collect();
        self.cache.clear();
    }

    fn resolve(&self, name: &str, size: u32, scale: u32) -> IconSource {
        let path = std::path::Path::new(name);
        if path.is_absolute() {
            return lookup::absolute(path);
        }
        if name.is_empty() || name.contains(['/', '\0']) || name == ".." {
            return IconSource::None;
        }
        lookup::exact(&self.chain, name, size, scale)
            .or_else(|| lookup::closest(&self.chain, name, size, scale))
            .or_else(|| lookup::pixmaps(&self.env.data_dirs, name))
            .unwrap_or(IconSource::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, env_with_data_dirs};

    fn fixture() -> (TempDir, Environment) {
        let dir = TempDir::new("icon-resolver");
        dir.write(
            "icons/base/index.theme",
            "[Icon Theme]\nName=Base\nDirectories=32x32/apps\n[32x32/apps]\nSize=32\n",
        );
        dir.write(
            "icons/demo/index.theme",
            "[Icon Theme]\nName=Demo\nInherits=base,demo\nDirectories=48x48/apps\n[48x48/apps]\nSize=48\n",
        );
        dir.write("icons/demo/48x48/apps/firefox.png", "x");
        dir.write("icons/base/32x32/apps/only-base.svg", "x");
        dir.write(
            "icons/hicolor/index.theme",
            "[Icon Theme]\nName=Hicolor\nDirectories=\n",
        );
        let env = env_with_data_dirs(&[dir.path()]);
        (dir, env)
    }

    #[test]
    fn inherited_icons_are_found_and_cycles_terminate() {
        let (_dir, env) = fixture();
        let mut resolver = IconResolver::new("demo", &env);
        assert!(matches!(
            resolver.lookup("firefox", 48, 1),
            IconSource::Raster(_, 48)
        ));
        assert!(matches!(
            resolver.lookup("only-base", 32, 1),
            IconSource::Svg(_)
        ));
    }

    #[test]
    fn fallback_is_used_for_unknown_names() {
        let (_dir, env) = fixture();
        let mut resolver = IconResolver::new("demo", &env);
        assert_eq!(resolver.lookup("does-not-exist", 48, 1), IconSource::None);
        assert_eq!(
            resolver.lookup_or_fallback("does-not-exist", 48, 1),
            IconSource::None
        );
    }

    #[test]
    fn absolute_path_entries_resolve_without_theme() {
        let (dir, env) = fixture();
        let path = dir.path().join("icons/demo/48x48/apps/firefox.png");
        let mut resolver = IconResolver::new("demo", &env);
        let name = path.to_string_lossy().into_owned();
        assert_eq!(resolver.lookup(&name, 48, 1), IconSource::Raster(path, 0));
    }

    #[test]
    fn revalidate_notices_new_theme_icons() {
        let (dir, env) = fixture();
        let mut resolver = IconResolver::new("demo", &env);
        assert_eq!(resolver.lookup("late", 48, 1), IconSource::None);
        crate::testutil::age_dir(&dir.path().join("icons/demo/48x48/apps"));
        resolver.reload();
        dir.write("icons/demo/48x48/apps/late.png", "x");
        assert!(resolver.revalidate());
        assert!(matches!(
            resolver.lookup("late", 48, 1),
            IconSource::Raster(_, 48)
        ));
    }

    #[test]
    fn invalid_names_never_touch_the_filesystem() {
        let (_dir, env) = fixture();
        let mut resolver = IconResolver::new("demo", &env);
        assert_eq!(resolver.lookup("../etc/passwd", 48, 1), IconSource::None);
        assert_eq!(resolver.lookup("", 48, 1), IconSource::None);
    }
}
