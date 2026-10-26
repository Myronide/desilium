//! Pure helpers for the keyboard page: names, display, conflict detection, key capture.

use desilium_config_types::{Action, CompositorSettings, Modifiers, Shortcut};
use desilium_ui::{Key, KeyEvent};

/// What the user sees for an action.
pub fn action_label(action: Action) -> &'static str {
    match action {
        Action::CloseFocusedWindow => "Close window",
        Action::LaunchTerminal => "Open a terminal",
        Action::Quit => "Quit Desilium",
        Action::ToggleMaximize => "Maximize or restore window",
        Action::ToggleFullscreen => "Full screen on or off",
        Action::MinimizeWindow => "Minimize window",
        Action::SnapLeft => "Snap window to the left half",
        Action::SnapRight => "Snap window to the right half",
        Action::CycleWindowsForward => "Next window",
        Action::CycleWindowsBackward => "Previous window",
        Action::RestoreMinimized => "Restore minimized window",
        Action::SwitchDesktopNext => "Next desktop",
        Action::SwitchDesktopPrev => "Previous desktop",
        Action::SwitchToDesktop1 => "Go to desktop 1",
        Action::SwitchToDesktop2 => "Go to desktop 2",
        Action::SwitchToDesktop3 => "Go to desktop 3",
        Action::SwitchToDesktop4 => "Go to desktop 4",
        Action::MoveToDesktop1 => "Move window to desktop 1",
        Action::MoveToDesktop2 => "Move window to desktop 2",
        Action::MoveToDesktop3 => "Move window to desktop 3",
        Action::MoveToDesktop4 => "Move window to desktop 4",
        Action::ToggleLauncher => "Open or close the launcher",
    }
}

/// How a keysym name reads on a keycap: `Return` is "Enter", `Up` is an arrow.
pub fn key_label(key: &str) -> String {
    match key {
        "Return" => "Enter".to_owned(),
        "Up" => "\u{2191}".to_owned(),
        "Down" => "\u{2193}".to_owned(),
        "Left" => "\u{2190}".to_owned(),
        "Right" => "\u{2192}".to_owned(),
        "space" => "Space".to_owned(),
        "BackSpace" => "Backspace".to_owned(),
        "Page_Up" => "Page Up".to_owned(),
        "Page_Down" => "Page Down".to_owned(),
        other if other.chars().count() == 1 => other.to_uppercase(),
        other => other.to_owned(),
    }
}

/// Keycap texts of a chord in display order: Ctrl, Alt, Shift, Super, key.
pub fn chord_parts(modifiers: Modifiers, key: &str) -> Vec<String> {
    let mut parts = Vec::new();
    for (on, name) in [
        (modifiers.ctrl, "Ctrl"),
        (modifiers.alt, "Alt"),
        (modifiers.shift, "Shift"),
        (modifiers.logo, "Super"),
    ] {
        if on {
            parts.push(name.to_owned());
        }
    }
    parts.push(key_label(key));
    parts
}

/// "Alt+Shift+Q", for messages.
pub fn chord_text(modifiers: Modifiers, key: &str) -> String {
    chord_parts(modifiers, key).join("+")
}

/// True when two key names denote the same key (the compositor matches case-insensitively).
pub fn same_key(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Index of the shortcut that already uses this chord, ignoring `except`.
pub fn find_conflict(
    shortcuts: &[Shortcut],
    modifiers: Modifiers,
    key: &str,
    except: Option<usize>,
) -> Option<usize> {
    shortcuts.iter().enumerate().position(|(index, shortcut)| {
        Some(index) != except && shortcut.modifiers == modifiers && same_key(&shortcut.key, key)
    })
}

/// A shortcut must carry Alt, Ctrl or Super; a bare key or Shift alone would break typing.
pub fn is_bindable(modifiers: Modifiers) -> bool {
    modifiers.alt || modifiers.ctrl || modifiers.logo
}

/// A comparable text form of a shortcut list, used as the registry value of the shortcuts setting.
pub fn fingerprint(shortcuts: &[Shortcut]) -> String {
    shortcuts
        .iter()
        .map(|s| {
            format!(
                "{}{}{}{}+{}={:?}",
                u8::from(s.modifiers.ctrl),
                u8::from(s.modifiers.alt),
                u8::from(s.modifiers.shift),
                u8::from(s.modifiers.logo),
                s.key.to_ascii_lowercase(),
                s.action
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// The built-in shortcut list.
pub fn defaults() -> Vec<Shortcut> {
    CompositorSettings::default().shortcuts
}

/// The built-in chord of `action` (its first default shortcut), if it has one.
pub fn default_for(action: Action) -> Option<Shortcut> {
    defaults()
        .into_iter()
        .find(|shortcut| shortcut.action == action)
}

/// Result of interpreting a key press while capturing a new shortcut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capture {
    /// Escape: stop capturing.
    Cancel,
    /// A usable chord.
    Chord {
        /// Held modifiers.
        modifiers: Modifiers,
        /// xkb keysym name.
        key: String,
    },
    /// Only a modifier so far or an unsupported combination; keep waiting and say why.
    Reject(&'static str),
}

/// Maps a key event to a shortcut chord.
///
/// Shift combined with symbols is refused: the compositor matches the key without
/// Shift applied, which a client cannot reliably reconstruct for every layout.
pub fn capture(event: &KeyEvent) -> Capture {
    let modifiers = Modifiers {
        alt: event.modifiers.alt,
        ctrl: event.modifiers.ctrl,
        shift: event.modifiers.shift,
        logo: event.modifiers.logo,
    };
    if event.key == Key::Escape {
        return Capture::Cancel;
    }
    let Some(key) = key_name(&event.key, modifiers.shift) else {
        return Capture::Reject("That key cannot be used in a shortcut. Try another one.");
    };
    if !is_bindable(modifiers) {
        return Capture::Reject("Hold Alt, Ctrl or Super together with the key.");
    }
    Capture::Chord { modifiers, key }
}

fn key_name(key: &Key, shift: bool) -> Option<String> {
    let name = match key {
        Key::Character(text) => return character_name(text, shift),
        Key::Enter => "Return",
        Key::Tab => "Tab",
        Key::Backspace => "BackSpace",
        Key::Delete => "Delete",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "Page_Up",
        Key::PageDown => "Page_Down",
        Key::Escape | Key::Other => return None,
    };
    Some(name.to_owned())
}

fn character_name(text: &str, shift: bool) -> Option<String> {
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if c == ' ' {
        return Some("space".to_owned());
    }
    if c.is_ascii_alphabetic() {
        return Some(c.to_ascii_lowercase().to_string());
    }
    // A shifted digit or symbol arrives as another symbol ("1" becomes "!").
    (c.is_ascii_digit() && !shift).then(|| c.to_string())
}

#[cfg(test)]
mod tests {
    use desilium_ui::Modifiers as UiModifiers;

    use super::*;

    fn alt() -> Modifiers {
        Modifiers {
            alt: true,
            ..Modifiers::default()
        }
    }

    fn event(key: Key, modifiers: UiModifiers) -> KeyEvent {
        KeyEvent {
            key,
            modifiers,
            text: None,
            repeat: false,
        }
    }

    fn held_alt() -> UiModifiers {
        UiModifiers {
            alt: true,
            ..UiModifiers::default()
        }
    }

    #[test]
    fn every_action_has_a_label() {
        for shortcut in defaults() {
            assert!(!action_label(shortcut.action).is_empty());
        }
    }

    #[test]
    fn chords_render_in_display_order() {
        let mods = Modifiers {
            alt: true,
            shift: true,
            ctrl: true,
            logo: false,
        };
        assert_eq!(chord_text(mods, "Tab"), "Ctrl+Alt+Shift+Tab");
        assert_eq!(chord_text(alt(), "Return"), "Alt+Enter");
        assert_eq!(chord_text(alt(), "Up"), "Alt+\u{2191}");
    }

    #[test]
    fn conflicts_ignore_case_and_the_edited_row() {
        let list = defaults();
        assert_eq!(find_conflict(&list, alt(), "Q", None), Some(0));
        assert_eq!(find_conflict(&list, alt(), "q", Some(0)), None);
        assert_eq!(find_conflict(&list, alt(), "F9", None), None);
    }

    #[test]
    fn default_shortcuts_have_no_conflicts_with_each_other() {
        let list = defaults();
        for (index, shortcut) in list.iter().enumerate() {
            assert_eq!(
                find_conflict(&list, shortcut.modifiers, &shortcut.key, Some(index)),
                None
            );
        }
    }

    #[test]
    fn bare_keys_and_shift_alone_are_not_bindable() {
        assert!(!is_bindable(Modifiers::default()));
        assert!(!is_bindable(Modifiers {
            shift: true,
            ..Modifiers::default()
        }));
        assert!(is_bindable(alt()));
    }

    #[test]
    fn capturing_letters_and_named_keys() {
        assert_eq!(
            capture(&event(Key::Character("Q".into()), held_alt())),
            Capture::Chord {
                modifiers: alt(),
                key: "q".into()
            }
        );
        assert_eq!(
            capture(&event(Key::Enter, held_alt())),
            Capture::Chord {
                modifiers: alt(),
                key: "Return".into()
            }
        );
        assert_eq!(
            capture(&event(Key::Character(" ".into()), held_alt())),
            Capture::Chord {
                modifiers: alt(),
                key: "space".into()
            }
        );
    }

    #[test]
    fn capture_rejects_unbindable_input_with_a_reason() {
        assert_eq!(capture(&event(Key::Escape, held_alt())), Capture::Cancel);
        let bare = capture(&event(Key::Character("q".into()), UiModifiers::default()));
        assert!(matches!(bare, Capture::Reject(reason) if reason.contains("Alt")));
        assert!(matches!(
            capture(&event(Key::Other, held_alt())),
            Capture::Reject(_)
        ));
        let shifted = UiModifiers {
            alt: true,
            shift: true,
            ..UiModifiers::default()
        };
        assert!(matches!(
            capture(&event(Key::Character("!".into()), shifted)),
            Capture::Reject(_)
        ));
        assert!(matches!(
            capture(&event(Key::Character("1".into()), shifted)),
            Capture::Reject(_)
        ));
    }

    #[test]
    fn fingerprint_changes_with_any_binding() {
        let base = defaults();
        let mut edited = base.clone();
        edited[0].key = "w".to_owned();
        assert_ne!(fingerprint(&base), fingerprint(&edited));
        assert_eq!(fingerprint(&base), fingerprint(&defaults()));
    }

    #[test]
    fn default_for_finds_the_builtin_chord() {
        let close = default_for(Action::CloseFocusedWindow).expect("default");
        assert_eq!(close.key, "q");
        assert!(close.modifiers.alt);
    }
}
