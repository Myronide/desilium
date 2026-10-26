//! The session loop: starts components, reacts to exits and signals, and shuts everything down.

use std::process::ExitStatus;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use tracing::{info, warn};

use crate::component::{ComponentSpec, Criticality, resolve_program};
use crate::error::SessionError;
use crate::policy::{Decision, ExitKind};
use crate::process::exit_kind;
use crate::signals::SignalListener;
use crate::slot::Slot;

/// Time given to components to exit after SIGTERM before they are killed.
pub const DEFAULT_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// How often the shutdown wait re-checks children if the event channel has closed.
const CLOSED_CHANNEL_POLL: Duration = Duration::from_millis(10);

/// Something the supervisor loop has to react to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorEvent {
    /// A child may have changed state; the loop checks all children.
    ChildChanged,
    /// The session should end cleanly.
    ShutdownRequested,
}

/// How a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionOutcome {
    /// The critical component (the compositor) exited with status 0.
    Normal,
    /// The critical component exited unsuccessfully; `code` is `None` when it was killed by a signal.
    CompositorExited { code: Option<i32> },
    /// A shutdown was requested through a signal or the event channel.
    Signalled,
}

/// Starts and supervises a set of components until the session ends.
pub struct Supervisor {
    specs: Vec<ComponentSpec>,
    shutdown_grace: Duration,
}

impl Supervisor {
    /// Creates a supervisor that starts `specs` in the given order.
    pub fn new(specs: Vec<ComponentSpec>) -> Self {
        Self {
            specs,
            shutdown_grace: DEFAULT_SHUTDOWN_GRACE,
        }
    }

    /// Overrides the time components get to exit after SIGTERM.
    pub fn with_shutdown_grace(mut self, grace: Duration) -> Self {
        self.shutdown_grace = grace;
        self
    }

    /// Runs the session with real process signals until it ends.
    pub fn run(self) -> Result<SessionOutcome, SessionError> {
        let (sender, events) = mpsc::channel();
        let _listener = SignalListener::start(sender)?;
        self.run_with(events)
    }

    /// Runs the session driven by `events`.
    ///
    /// Child exits must be announced as `ChildChanged`, which a `SignalListener` on the
    /// same channel does. `ShutdownRequested`, or the channel closing, ends the session.
    pub fn run_with(
        self,
        events: Receiver<SupervisorEvent>,
    ) -> Result<SessionOutcome, SessionError> {
        let mut session = Session::new(events, self.shutdown_grace);
        if let Err(error) = session.start_all(self.specs) {
            session.terminate_all();
            return Err(error);
        }
        let outcome = session.event_loop();
        session.terminate_all();
        Ok(outcome)
    }
}

enum Wake {
    Event(SupervisorEvent),
    Timeout,
    Closed,
}

fn wait_for_event(events: &Receiver<SupervisorEvent>, timeout: Option<Duration>) -> Wake {
    match timeout {
        None => events.recv().map_or(Wake::Closed, Wake::Event),
        Some(duration) => match events.recv_timeout(duration) {
            Ok(event) => Wake::Event(event),
            Err(RecvTimeoutError::Timeout) => Wake::Timeout,
            Err(RecvTimeoutError::Disconnected) => Wake::Closed,
        },
    }
}

struct Session {
    slots: Vec<Slot>,
    events: Receiver<SupervisorEvent>,
    shutdown_grace: Duration,
}

impl Session {
    fn new(events: Receiver<SupervisorEvent>, shutdown_grace: Duration) -> Self {
        Self {
            slots: Vec::new(),
            events,
            shutdown_grace,
        }
    }

    fn start_all(&mut self, specs: Vec<ComponentSpec>) -> Result<(), SessionError> {
        for spec in specs {
            let Some(program) = resolve_program(&spec.program) else {
                if spec.criticality == Criticality::Optional {
                    info!(component = %spec.name, program = ?spec.program, "optional component not installed, skipping");
                    continue;
                }
                return Err(SessionError::ComponentNotFound {
                    name: spec.name,
                    program: spec.program,
                });
            };
            let mut slot = Slot::new(spec, program);
            slot.start(Instant::now())?;
            self.slots.push(slot);
        }
        Ok(())
    }

    fn event_loop(&mut self) -> SessionOutcome {
        loop {
            let now = Instant::now();
            if let Some(outcome) = self.reap(now) {
                return outcome;
            }
            if let Some(outcome) = self.start_due(now) {
                return outcome;
            }
            let timeout = self.next_restart_in(now);
            match wait_for_event(&self.events, timeout) {
                Wake::Event(SupervisorEvent::ShutdownRequested) | Wake::Closed => {
                    return SessionOutcome::Signalled;
                }
                Wake::Event(SupervisorEvent::ChildChanged) | Wake::Timeout => {}
            }
        }
    }

    /// Applies the restart policy to every child that exited. Returns an outcome if the session must end.
    fn reap(&mut self, now: Instant) -> Option<SessionOutcome> {
        for slot in &mut self.slots {
            let Some((status, uptime)) = slot.try_reap(now) else {
                continue;
            };
            if matches!(
                slot.on_exit(exit_kind(status), uptime, now),
                Decision::EndSession
            ) {
                return Some(outcome_for(status));
            }
        }
        None
    }

    /// Starts components whose backoff has elapsed. Returns an outcome if a failed start ends the session.
    fn start_due(&mut self, now: Instant) -> Option<SessionOutcome> {
        for slot in &mut self.slots {
            if !slot.restart_due(now) {
                continue;
            }
            if matches!(slot.restart(now), Some(Decision::EndSession)) {
                return Some(SessionOutcome::CompositorExited { code: None });
            }
        }
        None
    }

    fn next_restart_in(&self, now: Instant) -> Option<Duration> {
        self.slots
            .iter()
            .filter_map(Slot::restart_at)
            .min()
            .map(|at| at.saturating_duration_since(now))
    }

    /// Asks every child to exit, waits up to the grace period, then kills whatever is left.
    fn terminate_all(&mut self) {
        info!("shutting down session");
        for slot in &mut self.slots {
            slot.cancel_restart();
            slot.signal(libc::SIGTERM);
        }
        let deadline = Instant::now()
            .checked_add(self.shutdown_grace)
            .unwrap_or_else(Instant::now);
        self.wait_until_exited(deadline);
        for slot in &mut self.slots {
            if slot.is_running() {
                warn!(component = %slot.name(), "did not exit within the grace period, sending SIGKILL");
                slot.kill_and_wait();
            }
        }
    }

    fn wait_until_exited(&mut self, deadline: Instant) {
        loop {
            self.reap_all();
            if !self.slots.iter().any(Slot::is_running) {
                return;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return;
            }
            // Shutdown ignores further requests; a closed channel must not turn this wait into a busy loop.
            if matches!(wait_for_event(&self.events, Some(remaining)), Wake::Closed) {
                thread::sleep(remaining.min(CLOSED_CHANNEL_POLL));
            }
        }
    }

    /// Reaps exited children during shutdown; their status is already logged by `try_reap`.
    fn reap_all(&mut self) {
        let now = Instant::now();
        for slot in &mut self.slots {
            slot.try_reap(now);
        }
    }
}

fn outcome_for(status: ExitStatus) -> SessionOutcome {
    match exit_kind(status) {
        ExitKind::Success => SessionOutcome::Normal,
        ExitKind::Failure => SessionOutcome::CompositorExited {
            code: status.code(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_channel_without_children_ends_the_loop_as_signalled() {
        let (sender, events) = mpsc::channel::<SupervisorEvent>();
        drop(sender);
        let mut session = Session::new(events, DEFAULT_SHUTDOWN_GRACE);
        assert_eq!(session.event_loop(), SessionOutcome::Signalled);
    }

    #[test]
    fn shutdown_event_ends_the_loop_as_signalled() {
        let (sender, events) = mpsc::channel();
        sender
            .send(SupervisorEvent::ShutdownRequested)
            .expect("receiver is alive");
        let mut session = Session::new(events, DEFAULT_SHUTDOWN_GRACE);
        assert_eq!(session.event_loop(), SessionOutcome::Signalled);
    }

    #[test]
    fn next_restart_is_none_without_pending_restarts() {
        let (_sender, events) = mpsc::channel();
        let session = Session::new(events, DEFAULT_SHUTDOWN_GRACE);
        assert_eq!(session.next_restart_in(Instant::now()), None);
    }
}
