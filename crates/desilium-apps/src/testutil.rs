//! Fixture helpers for tests: temporary directories that are removed on drop.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use crate::env::{DESKTOP_NAME, Environment};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Unwraps fixture setup failures with context; fixtures exist only in tests.
fn fixture<T, E: std::fmt::Debug>(result: Result<T, E>, what: &str) -> T {
    match result {
        Ok(value) => value,
        // pureline:allow(PL-RS-004) test-only fixture helper, never compiled into the crate
        Err(err) => panic!("{what}: {err:?}"),
    }
}

/// A uniquely named directory under the system temp dir, deleted when dropped.
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    /// Creates a fresh, empty directory whose name contains `label`.
    pub(crate) fn new(label: &str) -> Self {
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "desilium-apps-{label}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fixture(fs::create_dir_all(&path), "create fixture directory");
        Self(path)
    }

    /// Root of the fixture directory.
    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `contents` to `relative` below the fixture root, creating parents.
    pub(crate) fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let target = self.0.join(relative);
        if let Some(parent) = target.parent() {
            fixture(fs::create_dir_all(parent), "create fixture parent");
        }
        fixture(fs::write(&target, contents), "write fixture file");
        target
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Moves a directory's mtime to a fixed past instant so that later writes are detectable
/// even on filesystems with coarse timestamps.
pub(crate) fn age_dir(path: &Path) {
    let past = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let handle = fixture(fs::File::open(path), "open fixture directory");
    fixture(handle.set_modified(past), "set fixture mtime");
}

/// Environment whose only data directories are the given ones, German locale, no PATH.
pub(crate) fn env_with_data_dirs(dirs: &[&Path]) -> Environment {
    Environment {
        home: PathBuf::from("/nonexistent-home"),
        data_dirs: dirs.iter().map(|dir| dir.to_path_buf()).collect(),
        path_dirs: Vec::new(),
        desktops: vec![DESKTOP_NAME.to_owned()],
        locales: vec!["de".to_owned()],
        state_home: PathBuf::from("/nonexistent-state"),
    }
}
