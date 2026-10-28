//! The environment the session sees, injected so that tests never touch the process environment.

use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

/// Session-wide variables that always have the given value.
///
/// `XDG_CURRENT_DESKTOP` is a list: portals and applications try `Desilium` first and then fall
/// back to what they know for `KDE`, which is what KWin and its portal backend belong to.
const SESSION_VARIABLES: [(&str, &str); 5] = [
    ("XDG_SESSION_TYPE", "wayland"),
    ("XDG_CURRENT_DESKTOP", "Desilium:KDE"),
    ("XDG_SESSION_DESKTOP", "desilium"),
    ("DESKTOP_SESSION", "desilium"),
    ("KDE_SESSION_VERSION", "6"),
];

/// Toolkit hints that are applied only when the user has not set the variable.
const TOOLKIT_HINTS: [(&str, &str); 5] = [
    ("QT_QPA_PLATFORM", "wayland;xcb"),
    ("MOZ_ENABLE_WAYLAND", "1"),
    ("GDK_BACKEND", "wayland,x11"),
    ("ELECTRON_OZONE_PLATFORM_HINT", "auto"),
    ("SDL_VIDEODRIVER", "wayland,x11"),
];

/// A snapshot of environment variables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Environment {
    vars: BTreeMap<OsString, OsString>,
}

impl Environment {
    /// Builds an environment from explicit pairs (later pairs replace earlier ones).
    pub fn new(pairs: impl IntoIterator<Item = (OsString, OsString)>) -> Self {
        Self {
            vars: pairs.into_iter().collect(),
        }
    }

    /// Snapshots the current process environment.
    pub fn from_process() -> Self {
        Self::new(env::vars_os())
    }

    /// Value of `key`, if set.
    pub fn get(&self, key: &str) -> Option<&OsString> {
        self.vars.get(OsStr::new(key))
    }

    /// Value of `key` when it is set and not empty.
    pub fn non_empty(&self, key: &str) -> Option<&OsString> {
        self.get(key).filter(|value| !value.is_empty())
    }

    /// The `PATH` value, used to find helper binaries.
    pub fn search_path(&self) -> Option<&OsStr> {
        self.get("PATH").map(OsString::as_os_str)
    }

    /// The runtime directory that holds the Wayland socket.
    pub fn runtime_dir(&self) -> Option<PathBuf> {
        self.non_empty("XDG_RUNTIME_DIR").map(PathBuf::from)
    }
}

/// What the session does with one variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvChange {
    /// The variable is exported with `value`; `previous` is the value it replaces, if any.
    Set {
        value: OsString,
        previous: Option<OsString>,
    },
    /// A toolkit hint the user already set; the user's value stays.
    Kept { value: OsString },
}

/// One variable of the session environment and what happens to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvEntry {
    /// Variable name.
    pub key: &'static str,
    /// Exported value or kept user value.
    pub change: EnvChange,
}

impl EnvEntry {
    /// The value children see for this variable.
    pub fn value(&self) -> &OsString {
        match &self.change {
            EnvChange::Set { value, .. } | EnvChange::Kept { value } => value,
        }
    }
}

/// Computes the variables the session sets for its children.
///
/// Session variables are always forced; toolkit hints are kept when the user already set them.
pub fn session_environment(env: &Environment) -> Vec<EnvEntry> {
    let forced = SESSION_VARIABLES.iter().map(|&(key, value)| EnvEntry {
        key,
        change: EnvChange::Set {
            value: OsString::from(value),
            previous: env.get(key).cloned(),
        },
    });
    let hints = TOOLKIT_HINTS.iter().map(|&(key, value)| EnvEntry {
        key,
        change: match env.get(key) {
            Some(existing) => EnvChange::Kept {
                value: existing.clone(),
            },
            None => EnvChange::Set {
                value: OsString::from(value),
                previous: None,
            },
        },
    });
    forced.chain(hints).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(key: &str, value: &str) -> (OsString, OsString) {
        (OsString::from(key), OsString::from(value))
    }

    fn find<'a>(entries: &'a [EnvEntry], key: &str) -> &'a EnvEntry {
        entries
            .iter()
            .find(|entry| entry.key == key)
            .expect("entry is present")
    }

    #[test]
    fn empty_environment_gets_every_session_and_hint_variable() {
        let entries = session_environment(&Environment::default());
        assert_eq!(entries.len(), SESSION_VARIABLES.len() + TOOLKIT_HINTS.len());
        assert_eq!(find(&entries, "GDK_BACKEND").value(), "wayland,x11");
        assert_eq!(
            find(&entries, "XDG_CURRENT_DESKTOP").value(),
            "Desilium:KDE"
        );
        assert_eq!(find(&entries, "KDE_SESSION_VERSION").value(), "6");
    }

    #[test]
    fn session_variables_replace_user_values_and_record_the_previous_one() {
        let env = Environment::new([pair("XDG_SESSION_TYPE", "x11")]);
        let entries = session_environment(&env);
        assert_eq!(
            find(&entries, "XDG_SESSION_TYPE").change,
            EnvChange::Set {
                value: OsString::from("wayland"),
                previous: Some(OsString::from("x11")),
            }
        );
    }

    #[test]
    fn toolkit_hint_already_set_by_the_user_is_kept() {
        let env = Environment::new([pair("SDL_VIDEODRIVER", "x11")]);
        let entries = session_environment(&env);
        assert_eq!(
            find(&entries, "SDL_VIDEODRIVER").change,
            EnvChange::Kept {
                value: OsString::from("x11"),
            }
        );
    }

    #[test]
    fn empty_user_value_still_counts_as_set_for_hints() {
        let env = Environment::new([pair("MOZ_ENABLE_WAYLAND", "")]);
        let entries = session_environment(&env);
        assert!(matches!(
            find(&entries, "MOZ_ENABLE_WAYLAND").change,
            EnvChange::Kept { .. }
        ));
    }

    #[test]
    fn non_empty_ignores_empty_values() {
        let env = Environment::new([pair("DBUS_SESSION_BUS_ADDRESS", "")]);
        assert!(env.get("DBUS_SESSION_BUS_ADDRESS").is_some());
        assert!(env.non_empty("DBUS_SESSION_BUS_ADDRESS").is_none());
    }

    #[test]
    fn runtime_dir_and_search_path_come_from_the_injected_pairs() {
        let env = Environment::new([
            pair("XDG_RUNTIME_DIR", "/run/user/1000"),
            pair("PATH", "/bin"),
        ]);
        assert_eq!(env.runtime_dir(), Some(PathBuf::from("/run/user/1000")));
        assert_eq!(env.search_path(), Some(OsStr::new("/bin")));
    }
}
