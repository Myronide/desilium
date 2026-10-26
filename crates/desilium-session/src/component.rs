//! Describes what to run and how much the session depends on it.

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::policy::RestartPolicy;

/// How the session reacts when a component is missing or exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criticality {
    /// The session ends when this component exits (the compositor).
    Critical,
    /// Restarted with backoff whenever it fails. A missing binary is a configuration error.
    Restartable,
    /// Skipped with a log line when its binary is not installed.
    Optional,
}

/// Environment variable handed to a child on top of its spec environment.
pub type EnvVar = (OsString, OsString);

impl Criticality {
    /// Lower-case name used in plans and machine-readable output.
    pub fn label(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Restartable => "restartable",
            Self::Optional => "optional",
        }
    }
}

/// How a component relates to the Wayland socket the compositor creates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WaylandRole {
    /// Does not use the Wayland socket; started in order without waiting.
    #[default]
    None,
    /// Creates the Wayland socket (the compositor). Clients wait until its socket is up.
    Provider,
    /// A Wayland client: started after the provider's socket and given `WAYLAND_DISPLAY`.
    Client,
}

impl WaylandRole {
    /// Lower-case name used in plans and machine-readable output.
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Provider => "provider",
            Self::Client => "client",
        }
    }
}

/// Everything needed to start and supervise one component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentSpec {
    /// Name used in logs.
    pub name: String,
    /// Executable path, or a bare name looked up in `PATH`.
    pub program: PathBuf,
    /// Arguments passed to the executable.
    pub args: Vec<OsString>,
    /// Environment variables set for the child on top of the inherited environment.
    pub env: Vec<(OsString, OsString)>,
    /// How the session reacts to this component.
    pub criticality: Criticality,
    /// Restart behaviour for this component.
    pub policy: RestartPolicy,
    /// How this component uses the Wayland socket.
    pub wayland: WaylandRole,
}

impl ComponentSpec {
    /// Creates a spec without arguments or extra environment and with the default policy.
    pub fn new(
        name: impl Into<String>,
        program: impl Into<PathBuf>,
        criticality: Criticality,
    ) -> Self {
        Self {
            name: name.into(),
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            criticality,
            policy: RestartPolicy::default(),
            wayland: WaylandRole::None,
        }
    }
}

/// Finds the executable for `program`.
///
/// Paths with a directory component are checked as given; bare names are searched in `PATH`.
pub fn resolve_program(program: &Path) -> Option<PathBuf> {
    if program.is_absolute() || program.components().count() > 1 {
        return program.is_file().then_some(program.to_path_buf());
    }
    let search_path = env::var_os("PATH")?;
    env::split_paths(&search_path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}
