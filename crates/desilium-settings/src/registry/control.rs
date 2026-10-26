//! Control kinds and their value rules.

use super::value::{InvalidValue, Value, normalize_hex, normalize_text};

/// One option of a segmented control or dropdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    /// Stored value.
    pub value: &'static str,
    /// Label shown to the user.
    pub label: &'static str,
    /// Optional icon name (see `desilium_ui::icon_names`).
    pub icon: Option<&'static str>,
}

impl Choice {
    /// A choice without an icon.
    pub const fn new(value: &'static str, label: &'static str) -> Self {
        Self {
            value,
            label,
            icon: None,
        }
    }

    /// A choice with an icon.
    pub const fn with_icon(value: &'static str, label: &'static str, icon: &'static str) -> Self {
        Self {
            value,
            label,
            icon: Some(icon),
        }
    }
}

/// A discrete slider: the user picks one of a fixed set of stops.
///
/// Stops instead of a continuous range keep every reachable value valid and make
/// the control usable with a click, since documents report clicks, not drags.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slider {
    /// Selectable values, ascending.
    pub stops: &'static [f64],
    /// Label under every stop (same length as `stops`); empty strings show no label.
    pub labels: &'static [&'static str],
    /// Unit suffix of the value caption, e.g. `px` or `x`.
    pub unit: &'static str,
}

/// A bounded integer with minus and plus buttons.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stepper {
    /// Smallest value.
    pub min: i64,
    /// Largest value.
    pub max: i64,
}

/// The widget that edits a setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Control {
    /// Few options side by side.
    Segmented(&'static [Choice]),
    /// Dropdown list.
    Select(&'static [Choice]),
    /// On/off switch.
    Switch,
    /// Discrete slider.
    Slider(Slider),
    /// Minus/plus counter.
    Stepper(Stepper),
    /// Accent swatches plus a custom hex field; unset means "the theme's own accent".
    Accent,
    /// Thumbnail grid of installed wallpapers; unset means "the theme's background".
    Wallpaper,
    /// Dropdown of installed cursor themes; unset means "system default".
    CursorTheme,
    /// Single-line text, applied with Enter; empty means unset.
    Text {
        /// Hint shown while empty.
        placeholder: &'static str,
    },
    /// The keyboard shortcut list; its value is a fingerprint, edited through dedicated actions.
    Shortcuts,
    /// Read-only system information.
    About,
}

impl Control {
    /// True for controls that only show data.
    pub fn is_informational(&self) -> bool {
        matches!(self, Control::About)
    }

    /// Validates `input` for this control and returns the value to store.
    ///
    /// Numbers snap to the nearest stop or clamp into range, so a value that
    /// passes here is always inside what `desilium-config` validation accepts.
    pub fn normalize(&self, input: Value) -> Result<Value, InvalidValue> {
        match (self, input) {
            (Control::Segmented(choices) | Control::Select(choices), Value::Text(text)) => choices
                .iter()
                .any(|choice| choice.value == text)
                .then_some(Value::Text(text))
                .ok_or_else(|| InvalidValue::new("That option does not exist.")),
            (Control::Switch, value @ Value::Bool(_)) => Ok(value),
            (Control::Slider(slider), Value::Number(number)) => slider.snap(number),
            (Control::Stepper(stepper), Value::Number(number)) => Ok(stepper.clamp(number)),
            (Control::Accent, Value::Text(text)) => normalize_hex(&text).map(Value::Text),
            (Control::Wallpaper | Control::Text { .. }, Value::Text(text)) => normalize_text(&text),
            (Control::CursorTheme, Value::Text(text)) => normalize_cursor_theme(&text),
            (Control::Accent | Control::Wallpaper | Control::CursorTheme, Value::Unset) => {
                Ok(Value::Unset)
            }
            (Control::Text { .. }, Value::Unset) => Ok(Value::Unset),
            (Control::Shortcuts, value @ Value::Text(_)) => Ok(value),
            (Control::About, _) => Err(InvalidValue::new("This entry is read-only.")),
            _ => Err(InvalidValue::new("That value does not fit this setting.")),
        }
    }
}

fn normalize_cursor_theme(text: &str) -> Result<Value, InvalidValue> {
    let value = normalize_text(text)?;
    if matches!(&value, Value::Text(name) if name.contains('/')) {
        return Err(InvalidValue::new("A cursor theme name cannot contain '/'."));
    }
    Ok(value)
}

impl Slider {
    /// The stop closest to `number`.
    pub fn snap(&self, number: f64) -> Result<Value, InvalidValue> {
        if !number.is_finite() {
            return Err(InvalidValue::new("Enter a number."));
        }
        self.stops
            .iter()
            .copied()
            .min_by(|a, b| (a - number).abs().total_cmp(&(b - number).abs()))
            .map(Value::Number)
            .ok_or_else(|| InvalidValue::new("This slider has no positions."))
    }

    /// Index of the stop closest to `number`.
    pub fn index_of(&self, number: f64) -> usize {
        self.stops
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (*a - number).abs().total_cmp(&(*b - number).abs()))
            .map_or(0, |(index, _)| index)
    }

    /// Position of stop `index` along the track, `0.0` to `1.0`.
    pub fn fraction(&self, index: usize) -> f64 {
        let last = self.stops.len().saturating_sub(1);
        if last == 0 {
            return 0.0;
        }
        index.min(last) as f64 / last as f64
    }
}

impl Stepper {
    /// Rounds to a whole number inside the range.
    pub fn clamp(&self, number: f64) -> Value {
        let rounded = if number.is_finite() {
            number.round()
        } else {
            0.0
        };
        let clamped = rounded.clamp(self.min as f64, self.max as f64);
        Value::Number(clamped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEEDS: Slider = Slider {
        stops: &[0.0, 0.25, 0.5, 1.0, 2.0, 4.0],
        labels: &["Off", "", "", "1x", "", "4x"],
        unit: "x",
    };

    #[test]
    fn slider_snaps_to_the_nearest_stop() {
        assert_eq!(SPEEDS.snap(0.9), Ok(Value::Number(1.0)));
        assert_eq!(SPEEDS.snap(100.0), Ok(Value::Number(4.0)));
        assert_eq!(SPEEDS.snap(-5.0), Ok(Value::Number(0.0)));
        assert!(SPEEDS.snap(f64::NAN).is_err());
    }

    #[test]
    fn slider_positions_span_the_track() {
        assert_eq!(SPEEDS.index_of(1.0), 3);
        assert_eq!(SPEEDS.fraction(0), 0.0);
        assert_eq!(SPEEDS.fraction(5), 1.0);
        assert_eq!(SPEEDS.fraction(99), 1.0);
    }

    #[test]
    fn stepper_clamps_and_rounds() {
        let stepper = Stepper { min: 1, max: 32 };
        assert_eq!(stepper.clamp(0.0), Value::Number(1.0));
        assert_eq!(stepper.clamp(99.0), Value::Number(32.0));
        assert_eq!(stepper.clamp(4.6), Value::Number(5.0));
        assert_eq!(stepper.clamp(f64::NAN), Value::Number(1.0));
    }

    #[test]
    fn choices_must_exist() {
        const CHOICES: &[Choice] = &[Choice::new("a", "A"), Choice::new("b", "B")];
        let control = Control::Select(CHOICES);
        assert_eq!(
            control.normalize(Value::Text("a".into())),
            Ok(Value::Text("a".into()))
        );
        assert!(control.normalize(Value::Text("zzz".into())).is_err());
        assert!(control.normalize(Value::Bool(true)).is_err());
    }

    #[test]
    fn accent_accepts_hex_or_unset_only() {
        assert_eq!(
            Control::Accent.normalize(Value::Text("f0a".into())),
            Ok(Value::Text("#FF00AA".into()))
        );
        assert_eq!(Control::Accent.normalize(Value::Unset), Ok(Value::Unset));
        assert!(
            Control::Accent
                .normalize(Value::Text("nope".into()))
                .is_err()
        );
    }

    #[test]
    fn cursor_theme_refuses_paths() {
        assert!(
            Control::CursorTheme
                .normalize(Value::Text("../x".into()))
                .is_err()
        );
        assert_eq!(
            Control::CursorTheme.normalize(Value::Text("Adwaita".into())),
            Ok(Value::Text("Adwaita".into()))
        );
    }

    #[test]
    fn informational_controls_refuse_writes() {
        assert!(Control::About.normalize(Value::Unset).is_err());
        assert!(Control::About.is_informational());
        assert!(!Control::Shortcuts.is_informational());
    }
}
