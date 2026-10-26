//! Desilium's own session supervisor.
//!
//! Starts the compositor (critical), the shell and other helpers (restartable or optional),
//! restarts them with exponential backoff, detects crash loops and ends the session when the
//! compositor exits. The restart rules in `policy` are pure; `supervisor` connects them to processes.
//! Everything that happens before the first child starts lives in `startup`.

mod component;
mod error;
mod policy;
mod process;
mod signals;
mod slot;
mod supervisor;

pub mod startup;

pub use component::{ComponentSpec, Criticality, EnvVar, WaylandRole, resolve_program};
pub use error::SessionError;
pub use policy::{Decision, ExitKind, GiveUpReason, RestartPolicy, RestartTracker};
pub use signals::SignalListener;
pub use startup::dbus::{DbusAction, DbusInput, ensure_dbus_session_bus};
pub use startup::environment::Environment;
pub use startup::plan::{PlanError, PlanRequest, SessionPlan, build_plan};
pub use startup::readiness::{SocketWait, WaylandReadiness};
pub use supervisor::{DEFAULT_SHUTDOWN_GRACE, SessionOutcome, Supervisor, SupervisorEvent};
