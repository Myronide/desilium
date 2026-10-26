//! Persistent launch statistics that boost frequently and recently used applications.
//!
//! Counts decay exponentially with a fourteen-day half-life. Time is always passed in
//! as seconds since the Unix epoch so that tests can run with a fake clock.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::warn;

/// Half-life of a launch's influence, in seconds.
pub const HALF_LIFE_SECS: u64 = 14 * 24 * 60 * 60;
/// Largest score boost a single application can receive.
pub const MAX_BOOST: u32 = 150;
/// Launch count at which the boost reaches about 63 percent of [`MAX_BOOST`].
const SATURATION_USES: f64 = 3.0;
const FILE_VERSION: u32 = 1;
const FILE_NAME: &str = "apps-usage.json";
const STATE_SUBDIR: &str = "desilium";

/// Errors from writing the usage store. Reading problems are logged, not returned.
#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    /// The file or its directory could not be written.
    #[error("cannot write usage store {path}: {source}")]
    Write {
        /// Target file.
        path: PathBuf,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },
    /// The store could not be serialized.
    #[error("cannot encode usage store: {0}")]
    Encode(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Record {
    /// Launch count as of `last_used`; decays afterwards.
    count: f64,
    /// Seconds since the epoch of the most recent launch.
    last_used: u64,
}

#[derive(Serialize, Deserialize)]
struct Document {
    version: u32,
    apps: BTreeMap<String, Record>,
}

/// Launch counts per desktop id, optionally backed by a JSON file.
#[derive(Debug, Clone, Default)]
pub struct UsageStats {
    path: Option<PathBuf>,
    apps: BTreeMap<String, Record>,
}

impl UsageStats {
    /// `$XDG_STATE_HOME/desilium/apps-usage.json` for the given state home.
    pub fn default_path(state_home: &Path) -> PathBuf {
        state_home.join(STATE_SUBDIR).join(FILE_NAME)
    }

    /// Store that never touches the disk; [`UsageStats::save`] is a no-op.
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// Loads the store at `path`. A missing file is an empty store; a damaged one is
    /// logged and treated as empty, so the launcher keeps working.
    pub fn load(path: PathBuf) -> Self {
        let apps = match fs::read(&path) {
            Ok(bytes) => decode(&bytes).unwrap_or_else(|err| {
                warn!(path = %path.display(), %err, "ignoring damaged usage store");
                BTreeMap::new()
            }),
            Err(err) if err.kind() == ErrorKind::NotFound => BTreeMap::new(),
            Err(err) => {
                warn!(path = %path.display(), %err, "cannot read usage store");
                BTreeMap::new()
            }
        };
        Self {
            path: Some(path),
            apps,
        }
    }

    /// Records one launch of `id` at `now_secs`.
    pub fn record(&mut self, id: &str, now_secs: u64) {
        let count = self
            .apps
            .get(id)
            .map_or(0.0, |record| decayed(*record, now_secs));
        self.apps.insert(
            id.to_owned(),
            Record {
                count: count + 1.0,
                last_used: now_secs,
            },
        );
    }

    /// Decayed launch count of `id` at `now_secs`.
    pub fn score(&self, id: &str, now_secs: u64) -> f64 {
        self.apps
            .get(id)
            .map_or(0.0, |record| decayed(*record, now_secs))
    }

    /// Score boost in `0..=MAX_BOOST`, growing with use and saturating.
    pub fn boost(&self, id: &str, now_secs: u64) -> u32 {
        let uses = self.score(id, now_secs);
        let share = 1.0 - (-uses / SATURATION_USES).exp();
        (f64::from(MAX_BOOST) * share).round() as u32
    }

    /// Writes the store atomically: a temporary file in the same directory, then a rename.
    pub fn save(&self) -> Result<(), UsageError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let document = Document {
            version: FILE_VERSION,
            apps: self.apps.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&document)?;
        write_atomic(path, &bytes).map_err(|source| UsageError::Write {
            path: path.clone(),
            source,
        })
    }
}

fn decayed(record: Record, now_secs: u64) -> f64 {
    let elapsed = now_secs.saturating_sub(record.last_used);
    record.count * 0.5_f64.powf(elapsed as f64 / HALF_LIFE_SECS as f64)
}

fn decode(bytes: &[u8]) -> Result<BTreeMap<String, Record>, serde_json::Error> {
    let document: Document = serde_json::from_slice(bytes)?;
    if document.version != FILE_VERSION {
        return Ok(BTreeMap::new());
    }
    Ok(document
        .apps
        .into_iter()
        .filter(|(_, record)| record.count.is_finite() && record.count >= 0.0)
        .collect())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    const DAY: u64 = 24 * 60 * 60;

    #[test]
    fn round_trip_keeps_boosts_with_a_fake_clock() {
        let dir = TempDir::new("usage-roundtrip");
        let path = UsageStats::default_path(dir.path());
        let mut stats = UsageStats::load(path.clone());
        stats.record("foot.desktop", 1_000);
        stats.record("foot.desktop", 2_000);
        stats.save().expect("save");

        let reloaded = UsageStats::load(path);
        assert_eq!(
            reloaded.boost("foot.desktop", 3_000),
            stats.boost("foot.desktop", 3_000)
        );
        assert!(reloaded.boost("foot.desktop", 3_000) > 0);
        assert_eq!(reloaded.boost("unknown.desktop", 3_000), 0);
    }

    #[test]
    fn counts_decay_with_the_half_life() {
        let mut stats = UsageStats::in_memory();
        stats.record("app.desktop", 0);
        let half_life_later = stats.score("app.desktop", HALF_LIFE_SECS);
        assert!((half_life_later - 0.5).abs() < 1e-9);
        stats.record("app.desktop", HALF_LIFE_SECS);
        assert!((stats.score("app.desktop", HALF_LIFE_SECS) - 1.5).abs() < 1e-9);
    }

    #[test]
    fn recent_use_outranks_old_use() {
        let mut stats = UsageStats::in_memory();
        for _ in 0..5 {
            stats.record("old.desktop", 0);
        }
        stats.record("new.desktop", 0);
        assert!(stats.boost("old.desktop", 0) > stats.boost("new.desktop", 0));

        stats.record("new.desktop", 400 * DAY);
        assert!(stats.boost("new.desktop", 400 * DAY) > stats.boost("old.desktop", 400 * DAY));
    }

    #[test]
    fn boost_is_bounded_and_clock_skew_is_harmless() {
        let mut stats = UsageStats::in_memory();
        for _ in 0..100 {
            stats.record("busy.desktop", 10);
        }
        assert_eq!(stats.boost("busy.desktop", 10), MAX_BOOST);
        assert_eq!(stats.boost("busy.desktop", 0), MAX_BOOST);
    }

    #[test]
    fn damaged_or_foreign_files_load_as_empty() {
        let dir = TempDir::new("usage-damaged");
        let path = dir.write("apps-usage.json", "{not json");
        assert_eq!(UsageStats::load(path.clone()).boost("x.desktop", 0), 0);
        let wrong_version = dir.write(
            "other.json",
            r#"{"version": 99, "apps": {"x.desktop": {"count": 4.0, "last_used": 0}}}"#,
        );
        assert_eq!(UsageStats::load(wrong_version).boost("x.desktop", 0), 0);
    }

    #[test]
    fn save_creates_missing_directories() {
        let dir = TempDir::new("usage-mkdir");
        let path = dir.path().join("nested/deeper/apps-usage.json");
        let mut stats = UsageStats::load(path.clone());
        stats.record("a.desktop", 5);
        stats.save().expect("save creates parents");
        assert!(path.is_file());
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn in_memory_save_is_a_no_op() {
        assert!(UsageStats::in_memory().save().is_ok());
    }
}
