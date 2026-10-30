//! The long-running bridge: applies the configuration to KWin now and whenever it changes.
//!
//! One thread watches the configuration directory, one listens for shortcut presses, and the main
//! thread owns the [`Applier`]. Nothing here may stop the desktop from starting: every failure is
//! logged and retried, never returned to the session.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use desilium_apps::{Environment, LaunchCommand, TerminalSpec, launch::spawn};
use desilium_config::{ConfigStore, ReloadOutcome};
use desilium_config_types::DesiliumConfig;
use desilium_theme::Mode;
use inotify::{Inotify, WatchMask};

use crate::apply::{Applier, KConfigWriter};
use crate::plan::plan;
use crate::shortcuts::{self, LAUNCH_TERMINAL, QUIT, ShortcutEvent, ShortcutListener};

/// File name of the configuration inside its directory.
const CONFIG_FILE: &str = "config.toml";
/// Quiet time after the last file event before the configuration is re-read.
const DEBOUNCE: Duration = Duration::from_millis(150);
/// Waits between attempts while KWin or `kglobalacceld` is not up yet: doubling up to the cap.
const FIRST_RETRY: Duration = Duration::from_secs(1);
const RETRY_CAP: Duration = Duration::from_secs(30);
/// Attempts before the bridge stops retrying until the configuration changes again.
const MAX_RETRIES: u32 = 8;
const INOTIFY_BUFFER: usize = 4096;
/// How often the bridge looks for mice that were plugged in after the last apply. KWin has no
/// signal the bridge subscribes to for this, so it asks.
const DEVICE_RESCAN: Duration = Duration::from_secs(10);
/// Process name `desilium-session` has in `/proc/<pid>/comm`.
const SESSION_COMM: &str = "desilium-session";

/// How the bridge was started.
#[derive(Debug, Clone)]
pub struct Options {
    /// Directory holding `config.toml`.
    pub config_dir: PathBuf,
    /// `--theme dark|light`, as for the shell.
    pub theme: Option<Mode>,
    /// Apply once and return instead of watching.
    pub once: bool,
}

enum Event {
    ConfigTouched,
    Shortcut(ShortcutEvent),
}

/// Runs until the process is terminated (or after one pass with `once`).
pub fn run(options: &Options) -> io::Result<()> {
    let mut store = ConfigStore::open(&options.config_dir).store;
    let mut applier = Applier::new(KConfigWriter::default());
    let mut state = Retry::default();
    apply_now(options.theme, store.get(), &mut applier, &mut state);
    if options.once {
        return Ok(());
    }

    let (events, inbox) = mpsc::channel();
    watch_config(&options.config_dir, events.clone())?;
    let _listener = start_listener(events);

    let mut due: Option<Instant> = None;
    let mut rescan_at = Instant::now() + DEVICE_RESCAN;
    loop {
        let wake = next_wake(due, &state).map_or(rescan_at, |at| at.min(rescan_at));
        let received = inbox.recv_timeout(wake.saturating_duration_since(Instant::now()));
        match received {
            Ok(Event::ConfigTouched) => due = Some(Instant::now() + DEBOUNCE),
            Ok(Event::Shortcut(event)) => handle_shortcut(&event, store.get()),
            Err(RecvTimeoutError::Timeout) => {
                if due.is_some_and(|at| Instant::now() >= at) {
                    due = None;
                    reload(&mut store);
                    state = Retry::default();
                    apply_now(options.theme, store.get(), &mut applier, &mut state);
                } else if state.is_due() {
                    apply_now(options.theme, store.get(), &mut applier, &mut state);
                }
                if Instant::now() >= rescan_at {
                    rescan_at = Instant::now() + DEVICE_RESCAN;
                    let outcome = applier.rescan_pointer();
                    if outcome.written > 0 {
                        tracing::info!(written = outcome.written, "new mouse set up");
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

fn reload(store: &mut ConfigStore) {
    if let Some(ReloadOutcome::Rejected { reason }) = store.reload_if_changed() {
        tracing::warn!(%reason, "configuration change rejected, keeping the last good one");
    }
}

/// One apply attempt, with the retry bookkeeping around it.
fn apply_now(
    theme: Option<Mode>,
    config: &DesiliumConfig,
    applier: &mut Applier,
    state: &mut Retry,
) {
    let (wanted, warnings) = plan(config, theme);
    for warning in warnings {
        tracing::warn!("{warning}");
    }

    let bindings = shortcuts::bindings(&config.compositor);
    for skipped in &bindings.skipped {
        tracing::warn!("shortcut left out: {skipped}");
    }

    let outcome = applier.apply(&wanted, &bindings.list);
    if outcome.complete() {
        tracing::info!(written = outcome.written, "KWin is up to date");
        *state = Retry::default();
        return;
    }

    state.failed();

    match state.next_attempt {
        Some(_) => tracing::warn!(
            failed = outcome.failed,
            attempt = state.attempts,
            "KWin could not be fully configured yet, trying again"
        ),
        None => tracing::warn!(
            failed = outcome.failed,
            "giving up until the configuration changes; is KWin running?"
        ),
    }
}

/// Backoff state between failed apply attempts.
#[derive(Default)]
struct Retry {
    attempts: u32,
    next_attempt: Option<Instant>,
}

impl Retry {
    fn failed(&mut self) {
        self.attempts += 1;
        self.next_attempt = (self.attempts < MAX_RETRIES).then(|| {
            let factor = 1u32 << (self.attempts - 1).min(5);
            Instant::now() + (FIRST_RETRY * factor).min(RETRY_CAP)
        });
    }

    fn is_due(&self) -> bool {
        self.next_attempt.is_some_and(|at| Instant::now() >= at)
    }
}

fn next_wake(due: Option<Instant>, retry: &Retry) -> Option<Instant> {
    [due, retry.next_attempt].into_iter().flatten().min()
}

fn start_listener(events: Sender<Event>) -> Option<ShortcutListener> {
    ShortcutListener::spawn(move |event| {
        let _ = events.send(Event::Shortcut(event));
    })
    .inspect_err(|error| tracing::warn!(%error, "shortcut presses cannot be received"))
    .ok()
}

fn handle_shortcut(event: &ShortcutEvent, config: &DesiliumConfig) {
    match event.action.as_str() {
        LAUNCH_TERMINAL => launch_terminal(config.compositor.terminal.as_deref()),
        QUIT => end_session(),
        _ => {}
    }
}

fn launch_terminal(configured: Option<&str>) {
    let spec = configured
        .map(TerminalSpec::custom)
        .or_else(|| TerminalSpec::detect(&Environment::from_env()));
    let Some(spec) = spec else {
        tracing::warn!("no terminal emulator found");
        return;
    };
    if let Err(error) = spawn(&LaunchCommand::new(spec.program, Vec::new()), None) {
        tracing::warn!(%error, "terminal did not start");
    }
}

/// Asks the supervisor that started us to end the session. Only when it really is the session:
/// started by hand, the parent is somebody's shell and must not be signalled.
fn end_session() {
    let parent = std::os::unix::process::parent_id();
    let comm = fs::read_to_string(format!("/proc/{parent}/comm")).unwrap_or_default();
    if comm.trim() != SESSION_COMM {
        tracing::warn!("quit shortcut ignored: not started by {SESSION_COMM}");
        return;
    }

    tracing::info!(parent, "quit shortcut: ending the session");

    let status = std::process::Command::new("kill")
        .args(["-TERM", &parent.to_string()])
        .status();
    if let Err(error) = status {
        tracing::warn!(%error, "cannot signal the session");
    }
}

/// Watches the configuration directory and sends [`Event::ConfigTouched`] for `config.toml`.
fn watch_config(dir: &Path, events: Sender<Event>) -> io::Result<()> {
    fs::create_dir_all(dir)?;

    let mut inotify = Inotify::init()?;
    let mask = WatchMask::CLOSE_WRITE | WatchMask::MOVED_TO | WatchMask::CREATE | WatchMask::DELETE;
    inotify.watches().add(dir, mask)?;

    thread::Builder::new()
        .name("desilium-config-watch".to_owned())
        .spawn(move || {
            let mut buffer = [0u8; INOTIFY_BUFFER];
            loop {
                let Ok(batch) = inotify.read_events_blocking(&mut buffer) else {
                    tracing::warn!("configuration watch stopped; changes need a restart");
                    return;
                };

                let touched = batch
                    .filter_map(|event| event.name.map(std::ffi::OsStr::to_owned))
                    .any(|name| name == CONFIG_FILE);
                if touched && events.send(Event::ConfigTouched).is_err() {
                    return;
                }
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_up_to_the_cap_and_then_gives_up() {
        let mut retry = Retry::default();
        let mut waits = Vec::new();
        for _ in 0..MAX_RETRIES {
            retry.failed();
            waits.push(
                retry
                    .next_attempt
                    .map(|at| at.saturating_duration_since(Instant::now())),
            );
        }
        let secs = |wait: Option<Duration>| wait.map(|d| d.as_secs_f32().round() as u32);
        assert_eq!(secs(waits[0]), Some(1));
        assert_eq!(secs(waits[1]), Some(2));
        assert_eq!(secs(waits[2]), Some(4));
        assert_eq!(secs(waits[5]), Some(30));
        assert_eq!(
            waits[MAX_RETRIES as usize - 1],
            None,
            "the last attempt gives up"
        );
    }

    #[test]
    fn next_wake_is_the_earlier_of_debounce_and_retry() {
        let now = Instant::now();
        let later = now + Duration::from_secs(5);
        let retry = Retry {
            attempts: 1,
            next_attempt: Some(later),
        };
        assert_eq!(next_wake(Some(now), &retry), Some(now));
        assert_eq!(next_wake(None, &retry), Some(later));
        assert_eq!(next_wake(None, &Retry::default()), None);
    }

    #[test]
    fn config_watch_reports_only_the_config_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let (events, inbox) = mpsc::channel();
        watch_config(dir.path(), events).expect("watch");
        fs::write(dir.path().join("dock.toml"), "x").expect("write");
        assert!(inbox.recv_timeout(Duration::from_millis(300)).is_err());
        fs::write(dir.path().join(CONFIG_FILE), "x").expect("write");
        assert!(matches!(
            inbox.recv_timeout(Duration::from_secs(2)),
            Ok(Event::ConfigTouched)
        ));
    }
}
