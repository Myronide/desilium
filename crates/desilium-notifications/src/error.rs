//! Errors of the service layer. The pure model reports its own typed errors.

/// Failures while starting or running the D-Bus service.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("D-Bus error: {0}")]
    Bus(#[from] zbus::Error),
}
