//! Desilium notification service.
//!
//! [`model`] is pure state and policy with no D-Bus types. [`dbus`] exposes it on the
//! session bus as the freedesktop `Notifications` service and as the shell-facing
//! `org.desilium.Notifications1` interface.

pub mod dbus;
pub mod error;
pub mod model;

pub use error::ServiceError;
