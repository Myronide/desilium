//! Typed view of the `hints` dictionary. The D-Bus layer converts wire values into [`HintValue`].

use super::Urgency;
use super::image::RawImage;
use super::limits::{
    APP_NAME_MAX_CHARS, CATEGORY_MAX_CHARS, HINTS_MAX, ICON_REF_MAX_CHARS, line_or_none,
};

/// A hint value after conversion from the wire, independent of zvariant.
#[derive(Debug, Clone, PartialEq)]
pub enum HintValue {
    Bool(bool),
    Byte(u8),
    Int(i32),
    UInt(u32),
    Text(String),
    Image(RawImage),
    Other,
}

/// The hints the service understands.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hints {
    pub urgency: Option<Urgency>,
    pub desktop_entry: Option<String>,
    pub image_path: Option<String>,
    pub image_data: Option<RawImage>,
    pub category: Option<String>,
    pub transient: bool,
    pub resident: bool,
    pub progress: Option<u8>,
    pub sync_key: Option<String>,
}

/// Builds [`Hints`] from wire pairs. Unknown or mistyped keys are ignored.
pub fn parse<I>(pairs: I) -> Hints
where
    I: IntoIterator<Item = (String, HintValue)>,
{
    let mut hints = Hints::default();
    for (key, value) in pairs.into_iter().take(HINTS_MAX) {
        hints.apply(&key, value);
    }
    hints
}

impl Hints {
    fn apply(&mut self, key: &str, value: HintValue) {
        match (key, value) {
            ("urgency", HintValue::Byte(raw)) => self.urgency = Some(Urgency::from_byte(raw)),
            ("desktop-entry", HintValue::Text(text)) => {
                self.desktop_entry = line_or_none(&text, APP_NAME_MAX_CHARS);
            }
            ("image-path" | "image_path", HintValue::Text(text)) => {
                self.image_path = line_or_none(&text, ICON_REF_MAX_CHARS);
            }
            ("image-data" | "image_data" | "icon_data", HintValue::Image(image)) => {
                self.image_data = Some(image);
            }
            ("category", HintValue::Text(text)) => {
                self.category = line_or_none(&text, CATEGORY_MAX_CHARS);
            }
            ("transient", HintValue::Bool(flag)) => self.transient = flag,
            ("resident", HintValue::Bool(flag)) => self.resident = flag,
            ("value", HintValue::Int(raw)) => self.progress = percent(i64::from(raw)),
            ("value", HintValue::UInt(raw)) => self.progress = percent(i64::from(raw)),
            ("x-canonical-private-synchronous", HintValue::Text(text)) => {
                self.sync_key = line_or_none(&text, APP_NAME_MAX_CHARS);
            }
            _ => {}
        }
    }
}

/// Progress must be a percentage; anything else is ignored rather than clamped.
fn percent(raw: i64) -> Option<u8> {
    u8::try_from(raw).ok().filter(|value| *value <= 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> HintValue {
        HintValue::Text(value.to_string())
    }

    #[test]
    fn empty_input_yields_defaults() {
        assert_eq!(parse(Vec::new()), Hints::default());
    }

    #[test]
    fn urgency_byte_maps_and_unknown_values_fall_back_to_normal() {
        let high = parse(vec![("urgency".to_string(), HintValue::Byte(2))]);
        assert_eq!(high.urgency, Some(Urgency::Critical));
        let odd = parse(vec![("urgency".to_string(), HintValue::Byte(9))]);
        assert_eq!(odd.urgency, Some(Urgency::Normal));
    }

    #[test]
    fn mistyped_values_are_ignored() {
        let hints = parse(vec![
            ("urgency".to_string(), text("critical")),
            ("transient".to_string(), HintValue::Int(1)),
        ]);
        assert_eq!(hints, Hints::default());
    }

    #[test]
    fn strings_flags_and_image_path_are_read() {
        let hints = parse(vec![
            ("desktop-entry".to_string(), text("org.example.App")),
            ("image_path".to_string(), text("/usr/share/x.png")),
            ("category".to_string(), text("email.arrived")),
            ("transient".to_string(), HintValue::Bool(true)),
            ("resident".to_string(), HintValue::Bool(true)),
        ]);
        assert_eq!(hints.desktop_entry.as_deref(), Some("org.example.App"));
        assert_eq!(hints.image_path.as_deref(), Some("/usr/share/x.png"));
        assert_eq!(hints.category.as_deref(), Some("email.arrived"));
        assert!(hints.transient && hints.resident);
    }

    #[test]
    fn progress_accepts_only_percentages() {
        let ok = parse(vec![("value".to_string(), HintValue::Int(42))]);
        assert_eq!(ok.progress, Some(42));
        let high = parse(vec![("value".to_string(), HintValue::UInt(101))]);
        assert_eq!(high.progress, None);
        let negative = parse(vec![("value".to_string(), HintValue::Int(-1))]);
        assert_eq!(negative.progress, None);
    }

    #[test]
    fn sync_key_is_stored() {
        let hints = parse(vec![(
            "x-canonical-private-synchronous".to_string(),
            text("volume"),
        )]);
        assert_eq!(hints.sync_key.as_deref(), Some("volume"));
    }

    #[test]
    fn hint_count_is_capped() {
        let pairs = (0..HINTS_MAX + 10)
            .map(|index| (format!("x-{index}"), HintValue::Other))
            .chain(std::iter::once((
                "transient".to_string(),
                HintValue::Bool(true),
            )));
        assert!(!parse(pairs).transient);
    }
}
