//! Detects edits of the settings file with inotify, without polling.
//!
//! The settings directory is watched, not the file, because the store replaces
//! the file atomically. While the directory does not exist yet (nothing was ever
//! saved) its nearest existing ancestor is watched until it appears.

use std::{
    ffi::{OsStr, OsString},
    io,
    os::fd::{AsFd, BorrowedFd},
    path::{Path, PathBuf},
};

use inotify::{EventMask, Inotify, WatchDescriptor, WatchMask};

const CONFIG_FILE_NAME: &str = "config.toml";
const EVENT_BUFFER_BYTES: usize = 4096;
const MASK: WatchMask = WatchMask::CREATE
    .union(WatchMask::MODIFY)
    .union(WatchMask::CLOSE_WRITE)
    .union(WatchMask::MOVED_TO)
    .union(WatchMask::MOVED_FROM)
    .union(WatchMask::DELETE);

/// The directory to watch now: `dir` if it exists, else its closest existing ancestor.
pub fn nearest_existing_ancestor(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|path| path.is_dir())
        .map(Path::to_path_buf)
}

/// First path component of `dir` below `base`, the name whose creation matters while watching `base`.
fn child_towards(base: &Path, dir: &Path) -> Option<OsString> {
    dir.strip_prefix(base)
        .ok()?
        .components()
        .next()
        .map(|component| component.as_os_str().to_owned())
}

/// Watches the directory of the settings file.
pub struct ConfigWatcher {
    inotify: Inotify,
    dir: PathBuf,
    watching: Option<(WatchDescriptor, PathBuf)>,
}

impl ConfigWatcher {
    /// Starts watching. Fails only when inotify itself is unavailable.
    pub fn new(dir: PathBuf) -> io::Result<Self> {
        let mut watcher = Self {
            inotify: Inotify::init()?,
            dir,
            watching: None,
        };
        watcher.sync()?;
        Ok(watcher)
    }

    /// Descriptor to poll for readability.
    pub fn fd(&self) -> BorrowedFd<'_> {
        self.inotify.as_fd()
    }

    /// Moves the watch to the best directory available now.
    fn sync(&mut self) -> io::Result<()> {
        let Some(target) = nearest_existing_ancestor(&self.dir) else {
            return Ok(());
        };
        if self
            .watching
            .as_ref()
            .is_some_and(|(_, path)| *path == target)
        {
            return Ok(());
        }
        if let Some((descriptor, _)) = self.watching.take() {
            // The old directory may be gone already; that is fine.
            let _ = self.inotify.watches().remove(descriptor);
        }
        let descriptor = self.inotify.watches().add(&target, MASK)?;
        self.watching = Some((descriptor, target));
        Ok(())
    }

    /// Reads pending events. True when the settings file may have changed.
    pub fn drain(&mut self) -> io::Result<bool> {
        let mut buffer = [0u8; EVENT_BUFFER_BYTES];
        let mut relevant = false;
        loop {
            let events = match self.inotify.read_events(&mut buffer) {
                Ok(events) => events,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            };
            let mut any = false;
            for event in events {
                any = true;
                relevant |= self.is_relevant(event.name, event.mask);
            }
            if !any {
                break;
            }
        }
        self.sync()?;
        Ok(relevant)
    }

    fn is_relevant(&self, name: Option<&OsStr>, mask: EventMask) -> bool {
        if mask.contains(EventMask::IGNORED) {
            return true;
        }
        let (Some(name), Some((_, watched))) = (name, &self.watching) else {
            return false;
        };
        if *watched == self.dir {
            return name == OsStr::new(CONFIG_FILE_NAME);
        }
        child_towards(watched, &self.dir).is_some_and(|child| child == name)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn nearest_ancestor_skips_missing_directories() {
        let dir = tempfile::tempdir().expect("dir");
        let deep = dir.path().join("a/b/c");
        assert_eq!(
            nearest_existing_ancestor(&deep),
            Some(dir.path().to_path_buf())
        );
        fs::create_dir_all(dir.path().join("a")).expect("mkdir");
        assert_eq!(nearest_existing_ancestor(&deep), Some(dir.path().join("a")));
    }

    #[test]
    fn child_towards_names_the_next_component() {
        assert_eq!(
            child_towards(Path::new("/x"), Path::new("/x/desilium/sub")),
            Some(OsString::from("desilium"))
        );
        assert_eq!(
            child_towards(Path::new("/y"), Path::new("/x/desilium")),
            None
        );
    }

    #[test]
    fn writing_the_config_file_is_reported_and_other_files_are_not() {
        let dir = tempfile::tempdir().expect("dir");
        let mut watcher = ConfigWatcher::new(dir.path().to_path_buf()).expect("watcher");
        assert!(!watcher.drain().expect("idle"));
        fs::write(dir.path().join("other.txt"), b"x").expect("write");
        assert!(!watcher.drain().expect("other"));
        fs::write(dir.path().join(CONFIG_FILE_NAME), b"x").expect("write");
        assert!(watcher.drain().expect("config"));
        assert!(!watcher.drain().expect("drained"));
    }

    #[test]
    fn a_missing_directory_is_picked_up_when_it_appears() {
        let base = tempfile::tempdir().expect("dir");
        let dir = base.path().join("desilium");
        let mut watcher = ConfigWatcher::new(dir.clone()).expect("watcher");
        fs::create_dir(&dir).expect("mkdir");
        assert!(
            watcher.drain().expect("created"),
            "directory creation may precede the file"
        );
        fs::write(dir.join(CONFIG_FILE_NAME), b"x").expect("write");
        assert!(watcher.drain().expect("file"));
    }
}
