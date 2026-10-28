//! inotify watch on the configuration directory; the loop polls its fd, nothing else wakes.

use std::{
    ffi::OsString,
    fs, io,
    os::fd::{AsFd, BorrowedFd},
    path::Path,
};

use inotify::{Inotify, WatchMask};

const BUFFER_BYTES: usize = 4096;

/// Watches one directory for files being written, created, moved in or removed.
pub(crate) struct DirWatch {
    inotify: Inotify,
}

impl DirWatch {
    /// Watches `dir`, creating it when missing (the shell writes `dock.toml` there anyway).
    pub(crate) fn new(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let inotify = Inotify::init()?;
        let mask = WatchMask::CLOSE_WRITE
            | WatchMask::MOVED_TO
            | WatchMask::MOVED_FROM
            | WatchMask::CREATE
            | WatchMask::DELETE;
        inotify.watches().add(dir, mask)?;
        Ok(Self { inotify })
    }

    /// Watches those of `dirs` that exist, without creating any; for system directories such as
    /// `/usr/share/applications`. Errors only when none could be watched.
    pub(crate) fn existing(dirs: impl IntoIterator<Item = std::path::PathBuf>) -> io::Result<Self> {
        let inotify = Inotify::init()?;
        let mask = WatchMask::CLOSE_WRITE
            | WatchMask::MOVED_TO
            | WatchMask::MOVED_FROM
            | WatchMask::CREATE
            | WatchMask::DELETE;
        let mut watched = 0usize;
        for dir in dirs.into_iter().filter(|dir| dir.is_dir()) {
            if inotify.watches().add(&dir, mask).is_ok() {
                watched += 1;
            }
        }
        if watched == 0 {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no application directory to watch",
            ));
        }
        Ok(Self { inotify })
    }

    /// The descriptor to poll for readability.
    pub(crate) fn fd(&self) -> BorrowedFd<'_> {
        self.inotify.as_fd()
    }

    /// Names of the files that changed since the last call.
    pub(crate) fn drain(&mut self) -> io::Result<Vec<OsString>> {
        let mut buffer = [0u8; BUFFER_BYTES];
        let mut names = Vec::new();
        loop {
            let events = match self.inotify.read_events(&mut buffer) {
                Ok(events) => events,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(names),
                Err(error) => return Err(error),
            };
            let before = names.len();
            names.extend(events.filter_map(|event| event.name.map(OsString::from)));
            if names.len() == before {
                return Ok(names);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_written_files_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut watch = DirWatch::new(dir.path()).unwrap();
        assert!(watch.drain().unwrap().is_empty());
        fs::write(dir.path().join("dock.toml"), "x").unwrap();
        let names = watch.drain().unwrap();
        assert!(names.iter().any(|n| n == "dock.toml"), "{names:?}");
        assert!(watch.drain().unwrap().is_empty());
    }

    #[test]
    fn existing_skips_missing_directories_and_fails_without_any() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");
        assert!(DirWatch::existing([missing.clone()]).is_err());
        assert!(!missing.exists(), "nothing is created");
        let mut watch = DirWatch::existing([missing, dir.path().to_path_buf()]).unwrap();
        fs::write(dir.path().join("app.desktop"), "x").unwrap();
        assert!(watch.drain().unwrap().iter().any(|n| n == "app.desktop"));
    }

    #[test]
    fn creates_a_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a/b");
        assert!(DirWatch::new(&nested).is_ok());
        assert!(nested.is_dir());
    }
}
