//! Cursor theme lookup: XDG-style search roots, `Inherits=` chains and lazy caching.

use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::builtin::{builtin_cursor, normalized_size};
use crate::file::parse_cursor_file;
use crate::frames::LoadedCursor;
use crate::kind::KIND_COUNT;
use crate::{CursorImage, CursorKind};

const DEFAULT_THEME: &str = "default";
const THEME_ENV: &str = "XCURSOR_THEME";
const PATH_ENV: &str = "XCURSOR_PATH";
const HOME_ENV: &str = "HOME";
const INDEX_FILE: &str = "index.theme";
const CURSORS_DIR: &str = "cursors";
const INHERITS_KEY: &str = "Inherits=";
/// Bounds the inheritance walk so a pathological theme tree cannot run away.
const MAX_CHAIN_LEN: usize = 16;
const SYSTEM_ROOTS: [&str; 2] = ["/usr/share/icons", "/usr/share/pixmaps"];

/// A cursor theme with lazily parsed files; loading never fails.
pub struct CursorTheme {
    size: u32,
    roots: Vec<PathBuf>,
    chain: Vec<String>,
    files: [OnceLock<Option<LoadedCursor>>; KIND_COUNT],
}

impl CursorTheme {
    /// Loads `name`, or `$XCURSOR_THEME`, or "default", searching `$XCURSOR_PATH` and the usual icon dirs.
    pub fn load(name: Option<&str>, size: u32) -> CursorTheme {
        let env_name = env::var(THEME_ENV).ok();
        let resolved = name.or(env_name.as_deref());
        Self::load_from_dirs(resolved, size, &default_search_roots())
    }

    /// Loads a theme from explicit roots; each root holds `<theme>/index.theme` and `<theme>/cursors/`.
    pub fn load_from_dirs(name: Option<&str>, size: u32, roots: &[PathBuf]) -> CursorTheme {
        let root_name = name.filter(|n| !n.is_empty()).unwrap_or(DEFAULT_THEME);
        CursorTheme {
            size: normalized_size(size),
            roots: roots.to_vec(),
            chain: inherit_chain(root_name, roots),
            files: std::array::from_fn(|_| OnceLock::new()),
        }
    }

    /// Frame for `kind` at the nominal size closest to `size * scale`, animated by `elapsed_ms`.
    ///
    /// Returns a reference that stays valid for the life of the theme (or of the process for fallbacks).
    pub fn image(&self, kind: CursorKind, scale: u32, elapsed_ms: u64) -> &CursorImage {
        let target = self.size.saturating_mul(scale.max(1));
        self.loaded(kind)
            .and_then(|cursor| cursor.nearest(target))
            .and_then(|set| set.frame_at(elapsed_ms))
            .unwrap_or_else(|| builtin_image(kind, target))
    }

    /// Whether the file backing `kind` has more than one frame at some nominal size.
    pub fn is_animated(&self, kind: CursorKind) -> bool {
        self.loaded(kind).is_some_and(LoadedCursor::is_animated)
    }

    /// Themes searched for cursor files, in priority order; the first entry is the requested theme.
    pub fn theme_chain(&self) -> &[String] {
        &self.chain
    }

    fn loaded(&self, kind: CursorKind) -> Option<&LoadedCursor> {
        let slot = self.files.get(kind.index())?;
        slot.get_or_init(|| find_cursor_file(kind, &self.chain, &self.roots))
            .as_ref()
    }
}

/// Process-wide cache of fallback images, keyed by kind and pixel size.
type BuiltinCache = Mutex<HashMap<(CursorKind, u32), &'static CursorImage>>;

fn builtin_image(kind: CursorKind, target: u32) -> &'static CursorImage {
    static CACHE: OnceLock<BuiltinCache> = OnceLock::new();
    let size = normalized_size(target);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (kind, size);
    let mut entries = cache.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(image) = entries.get(&key).copied() {
        return image;
    }
    // Leaked on purpose: one entry per (kind, clamped size) for the process lifetime,
    // which keeps `image()` returning plain references without per-call allocation.
    let image: &'static CursorImage = Box::leak(Box::new(builtin_cursor(kind, size)));
    entries.insert(key, image);
    image
}

fn find_cursor_file(kind: CursorKind, chain: &[String], roots: &[PathBuf]) -> Option<LoadedCursor> {
    let theme_dirs = chain.iter().flat_map(|theme| {
        roots
            .iter()
            .map(move |root| root.join(theme).join(CURSORS_DIR))
    });
    theme_dirs
        .flat_map(|dir| kind.xcursor_names().iter().map(move |name| dir.join(name)))
        .find_map(|path| {
            fs::read(path)
                .ok()
                .and_then(|bytes| parse_cursor_file(&bytes))
        })
}

/// Requested theme followed by everything it inherits, breadth-first and cycle-safe.
fn inherit_chain(root: &str, roots: &[PathBuf]) -> Vec<String> {
    let mut chain: Vec<String> = Vec::new();
    let mut queue = VecDeque::from([root.to_owned()]);
    while let Some(theme) = queue.pop_front() {
        if chain.len() >= MAX_CHAIN_LEN {
            break;
        }
        if !is_plain_theme_name(&theme) || chain.contains(&theme) {
            continue;
        }
        queue.extend(read_inherits(&theme, roots));
        chain.push(theme);
    }
    chain
}

fn is_plain_theme_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && name != "." && name != ".."
}

fn read_inherits(theme: &str, roots: &[PathBuf]) -> Vec<String> {
    roots
        .iter()
        .find_map(|root| fs::read_to_string(root.join(theme).join(INDEX_FILE)).ok())
        .map(|content| parse_inherits(&content))
        .unwrap_or_default()
}

fn parse_inherits(content: &str) -> Vec<String> {
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix(INHERITS_KEY))
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn default_search_roots() -> Vec<PathBuf> {
    let home = env::var_os(HOME_ENV).map(PathBuf::from);
    let mut roots: Vec<PathBuf> = env::var_os(PATH_ENV)
        .map(|value| {
            env::split_paths(&value)
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| expand_home(path, home.as_deref()))
                .collect()
        })
        .unwrap_or_default();
    if let Some(home) = &home {
        roots.push(home.join(".icons"));
        roots.push(home.join(".local/share/icons"));
    }
    roots.extend(SYSTEM_ROOTS.iter().map(PathBuf::from));
    roots
}

fn expand_home(path: PathBuf, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix("~"), home) {
        (Ok(rest), Some(home)) => home.join(rest),
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use tempfile::TempDir;

    use super::*;
    use crate::test_support::{frame, write_theme};

    const COLOUR: u32 = 0xff_11_22_33;

    fn temp_root() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn roots_of(dir: &TempDir) -> Vec<PathBuf> {
        vec![dir.path().to_path_buf()]
    }

    #[test]
    fn selects_nearest_nominal_size_with_scale() {
        let dir = temp_root();
        let frames = vec![
            frame(24, 0, (0, 0), COLOUR),
            frame(32, 0, (0, 0), COLOUR),
            frame(48, 0, (0, 0), COLOUR),
        ];
        write_theme(dir.path(), "sizes", &[], &[("default", frames)]);
        let theme = CursorTheme::load_from_dirs(Some("sizes"), 24, &roots_of(&dir));
        assert_eq!(theme.image(CursorKind::Default, 1, 0).width, 24);
        assert_eq!(theme.image(CursorKind::Default, 2, 0).width, 48);
        assert_eq!(theme.image(CursorKind::Default, 3, 0).width, 48);
    }

    #[test]
    fn size_tie_prefers_larger_nominal_size() {
        let dir = temp_root();
        let frames = vec![frame(24, 0, (0, 0), COLOUR), frame(32, 0, (0, 0), COLOUR)];
        write_theme(dir.path(), "tie", &[], &[("default", frames)]);
        let theme = CursorTheme::load_from_dirs(Some("tie"), 28, &roots_of(&dir));
        assert_eq!(theme.image(CursorKind::Default, 1, 0).width, 32);
    }

    #[test]
    fn animation_frames_follow_elapsed_time_and_wrap() {
        let dir = temp_root();
        let frames = vec![
            frame(24, 10, (0, 0), COLOUR),
            frame(24, 20, (1, 0), COLOUR),
            frame(24, 30, (2, 0), COLOUR),
        ];
        write_theme(dir.path(), "anim", &[], &[("default", frames)]);
        let theme = CursorTheme::load_from_dirs(Some("anim"), 24, &roots_of(&dir));
        let hotspot_at = |elapsed| theme.image(CursorKind::Default, 1, elapsed).hotspot_x;
        assert_eq!(hotspot_at(0), 0);
        assert_eq!(hotspot_at(10), 1);
        assert_eq!(hotspot_at(30), 2);
        assert_eq!(hotspot_at(60), 0);
        assert_eq!(hotspot_at(75), 1);
        assert!(theme.is_animated(CursorKind::Default));
        assert!(!theme.is_animated(CursorKind::Text));
    }

    #[test]
    fn inherits_chain_follows_parents_and_survives_cycles() {
        let dir = temp_root();
        write_theme(dir.path(), "a", &["b"], &[]);
        write_theme(
            dir.path(),
            "b",
            &["c"],
            &[("default", vec![frame(24, 0, (5, 0), COLOUR)])],
        );
        write_theme(dir.path(), "c", &["a"], &[]);
        let theme = CursorTheme::load_from_dirs(Some("a"), 24, &roots_of(&dir));
        assert_eq!(theme.theme_chain(), ["a", "b", "c"]);
        assert_eq!(theme.image(CursorKind::Default, 1, 0).hotspot_x, 5);
    }

    #[test]
    fn child_theme_overrides_inherited_cursor() {
        let dir = temp_root();
        write_theme(
            dir.path(),
            "child",
            &["parent"],
            &[("default", vec![frame(24, 0, (7, 0), COLOUR)])],
        );
        write_theme(
            dir.path(),
            "parent",
            &[],
            &[("default", vec![frame(24, 0, (3, 0), COLOUR)])],
        );
        let theme = CursorTheme::load_from_dirs(Some("child"), 24, &roots_of(&dir));
        assert_eq!(theme.image(CursorKind::Default, 1, 0).hotspot_x, 7);
    }

    #[test]
    fn missing_theme_falls_back_to_builtin() {
        let dir = temp_root();
        let theme = CursorTheme::load_from_dirs(Some("missing"), 24, &roots_of(&dir));
        assert_eq!(
            theme.image(CursorKind::Default, 1, 0),
            &builtin_cursor(CursorKind::Default, 24)
        );
        assert!(!theme.is_animated(CursorKind::Default));
    }

    #[test]
    fn unparsable_cursor_file_falls_back_to_builtin() {
        let dir = temp_root();
        write_theme(dir.path(), "broken", &[], &[]);
        let cursors = dir.path().join("broken").join(CURSORS_DIR);
        fs::write(cursors.join("default"), b"not a cursor").expect("write");
        let theme = CursorTheme::load_from_dirs(Some("broken"), 24, &roots_of(&dir));
        assert_eq!(
            theme.image(CursorKind::Default, 1, 0),
            &builtin_cursor(CursorKind::Default, 24)
        );
    }

    #[test]
    fn repeated_lookups_return_the_same_cached_image() {
        let dir = temp_root();
        write_theme(
            dir.path(),
            "cached",
            &[],
            &[("default", vec![frame(24, 0, (0, 0), COLOUR)])],
        );
        let theme = CursorTheme::load_from_dirs(Some("cached"), 24, &roots_of(&dir));
        let first = theme.image(CursorKind::Default, 1, 0);
        assert!(ptr::eq(first, theme.image(CursorKind::Default, 1, 0)));
        let fallback = CursorTheme::load_from_dirs(Some("none"), 24, &roots_of(&dir));
        let builtin_first = fallback.image(CursorKind::Text, 1, 0);
        assert!(ptr::eq(
            builtin_first,
            fallback.image(CursorKind::Text, 1, 0)
        ));
    }

    #[test]
    fn absent_name_and_zero_size_use_defaults() {
        let dir = temp_root();
        let theme = CursorTheme::load_from_dirs(None, 0, &roots_of(&dir));
        assert_eq!(theme.theme_chain(), [DEFAULT_THEME]);
        assert_eq!(theme.image(CursorKind::Default, 1, 0).width, 24);
    }

    #[test]
    fn index_theme_inherits_line_is_parsed() {
        let content = "[Icon Theme]\nName=Test\nInherits= base , extra,\n";
        assert_eq!(parse_inherits(content), ["base", "extra"]);
        assert!(parse_inherits("[Icon Theme]\nName=Test\n").is_empty());
    }

    #[test]
    fn theme_names_with_separators_are_ignored() {
        let dir = temp_root();
        let chain = inherit_chain("../escape", &roots_of(&dir));
        assert!(chain.is_empty());
    }

    #[test]
    fn load_with_unknown_name_falls_back_without_panicking() {
        let theme = CursorTheme::load(Some("desilium-test-no-such-theme"), 24);
        assert_eq!(theme.image(CursorKind::Pointer, 1, 0).width, 24);
    }
}
