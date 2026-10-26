//! Supervisor integration tests with tiny `sh` components and millisecond-scale policies.
//!
//! Each test waits on the supervisor's own outcome instead of sleeping for a fixed time.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use desilium_session::{
    ComponentSpec, Criticality, RestartPolicy, SessionError, SessionOutcome, SignalListener,
    Supervisor, SupervisorEvent,
};

const SHELL: &str = "/bin/sh";
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
const LOG_ENV: &str = "DESILIUM_TEST_LOG";
const PID_ENV: &str = "DESILIUM_TEST_PID";
const COMPOSITOR_PID_ENV: &str = "DESILIUM_TEST_COMPOSITOR_PID";

/// A per-test directory under the system temp dir, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("desilium-session-{test}-{}", std::process::id()));
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

fn fast_policy(crash_loop_max: usize) -> RestartPolicy {
    RestartPolicy {
        initial_backoff: Duration::from_millis(5),
        max_backoff: Duration::from_millis(20),
        factor: 2,
        stable_after: Duration::from_secs(3600),
        crash_loop_max,
        crash_loop_window: Duration::from_secs(3600),
    }
}

fn env_var(key: &str, value: &Path) -> (OsString, OsString) {
    (OsString::from(key), value.as_os_str().to_owned())
}

fn sh_component(
    name: &str,
    script: &str,
    criticality: Criticality,
    policy: RestartPolicy,
    env: Vec<(OsString, OsString)>,
) -> ComponentSpec {
    let mut spec = ComponentSpec::new(name, SHELL, criticality);
    spec.args = vec![OsString::from("-c"), OsString::from(script)];
    spec.policy = policy;
    spec.env = env;
    spec
}

/// Runs the supervisor with a live signal listener, optionally pre-loading one event.
fn supervise(
    specs: Vec<ComponentSpec>,
    preloaded: Option<SupervisorEvent>,
) -> Result<SessionOutcome, SessionError> {
    let (sender, events) = mpsc::channel();
    let _listener = SignalListener::start(sender.clone())?;
    if let Some(event) = preloaded {
        sender.send(event).expect("receiver is alive");
    }
    Supervisor::new(specs)
        .with_shutdown_grace(SHUTDOWN_GRACE)
        .run_with(events)
}

fn count_lines(path: &Path) -> usize {
    fs::read_to_string(path).map_or(0, |text| text.lines().count())
}

fn read_pid(path: &Path) -> u32 {
    fs::read_to_string(path)
        .expect("pid file written")
        .trim()
        .parse()
        .expect("pid file holds a number")
}

fn process_exists(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[test]
fn restarts_a_crashing_component_after_backoff() {
    let scratch = Scratch::new("restart");
    let log = scratch.file("runs.log");
    let specs = vec![
        sh_component(
            "compositor",
            r#"touch "$DESILIUM_TEST_LOG"; i=0
               while [ "$(wc -l < "$DESILIUM_TEST_LOG")" -lt 3 ]; do
                   sleep 0.01; i=$((i+1)); [ "$i" -lt 1000 ] || exit 2
               done"#,
            Criticality::Critical,
            fast_policy(100),
            vec![env_var(LOG_ENV, &log)],
        ),
        sh_component(
            "flaky",
            r#"echo run >> "$DESILIUM_TEST_LOG"; exit 1"#,
            Criticality::Restartable,
            fast_policy(100),
            vec![env_var(LOG_ENV, &log)],
        ),
    ];

    let outcome = supervise(specs, None).expect("session runs");

    assert_eq!(outcome, SessionOutcome::Normal);
    assert!(count_lines(&log) >= 3, "flaky component was restarted");
}

#[test]
fn gives_up_on_a_crash_loop_and_keeps_the_session_running() {
    let scratch = Scratch::new("crash-loop");
    let log = scratch.file("runs.log");
    let specs = vec![
        sh_component(
            "compositor",
            r#"touch "$DESILIUM_TEST_LOG"; i=0
               while [ "$(wc -l < "$DESILIUM_TEST_LOG")" -lt 3 ]; do
                   sleep 0.01; i=$((i+1)); [ "$i" -lt 1000 ] || exit 2
               done
               sleep 0.3"#,
            Criticality::Critical,
            fast_policy(100),
            vec![env_var(LOG_ENV, &log)],
        ),
        sh_component(
            "looping",
            r#"echo run >> "$DESILIUM_TEST_LOG"; exit 1"#,
            Criticality::Restartable,
            fast_policy(3),
            vec![env_var(LOG_ENV, &log)],
        ),
    ];

    let outcome = supervise(specs, None).expect("session runs");

    assert_eq!(outcome, SessionOutcome::Normal);
    assert_eq!(
        count_lines(&log),
        3,
        "no run after the crash loop was detected"
    );
}

#[test]
fn critical_exit_ends_the_session_and_terminates_the_others() {
    let specs = vec![
        sh_component(
            "compositor",
            "exit 3",
            Criticality::Critical,
            fast_policy(100),
            Vec::new(),
        ),
        sh_component(
            "helper",
            "sleep 60",
            Criticality::Restartable,
            fast_policy(100),
            Vec::new(),
        ),
    ];

    let started = Instant::now();
    let outcome = supervise(specs, None).expect("session runs");

    assert_eq!(outcome, SessionOutcome::CompositorExited { code: Some(3) });
    assert!(
        started.elapsed() < SHUTDOWN_GRACE,
        "helper was terminated by SIGTERM instead of waiting out the grace period"
    );
}

#[test]
fn injected_shutdown_terminates_all_components() {
    let specs = vec![
        sh_component(
            "compositor",
            "sleep 60",
            Criticality::Critical,
            fast_policy(100),
            Vec::new(),
        ),
        sh_component(
            "helper",
            "sleep 60",
            Criticality::Restartable,
            fast_policy(100),
            Vec::new(),
        ),
    ];

    let started = Instant::now();
    let outcome = supervise(specs, Some(SupervisorEvent::ShutdownRequested)).expect("session runs");

    assert_eq!(outcome, SessionOutcome::Signalled);
    assert!(
        started.elapsed() < SHUTDOWN_GRACE,
        "components honoured SIGTERM"
    );
}

#[test]
fn missing_optional_component_is_skipped() {
    let specs = vec![
        sh_component(
            "compositor",
            "exit 0",
            Criticality::Critical,
            fast_policy(100),
            Vec::new(),
        ),
        ComponentSpec::new(
            "shell",
            "/nonexistent/desilium-test/desilium-shell",
            Criticality::Optional,
        ),
    ];

    let outcome = supervise(specs, None).expect("optional component is not an error");

    assert_eq!(outcome, SessionOutcome::Normal);
}

#[test]
fn missing_required_component_is_an_error() {
    let specs = vec![
        sh_component(
            "compositor",
            "sleep 60",
            Criticality::Critical,
            fast_policy(100),
            Vec::new(),
        ),
        ComponentSpec::new(
            "shell",
            "/nonexistent/desilium-test/desilium-shell",
            Criticality::Restartable,
        ),
    ];

    let result = supervise(specs, None);

    assert!(matches!(
        result,
        Err(SessionError::ComponentNotFound { ref name, .. }) if name == "shell"
    ));
}

#[test]
fn no_child_process_is_left_behind() {
    let scratch = Scratch::new("zombies");
    let compositor_pid_file = scratch.file("compositor.pid");
    let helper_pid_file = scratch.file("helper.pid");
    let specs = vec![
        sh_component(
            "compositor",
            r#"echo $$ > "$DESILIUM_TEST_COMPOSITOR_PID"; i=0
               while [ ! -s "$DESILIUM_TEST_PID" ]; do
                   sleep 0.01; i=$((i+1)); [ "$i" -lt 1000 ] || exit 2
               done"#,
            Criticality::Critical,
            fast_policy(100),
            vec![
                env_var(COMPOSITOR_PID_ENV, &compositor_pid_file),
                env_var(PID_ENV, &helper_pid_file),
            ],
        ),
        sh_component(
            "helper",
            r#"echo $$ > "$DESILIUM_TEST_PID"; exec sleep 60"#,
            Criticality::Restartable,
            fast_policy(100),
            vec![env_var(PID_ENV, &helper_pid_file)],
        ),
    ];

    let outcome = supervise(specs, None).expect("session runs");

    assert_eq!(outcome, SessionOutcome::Normal);
    for pid in [read_pid(&compositor_pid_file), read_pid(&helper_pid_file)] {
        assert!(!process_exists(pid), "process {pid} was not reaped");
    }
}
