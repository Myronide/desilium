//! D-Bus session bus decision. The session needs a user bus for portals, notifications and settings.
//!
//! The decision is pure: the caller reports what it sees in the environment and the supervisor
//! acts on the returned action (see `docs/session.md`). Nothing here executes anything.

use std::path::PathBuf;

/// Variable that holds the session bus address when a bus is running.
pub const BUS_ADDRESS_VARIABLE: &str = "DBUS_SESSION_BUS_ADDRESS";
/// Set on the process that `dbus-run-session` re-executed, so the re-exec happens only once.
pub const REEXEC_MARKER_VARIABLE: &str = "DESILIUM_DBUS_REEXEC";

/// What the session should do about its D-Bus session bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbusAction {
    /// A session bus address is already set; keep it.
    UseExisting,
    /// No bus is set: run this session again under `dbus-run-session`.
    ReexecUnder {
        /// Path of the `dbus-run-session` launcher.
        launcher: PathBuf,
    },
    /// No bus and no way to create one; the session continues and logs a warning.
    Unavailable,
}

impl DbusAction {
    /// Lower-case name used in plans and machine-readable output.
    pub fn label(&self) -> &'static str {
        match self {
            Self::UseExisting => "use_existing",
            Self::ReexecUnder { .. } => "reexec",
            Self::Unavailable => "unavailable",
        }
    }
}

/// The facts the decision depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbusInput {
    /// `DBUS_SESSION_BUS_ADDRESS` is set and non-empty.
    pub bus_address_set: bool,
    /// This process was already started by our own re-exec.
    pub already_reexecuted: bool,
    /// `dbus-run-session` found on `PATH`, if any.
    pub launcher: Option<PathBuf>,
}

/// Decides how the session gets its D-Bus session bus.
///
/// An existing bus is always reused. Otherwise the session re-executes itself once under
/// `dbus-run-session`; the marker prevents a loop when the launcher does not provide an address.
pub fn ensure_dbus_session_bus(input: &DbusInput) -> DbusAction {
    if input.bus_address_set {
        return DbusAction::UseExisting;
    }
    match (&input.launcher, input.already_reexecuted) {
        (Some(launcher), false) => DbusAction::ReexecUnder {
            launcher: launcher.clone(),
        },
        _ => DbusAction::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(bus: bool, reexec: bool, launcher: Option<&str>) -> DbusInput {
        DbusInput {
            bus_address_set: bus,
            already_reexecuted: reexec,
            launcher: launcher.map(PathBuf::from),
        }
    }

    #[test]
    fn existing_bus_is_reused_even_when_a_launcher_exists() {
        let action =
            ensure_dbus_session_bus(&input(true, false, Some("/usr/bin/dbus-run-session")));
        assert_eq!(action, DbusAction::UseExisting);
    }

    #[test]
    fn missing_bus_with_launcher_reexecutes_once() {
        let action =
            ensure_dbus_session_bus(&input(false, false, Some("/usr/bin/dbus-run-session")));
        assert_eq!(
            action,
            DbusAction::ReexecUnder {
                launcher: PathBuf::from("/usr/bin/dbus-run-session"),
            }
        );
    }

    #[test]
    fn missing_bus_without_launcher_is_unavailable() {
        let action = ensure_dbus_session_bus(&input(false, false, None));
        assert_eq!(action, DbusAction::Unavailable);
    }

    #[test]
    fn second_pass_never_reexecutes_again() {
        let action =
            ensure_dbus_session_bus(&input(false, true, Some("/usr/bin/dbus-run-session")));
        assert_eq!(action, DbusAction::Unavailable);
    }

    #[test]
    fn labels_match_the_machine_readable_names() {
        assert_eq!(DbusAction::UseExisting.label(), "use_existing");
        assert_eq!(DbusAction::Unavailable.label(), "unavailable");
        let reexec = DbusAction::ReexecUnder {
            launcher: PathBuf::from("x"),
        };
        assert_eq!(reexec.label(), "reexec");
    }
}
