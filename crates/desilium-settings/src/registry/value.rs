//! Setting values and their validation, independent of any UI.

use std::fmt;

use crate::i18n::tr;

/// Longest free-text value accepted (terminal command line, cursor theme name).
const MAX_TEXT_CHARS: usize = 200;
const HEX_SHORT_DIGITS: usize = 3;
const HEX_RGB_DIGITS: usize = 6;
const HEX_RGBA_DIGITS: usize = 8;

/// The value of one setting as the UI sees it.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// On/off.
    Bool(bool),
    /// Any number (speeds, sizes, counts).
    Number(f64),
    /// Text, a choice id or a path.
    Text(String),
    /// Not set: the default of the underlying option applies (accent, wallpaper, cursor theme).
    Unset,
}

impl Value {
    /// Short human-readable form, used in the "What this page controls" box.
    pub fn display(&self) -> String {
        match self {
            Value::Bool(true) => "on".to_owned(),
            Value::Bool(false) => "off".to_owned(),
            Value::Number(number) => format_number(*number),
            Value::Text(text) => text.clone(),
            Value::Unset => "default".to_owned(),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display())
    }
}

/// Formats `1.0` as `1.0` and `24.0` as `24`, trimming noise from float conversions.
pub fn format_number(number: f64) -> String {
    let rounded = (number * 100.0).round() / 100.0;
    if rounded.fract() == 0.0 && rounded.abs() >= 2.0 {
        return format!("{rounded:.0}");
    }
    let text = format!("{rounded:.2}");
    let trimmed = text.trim_end_matches('0');
    if trimmed.ends_with('.') {
        return format!("{trimmed}0");
    }
    trimmed.to_owned()
}

/// Why a value was refused. The message is shown to the user as is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct InvalidValue(pub String);

impl InvalidValue {
    /// The message for catalog `key` in the current language; `english` is the source text.
    pub(super) fn translated(key: &str, english: &str) -> Self {
        Self(tr(key, english))
    }
}

/// Normalizes a user-typed colour to `#RRGGBB` (or `#RRGGBBAA`), uppercase.
///
/// Accepts an optional leading `#` and the 3-digit shorthand.
pub fn normalize_hex(input: &str) -> Result<String, InvalidValue> {
    let digits = input.trim().trim_start_matches('#');
    let all_hex = digits.chars().all(|c| c.is_ascii_hexdigit());
    let valid_len = matches!(
        digits.len(),
        HEX_SHORT_DIGITS | HEX_RGB_DIGITS | HEX_RGBA_DIGITS
    );
    if !all_hex || !valid_len {
        return Err(InvalidValue::translated(
            "ui.error.hex_colour",
            "Enter a colour like #7B61FF (3, 6 or 8 hex digits).",
        ));
    }
    let expanded: String = if digits.len() == HEX_SHORT_DIGITS {
        digits.chars().flat_map(|c| [c, c]).collect()
    } else {
        digits.to_owned()
    };
    Ok(format!("#{}", expanded.to_ascii_uppercase()))
}

/// Trims free text; empty means unset. Control characters and absurd lengths are refused.
pub fn normalize_text(input: &str) -> Result<Value, InvalidValue> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(Value::Unset);
    }
    if trimmed.chars().any(char::is_control) {
        return Err(InvalidValue::translated(
            "ui.error.control_characters",
            "This value cannot contain control characters.",
        ));
    }
    if trimmed.chars().count() > MAX_TEXT_CHARS {
        return Err(InvalidValue::translated(
            "ui.error.too_long",
            "This value is too long.",
        ));
    }
    Ok(Value::Text(trimmed.to_owned()))
}

/// Parses text typed into a number field into a [`Value::Number`].
pub fn parse_number(input: &str) -> Result<f64, InvalidValue> {
    input
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
        .ok_or_else(|| InvalidValue::translated("ui.error.enter_number", "Enter a number."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_normalized_and_validated() {
        assert_eq!(normalize_hex("7b61ff").as_deref(), Ok("#7B61FF"));
        assert_eq!(normalize_hex(" #7b61ff ").as_deref(), Ok("#7B61FF"));
        assert_eq!(normalize_hex("#f0a").as_deref(), Ok("#FF00AA"));
        assert_eq!(normalize_hex("#11223344").as_deref(), Ok("#11223344"));
    }

    #[test]
    fn bad_hex_is_refused_with_a_message() {
        for bad in [
            "", "#", "#12", "#1234", "#12345", "#GG0000", "red", "#1234567",
        ] {
            let error = normalize_hex(bad).expect_err(bad);
            assert!(error.0.contains("#7B61FF"), "{bad}");
        }
    }

    #[test]
    fn text_normalization() {
        assert_eq!(normalize_text("  foot "), Ok(Value::Text("foot".into())));
        assert_eq!(normalize_text("   "), Ok(Value::Unset));
        assert!(normalize_text("a\nb").is_err());
        assert!(normalize_text(&"x".repeat(MAX_TEXT_CHARS + 1)).is_err());
    }

    #[test]
    fn numbers_format_compactly() {
        assert_eq!(format_number(1.0), "1.0");
        assert_eq!(format_number(0.25), "0.25");
        assert_eq!(format_number(1.5), "1.5");
        assert_eq!(format_number(24.0), "24");
        assert_eq!(format_number(0.1 + 0.2), "0.3");
    }

    #[test]
    fn number_parsing_refuses_nan_and_garbage() {
        assert_eq!(parse_number(" 3 "), Ok(3.0));
        assert!(parse_number("NaN").is_err());
        assert!(parse_number("inf").is_err());
        assert!(parse_number("abc").is_err());
    }
}
