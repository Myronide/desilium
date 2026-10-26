//! Errors that stop the supervisor from starting or running a session.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

/// Failures that end the session before or outside normal component lifecycles.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// A component that must exist (not `Optional`) has no executable.
    #[error("required component `{name}` was not found (looked for {program:?})")]
    ComponentNotFound { name: String, program: PathBuf },
    /// The operating system refused to start a component during session start-up.
    #[error("failed to start component `{name}`")]
    Spawn {
        name: String,
        #[source]
        source: io::Error,
    },
    /// The runtime directory that holds the Wayland socket could not be read.
    #[error("cannot read runtime directory {path:?}")]
    RuntimeDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A Wayland provider was declared but the supervisor has no runtime directory to watch.
    #[error("component `{name}` provides a Wayland socket but no runtime directory is configured")]
    MissingRuntimeDir { name: String },
    /// A Wayland client was declared before any provider could start.
    #[error("component `{name}` needs a Wayland socket but no provider started before it")]
    NoWaylandProvider { name: String },
    /// The compositor did not create a new Wayland socket within the timeout.
    #[error("no Wayland socket appeared in {runtime_dir:?} within {timeout:?}")]
    WaylandSocketTimeout {
        runtime_dir: PathBuf,
        timeout: Duration,
    },
    /// The Wayland provider exited before it created its socket.
    #[error("component `{name}` exited before creating a Wayland socket")]
    ProviderExitedEarly { name: String },
    /// Signal handling could not be installed or its thread could not start.
    #[error("failed to set up signal handling")]
    Signals(#[source] io::Error),
}
