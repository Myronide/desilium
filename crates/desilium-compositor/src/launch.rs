use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};
use tracing::{info, warn};

const TERMINAL_CANDIDATES: [&str; 5] = ["kitty", "foot", "konsole", "alacritty", "weston-terminal"];

/// File name of the desktop shell. The compositor trusts shell clients by this name
/// (see `state/shell/trust.rs`), so a shell started from another file name cannot use the protocol.
pub const SHELL_BINARY: &str = "desilium-shell";
/// Asks a shell started for the launcher shortcut to show its launcher.
const OPEN_LAUNCHER_FLAG: &str = "--open-launcher";
/// Shortest time between two attempts to start a shell for the launcher.
const LAUNCHER_RETRY: Duration = Duration::from_secs(1);

/// True while a shell started by this compositor is running.
static SHELL_RUNNING: AtomicBool = AtomicBool::new(false);

/// Bookkeeping of the launcher fallback; only the event loop touches it.
struct LauncherAttempts {
    last: Option<Instant>,
    missing_reported: bool,
}

static LAUNCHER_ATTEMPTS: Mutex<LauncherAttempts> = Mutex::new(LauncherAttempts {
    last: None,
    missing_reported: false,
});

/// The configured terminal, or the first known emulator found on `PATH`.
pub fn find_terminal(configured: Option<&str>) -> Option<String> {
    if let Some(program) = configured {
        return Some(program.to_owned());
    }
    TERMINAL_CANDIDATES
        .iter()
        .find(|program| is_on_path(program))
        .map(|program| (*program).to_owned())
}

fn is_on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| is_executable(&dir.join(program)))
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Starts a client that talks to this compositor, never to the host session.
pub fn spawn(program: &str, socket_name: &OsStr) {
    let words = split_words(program);
    let mut parts = words.iter();
    let Some(binary) = parts.next() else {
        return;
    };
    let mut command = Command::new(binary);
    command.args(parts);
    configure_client(&mut command, socket_name);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            warn!(%program, %error, "cannot start client");
            return;
        }
    };
    info!(%program, pid = child.id(), "client started");
    // A small reaper thread keeps exited clients from lingering as zombies.
    std::thread::spawn(move || child.wait());
}

/// The environment every client gets: this compositor's display and no X11 display.
fn configure_client(command: &mut Command, socket_name: &OsStr) {
    command
        .env("WAYLAND_DISPLAY", socket_name)
        .env("XDG_SESSION_TYPE", "wayland")
        .env_remove("DISPLAY");
}

/// Delay before the first restart of an exited shell; it grows with each quick failure.
const SHELL_RESTART_DELAY: Duration = Duration::from_millis(500);
/// A shell that ran this long counts as healthy and resets the failure count.
const SHELL_STABLE_RUN: Duration = Duration::from_secs(10);
/// Quick failures in a row after which the compositor stops restarting the shell.
const SHELL_MAX_QUICK_FAILURES: u32 = 5;

/// Starts the desktop shell at `program` and keeps it running.
///
/// An exited shell is started again (after a short delay, so a shell that crashes at once does not
/// spin); `args` apply to the first start only. After repeated quick failures the compositor
/// gives up and the launcher shortcut may start a shell again. Under a session supervisor the
/// compositor is started with `--no-shell` and this is not used.
/// Returns false when the first start failed.
pub fn start_shell(program: &Path, args: &[&str], socket_name: &OsStr) -> bool {
    let Some(child) = spawn_shell(program, args, socket_name) else {
        return false;
    };
    SHELL_RUNNING.store(true, Ordering::SeqCst);
    let program = program.to_path_buf();
    let socket_name = socket_name.to_owned();
    std::thread::spawn(move || supervise_shell(child, &program, &socket_name));
    true
}

fn spawn_shell(program: &Path, args: &[&str], socket_name: &OsStr) -> Option<Child> {
    let mut command = Command::new(program);
    command.args(args);
    configure_client(&mut command, socket_name);
    match command.spawn() {
        Ok(child) => {
            info!(path = %program.display(), pid = child.id(), "desilium-shell started");
            Some(child)
        }
        Err(error) => {
            warn!(path = %program.display(), %error, "cannot start desilium-shell");
            None
        }
    }
}

/// Waits for the shell and starts it again each time it exits.
fn supervise_shell(mut child: Child, program: &Path, socket_name: &OsStr) {
    let mut quick_failures = 0u32;
    loop {
        let started = Instant::now();
        match child.wait() {
            Ok(status) => warn!(%status, "desilium-shell exited"),
            Err(error) => warn!(%error, "waiting for desilium-shell failed"),
        }
        quick_failures = if started.elapsed() >= SHELL_STABLE_RUN {
            0
        } else {
            quick_failures + 1
        };
        if quick_failures >= SHELL_MAX_QUICK_FAILURES {
            warn!("desilium-shell keeps exiting at once; not restarting it again");
            break;
        }
        std::thread::sleep(SHELL_RESTART_DELAY * quick_failures.max(1));
        match spawn_shell(program, &[], socket_name) {
            Some(next) => child = next,
            None => break,
        }
    }
    SHELL_RUNNING.store(false, Ordering::SeqCst);
}

/// Starts a shell for the launcher shortcut when no shell of this compositor is running.
///
/// Attempts are rate-limited, so a missing or broken shell is not respawned on every key press.
/// The first missing shell is logged once; later ones stay quiet.
pub fn open_launcher(socket_name: &OsStr, now: Instant) {
    if SHELL_RUNNING.load(Ordering::SeqCst) {
        return;
    }
    let mut attempts = LAUNCHER_ATTEMPTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if !retry_allowed(attempts.last, now) {
        return;
    }
    attempts.last = Some(now);
    let Some(shell) = find_shell() else {
        if !attempts.missing_reported {
            attempts.missing_reported = true;
            info!(
                "no desilium-shell next to the compositor or on PATH; the launcher is unavailable"
            );
        }
        return;
    };
    start_shell(&shell, &[OPEN_LAUNCHER_FLAG], socket_name);
}

/// Whether a launcher attempt at `now` is allowed after the previous one at `last`.
pub fn retry_allowed(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.saturating_duration_since(last) >= LAUNCHER_RETRY)
}

/// The shell executable: next to the compositor binary first, then anywhere on `PATH`.
pub fn find_shell() -> Option<PathBuf> {
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    find_in(sibling.into_iter().chain(path_dirs))
}

/// The first executable `SHELL_BINARY` in `dirs`, in order.
fn find_in(dirs: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    dirs.into_iter()
        .map(|dir| dir.join(SHELL_BINARY))
        .find(|candidate| is_executable(candidate))
}

/// Splits a command line into words, honouring single and double quotes.
fn split_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    for ch in line.chars() {
        match (quote, ch) {
            (Some(open), _) if ch == open => quote = None,
            (Some(_), _) => current.push(ch),
            (None, '\'' | '"') => {
                quote = Some(ch);
                in_word = true;
            }
            (None, _) if ch.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            (None, _) => {
                current.push(ch);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A fresh scratch directory for one test; removed by the caller.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("desilium-launch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    fn write_file(path: &Path, mode: u32) {
        fs::write(path, b"#!/bin/sh\n").expect("write file");
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("chmod");
    }

    #[test]
    fn finds_an_executable_shell_in_the_first_directory_that_has_one() {
        let first = scratch("first");
        let second = scratch("second");
        write_file(&second.join(SHELL_BINARY), 0o755);
        let found = find_in([first.clone(), second.clone()]);
        assert_eq!(found, Some(second.join(SHELL_BINARY)));
        let _ = fs::remove_dir_all(&first);
        let _ = fs::remove_dir_all(&second);
    }

    #[test]
    fn ignores_a_shell_that_is_not_executable() {
        let dir = scratch("plain");
        write_file(&dir.join(SHELL_BINARY), 0o644);
        assert_eq!(find_in([dir.clone()]), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_other_program_names() {
        let dir = scratch("other");
        write_file(&dir.join("desilium-settings"), 0o755);
        assert_eq!(find_in([dir.clone()]), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launcher_attempts_are_rate_limited_to_one_per_second() {
        let start = Instant::now();
        assert!(retry_allowed(None, start));
        let soon = start + Duration::from_millis(999);
        assert!(!retry_allowed(Some(start), soon));
        let later = start + LAUNCHER_RETRY;
        assert!(retry_allowed(Some(start), later));
    }

    #[test]
    fn a_clock_going_backwards_does_not_panic_or_allow_a_burst() {
        let start = Instant::now();
        let earlier = start.checked_sub(Duration::from_secs(5)).unwrap_or(start);
        assert!(!retry_allowed(Some(start), earlier));
    }

    #[test]
    fn splits_plain_words() {
        assert_eq!(split_words("foot  -e  htop"), ["foot", "-e", "htop"]);
    }

    #[test]
    fn keeps_quoted_groups_together() {
        assert_eq!(
            split_words("foot -e sh -c 'echo hi; sleep 5'"),
            ["foot", "-e", "sh", "-c", "echo hi; sleep 5"]
        );
        assert_eq!(split_words(r#"a "b c" d"#), ["a", "b c", "d"]);
    }

    #[test]
    fn empty_quotes_make_an_empty_word() {
        assert_eq!(split_words("a '' b"), ["a", "", "b"]);
        assert!(split_words("   ").is_empty());
    }
}
