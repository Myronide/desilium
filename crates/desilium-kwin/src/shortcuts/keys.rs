//! Shortcut keys as the integers KGlobalAccel stores: a Qt key code or-ed with Qt modifiers.

use desilium_config_types::Modifiers;

const QT_SHIFT: i32 = 0x0200_0000;
const QT_CTRL: i32 = 0x0400_0000;
const QT_ALT: i32 = 0x0800_0000;
const QT_META: i32 = 0x1000_0000;

const QT_KEY_ESCAPE: i32 = 0x0100_0000;
const QT_KEY_TAB: i32 = 0x0100_0001;
const QT_KEY_BACKTAB: i32 = 0x0100_0002;
const QT_KEY_BACKSPACE: i32 = 0x0100_0003;
const QT_KEY_RETURN: i32 = 0x0100_0004;
const QT_KEY_ENTER: i32 = 0x0100_0005;
const QT_KEY_INSERT: i32 = 0x0100_0006;
const QT_KEY_DELETE: i32 = 0x0100_0007;
const QT_KEY_PRINT: i32 = 0x0100_0009;
const QT_KEY_HOME: i32 = 0x0100_0010;
const QT_KEY_END: i32 = 0x0100_0011;
const QT_KEY_LEFT: i32 = 0x0100_0012;
const QT_KEY_UP: i32 = 0x0100_0013;
const QT_KEY_RIGHT: i32 = 0x0100_0014;
const QT_KEY_DOWN: i32 = 0x0100_0015;
const QT_KEY_PAGE_UP: i32 = 0x0100_0016;
const QT_KEY_PAGE_DOWN: i32 = 0x0100_0017;
const QT_KEY_F1: i32 = 0x0100_0030;
const QT_KEY_SPACE: i32 = 0x20;
const FUNCTION_KEYS: i32 = 12;

/// The Qt key code of an xkb keysym name, without modifiers.
fn key_code(name: &str) -> Option<i32> {
    let mut chars = name.chars();
    if let (Some(only), None) = (chars.next(), chars.clone().next()) {
        return single_character(only);
    }
    let lower = name.to_ascii_lowercase();
    if let Some(number) = lower.strip_prefix('f').and_then(|n| n.parse::<i32>().ok()) {
        return (1..=FUNCTION_KEYS)
            .contains(&number)
            .then(|| QT_KEY_F1 + number - 1);
    }
    Some(match lower.as_str() {
        "return" => QT_KEY_RETURN,
        "kp_enter" => QT_KEY_ENTER,
        "tab" | "iso_left_tab" => QT_KEY_TAB,
        "escape" => QT_KEY_ESCAPE,
        "backspace" => QT_KEY_BACKSPACE,
        "insert" => QT_KEY_INSERT,
        "delete" => QT_KEY_DELETE,
        "print" => QT_KEY_PRINT,
        "home" => QT_KEY_HOME,
        "end" => QT_KEY_END,
        "left" => QT_KEY_LEFT,
        "up" => QT_KEY_UP,
        "right" => QT_KEY_RIGHT,
        "down" => QT_KEY_DOWN,
        "page_up" | "prior" => QT_KEY_PAGE_UP,
        "page_down" | "next" => QT_KEY_PAGE_DOWN,
        "space" => QT_KEY_SPACE,
        _ => return None,
    })
}

/// Letters are upper case in Qt, digits and printable ASCII keep their code.
fn single_character(character: char) -> Option<i32> {
    match character {
        'a'..='z' => Some(i32::from(character.to_ascii_uppercase() as u8)),
        'A'..='Z' | '0'..='9' => Some(i32::from(character as u8)),
        ' ' => Some(QT_KEY_SPACE),
        _ => None,
    }
}

/// The integer for `modifiers` plus the keysym `key`, or `None` for a key without a Qt code.
///
/// Shift+Tab is Qt's separate `Backtab` key, which is how KWin stores its own reverse bindings.
pub fn qt_key(modifiers: &Modifiers, key: &str) -> Option<i32> {
    let mut code = key_code(key)?;
    if code == QT_KEY_TAB && (modifiers.shift || key.eq_ignore_ascii_case("iso_left_tab")) {
        code = QT_KEY_BACKTAB;
    }
    let flags = [
        (modifiers.shift, QT_SHIFT),
        (modifiers.ctrl, QT_CTRL),
        (modifiers.alt, QT_ALT),
        (modifiers.logo, QT_META),
    ];
    let modifier_bits = flags
        .into_iter()
        .filter(|(held, _)| *held)
        .fold(0, |bits, (_, flag)| bits | flag);
    Some(code | modifier_bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alt() -> Modifiers {
        Modifiers {
            alt: true,
            ..Modifiers::default()
        }
    }

    #[test]
    fn letters_digits_and_space_use_ascii_codes() {
        assert_eq!(qt_key(&alt(), "q"), Some(QT_ALT | 0x51));
        assert_eq!(qt_key(&alt(), "Q"), Some(QT_ALT | 0x51));
        assert_eq!(qt_key(&alt(), "1"), Some(QT_ALT | 0x31));
        assert_eq!(qt_key(&alt(), "space"), Some(QT_ALT | 0x20));
    }

    #[test]
    fn named_keys_use_qt_codes() {
        assert_eq!(qt_key(&alt(), "Return"), Some(QT_ALT | 0x0100_0004));
        assert_eq!(qt_key(&alt(), "Up"), Some(QT_ALT | 0x0100_0013));
        assert_eq!(qt_key(&alt(), "Left"), Some(QT_ALT | 0x0100_0012));
        assert_eq!(qt_key(&alt(), "Right"), Some(QT_ALT | 0x0100_0014));
        assert_eq!(qt_key(&alt(), "Down"), Some(QT_ALT | 0x0100_0015));
    }

    #[test]
    fn function_keys_are_consecutive() {
        assert_eq!(qt_key(&Modifiers::default(), "F1"), Some(0x0100_0030));
        assert_eq!(qt_key(&Modifiers::default(), "F12"), Some(0x0100_003B));
        assert_eq!(qt_key(&Modifiers::default(), "F13"), None);
        assert_eq!(qt_key(&Modifiers::default(), "F0"), None);
    }

    #[test]
    fn modifiers_combine() {
        let all = Modifiers {
            alt: true,
            ctrl: true,
            shift: true,
            logo: true,
        };
        let expected = QT_SHIFT | QT_CTRL | QT_ALT | QT_META | 0x41;
        assert_eq!(qt_key(&all, "a"), Some(expected));
        assert_eq!(qt_key(&Modifiers::default(), "a"), Some(0x41));
    }

    #[test]
    fn shift_tab_becomes_backtab() {
        let alt_shift = Modifiers {
            alt: true,
            shift: true,
            ..Modifiers::default()
        };
        assert_eq!(
            qt_key(&alt_shift, "Tab"),
            Some(QT_ALT | QT_SHIFT | QT_KEY_BACKTAB)
        );
        assert_eq!(qt_key(&alt(), "Tab"), Some(QT_ALT | QT_KEY_TAB));
    }

    #[test]
    fn unknown_keys_have_no_code() {
        assert_eq!(qt_key(&alt(), "XF86AudioMute"), None);
        assert_eq!(qt_key(&alt(), ""), None);
        assert_eq!(qt_key(&alt(), "ä"), None);
    }
}
