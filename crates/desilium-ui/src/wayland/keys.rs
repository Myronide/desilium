//! xkb keysym mapping and software key repeat.
//!
//! The compositor tells the client rate and delay but never repeats keys itself.

use std::time::{Duration, Instant};

use smithay_client_toolkit::seat::keyboard::{KeyEvent as SctkKey, Keysym, RepeatInfo};

use super::config::SurfaceId;
use crate::{Key, KeyEvent, Modifiers};

const MILLIS_PER_SECOND: u64 = 1000;

/// Maps an sctk key press to our [`KeyEvent`].
///
/// `text` is only set for printable input without Ctrl/Alt/Logo, so shortcuts do not type.
pub(super) fn translate(event: &SctkKey, modifiers: Modifiers, repeat: bool) -> KeyEvent {
    let printable = event
        .utf8
        .as_ref()
        .filter(|text| !text.is_empty() && !text.chars().any(char::is_control));
    let shortcut = modifiers.ctrl || modifiers.alt || modifiers.logo;
    let key = named_key(event.keysym)
        .or_else(|| {
            printable
                .cloned()
                .or_else(|| event.keysym.key_char().map(String::from))
                .map(Key::Character)
        })
        .unwrap_or(Key::Other);
    let text = printable.filter(|_| !shortcut).cloned();
    KeyEvent {
        key,
        modifiers,
        text,
        repeat,
    }
}

fn named_key(keysym: Keysym) -> Option<Key> {
    let key = match keysym {
        Keysym::Return | Keysym::KP_Enter => Key::Enter,
        Keysym::BackSpace => Key::Backspace,
        Keysym::Delete | Keysym::KP_Delete => Key::Delete,
        Keysym::Tab | Keysym::ISO_Left_Tab => Key::Tab,
        Keysym::Escape => Key::Escape,
        Keysym::Left | Keysym::KP_Left => Key::ArrowLeft,
        Keysym::Right | Keysym::KP_Right => Key::ArrowRight,
        Keysym::Up | Keysym::KP_Up => Key::ArrowUp,
        Keysym::Down | Keysym::KP_Down => Key::ArrowDown,
        Keysym::Home | Keysym::KP_Home => Key::Home,
        Keysym::End | Keysym::KP_End => Key::End,
        Keysym::Page_Up | Keysym::KP_Page_Up => Key::PageUp,
        Keysym::Page_Down | Keysym::KP_Page_Down => Key::PageDown,
        _ => return None,
    };
    Some(key)
}

/// The key currently held down and when it fires next.
#[derive(Default)]
pub(super) struct KeyRepeat {
    info: Option<(Duration, Duration)>,
    held: Option<Held>,
}

struct Held {
    surface: SurfaceId,
    raw_code: u32,
    event: KeyEvent,
    due: Instant,
}

impl KeyRepeat {
    pub(super) fn set_info(&mut self, info: RepeatInfo) {
        self.info = match info {
            RepeatInfo::Disable => None,
            RepeatInfo::Repeat { rate, delay } => {
                let interval = Duration::from_millis(MILLIS_PER_SECOND / u64::from(rate.get()));
                Some((Duration::from_millis(u64::from(delay)), interval))
            }
        };
    }

    /// Starts repeating `event` after the configured delay.
    pub(super) fn press(
        &mut self,
        surface: SurfaceId,
        raw_code: u32,
        event: KeyEvent,
        now: Instant,
    ) {
        let repeatable = event.key != Key::Other || event.text.is_some();
        let Some((delay, _)) = self.info.filter(|_| repeatable) else {
            self.held = None;
            return;
        };
        self.held = Some(Held {
            surface,
            raw_code,
            event,
            due: now + delay,
        });
    }

    /// Stops repeating when the held key (or any key, with `None`) is released.
    pub(super) fn release(&mut self, raw_code: Option<u32>) {
        let matches = self
            .held
            .as_ref()
            .is_some_and(|held| raw_code.is_none_or(|code| code == held.raw_code));
        if matches {
            self.held = None;
        }
    }

    /// Time until the next repeat, `None` when nothing repeats (no wakeups needed).
    pub(super) fn next_timeout(&self, now: Instant) -> Option<Duration> {
        self.held
            .as_ref()
            .map(|held| held.due.saturating_duration_since(now))
    }

    /// Repeats that are due at `now`, as `(surface, event)`.
    pub(super) fn due(&mut self, now: Instant) -> Vec<(SurfaceId, KeyEvent)> {
        let Some((_, interval)) = self.info else {
            return Vec::new();
        };
        let Some(held) = self.held.as_mut() else {
            return Vec::new();
        };
        let mut fired = Vec::new();
        while held.due <= now {
            let mut event = held.event.clone();
            event.repeat = true;
            fired.push((held.surface, event));
            held.due += interval;
        }
        fired
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;

    fn repeating() -> KeyRepeat {
        let mut repeat = KeyRepeat::default();
        repeat.set_info(RepeatInfo::Repeat {
            rate: NonZeroU32::new(25).unwrap(),
            delay: 400,
        });
        repeat
    }

    #[test]
    fn repeats_after_the_delay_at_the_rate() {
        let t0 = Instant::now();
        let mut repeat = repeating();
        repeat.press(SurfaceId(1), 30, KeyEvent::character("a"), t0);
        assert_eq!(repeat.next_timeout(t0), Some(Duration::from_millis(400)));
        assert!(repeat.due(t0 + Duration::from_millis(399)).is_empty());
        let fired = repeat.due(t0 + Duration::from_millis(480));
        assert_eq!(fired.len(), 3, "400, 440, 480 ms");
        assert!(fired.iter().all(|(_, event)| event.repeat));
    }

    #[test]
    fn release_stops_only_the_held_key() {
        let t0 = Instant::now();
        let mut repeat = repeating();
        repeat.press(SurfaceId(1), 30, KeyEvent::character("a"), t0);
        repeat.release(Some(31));
        assert!(repeat.next_timeout(t0).is_some());
        repeat.release(Some(30));
        assert_eq!(repeat.next_timeout(t0), None);
    }

    #[test]
    fn disabled_repeat_never_fires() {
        let t0 = Instant::now();
        let mut repeat = KeyRepeat::default();
        repeat.set_info(RepeatInfo::Disable);
        repeat.press(SurfaceId(1), 30, KeyEvent::character("a"), t0);
        assert_eq!(repeat.next_timeout(t0), None);
        assert!(repeat.due(t0 + Duration::from_secs(5)).is_empty());
    }

    #[test]
    fn named_keys_are_mapped() {
        assert_eq!(named_key(Keysym::Return), Some(Key::Enter));
        assert_eq!(named_key(Keysym::Left), Some(Key::ArrowLeft));
        assert_eq!(named_key(Keysym::a), None);
    }
}
