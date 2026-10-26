//! Keyboard handling outside text fields.

use std::time::Instant;

use desilium_ui::{Key, KeyEvent};

use super::App;
use crate::refresh::Refresh;

impl App {
    /// A key press that the window layer forwarded. `typing` is true while a text field has focus.
    pub fn on_key(&mut self, event: &KeyEvent, typing: bool, now: Instant) -> Refresh {
        if self.model.rebind.is_some() {
            return self.capture_key(event, now);
        }
        if event.repeat {
            return Refresh::default();
        }
        match &event.key {
            Key::Character(text) if text == "/" && !typing && !event.modifiers.ctrl => Refresh {
                focus_search: true,
                ..Refresh::default()
            },
            Key::Escape => self.escape(),
            _ => Refresh::default(),
        }
    }

    /// Escape closes the topmost transient thing: dropdown, then search.
    fn escape(&mut self) -> Refresh {
        if self.model.open_select.take().is_some() {
            return Refresh::main();
        }
        if self.model.searching() {
            self.model.query.clear();
            return Refresh {
                clear_search: true,
                ..Refresh::page()
            };
        }
        Refresh::default()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::app_in;
    use super::*;
    use desilium_ui::Modifiers;

    fn press(key: Key) -> KeyEvent {
        KeyEvent {
            key,
            modifiers: Modifiers::default(),
            text: None,
            repeat: false,
        }
    }

    #[test]
    fn slash_focuses_search_unless_typing() {
        let (_dir, mut app) = app_in();
        let now = Instant::now();
        let slash = press(Key::Character("/".into()));
        assert!(app.on_key(&slash, false, now).focus_search);
        assert!(!app.on_key(&slash, true, now).focus_search);
    }

    #[test]
    fn escape_clears_a_search() {
        let (_dir, mut app) = app_in();
        app.model_mut().query = "accent".into();
        let refresh = app.on_key(&press(Key::Escape), true, Instant::now());
        assert!(refresh.clear_search && refresh.main);
        assert!(!app.model().searching());
    }
}
