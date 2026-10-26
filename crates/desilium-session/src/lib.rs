//! Desilium's own session supervisor.
//!
//! Starts the compositor (critical), the shell and other helpers (restartable or optional),
//! restarts them with exponential backoff, detects crash loops and ends the session when the
//! compositor exits. The restart rules in `policy` are pure; `supervisor` connects them to processes.

mod component;
mod error;
mod policy;
mod process;
mod signals;
mod slot;
mod supervisor;

pub use component::{ComponentSpec, Criticality, resolve_program};
pub use error::SessionError;
pub use policy::{Decision, ExitKind, GiveUpReason, RestartPolicy, RestartTracker};
pub use signals::SignalListener;
pub use supervisor::{DEFAULT_SHUTDOWN_GRACE, SessionOutcome, Supervisor, SupervisorEvent};
