//! Event-driven detection of external edits to the configuration file.
//!
//! inotify watches the configuration directory. While the directory is missing,
//! the nearest existing ancestor is watched instead, filtered by the name of the
//! path component that leads to the directory. Relevant events arm a debounce
//! timer on the event loop, so a burst of writes (editors often write several
//! times) causes exactly one reload. Nothing polls; idle wake-ups are zero.

use std::cell::{Cell, RefCell};
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use inotify::{Event, EventMask, Inotify, WatchDescriptor, WatchMask, Watches};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use tracing::warn;

/// Name of the configuration file inside the watched directory.
pub const CONFIG_FILE_NAME: &str = "config.toml";
/// Quiet period after the last relevant event before the file is re-read.
const DEBOUNCE: Duration = Duration::from_millis(100);
/// Large enough for many events per read; leftovers stay queued for the next pass.
const EVENT_BUFFER_BYTES: usize = 4096;

/// Failure to start watching. Live updates are then disabled; the compositor keeps running.
#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("inotify is unavailable: {0}")]
    Inotify(#[from] io::Error),
    #[error("the event loop refused the watch: {0}")]
    EventLoop(smithay::reexports::calloop::Error),
}

/// Watches the configuration directory and calls `on_change` once per burst of changes.
///
/// The callback runs on the event loop, after the debounce period.
pub fn install<S: 'static>(
    handle: &LoopHandle<'static, S>,
    dir: PathBuf,
    on_change: impl Fn(&mut S) + 'static,
) -> Result<(), WatchError> {
    let inotify = Rc::new(RefCell::new(Inotify::init()?));
    let mut watch = DirWatch::new(dir);
    let mut watches = inotify.borrow().watches();
    watch.sync(&mut watches)?;
    // The loop owns a duplicate of the descriptor; the callback reads through the shared instance.
    let readiness_fd = inotify.borrow().as_fd().try_clone_to_owned()?;
    let debouncer = Debouncer::new(handle.clone(), Rc::new(on_change));
    let source = Generic::new(readiness_fd, Interest::READ, Mode::Level);
    handle
        .insert_source(source, move |_, _, _| {
            let mut inotify = inotify.borrow_mut();
            match watch.drain(&mut inotify) {
                Ok(true) => debouncer.arm(),
                Ok(false) => {}
                Err(error) => {
                    warn!(%error, "configuration watch stopped; external edits need a restart");
                    return Ok(PostAction::Remove);
                }
            }
            Ok(PostAction::Continue)
        })
        .map_err(|error| WatchError::EventLoop(error.error))?;
    Ok(())
}

/// Restarts a single timer per burst. Each relevant event pushes the reload back by [`DEBOUNCE`].
struct Debouncer<S> {
    handle: LoopHandle<'static, S>,
    on_change: Rc<dyn Fn(&mut S)>,
    pending: Rc<Cell<Option<RegistrationToken>>>,
}

impl<S: 'static> Debouncer<S> {
    fn new(handle: LoopHandle<'static, S>, on_change: Rc<dyn Fn(&mut S)>) -> Self {
        Self {
            handle,
            on_change,
            pending: Rc::new(Cell::new(None)),
        }
    }

    fn arm(&self) {
        if let Some(previous) = self.pending.take() {
            self.handle.remove(previous);
        }
        let pending = Rc::clone(&self.pending);
        let on_change = Rc::clone(&self.on_change);
        let timer = Timer::from_duration(DEBOUNCE);
        let inserted = self.handle.insert_source(timer, move |_, _, state| {
            pending.set(None);
            on_change(state);
            TimeoutAction::Drop
        });
        match inserted {
            Ok(token) => self.pending.set(Some(token)),
            Err(error) => warn!(error = %error.error, "cannot schedule a configuration reload"),
        }
    }
}

/// Keeps the inotify watches matched to whether the directory currently exists.
struct DirWatch {
    dir: PathBuf,
    /// Watch on the directory itself, present while it exists.
    dir_wd: Option<WatchDescriptor>,
    /// Watch on the nearest existing ancestor, present while the directory is missing.
    anchor: Option<Anchor>,
}

struct Anchor {
    wd: WatchDescriptor,
    /// Name of the entry inside the anchor that leads toward the directory.
    child: OsString,
}

impl DirWatch {
    fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            dir_wd: None,
            anchor: None,
        }
    }

    /// Reads all queued events. Returns `true` when the configuration file may have changed.
    fn drain(&mut self, inotify: &mut Inotify) -> io::Result<bool> {
        let mut buffer = [0u8; EVENT_BUFFER_BYTES];
        let events = match inotify.read_events(&mut buffer) {
            Ok(events) => events,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) => return Err(error),
        };
        let mut watches = inotify.watches();
        let mut reload = false;
        for event in events {
            match self.observe(&event, &mut watches) {
                Ok(changed) => reload |= changed,
                Err(error) => {
                    warn!(%error, "configuration watch could not follow a directory change")
                }
            }
        }
        Ok(reload)
    }

    /// Interprets one event. Returns `true` when the configuration file may have changed.
    fn observe(&mut self, event: &Event<&OsStr>, watches: &mut Watches) -> io::Result<bool> {
        if self.dir_wd.as_ref() == Some(&event.wd) {
            return self.observe_dir(event, watches);
        }
        let is_anchor = self
            .anchor
            .as_ref()
            .is_some_and(|anchor| anchor.wd == event.wd);
        if is_anchor {
            return self.observe_anchor(event, watches);
        }
        Ok(false)
    }

    fn observe_dir(&mut self, event: &Event<&OsStr>, watches: &mut Watches) -> io::Result<bool> {
        if event.mask.contains(EventMask::IGNORED) {
            // The kernel dropped the watch, which happens when the directory is removed.
            self.dir_wd = None;
            self.sync(watches)?;
            return Ok(true);
        }
        let Some(name) = event.name else {
            return Ok(false);
        };
        Ok(name == CONFIG_FILE_NAME && touches_file(event.mask))
    }

    fn observe_anchor(&mut self, event: &Event<&OsStr>, watches: &mut Watches) -> io::Result<bool> {
        if event.mask.contains(EventMask::IGNORED) {
            self.anchor = None;
            self.sync(watches)?;
            return Ok(false);
        }
        let Some(name) = event.name else {
            return Ok(false);
        };
        let concerns_dir = self
            .anchor
            .as_ref()
            .is_some_and(|anchor| anchor.child == name);
        if !concerns_dir {
            return Ok(false);
        }
        self.sync(watches)?;
        Ok(true)
    }

    /// Brings the watches in line with the directory's current existence.
    fn sync(&mut self, watches: &mut Watches) -> io::Result<()> {
        if self.dir.is_dir() {
            if let Some(anchor) = self.anchor.take() {
                watches.remove(anchor.wd).ok();
            }
            if self.dir_wd.is_none() {
                self.dir_wd = Some(watches.add(&self.dir, dir_mask())?);
            }
            return Ok(());
        }
        if let Some(wd) = self.dir_wd.take() {
            watches.remove(wd).ok();
        }
        if let Some(previous) = self.anchor.take() {
            watches.remove(previous.wd).ok();
        }
        self.anchor = nearest_anchor(&self.dir, watches)?;
        Ok(())
    }
}

fn nearest_anchor(dir: &Path, watches: &mut Watches) -> io::Result<Option<Anchor>> {
    let Some((ancestor, child)) = ancestor_and_child(dir) else {
        return Ok(None);
    };
    let wd = watches.add(&ancestor, anchor_mask())?;
    Ok(Some(Anchor { wd, child }))
}

/// The nearest existing ancestor of `dir`, and the name of the entry below it on the way to `dir`.
fn ancestor_and_child(dir: &Path) -> Option<(PathBuf, OsString)> {
    let ancestor = dir
        .ancestors()
        .skip(1)
        .find(|candidate| candidate.is_dir())?;
    let child = dir.strip_prefix(ancestor).ok()?.components().next()?;
    Some((ancestor.to_path_buf(), child.as_os_str().to_owned()))
}

fn dir_mask() -> WatchMask {
    WatchMask::CREATE
        | WatchMask::DELETE
        | WatchMask::MODIFY
        | WatchMask::CLOSE_WRITE
        | WatchMask::MOVED_TO
        | WatchMask::MOVED_FROM
        | WatchMask::DELETE_SELF
        | WatchMask::MOVE_SELF
}

fn anchor_mask() -> WatchMask {
    WatchMask::CREATE | WatchMask::DELETE | WatchMask::MOVED_TO | WatchMask::MOVED_FROM
}

/// Events that can mean the configuration file was created, replaced, changed or removed.
fn touches_file(mask: EventMask) -> bool {
    mask.intersects(
        EventMask::CREATE
            | EventMask::DELETE
            | EventMask::MODIFY
            | EventMask::CLOSE_WRITE
            | EventMask::MOVED_TO
            | EventMask::MOVED_FROM,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::reexports::calloop::EventLoop;
    use std::fs;
    use std::time::Instant;

    const DISPATCH_STEP: Duration = Duration::from_millis(10);
    const FIRST_CHANGE_TIMEOUT: Duration = Duration::from_secs(5);
    /// Long enough to see whether a second, unwanted reload follows the first.
    const SETTLE_WINDOW: Duration = Duration::from_millis(400);
    const WRITES_IN_BURST: u32 = 6;

    /// Runs the loop until `done` holds or `limit` passes.
    fn dispatch_until(
        event_loop: &mut EventLoop<'static, ()>,
        limit: Duration,
        done: impl Fn() -> bool,
    ) {
        let deadline = Instant::now() + limit;
        while !done() && Instant::now() < deadline {
            if event_loop.dispatch(Some(DISPATCH_STEP), &mut ()).is_err() {
                return;
            }
        }
    }

    fn counting_watch(dir: PathBuf) -> Option<(EventLoop<'static, ()>, Rc<Cell<u32>>)> {
        let event_loop = EventLoop::try_new().ok()?;
        let calls = Rc::new(Cell::new(0));
        let counter = Rc::clone(&calls);
        match install(&event_loop.handle(), dir, move |_: &mut ()| {
            counter.set(counter.get() + 1);
        }) {
            Ok(()) => Some((event_loop, calls)),
            Err(error) => {
                eprintln!("skipping inotify test: {error}");
                None
            }
        }
    }

    #[test]
    fn ancestor_is_the_first_existing_parent() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("missing").join("desilium");
        let (ancestor, child) = ancestor_and_child(&dir).expect("anchor");
        assert_eq!(ancestor, root.path());
        assert_eq!(child, OsString::from("missing"));
    }

    #[test]
    fn existing_directory_needs_no_anchor() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("desilium");
        fs::create_dir_all(&dir).expect("mkdir");
        let (ancestor, child) = ancestor_and_child(&dir).expect("anchor");
        assert_eq!(ancestor, root.path());
        assert_eq!(child, OsString::from("desilium"));
    }

    #[test]
    fn touches_file_accepts_create_modify_move_and_delete() {
        for mask in [
            EventMask::CREATE,
            EventMask::MODIFY,
            EventMask::MOVED_TO,
            EventMask::MOVED_FROM,
            EventMask::DELETE,
            EventMask::CLOSE_WRITE,
        ] {
            assert!(touches_file(mask), "{mask:?}");
        }
        assert!(!touches_file(EventMask::ACCESS));
        assert!(!touches_file(EventMask::ATTRIB));
    }

    #[test]
    fn burst_of_writes_causes_one_reload() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("desilium");
        fs::create_dir_all(&dir).expect("mkdir");
        let Some((mut event_loop, calls)) = counting_watch(dir.clone()) else {
            return;
        };
        for round in 0..WRITES_IN_BURST {
            fs::write(
                dir.join(CONFIG_FILE_NAME),
                format!("cursor_size = {round}\n"),
            )
            .expect("write");
        }
        dispatch_until(&mut event_loop, FIRST_CHANGE_TIMEOUT, || calls.get() > 0);
        dispatch_until(&mut event_loop, SETTLE_WINDOW, || false);
        assert_eq!(calls.get(), 1, "one burst must produce one reload");
    }

    #[test]
    fn unrelated_files_do_not_trigger_a_reload() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("desilium");
        fs::create_dir_all(&dir).expect("mkdir");
        let Some((mut event_loop, calls)) = counting_watch(dir.clone()) else {
            return;
        };
        fs::write(dir.join("notes.txt"), "hello").expect("write");
        dispatch_until(&mut event_loop, SETTLE_WINDOW, || false);
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn directory_created_later_is_picked_up() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("desilium");
        let Some((mut event_loop, calls)) = counting_watch(dir.clone()) else {
            return;
        };
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join(CONFIG_FILE_NAME), "cursor_size = 40\n").expect("write");
        dispatch_until(&mut event_loop, FIRST_CHANGE_TIMEOUT, || calls.get() > 0);
        dispatch_until(&mut event_loop, SETTLE_WINDOW, || false);
        assert!(calls.get() >= 1, "a late directory must still be watched");
    }
}
