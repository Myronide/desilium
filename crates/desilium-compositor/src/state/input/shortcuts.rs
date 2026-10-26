use desilium_config_types::{Action, CompositorSettings, Modifiers};
use smithay::input::keyboard::ModifiersState;
use smithay::input::keyboard::xkb::{self, Keysym};
use tracing::warn;

/// Shift+Tab can report `ISO_Left_Tab` as its shifted symbol on some layouts.
const KEY_TAB: u32 = xkb::keysyms::KEY_Tab;
const KEY_ISO_LEFT_TAB: u32 = xkb::keysyms::KEY_ISO_Left_Tab;

struct Binding {
    modifiers: Modifiers,
    keysym: Keysym,
    action: Action,
}

/// Shortcuts from the settings, resolved to keysyms once at startup.
pub struct ShortcutTable {
    bindings: Vec<Binding>,
}

impl ShortcutTable {
    /// Unknown key names are skipped so a bad entry never prevents startup.
    pub fn from_settings(settings: &CompositorSettings) -> Self {
        let bindings = settings.shortcuts.iter().filter_map(|shortcut| {
            let keysym = xkb::keysym_from_name(&shortcut.key, xkb::KEYSYM_CASE_INSENSITIVE);
            if keysym.raw() == 0 {
                warn!(key = %shortcut.key, "ignoring shortcut with unknown key name");
                return None;
            }
            Some(Binding {
                modifiers: shortcut.modifiers,
                keysym,
                action: shortcut.action,
            })
        });
        Self {
            bindings: bindings.collect(),
        }
    }

    /// `raw_syms` are the layout symbols without modifier translation, so Shift+q still matches "q".
    pub fn lookup(&self, state: &ModifiersState, raw_syms: &[Keysym]) -> Option<Action> {
        let pressed = Modifiers {
            alt: state.alt,
            ctrl: state.ctrl,
            shift: state.shift,
            logo: state.logo,
        };
        self.bindings
            .iter()
            .find(|binding| binding.modifiers == pressed && matches_key(binding.keysym, raw_syms))
            .map(|binding| binding.action)
    }
}

/// A binding for `Tab` also matches `ISO_Left_Tab`, so Alt+Shift+Tab reaches the backward cycle.
fn matches_key(bound: Keysym, raw_syms: &[Keysym]) -> bool {
    raw_syms
        .iter()
        .any(|&sym| sym == bound || (bound.raw() == KEY_TAB && sym.raw() == KEY_ISO_LEFT_TAB))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keysym(name: &str) -> Keysym {
        xkb::keysym_from_name(name, xkb::KEYSYM_NO_FLAGS)
    }

    fn mods(alt: bool, shift: bool) -> ModifiersState {
        ModifiersState {
            alt,
            shift,
            ..ModifiersState::default()
        }
    }

    fn table() -> ShortcutTable {
        ShortcutTable::from_settings(&CompositorSettings::default())
    }

    #[test]
    fn alt_tab_cycles_forward_and_alt_shift_tab_backward() {
        let table = table();
        let tab = [keysym("Tab")];
        assert_eq!(
            table.lookup(&mods(true, false), &tab),
            Some(Action::CycleWindowsForward)
        );
        assert_eq!(
            table.lookup(&mods(true, true), &tab),
            Some(Action::CycleWindowsBackward)
        );
    }

    #[test]
    fn shifted_tab_symbol_still_reaches_backward_cycle() {
        let table = table();
        let left_tab = [keysym("ISO_Left_Tab")];
        assert_eq!(
            table.lookup(&mods(true, true), &left_tab),
            Some(Action::CycleWindowsBackward)
        );
    }

    #[test]
    fn window_action_keys_resolve_by_xkb_name() {
        let table = table();
        let cases = [
            ("Up", Action::ToggleMaximize),
            ("f", Action::ToggleFullscreen),
            ("Down", Action::MinimizeWindow),
            ("Left", Action::SnapLeft),
            ("Right", Action::SnapRight),
        ];
        for (name, action) in cases {
            assert_eq!(
                table.lookup(&mods(true, false), &[keysym(name)]),
                Some(action),
                "{name}"
            );
        }
    }

    #[test]
    fn alt_shift_m_restores_minimized() {
        let table = table();
        assert_eq!(
            table.lookup(&mods(true, true), &[keysym("m")]),
            Some(Action::RestoreMinimized)
        );
    }

    #[test]
    fn missing_modifier_does_not_trigger_window_action() {
        let table = table();
        assert_eq!(table.lookup(&mods(false, false), &[keysym("Left")]), None);
        assert_eq!(
            table.lookup(&mods(true, false), &[keysym("Tab")]),
            Some(Action::CycleWindowsForward)
        );
    }
}
