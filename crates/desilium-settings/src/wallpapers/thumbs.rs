//! Thumbnail generation with an on-disk cache and a background worker.

use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
    time::UNIX_EPOCH,
};

use desilium_wallpaper::{FitMode, LoadOptions, Size, Wallpaper, WallpaperError};

const CACHE_SUBDIR: &str = "desilium/thumbs";
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
/// Source images are shrunk to this right after decoding; thumbnails never need more.
const DECODE_LIMIT: Size = Size::new(2560, 1440);
/// A reference screen: non-fill modes (center, tile, fit) are previewed on this canvas.
const REFERENCE_SCREEN: Size = Size::new(1920, 1080);

/// What a thumbnail looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbSpec {
    /// Output width in pixels.
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// How the wallpaper is placed on the (reference) screen first.
    pub mode: FitMode,
    /// Colour of letterbox bars, RGBA.
    pub background: [u8; 4],
}

/// Why a thumbnail could not be made.
#[derive(Debug, thiserror::Error)]
pub enum ThumbError {
    /// The wallpaper could not be decoded.
    #[error(transparent)]
    Wallpaper(#[from] WallpaperError),
    /// The cache could not be written.
    #[error("thumbnail cache: {0}")]
    Cache(#[from] io::Error),
    /// PNG encoding failed.
    #[error("thumbnail encoding: {0}")]
    Encode(#[from] png::EncodingError),
}

/// `$XDG_CACHE_HOME/desilium/thumbs`, or `~/.cache/desilium/thumbs`.
pub fn cache_dir(xdg_cache_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
    let base = match (
        xdg_cache_home.filter(|v| !v.is_empty()),
        home.filter(|v| !v.is_empty()),
    ) {
        (Some(xdg), _) => PathBuf::from(xdg),
        (None, Some(home)) => PathBuf::from(home).join(".cache"),
        (None, None) => env::temp_dir(),
    };
    base.join(CACHE_SUBDIR)
}

fn mode_tag(mode: FitMode) -> &'static str {
    match mode {
        FitMode::Fill => "fill",
        FitMode::Fit => "fit",
        FitMode::Stretch => "stretch",
        FitMode::Center => "center",
        FitMode::Tile => "tile",
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

/// Cache file name for a source file state and spec. Changes when the source changes.
pub(super) fn cache_name(source: &Path, modified_secs: u64, len: u64, spec: &ThumbSpec) -> String {
    let identity = format!("{}|{modified_secs}|{len}", source.display());
    format!(
        "{:016x}-{}x{}-{}-{:02x}{:02x}{:02x}.png",
        fnv1a(identity.as_bytes()),
        spec.width,
        spec.height,
        mode_tag(spec.mode),
        spec.background[0],
        spec.background[1],
        spec.background[2],
    )
}

fn cached_path(source: &Path, spec: &ThumbSpec, cache: &Path) -> Option<PathBuf> {
    let meta = fs::metadata(source).ok()?;
    let secs = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |age| age.as_secs());
    Some(cache.join(cache_name(source, secs, meta.len(), spec)))
}

/// The cached thumbnail if it already exists; never decodes anything.
pub fn lookup(source: &Path, spec: &ThumbSpec, cache: &Path) -> Option<PathBuf> {
    cached_path(source, spec, cache).filter(|path| path.is_file())
}

/// Returns the thumbnail, generating and caching it when missing. Blocking: run off the UI thread.
pub fn ensure_thumb(source: &Path, spec: &ThumbSpec, cache: &Path) -> Result<PathBuf, ThumbError> {
    if let Some(found) = lookup(source, spec, cache) {
        return Ok(found);
    }
    let target = cached_path(source, spec, cache)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "wallpaper file is gone"))?;
    let rgba = render_thumb(source, spec)?;
    fs::create_dir_all(cache)?;
    write_png_atomic(&target, spec, &rgba)?;
    Ok(target)
}

fn render_thumb(source: &Path, spec: &ThumbSpec) -> Result<Vec<u8>, ThumbError> {
    let options = LoadOptions {
        max_size: Some(DECODE_LIMIT),
    };
    let wallpaper = Wallpaper::load_with(source, &options)?;
    let output = Size::new(spec.width, spec.height);
    if spec.mode == FitMode::Fill {
        return Ok(wallpaper.render(output, FitMode::Fill, spec.background));
    }
    let screen = wallpaper.render(REFERENCE_SCREEN, spec.mode, spec.background);
    let on_screen = Wallpaper::from_rgba(REFERENCE_SCREEN, screen)?;
    Ok(on_screen.render(output, FitMode::Fill, spec.background))
}

fn write_png_atomic(target: &Path, spec: &ThumbSpec, rgba: &[u8]) -> Result<(), ThumbError> {
    let temp = target.with_extension("png.tmp");
    let file = fs::File::create(&temp)?;
    let mut encoder = png::Encoder::new(io::BufWriter::new(file), spec.width, spec.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    fs::rename(&temp, target)?;
    Ok(())
}

/// A thumbnail to produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbJob {
    /// Source image.
    pub source: PathBuf,
    /// Wanted look.
    pub spec: ThumbSpec,
}

/// The outcome of a [`ThumbJob`].
#[derive(Debug)]
pub struct ThumbResult {
    /// The job it answers.
    pub job: ThumbJob,
    /// Path of the PNG, or the reason there is none.
    pub outcome: Result<PathBuf, ThumbError>,
}

/// One background thread that turns jobs into cached PNGs.
pub struct ThumbWorker {
    jobs: Option<Sender<ThumbJob>>,
    thread: Option<JoinHandle<()>>,
}

impl ThumbWorker {
    /// Starts the worker; `deliver` is called on the worker thread for every finished job.
    pub fn spawn(cache: PathBuf, deliver: impl Fn(ThumbResult) + Send + 'static) -> Self {
        let (jobs, inbox) = mpsc::channel::<ThumbJob>();
        let thread = thread::Builder::new()
            .name("thumbnails".to_owned())
            .spawn(move || {
                for job in inbox {
                    let outcome = ensure_thumb(&job.source, &job.spec, &cache);
                    deliver(ThumbResult { job, outcome });
                }
            })
            .ok();
        Self {
            jobs: Some(jobs),
            thread,
        }
    }

    /// Queues a job. A dead worker drops it silently; the UI keeps its placeholder.
    pub fn submit(&self, job: ThumbJob) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(job);
        }
    }
}

impl Drop for ThumbWorker {
    fn drop(&mut self) {
        // Closing the channel ends the worker loop; do not wait for a running decode.
        self.jobs = None;
        self.thread = None;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use super::*;

    const SPEC: ThumbSpec = ThumbSpec {
        width: 40,
        height: 24,
        mode: FitMode::Fill,
        background: [0, 0, 0, 255],
    };

    fn write_source(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let file = fs::File::create(&path).expect("create");
        let mut encoder = png::Encoder::new(file, 64, 32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let pixels = vec![200u8; 64 * 32 * 4];
        encoder
            .write_header()
            .expect("header")
            .write_image_data(&pixels)
            .expect("data");
        path
    }

    #[test]
    fn cache_dir_follows_xdg_then_home() {
        assert_eq!(
            cache_dir(Some("/c".into()), Some("/h".into())),
            PathBuf::from("/c/desilium/thumbs")
        );
        assert_eq!(
            cache_dir(Some(OsString::new()), Some("/h".into())),
            PathBuf::from("/h/.cache/desilium/thumbs")
        );
    }

    #[test]
    fn cache_name_changes_with_source_state_and_spec() {
        let base = cache_name(Path::new("/w/a.jpg"), 10, 100, &SPEC);
        assert_ne!(base, cache_name(Path::new("/w/a.jpg"), 11, 100, &SPEC));
        assert_ne!(base, cache_name(Path::new("/w/a.jpg"), 10, 101, &SPEC));
        assert_ne!(base, cache_name(Path::new("/w/b.jpg"), 10, 100, &SPEC));
        let fitted = ThumbSpec {
            mode: FitMode::Fit,
            ..SPEC
        };
        assert_ne!(base, cache_name(Path::new("/w/a.jpg"), 10, 100, &fitted));
        assert!(base.ends_with(".png"));
    }

    #[test]
    fn thumbnail_is_generated_once_then_served_from_cache() {
        let dir = tempfile::tempdir().expect("dir");
        let cache = dir.path().join("cache");
        let source = write_source(dir.path(), "a.png");
        assert_eq!(lookup(&source, &SPEC, &cache), None);
        let made = ensure_thumb(&source, &SPEC, &cache).expect("thumb");
        assert!(made.is_file());
        assert_eq!(lookup(&source, &SPEC, &cache), Some(made.clone()));
        let modified = fs::metadata(&made).expect("meta").modified().expect("time");
        let again = ensure_thumb(&source, &SPEC, &cache).expect("again");
        assert_eq!(again, made);
        assert_eq!(
            fs::metadata(&again)
                .expect("meta")
                .modified()
                .expect("time"),
            modified
        );
    }

    #[test]
    fn non_fill_modes_render_on_a_reference_screen() {
        let dir = tempfile::tempdir().expect("dir");
        let source = write_source(dir.path(), "a.png");
        let spec = ThumbSpec {
            mode: FitMode::Center,
            ..SPEC
        };
        let made = ensure_thumb(&source, &spec, &dir.path().join("cache")).expect("thumb");
        assert!(made.is_file());
    }

    #[test]
    fn broken_images_report_an_error_and_leave_no_file() {
        let dir = tempfile::tempdir().expect("dir");
        let source = dir.path().join("bad.png");
        fs::write(&source, b"not an image").expect("write");
        let cache = dir.path().join("cache");
        assert!(ensure_thumb(&source, &SPEC, &cache).is_err());
        assert_eq!(lookup(&source, &SPEC, &cache), None);
    }

    #[test]
    fn worker_delivers_results() {
        let dir = tempfile::tempdir().expect("dir");
        let source = write_source(dir.path(), "a.png");
        let (tx, rx) = channel();
        let worker = ThumbWorker::spawn(dir.path().join("cache"), move |result| {
            let _ = tx.send(result);
        });
        worker.submit(ThumbJob {
            source: source.clone(),
            spec: SPEC,
        });
        let result = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("result");
        assert_eq!(result.job.source, source);
        assert!(result.outcome.is_ok());
    }
}
