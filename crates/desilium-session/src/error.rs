//! Errors that stop the supervisor from starting or running a session.

use std::io;
use std::path::PathBuf;

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
    /// Signal handling could not be installed or its thread could not start.
    #[error("failed to set up signal handling")]
    Signals(#[source] io::Error),
}
