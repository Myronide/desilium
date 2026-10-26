use serde::{Deserialize, Serialize};

use crate::action::Action;

/// Modifier keys held together with the shortcut key.
///
/// Missing flags in a config file default to `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Modifiers {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub logo: bool,
}

/// A key combination bound to an [`Action`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortcut {
    pub modifiers: Modifiers,
    /// xkb keysym name, e.g. `"q"` or `"Return"`.
    pub key: String,
    pub action: Action,
}

impl Shortcut {
    pub fn new(modifiers: Modifiers, key: &str, action: Action) -> Self {
        Self {
            modifiers,
            key: key.to_owned(),
            action,
        }
    }
}
