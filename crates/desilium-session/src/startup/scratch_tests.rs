//! Temporary directories with fake executables for discovery tests. Removed on drop.

use std::fs;
use std::path::PathBuf;

/// A per-test directory under the system temp directory.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    /// Creates an empty directory named after `test` and the process id.
    pub(crate) fn new(test: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("desilium-session-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        Self(dir)
    }

    /// The root of this scratch directory.
    pub(crate) fn path(&self) -> PathBuf {
        self.0.clone()
    }

    /// Creates a fake executable at `relative` and returns its path.
    pub(crate) fn touch(&self, relative: &str) -> PathBuf {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap_or(&self.0)).expect("create dirs");
        fs::write(&path, b"#!/bin/sh\n").expect("write fake binary");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
