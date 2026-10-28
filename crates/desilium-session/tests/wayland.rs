// Test code: a failing unwrap or expect is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Wayland gate tests: `sh` scripts stand in for the compositor (provider) and for clients.
//!
//! The provider creates a `wayland-N` file in a scratch runtime directory the same way the real
//! compositor creates its socket. Clients record what they see, so the tests check ordering and
//! the `WAYLAND_DISPLAY` hand-over without a real display.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use desilium_session::{
    ComponentSpec, Criticality, RestartPolicy, SessionError, SessionOutcome, SignalListener,
    Supervisor, SupervisorEvent, WaylandReadiness, WaylandRole,
};

const SHELL: &str = "/bin/sh";
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
const LOG_ENV: &str = "DESILIUM_TEST_LOG";
const RUNTIME_ENV: &str = "XDG_RUNTIME_DIR";

/// A per-test directory under the system temp dir, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "desilium-session-wayland-{test}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        Self(dir)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn quick_readiness(runtime_dir: &Path, timeout: Duration) -> WaylandReadiness {
    WaylandReadiness {
        runtime_dir: runtime_dir.to_path_buf(),
        timeout,
        poll: Duration::from_millis(5),
    }
}

fn policy() -> RestartPolicy {
    RestartPolicy {
        initial_backoff: Duration::from_millis(5),
        max_backoff: Duration::from_millis(20),
        factor: 2,
        stable_after: Duration::from_secs(3600),
        crash_loop_max: 100,
        crash_loop_window: Duration::from_secs(3600),
    }
}

fn script_component(
    name: &str,
    script: &str,
    criticality: Criticality,
    wayland: WaylandRole,
    env: Vec<(OsString, OsString)>,
) -> ComponentSpec {
    let mut spec = ComponentSpec::new(name, SHELL, criticality);
    spec.args = vec![OsString::from("-c"), OsString::from(script)];
    spec.policy = policy();
    spec.env = env;
    spec.wayland = wayland;
    spec
}

fn env_pair(key: &str, value: &Path) -> (OsString, OsString) {
    (OsString::from(key), value.as_os_str().to_owned())
}

fn supervise(
    specs: Vec<ComponentSpec>,
    readiness: Option<WaylandReadiness>,
) -> Result<SessionOutcome, SessionError> {
    let (sender, events) = mpsc::channel::<SupervisorEvent>();
    let _listener = SignalListener::start(sender)?;
    let mut supervisor = Supervisor::new(specs).with_shutdown_grace(SHUTDOWN_GRACE);
    if let Some(readiness) = readiness {
        supervisor = supervisor.with_wayland_readiness(readiness);
    }
    supervisor.run_with(events)
}

#[test]
fn clients_start_after_the_socket_and_receive_its_name() {
    let scratch = Scratch::new("handover");
    let runtime = scratch.file("runtime");
    fs::create_dir_all(&runtime).expect("runtime dir");
    fs::write(runtime.join("wayland-0"), b"").expect("pre-existing socket of another display");
    let log = scratch.file("client.log");
    let runtime_env = vec![env_pair(RUNTIME_ENV, &runtime)];

    let provider = script_component(
        "compositor",
        r#"touch "$XDG_RUNTIME_DIR/wayland-1"; sleep 0.3"#,
        Criticality::Critical,
        WaylandRole::Provider,
        runtime_env.clone(),
    );
    let mut client_env = runtime_env;
    client_env.push(env_pair(LOG_ENV, &log));
    let client = script_component(
        "shell",
        r#"{ printf '%s\n' "$WAYLAND_DISPLAY"; [ -e "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ] && echo socket-present; } >> "$DESILIUM_TEST_LOG"; exit 0"#,
        Criticality::Restartable,
        WaylandRole::Client,
        client_env,
    );

    let outcome = supervise(
        vec![provider, client],
        Some(quick_readiness(&runtime, Duration::from_secs(5))),
    )
    .expect("session runs");

    assert_eq!(outcome, SessionOutcome::Normal);
    let seen = fs::read_to_string(&log).expect("client wrote its log");
    assert!(seen.lines().any(|line| line == "wayland-1"), "got {seen:?}");
    assert!(!seen.lines().any(|line| line == "wayland-0"));
    assert!(
        seen.contains("socket-present"),
        "client started before the socket existed"
    );
}

#[test]
fn provider_that_never_creates_a_socket_times_out() {
    let scratch = Scratch::new("timeout");
    let runtime = scratch.file("runtime");
    fs::create_dir_all(&runtime).expect("runtime dir");
    let provider = script_component(
        "compositor",
        "sleep 30",
        Criticality::Critical,
        WaylandRole::Provider,
        Vec::new(),
    );

    let error = supervise(
        vec![provider],
        Some(quick_readiness(&runtime, Duration::from_millis(150))),
    )
    .expect_err("no socket appears");

    assert!(
        matches!(error, SessionError::WaylandSocketTimeout { .. }),
        "got {error:?}"
    );
}

#[test]
fn provider_that_exits_before_its_socket_is_reported() {
    let scratch = Scratch::new("early-exit");
    let runtime = scratch.file("runtime");
    fs::create_dir_all(&runtime).expect("runtime dir");
    let provider = script_component(
        "compositor",
        "exit 3",
        Criticality::Critical,
        WaylandRole::Provider,
        Vec::new(),
    );

    let error = supervise(
        vec![provider],
        Some(quick_readiness(&runtime, Duration::from_secs(5))),
    )
    .expect_err("provider exits first");

    assert!(
        matches!(error, SessionError::ProviderExitedEarly { ref name } if name == "compositor"),
        "got {error:?}"
    );
}

#[test]
fn client_listed_before_a_provider_is_rejected() {
    let scratch = Scratch::new("no-provider");
    let client = script_component(
        "shell",
        "exit 0",
        Criticality::Restartable,
        WaylandRole::Client,
        Vec::new(),
    );

    let error = supervise(
        vec![client],
        Some(quick_readiness(
            &scratch.file("runtime"),
            Duration::from_secs(1),
        )),
    )
    .expect_err("client needs a provider");

    assert!(
        matches!(error, SessionError::NoWaylandProvider { .. }),
        "got {error:?}"
    );
}

#[test]
fn provider_without_a_runtime_directory_is_rejected() {
    let provider = script_component(
        "compositor",
        "exit 0",
        Criticality::Critical,
        WaylandRole::Provider,
        Vec::new(),
    );

    let error = supervise(vec![provider], None).expect_err("readiness is required");

    assert!(
        matches!(error, SessionError::MissingRuntimeDir { .. }),
        "got {error:?}"
    );
}

#[test]
fn unreadable_runtime_directory_is_reported_before_start() {
    let scratch = Scratch::new("missing-runtime");
    let provider = script_component(
        "compositor",
        "exit 0",
        Criticality::Critical,
        WaylandRole::Provider,
        Vec::new(),
    );

    let error = supervise(
        vec![provider],
        Some(quick_readiness(
            &scratch.file("absent"),
            Duration::from_secs(1),
        )),
    )
    .expect_err("directory does not exist");

    assert!(
        matches!(error, SessionError::RuntimeDir { .. }),
        "got {error:?}"
    );
}
