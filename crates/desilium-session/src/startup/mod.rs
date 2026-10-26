//! Everything that happens before the first child starts: environment, discovery, plan,
//! D-Bus decision, Wayland readiness and the plan renderers used by `--dry-run`.

pub mod dbus;
pub mod defaults;
pub mod environment;
pub mod plan;
pub mod readiness;
pub mod render;
#[cfg(test)]
pub(crate) mod scratch_tests;
