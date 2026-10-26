//! Session-bus layer: thin adapters from D-Bus calls to the pure model.

mod convert;
mod freedesktop;
mod service;
mod shell;

pub use freedesktop::{PATH as FREEDESKTOP_PATH, SERVICE as FREEDESKTOP_SERVICE};
pub use service::Service;
pub use shell::{
    PATH as SHELL_PATH, SERVICE as SHELL_SERVICE, WireAction, WireGroup, WireNotification, WireSpan,
};
