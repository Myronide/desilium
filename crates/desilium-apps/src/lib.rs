//! Application catalog for the Desilium launcher and dock.
//!
//! Pure CPU and filesystem logic: `.desktop` parsing, the XDG application
//! catalog, freedesktop icon lookup, Exec expansion and process spawning, and
//! launcher ranking. Nothing here talks to Wayland or draws anything. Fallible
//! I/O is logged and degrades to an empty or fallback result.

pub mod catalog;
pub mod desktop;
pub mod env;
pub mod icons;
pub mod launch;
pub mod search;
#[cfg(test)]
pub(crate) mod testutil;

pub use catalog::Catalog;
pub use desktop::{DesktopEntry, DesktopId, MainCategory};
pub use env::Environment;
pub use icons::{IconResolver, IconSource};
pub use launch::{LaunchCommand, TerminalSpec};
pub use search::{Hit, Search, UsageStats};
