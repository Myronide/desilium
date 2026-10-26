//! One supervised child: its spawn spec, its restart tracker and its lifecycle state.

use std::ffi::c_int;
use std::path::PathBuf;
use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

use tracing::{error, info, warn};

use crate::component::{ComponentSpec, EnvVar};
use crate::error::SessionError;
use crate::policy::{Decision, ExitKind, RestartTracker};
use crate::process;

#[derive(Debug)]
enum SlotState {
    Running { child: Child, started_at: Instant },
    Waiting { restart_at: Instant },
    Stopped,
}

/// A component as the supervisor tracks it while the session runs.
#[derive(Debug)]
pub(crate) struct Slot {
    spec: ComponentSpec,
    program: PathBuf,
    tracker: RestartTracker,
    state: SlotState,
}

impl Slot {
    pub(crate) fn new(spec: ComponentSpec, program: PathBuf) -> Self {
        let tracker = RestartTracker::new(spec.policy, spec.criticality);
        Self {
            spec,
            program,
            tracker,
            state: SlotState::Stopped,
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.spec.name
    }

    /// Spawns the child; `extra_env` carries session-wide variables such as `WAYLAND_DISPLAY`.
    pub(crate) fn start(&mut self, now: Instant, extra_env: &[EnvVar]) -> Result<(), SessionError> {
        let child = process::spawn(&self.spec, &self.program, extra_env).map_err(|source| {
            SessionError::Spawn {
                name: self.spec.name.clone(),
                source,
            }
        })?;
        info!(component = %self.spec.name, pid = child.id(), program = ?self.program, "component started");
        self.state = SlotState::Running {
            child,
            started_at: now,
        };
        Ok(())
    }

    /// Starts the component after its backoff. Returns a decision only when the start failed.
    pub(crate) fn restart(&mut self, now: Instant, extra_env: &[EnvVar]) -> Option<Decision> {
        match self.start(now, extra_env) {
            Ok(()) => None,
            Err(error) => {
                warn!(component = %self.spec.name, error = %error, "restart failed to spawn");
                Some(self.on_exit(ExitKind::Failure, Duration::ZERO, now))
            }
        }
    }

    pub(crate) fn restart_due(&self, now: Instant) -> bool {
        matches!(self.state, SlotState::Waiting { restart_at } if restart_at <= now)
    }

    pub(crate) fn restart_at(&self) -> Option<Instant> {
        match self.state {
            SlotState::Waiting { restart_at } => Some(restart_at),
            SlotState::Running { .. } | SlotState::Stopped => None,
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        matches!(self.state, SlotState::Running { .. })
    }

    /// Drops a pending restart, used when the session is shutting down.
    pub(crate) fn cancel_restart(&mut self) {
        if matches!(self.state, SlotState::Waiting { .. }) {
            self.state = SlotState::Stopped;
        }
    }

    /// Reaps the child if it has exited. Returns its status and how long it ran.
    pub(crate) fn try_reap(&mut self, now: Instant) -> Option<(ExitStatus, Duration)> {
        let SlotState::Running { child, started_at } = &mut self.state else {
            return None;
        };
        let status = match child.try_wait() {
            Ok(Some(status)) => status,
            Ok(None) => return None,
            Err(error) => {
                warn!(component = %self.spec.name, error = %error, "failed to query child status");
                return None;
            }
        };
        let uptime = now.saturating_duration_since(*started_at);
        self.state = SlotState::Stopped;
        info!(component = %self.spec.name, status = ?status, uptime = ?uptime, "component exited");
        Some((status, uptime))
    }

    /// Applies the restart policy to an exit and moves the slot into its next state.
    pub(crate) fn on_exit(&mut self, exit: ExitKind, uptime: Duration, now: Instant) -> Decision {
        let decision = self.tracker.on_exit(now, uptime, exit);
        self.state = match decision {
            Decision::RestartAfter(delay) => {
                info!(component = %self.spec.name, delay = ?delay, "restarting component after backoff");
                SlotState::Waiting {
                    restart_at: now.checked_add(delay).unwrap_or(now),
                }
            }
            Decision::GiveUp(reason) => {
                error!(component = %self.spec.name, reason = ?reason, "giving up on component");
                SlotState::Stopped
            }
            Decision::EndSession => {
                info!(component = %self.spec.name, "critical component ended, ending session");
                SlotState::Stopped
            }
        };
        decision
    }

    /// Sends `signal` to the child's process group. Does nothing if the child is not running.
    pub(crate) fn signal(&self, signal: c_int) {
        let SlotState::Running { child, .. } = &self.state else {
            return;
        };
        if let Err(error) = process::signal_group(child.id(), signal) {
            warn!(component = %self.spec.name, error = %error, "failed to signal component");
        }
    }

    /// Kills the child's process group and reaps the child.
    pub(crate) fn kill_and_wait(&mut self) {
        let SlotState::Running { child, .. } = &mut self.state else {
            return;
        };
        if let Err(error) = process::signal_group(child.id(), libc::SIGKILL) {
            warn!(component = %self.spec.name, error = %error, "failed to send SIGKILL");
        }
        if let Err(error) = child.wait() {
            warn!(component = %self.spec.name, error = %error, "failed to reap killed component");
        }
        self.state = SlotState::Stopped;
    }
}
