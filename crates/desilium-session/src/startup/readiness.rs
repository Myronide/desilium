//! Wayland readiness: the compositor is ready once it creates a new `wayland-N` socket.
//!
//! The compositor picks its own socket name (the first free `wayland-N` in `XDG_RUNTIME_DIR`),
//! so the supervisor cannot dictate it. Instead it lists the sockets before the compositor starts
//! and waits for one that was not there before. Clients never see a half-ready display.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

const SOCKET_PREFIX: &str = "wayland-";
const DEFAULT_SOCKET_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_SOCKET_POLL: Duration = Duration::from_millis(25);

/// Where and how long the supervisor waits for the compositor's socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaylandReadiness {
    /// Directory the compositor creates its socket in (`XDG_RUNTIME_DIR`).
    pub runtime_dir: PathBuf,
    /// Longest wait before the session gives up on the compositor.
    pub timeout: Duration,
    /// Interval between two directory listings.
    pub poll: Duration,
}

impl WaylandReadiness {
    /// Watches `runtime_dir` with the default timeout and poll interval.
    pub fn new(runtime_dir: impl Into<PathBuf>) -> Self {
        Self {
            runtime_dir: runtime_dir.into(),
            timeout: DEFAULT_SOCKET_TIMEOUT,
            poll: DEFAULT_SOCKET_POLL,
        }
    }
}

/// Result of waiting for the provider's socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketWait {
    /// The provider created this socket name.
    Ready(String),
    /// No new socket appeared before the timeout.
    TimedOut,
    /// The provider exited before creating a socket.
    ProviderExited,
}

/// Whether `name` looks like a Wayland display socket such as `wayland-1`.
pub fn is_socket_name(name: &str) -> bool {
    name.strip_prefix(SOCKET_PREFIX).is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// Lists the Wayland socket names currently present in `runtime_dir`.
pub fn list_sockets(runtime_dir: &Path) -> io::Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(runtime_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if let Some(name) = name.to_str().filter(|name| is_socket_name(name)) {
            names.insert(name.to_owned());
        }
    }
    Ok(names)
}

/// The socket present in `after` but not in `before`; the lowest name wins if several appeared.
pub fn new_socket(before: &BTreeSet<String>, after: &BTreeSet<String>) -> Option<String> {
    after.difference(before).next().cloned()
}

/// Polls until a socket that was absent from `before` appears, the provider stops or time runs out.
///
/// `still_running` is asked after each poll; it returns `false` once the provider has exited.
/// A socket that appears together with the provider's exit still counts as ready.
pub fn wait_for_socket(
    readiness: &WaylandReadiness,
    before: &BTreeSet<String>,
    mut still_running: impl FnMut() -> bool,
) -> SocketWait {
    let started = Instant::now();
    loop {
        let current = list_sockets(&readiness.runtime_dir).unwrap_or_default();
        if let Some(name) = new_socket(before, &current) {
            return SocketWait::Ready(name);
        }
        if !still_running() {
            return SocketWait::ProviderExited;
        }
        if started.elapsed() >= readiness.timeout {
            return SocketWait::TimedOut;
        }
        thread::sleep(readiness.poll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn scratch(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("desilium-readiness-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    fn quick(runtime_dir: PathBuf, timeout: Duration) -> WaylandReadiness {
        WaylandReadiness {
            runtime_dir,
            timeout,
            poll: Duration::from_millis(1),
        }
    }

    #[test]
    fn socket_names_follow_the_wayland_pattern() {
        assert!(is_socket_name("wayland-0"));
        assert!(is_socket_name("wayland-12"));
        assert!(!is_socket_name("wayland-0.lock"));
        assert!(!is_socket_name("wayland-"));
        assert!(!is_socket_name("wayland-x"));
        assert!(!is_socket_name("pulse-0"));
    }

    #[test]
    fn new_socket_is_the_difference_between_listings() {
        let before = names(&["wayland-0"]);
        let after = names(&["wayland-0", "wayland-1"]);
        assert_eq!(new_socket(&before, &after), Some("wayland-1".to_owned()));
        assert_eq!(new_socket(&after, &after), None);
    }

    #[test]
    fn listing_ignores_unrelated_entries() {
        let dir = scratch("list");
        fs::write(dir.join("wayland-1"), b"").expect("socket stand-in");
        fs::write(dir.join("wayland-1.lock"), b"").expect("lock file");
        fs::write(dir.join("pipewire-0"), b"").expect("other file");
        let listed = list_sockets(&dir).expect("readable dir");
        assert_eq!(listed, names(&["wayland-1"]));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn appearing_socket_is_reported_ready() {
        let dir = scratch("ready");
        fs::write(dir.join("wayland-0"), b"").expect("old socket");
        let before = list_sockets(&dir).expect("readable dir");
        fs::write(dir.join("wayland-1"), b"").expect("new socket");
        let outcome = wait_for_socket(&quick(dir.clone(), Duration::from_secs(5)), &before, || {
            true
        });
        assert_eq!(outcome, SocketWait::Ready("wayland-1".to_owned()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_socket_times_out() {
        let dir = scratch("timeout");
        let outcome = wait_for_socket(
            &quick(dir.clone(), Duration::ZERO),
            &BTreeSet::new(),
            || true,
        );
        assert_eq!(outcome, SocketWait::TimedOut);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn exited_provider_stops_the_wait_early() {
        let dir = scratch("exited");
        let outcome = wait_for_socket(
            &quick(dir.clone(), Duration::from_secs(60)),
            &BTreeSet::new(),
            || false,
        );
        assert_eq!(outcome, SocketWait::ProviderExited);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_runtime_dir_is_treated_as_no_socket_yet() {
        let dir = scratch("missing-dir").join("does-not-exist");
        let outcome = wait_for_socket(&quick(dir, Duration::ZERO), &BTreeSet::new(), || true);
        assert_eq!(outcome, SocketWait::TimedOut);
    }
}
