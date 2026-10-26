//! Thumbnail bookkeeping: what is cached, what is queued, what the preview shows.

use std::path::{Path, PathBuf};

use desilium_theme::Theme;

use super::App;
use crate::{
    model::PreviewThumb,
    refresh::Refresh,
    wallpapers::{FitMode, ThumbJob, ThumbResult, ThumbSpec, ensure_thumb, lookup},
};

/// Card thumbnails are 104x66 logical pixels, rendered at 2x for sharp edges.
const CARD_SIZE: (u32, u32) = (208, 132);
/// The preview box is 16:9; this is enough for a 2x display.
const PREVIEW_SIZE: (u32, u32) = (704, 396);
const OPAQUE: u8 = 255;

/// Config string to fit mode; unknown strings behave like the default, which validation guarantees anyway.
pub(super) fn fit_mode(text: &str) -> FitMode {
    match text {
        "fit" => FitMode::Fit,
        "stretch" => FitMode::Stretch,
        "center" => FitMode::Center,
        "tile" => FitMode::Tile,
        _ => FitMode::Fill,
    }
}

fn job_key(job: &ThumbJob) -> String {
    format!(
        "{}|{}x{}|{:?}|{:?}",
        job.source.display(),
        job.spec.width,
        job.spec.height,
        job.spec.mode,
        job.spec.background
    )
}

impl App {
    fn card_spec(&self) -> ThumbSpec {
        ThumbSpec {
            width: CARD_SIZE.0,
            height: CARD_SIZE.1,
            mode: FitMode::Fill,
            background: [0, 0, 0, OPAQUE],
        }
    }

    /// The preview thumbnail's look. Letterbox bars only exist for modes that do not cover the
    /// screen, and only then does the colour depend on the theme (never on the accent, so
    /// picking an accent does not invalidate a thumbnail that is still being rendered).
    pub(super) fn preview_spec(&self) -> ThumbSpec {
        let mode = fit_mode(&self.store.get().appearance.wallpaper_mode);
        let covers = matches!(mode, FitMode::Fill | FitMode::Stretch);
        let base = if covers {
            [0, 0, 0, OPAQUE]
        } else {
            let [red, green, blue, _] = Theme::for_mode(self.theme.mode)
                .palette
                .background
                .to_bytes();
            [red, green, blue, OPAQUE]
        };
        ThumbSpec {
            width: PREVIEW_SIZE.0,
            height: PREVIEW_SIZE.1,
            mode,
            background: base,
        }
    }

    /// Makes sure every wallpaper card and the preview have a thumbnail, rendering missing ones
    /// in the background (or right here when there is no worker).
    pub(super) fn request_thumbs(&mut self) {
        let card = self.card_spec();
        let sources: Vec<PathBuf> = self
            .model
            .wallpapers
            .iter()
            .map(|w| w.path.clone())
            .collect();
        for source in sources {
            self.want(ThumbJob { source, spec: card });
        }
        let selected = self.store.get().appearance.wallpaper.clone();
        self.model.preview = None;
        if let Some(path) = selected {
            let job = ThumbJob {
                source: PathBuf::from(path),
                spec: self.preview_spec(),
            };
            self.want(job);
        }
    }

    fn want(&mut self, job: ThumbJob) {
        if let Some(png) = lookup(&job.source, &job.spec, &self.cache) {
            self.accept(&job, png);
            return;
        }
        let key = job_key(&job);
        if self.pending_thumbs.contains(&key) {
            return;
        }
        let Some(worker) = &self.worker else {
            if let Ok(png) = ensure_thumb(&job.source, &job.spec, &self.cache) {
                self.accept(&job, png);
            }
            return;
        };
        self.pending_thumbs.push(key);
        worker.submit(job);
    }

    fn accept(&mut self, job: &ThumbJob, png: PathBuf) {
        if job.spec == self.card_spec() {
            self.model.thumbs.insert(job.source.clone(), png);
            return;
        }
        let current = self
            .store
            .get()
            .appearance
            .wallpaper
            .as_deref()
            .map(Path::new);
        if current == Some(job.source.as_path()) && job.spec == self.preview_spec() {
            self.model.preview = Some(PreviewThumb {
                source: job.source.clone(),
                mode: job.spec.mode,
                png,
            });
        }
    }

    /// A background thumbnail finished. Failures keep the placeholder and are logged.
    pub fn on_thumb(&mut self, result: ThumbResult) -> Refresh {
        self.pending_thumbs
            .retain(|key| *key != job_key(&result.job));
        match result.outcome {
            Ok(png) => {
                self.accept(&result.job, png);
                Refresh::main()
            }
            Err(error) => {
                tracing::warn!(source = %result.job.source.display(), %error, "thumbnail failed");
                Refresh::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_modes_parse() {
        assert_eq!(fit_mode("fit"), FitMode::Fit);
        assert_eq!(fit_mode("tile"), FitMode::Tile);
        assert_eq!(fit_mode("fill"), FitMode::Fill);
        assert_eq!(fit_mode("???"), FitMode::Fill);
    }
}
