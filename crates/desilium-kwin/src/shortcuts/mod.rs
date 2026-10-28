//! Global keyboard shortcuts through KGlobalAccel.
//!
//! The configured [`Shortcut`]s are pure data; [`bindings`] turns them into KGlobalAccel
//! key lists per target, [`register`] hands them to `kglobalacceld`, and a
//! [`ShortcutListener`] delivers the presses of Desilium's own component.
//!
//! The D-Bus names and the key encoding follow KGlobalAccel 6 and could not be checked against
//! a running `kglobalacceld` when this was written.

mod actions;
mod keys;
mod listener;
mod register;

use desilium_config_types::CompositorSettings;

pub use actions::{
    COMPONENT, KWIN_COMPONENT, LAUNCH_TERMINAL, QUIT, RESTORE_MINIMIZED, TOGGLE_LAUNCHER, Target,
    target,
};
pub use keys::qt_key;
pub use listener::{ShortcutEvent, ShortcutListener};
pub use register::register;

/// Most keys KGlobalAccel keeps per action.
const MAX_KEYS_PER_ACTION: usize = 4;

/// All keys that trigger one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// What the keys trigger.
    pub target: Target,
    /// Encoded keys, in configuration order.
    pub keys: Vec<i32>,
}

/// The result of translating the configured shortcuts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Bindings {
    /// One entry per target, in the order its first shortcut appears.
    pub list: Vec<Binding>,
    /// Descriptions of shortcuts that were left out, for the log.
    pub skipped: Vec<String>,
}

/// Groups the configured shortcuts by target. Shortcuts with a key Qt has no code for are skipped.
pub fn bindings(settings: &CompositorSettings) -> Bindings {
    let mut result = Bindings::default();
    for shortcut in &settings.shortcuts {
        let Some(code) = qt_key(&shortcut.modifiers, &shortcut.key) else {
            result.skipped.push(format!(
                "unknown key \"{}\" for {:?}",
                shortcut.key, shortcut.action
            ));
            continue;
        };
        let wanted = target(shortcut.action);
        match result.list.iter_mut().find(|b| b.target == wanted) {
            Some(existing) if existing.keys.len() >= MAX_KEYS_PER_ACTION => {
                result.skipped.push(format!(
                    "more than {MAX_KEYS_PER_ACTION} keys for {:?}",
                    shortcut.action
                ))
            }
            Some(existing) if existing.keys.contains(&code) => {}
            Some(existing) => existing.keys.push(code),
            None => result.list.push(Binding {
                target: wanted,
                keys: vec![code],
            }),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_config_types::{Action, Modifiers, Shortcut};

    fn alt() -> Modifiers {
        Modifiers {
            alt: true,
            ..Modifiers::default()
        }
    }

    fn settings(shortcuts: Vec<Shortcut>) -> CompositorSettings {
        CompositorSettings {
            shortcuts,
            ..CompositorSettings::default()
        }
    }

    #[test]
    fn defaults_translate_completely() {
        let result = bindings(&CompositorSettings::default());
        assert!(result.skipped.is_empty(), "{:?}", result.skipped);
        assert_eq!(
            result.list.len(),
            CompositorSettings::default().shortcuts.len(),
            "every default shortcut has its own target"
        );
        let launcher = result
            .list
            .iter()
            .find(|b| b.target == Target::Desilium(TOGGLE_LAUNCHER))
            .expect("launcher binding");
        assert_eq!(launcher.keys, vec![0x0800_0000 | 0x20]);
    }

    #[test]
    fn several_shortcuts_for_one_action_share_a_binding() {
        let result = bindings(&settings(vec![
            Shortcut::new(alt(), "q", Action::CloseFocusedWindow),
            Shortcut::new(alt(), "F4", Action::CloseFocusedWindow),
            Shortcut::new(alt(), "q", Action::CloseFocusedWindow),
        ]));
        assert_eq!(result.list.len(), 1);
        assert_eq!(result.list[0].keys.len(), 2, "duplicates collapse");
    }

    #[test]
    fn at_most_four_keys_per_action() {
        let shortcuts = ["a", "b", "c", "d", "e"]
            .into_iter()
            .map(|key| Shortcut::new(alt(), key, Action::Quit))
            .collect();
        let result = bindings(&settings(shortcuts));
        assert_eq!(result.list[0].keys.len(), MAX_KEYS_PER_ACTION);
        assert_eq!(result.skipped.len(), 1);
    }

    #[test]
    fn unknown_keys_are_skipped_not_fatal() {
        let result = bindings(&settings(vec![
            Shortcut::new(alt(), "XF86Calculator", Action::LaunchTerminal),
            Shortcut::new(alt(), "Return", Action::LaunchTerminal),
        ]));
        assert_eq!(result.list.len(), 1);
        assert_eq!(result.skipped.len(), 1);
        assert!(result.skipped[0].contains("XF86Calculator"));
    }

    #[test]
    fn no_shortcuts_means_no_bindings() {
        let result = bindings(&settings(Vec::new()));
        assert!(result.list.is_empty() && result.skipped.is_empty());
    }
}
