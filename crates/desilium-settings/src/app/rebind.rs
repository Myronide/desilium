//! Re-binding shortcuts: key capture, conflict detection, swap.

use std::time::Instant;

use desilium_config_types::{Action, Modifiers};
use desilium_ui::KeyEvent;

use super::App;
use crate::{
    changes,
    i18n::tr_args,
    model::{PendingRebind, Rebind},
    refresh::Refresh,
    shortcuts::{self, Capture, action_label, chord_text},
};

fn flags(modifiers: Modifiers) -> [bool; 4] {
    [
        modifiers.alt,
        modifiers.ctrl,
        modifiers.shift,
        modifiers.logo,
    ]
}

fn from_flags(flags: [bool; 4]) -> Modifiers {
    Modifiers {
        alt: flags[0],
        ctrl: flags[1],
        shift: flags[2],
        logo: flags[3],
    }
}

/// "Close window is now Alt+Q", the toast after a re-binding.
fn now_bound(action: Action, modifiers: Modifiers, key: &str) -> String {
    tr_args(
        "ui.change.shortcut_now",
        "{action} is now {chord}",
        &[
            ("action", &action_label(action)),
            ("chord", &chord_text(modifiers, key)),
        ],
    )
}

impl App {
    pub(super) fn rebind_action(&mut self, name: &str, arg: &str, now: Instant) -> Refresh {
        let index = arg.parse::<usize>().ok();
        match (name, index) {
            ("rebind", Some(index)) => self.start_rebind(index),
            ("rebind-cancel", _) => self.cancel_rebind(),
            ("rebind-swap", _) => self.swap_pending(now),
            ("shortcut-reset", Some(index)) => self.reset_shortcut(index, now),
            _ => Refresh::default(),
        }
    }

    fn start_rebind(&mut self, index: usize) -> Refresh {
        if index >= self.store.get().compositor.shortcuts.len() {
            return Refresh::default();
        }
        self.model.rebind = Some(Rebind {
            index,
            message: None,
            pending: None,
        });
        Refresh::main()
    }

    fn cancel_rebind(&mut self) -> Refresh {
        self.model.rebind = None;
        Refresh::main()
    }

    /// A key press while a shortcut is being captured.
    pub(super) fn capture_key(&mut self, event: &KeyEvent, now: Instant) -> Refresh {
        let Some(rebind) = self.model.rebind.clone() else {
            return Refresh::default();
        };
        if event.repeat || rebind.pending.is_some() {
            return Refresh::default();
        }
        match shortcuts::capture(event) {
            Capture::Cancel => self.cancel_rebind(),
            Capture::Reject(reason) => {
                self.model.rebind = Some(Rebind {
                    message: Some(reason),
                    ..rebind
                });
                Refresh::main()
            }
            Capture::Chord { modifiers, key } => self.assign(rebind.index, modifiers, key, now),
        }
    }

    /// Gives shortcut `index` a new chord, or asks first when another action owns it.
    fn assign(&mut self, index: usize, modifiers: Modifiers, key: String, now: Instant) -> Refresh {
        let list = &self.store.get().compositor.shortcuts;
        if let Some(conflict) = shortcuts::find_conflict(list, modifiers, &key, Some(index)) {
            self.model.rebind = Some(Rebind {
                index,
                message: None,
                pending: Some(PendingRebind {
                    modifiers: flags(modifiers),
                    key,
                    conflict,
                }),
            });
            return Refresh::main();
        }
        let Some(shortcut) = list.get(index) else {
            return self.cancel_rebind();
        };
        let summary = now_bound(shortcut.action, modifiers, &key);
        let result = changes::edit_shortcuts(&mut self.store, summary, |list| {
            if let Some(target) = list.get_mut(index) {
                target.modifiers = modifiers;
                target.key = key;
            }
        });
        self.finish_shortcut_change(result, now)
    }

    /// Confirms a captured chord that collided: the two actions trade chords.
    fn swap_pending(&mut self, now: Instant) -> Refresh {
        let Some(rebind) = self.model.rebind.clone() else {
            return Refresh::default();
        };
        let Some(pending) = rebind.pending else {
            return Refresh::default();
        };
        let (index, other) = (rebind.index, pending.conflict);
        let modifiers = from_flags(pending.modifiers);
        let Some(action) = self
            .store
            .get()
            .compositor
            .shortcuts
            .get(index)
            .map(|s| s.action)
        else {
            return self.cancel_rebind();
        };
        let summary = now_bound(action, modifiers, &pending.key);
        let result = changes::edit_shortcuts(&mut self.store, summary, |list| {
            let Some((old_modifiers, old_key)) = list
                .get(index)
                .map(|entry| (entry.modifiers, entry.key.clone()))
            else {
                return;
            };
            let Some(swapped) = list.get_mut(other) else {
                return;
            };
            swapped.modifiers = old_modifiers;
            swapped.key = old_key;
            let Some(rebound) = list.get_mut(index) else {
                return;
            };
            rebound.modifiers = modifiers;
            rebound.key = pending.key;
        });
        self.finish_shortcut_change(result, now)
    }

    /// Back to the built-in chord of that action.
    fn reset_shortcut(&mut self, index: usize, now: Instant) -> Refresh {
        let Some(shortcut) = self.store.get().compositor.shortcuts.get(index) else {
            return Refresh::default();
        };
        let Some(default) = shortcuts::default_for(shortcut.action) else {
            return Refresh::default();
        };
        self.model.rebind = None;
        self.assign(index, default.modifiers, default.key, now)
    }

    fn finish_shortcut_change(
        &mut self,
        result: Result<changes::ChangeOutcome, changes::ChangeError>,
        now: Instant,
    ) -> Refresh {
        self.model.rebind = None;
        match result {
            Ok(outcome) => self.finish_change(outcome, now),
            Err(error) => {
                self.fail(&error, None);
                Refresh::main()
            }
        }
    }
}
