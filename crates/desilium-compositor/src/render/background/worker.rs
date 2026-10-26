//! Decoding and fitting on a worker thread so the render path never touches the disk.

use super::request::{CacheKey, WallpaperSource};
use desilium_wallpaper::{LoadOptions, Size, Wallpaper, WallpaperError, average_color};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

/// Source images larger than this are downscaled right after decoding to bound memory.
const MAX_DECODE_EDGE: u32 = 8192;
const OPAQUE: u8 = u8::MAX;

/// A finished render: tightly packed RGBA8 at the key's output size.
pub struct Rendered {
    pub size: Size,
    pub rgba: Vec<u8>,
}

/// Outcome of polling a job.
pub enum Poll {
    Running,
    Done(Result<Rendered, String>),
}

/// A running (or finished) background render; dropping it cancels the work.
pub struct Job {
    pub key: CacheKey,
    results: Receiver<Result<Rendered, String>>,
    cancel: Arc<AtomicBool>,
}

impl Job {
    /// Starts decoding `source` for `key`; the error is the reason the thread could not start.
    pub fn spawn(key: CacheKey, source: WallpaperSource) -> Result<Job, String> {
        let (sender, results) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let size = key.size;
        thread::Builder::new()
            .name("desilium-wallpaper".into())
            .spawn(move || {
                let outcome = prepare(&source, size, &flag);
                // The receiver is gone when the job was cancelled; nothing to report then.
                if let Some(outcome) = outcome {
                    let _ = sender.send(outcome.map_err(|error| error.to_string()));
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Job {
            key,
            results,
            cancel,
        })
    }

    /// Non-blocking check for the result.
    pub fn poll(&self) -> Poll {
        match self.results.try_recv() {
            Ok(outcome) => Poll::Done(outcome),
            Err(TryRecvError::Empty) => Poll::Running,
            Err(TryRecvError::Disconnected) => Poll::Done(Err("worker stopped".into())),
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// `None` when cancelled before the expensive step.
fn prepare(
    source: &WallpaperSource,
    size: Size,
    cancel: &AtomicBool,
) -> Option<Result<Rendered, WallpaperError>> {
    let options = LoadOptions {
        max_size: Some(Size::new(MAX_DECODE_EDGE, MAX_DECODE_EDGE)),
    };
    let wallpaper = match Wallpaper::load_with(&source.path, &options) {
        Ok(wallpaper) => wallpaper,
        Err(error) => return Some(Err(error)),
    };
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    let mut backdrop = average_color(wallpaper.rgba(), wallpaper.dimensions());
    // Letterbox bars must be opaque so the texture can be declared opaque.
    backdrop[3] = OPAQUE;
    let rgba = wallpaper.render(size, source.mode, backdrop);
    Some(Ok(Rendered { size, rgba }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_wallpaper::FitMode;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn wait(job: &Job) -> Result<Rendered, String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Poll::Done(outcome) = job.poll() {
                return outcome;
            }
            thread::sleep(Duration::from_millis(5));
        }
        Err("timeout".into())
    }

    fn write_png(path: &PathBuf) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, 4, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[200u8; 4 * 2 * 4]).unwrap();
    }

    fn job_for(path: PathBuf, mode: FitMode) -> Job {
        let source = WallpaperSource { path, mode };
        let key = CacheKey::new(&source, Size::new(16, 9), 1.0);
        Job::spawn(key, source).unwrap()
    }

    #[test]
    fn renders_to_the_requested_size() {
        let path = std::env::temp_dir().join(format!("desilium-wp-ok-{}.png", std::process::id()));
        write_png(&path);
        let rendered = wait(&job_for(path.clone(), FitMode::Fit)).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(rendered.size, Size::new(16, 9));
        assert_eq!(rendered.rgba.len(), 16 * 9 * 4);
        assert!(rendered.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn missing_file_reports_an_error() {
        let path = PathBuf::from("/nonexistent/desilium/none.png");
        assert!(wait(&job_for(path, FitMode::Fill)).is_err());
    }

    #[test]
    fn corrupt_file_reports_an_error() {
        let path = std::env::temp_dir().join(format!("desilium-wp-bad-{}.png", std::process::id()));
        std::fs::write(&path, b"not an image").unwrap();
        let outcome = wait(&job_for(path.clone(), FitMode::Fill));
        std::fs::remove_file(&path).unwrap();
        assert!(outcome.is_err());
    }
}
