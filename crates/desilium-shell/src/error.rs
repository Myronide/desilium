//! Errors that stop the shell at start-up or end its loop.

use crate::services::protocol::ProtocolError;

/// Why the shell cannot run.
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    /// No compositor to talk to.
    #[error("cannot connect to the Wayland compositor: {0}")]
    Connect(#[from] wayland_client::ConnectError),
    /// A surface or the UI host failed.
    #[error("UI error: {0}")]
    Ui(#[from] desilium_ui::UiError),
    /// The shell protocol failed.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// The event loop failed.
    #[error("event loop error: {0}")]
    EventLoop(String),
}
