//! Who may bind the shell protocol: the shell executable, or anyone in development mode.

use crate::state::ClientState;
use smithay::reexports::wayland_server::{Client, DisplayHandle};
use std::path::Path;

/// Executable names allowed to bind `desilium_shell_manager_v1`.
pub const TRUSTED_EXECUTABLES: &[&str] = &["desilium-shell"];

/// Environment variable that lets any client bind the protocol (development only).
pub const ALLOW_ANY_VAR: &str = "DESILIUM_SHELL_ALLOW_ANY";

/// Suffix the kernel appends to `/proc/<pid>/exe` when the binary was replaced on disk.
const DELETED_SUFFIX: &str = " (deleted)";

/// True when the value of [`ALLOW_ANY_VAR`] switches the check off.
pub fn allow_any_enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Decides trust from the client's executable path.
///
/// `exe` is `None` when the executable could not be determined; such a client is only trusted in
/// the development mode.
pub fn is_trusted_executable(exe: Option<&Path>, allow_any: bool) -> bool {
    if allow_any {
        return true;
    }
    let Some(name) = exe.and_then(Path::file_name).and_then(|name| name.to_str()) else {
        return false;
    };
    let name = name.strip_suffix(DELETED_SUFFIX).unwrap_or(name);
    TRUSTED_EXECUTABLES.contains(&name)
}

/// Reads the executable of process `pid` from `/proc`.
pub fn executable_of(pid: i32) -> Option<std::path::PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

/// True when the development override is set in the environment.
pub fn development_mode() -> bool {
    allow_any_enabled(std::env::var(ALLOW_ANY_VAR).ok().as_deref())
}

/// Applies the trust policy to a connected client using its process credentials.
pub fn client_is_trusted(client: &Client, dh: &DisplayHandle) -> bool {
    let exe = client
        .get_credentials(dh)
        .ok()
        .and_then(|credentials| executable_of(credentials.pid));
    is_trusted_executable(exe.as_deref(), development_mode())
}

/// Whether the connection recorded as trusted when it was accepted.
pub fn client_may_bind(client: &Client) -> bool {
    client
        .get_data::<ClientState>()
        .is_some_and(ClientState::shell_trusted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn trusted(path: &str) -> bool {
        is_trusted_executable(Some(Path::new(path)), false)
    }

    #[test]
    fn the_shell_binary_is_trusted_from_any_directory() {
        assert!(trusted("/usr/bin/desilium-shell"));
        assert!(trusted("/home/me/code/target/debug/desilium-shell"));
    }

    #[test]
    fn a_replaced_binary_is_still_the_shell() {
        assert!(trusted("/usr/bin/desilium-shell (deleted)"));
    }

    #[test]
    fn other_programs_are_rejected() {
        assert!(!trusted("/usr/bin/foot"));
        assert!(!trusted("/usr/bin/desilium-shell-evil"));
        assert!(!trusted("/tmp/desilium-shell/payload"));
        assert!(!trusted("/"));
    }

    #[test]
    fn unknown_executable_is_rejected_unless_development_mode() {
        assert!(!is_trusted_executable(None, false));
        assert!(is_trusted_executable(None, true));
    }

    #[test]
    fn development_mode_trusts_everyone() {
        assert!(is_trusted_executable(
            Some(&PathBuf::from("/usr/bin/foot")),
            true
        ));
    }

    #[test]
    fn only_the_value_one_enables_development_mode() {
        assert!(allow_any_enabled(Some("1")));
        assert!(!allow_any_enabled(Some("0")));
        assert!(!allow_any_enabled(Some("true")));
        assert!(!allow_any_enabled(None));
    }

    #[test]
    fn own_process_executable_is_readable() {
        assert!(executable_of(i32::try_from(std::process::id()).unwrap_or(0)).is_some());
    }
}
