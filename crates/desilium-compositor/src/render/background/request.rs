//! What to show and how to recognise an already prepared result; pure logic, no GPU.

use desilium_wallpaper::{FitMode, Size};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Longest output edge a wallpaper texture may have (8K UHD width).
pub const MAX_EDGE_PX: u32 = 7680;
/// Most pixels a wallpaper texture may have (8K UHD); caps the allocation before it happens.
pub const MAX_PIXELS: u64 = 7680 * 4320;
const SCALE_MILLI: f64 = 1000.0;
const HOME_PREFIX: &str = "~/";

/// A wallpaper file plus the fit mode to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallpaperSource {
    pub path: PathBuf,
    pub mode: FitMode,
}

/// Everything that makes a rendered wallpaper texture distinct; equal keys mean the cache is valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey {
    pub path: PathBuf,
    pub mtime: Option<SystemTime>,
    pub mode: FitMode,
    pub size: Size,
    /// Output scale in thousandths, so fractional scales compare exactly.
    pub scale_milli: u32,
}

impl CacheKey {
    /// Builds the key for `source` on an output of `size` physical pixels at `scale`.
    pub fn new(source: &WallpaperSource, size: Size, scale: f64) -> Self {
        Self {
            path: source.path.clone(),
            mtime: modified_time(&source.path),
            mode: source.mode,
            size,
            scale_milli: scale_milli(scale),
        }
    }
}

fn modified_time(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn scale_milli(scale: f64) -> u32 {
    (scale * SCALE_MILLI)
        .round()
        .clamp(0.0, f64::from(u32::MAX)) as u32
}

/// Maps the config string onto a fit mode; unknown values fall back to `fill`.
pub fn parse_mode(name: &str) -> FitMode {
    match name.trim().to_ascii_lowercase().as_str() {
        "fit" => FitMode::Fit,
        "stretch" => FitMode::Stretch,
        "center" | "centre" => FitMode::Center,
        "tile" => FitMode::Tile,
        _ => FitMode::Fill,
    }
}

/// Whether `name` is spelled like one of the known modes.
fn is_mode_name(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "fill" | "fit" | "stretch" | "center" | "centre" | "tile"
    )
}

/// Parses `path[:mode]`; a trailing `:word` is a mode only when it names one.
pub fn parse_override(value: &str) -> Option<WallpaperSource> {
    let value = value.trim();
    let (path, mode) = match value.rsplit_once(':') {
        Some((path, mode)) if is_mode_name(mode) => (path, parse_mode(mode)),
        _ => (value, FitMode::Fill),
    };
    if path.is_empty() {
        return None;
    }
    Some(WallpaperSource {
        path: expand_home(path),
        mode,
    })
}

/// Expands a leading `~/` using `$HOME`; other paths are used as written.
pub fn expand_home(path: &str) -> PathBuf {
    let home = std::env::var_os("HOME");
    match (path.strip_prefix(HOME_PREFIX), home) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// Physical pixel size of the output, or `None` when it is empty or above the hard cap.
pub fn checked_size(width: i32, height: i32) -> Option<Size> {
    let (w, h) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
    let within = w > 0
        && h > 0
        && w <= MAX_EDGE_PX
        && h <= MAX_EDGE_PX
        && u64::from(w) * u64::from(h) <= MAX_PIXELS;
    within.then_some(Size::new(w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(path: &str, mode: FitMode) -> WallpaperSource {
        WallpaperSource {
            path: PathBuf::from(path),
            mode,
        }
    }

    #[test]
    fn modes_parse_and_unknown_falls_back_to_fill() {
        assert_eq!(parse_mode("fit"), FitMode::Fit);
        assert_eq!(parse_mode("Stretch"), FitMode::Stretch);
        assert_eq!(parse_mode(" center "), FitMode::Center);
        assert_eq!(parse_mode("tile"), FitMode::Tile);
        assert_eq!(parse_mode("fill"), FitMode::Fill);
        assert_eq!(parse_mode("zoom"), FitMode::Fill);
        assert_eq!(parse_mode(""), FitMode::Fill);
    }

    #[test]
    fn override_splits_a_trailing_mode_only() {
        let parsed = parse_override("/a/b.jpg:fit").unwrap();
        assert_eq!(
            (parsed.path.as_path(), parsed.mode),
            (Path::new("/a/b.jpg"), FitMode::Fit)
        );
        let plain = parse_override("/a/b.jpg").unwrap();
        assert_eq!(plain.mode, FitMode::Fill);
        let odd = parse_override("/a/b:c.jpg").unwrap();
        assert_eq!(odd.path, Path::new("/a/b:c.jpg"));
        assert!(parse_override("  ").is_none());
    }

    #[test]
    fn cache_key_tracks_every_input() {
        let size = Size::new(1920, 1080);
        let base = CacheKey::new(&source("/nonexistent/a.png", FitMode::Fill), size, 1.0);
        assert_eq!(
            base,
            CacheKey::new(&source("/nonexistent/a.png", FitMode::Fill), size, 1.0)
        );
        let other_mode = CacheKey::new(&source("/nonexistent/a.png", FitMode::Fit), size, 1.0);
        let other_scale = CacheKey::new(&source("/nonexistent/a.png", FitMode::Fill), size, 1.5);
        let other_size = CacheKey::new(
            &source("/nonexistent/a.png", FitMode::Fill),
            Size::new(10, 10),
            1.0,
        );
        let other_path = CacheKey::new(&source("/nonexistent/b.png", FitMode::Fill), size, 1.0);
        for changed in [other_mode, other_scale, other_size, other_path] {
            assert_ne!(base, changed);
        }
    }

    #[test]
    fn cache_key_sees_a_modified_file() {
        let path = std::env::temp_dir().join(format!("desilium-wp-key-{}", std::process::id()));
        std::fs::write(&path, b"one").unwrap();
        let src = WallpaperSource {
            path: path.clone(),
            mode: FitMode::Fill,
        };
        let size = Size::new(8, 8);
        let before = CacheKey::new(&src, size, 1.0);
        assert!(before.mtime.is_some());
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_ne!(before, CacheKey::new(&src, size, 1.0));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn size_cap_rejects_empty_and_huge_outputs() {
        assert_eq!(checked_size(2560, 1440), Some(Size::new(2560, 1440)));
        assert_eq!(checked_size(7680, 4320), Some(Size::new(7680, 4320)));
        assert!(checked_size(0, 100).is_none());
        assert!(checked_size(-5, 100).is_none());
        assert!(checked_size(7681, 100).is_none());
        assert!(checked_size(7680, 7680).is_none());
    }
}
