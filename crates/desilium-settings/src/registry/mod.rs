//! The settings registry: a declarative table that drives pages, search and resets.
//!
//! Every user-facing option is one [`SettingDef`]: its key path in
//! `DesiliumConfig`, texts, search keywords, the [`Control`] that edits it and a
//! getter/setter pair. The default value is never written twice: it is whatever
//! the getter returns for `DesiliumConfig::default()`. Adding a setting means
//! adding one entry to `defs.rs`; pages, search, "Default" badges and resets pick
//! it up without further code.

mod control;
mod defs;
mod page;
mod value;

use desilium_config_types::DesiliumConfig;

pub use control::{Choice, Control, Slider, Stepper};
pub use page::{NAV, NavEntry, Page};
pub use value::{InvalidValue, Value, format_number, normalize_hex, normalize_text, parse_number};

/// Reads a setting out of a configuration.
pub type Getter = fn(&DesiliumConfig) -> Value;
/// Writes an already validated value into a configuration.
pub type Setter = fn(&mut DesiliumConfig, &Value);

/// One configurable option.
#[derive(Debug)]
pub struct SettingDef {
    /// Key path in the configuration, e.g. `appearance.accent`. Unique.
    pub id: &'static str,
    /// Page that shows it.
    pub page: Page,
    /// Card title on that page; consecutive settings with the same section share a card.
    pub section: &'static str,
    /// Row title.
    pub title: &'static str,
    /// One-line explanation under the title.
    pub description: &'static str,
    /// Extra search terms (synonyms, related words).
    pub keywords: &'static [&'static str],
    /// The widget that edits it.
    pub control: Control,
    /// Reads the current value.
    pub get: Getter,
    /// Writes a new value.
    pub set: Setter,
}

impl SettingDef {
    /// The built-in default value.
    pub fn default_value(&self) -> Value {
        (self.get)(&DesiliumConfig::default())
    }

    /// True when `config` holds the default for this setting.
    pub fn is_default(&self, config: &DesiliumConfig) -> bool {
        (self.get)(config) == self.default_value()
    }

    /// Validates `input` against the control's rules and returns the value to store.
    pub fn normalize(&self, input: Value) -> Result<Value, InvalidValue> {
        self.control.normalize(input)
    }

    /// Interprets text from a UI action argument or the command line for this setting's control.
    ///
    /// `None` when the text cannot even be read as the control's kind of value (for example
    /// "abc" for a number); range and choice checks happen later in [`normalize`](Self::normalize).
    pub fn value_from_text(&self, text: &str) -> Option<Value> {
        match self.control {
            Control::Switch => text.parse::<bool>().ok().map(Value::Bool),
            Control::Slider(_) | Control::Stepper(_) => parse_number(text).ok().map(Value::Number),
            _ => Some(Value::Text(text.to_owned())),
        }
    }

    /// True for controls that fill a whole card instead of a row (accent, wallpaper, lists).
    pub fn fills_card(&self) -> bool {
        matches!(
            self.control,
            Control::Accent | Control::Wallpaper | Control::Shortcuts | Control::About
        )
    }

    /// True for rows that show data and have nothing to reset (the About page).
    pub fn is_informational(&self) -> bool {
        self.control.is_informational()
    }
}

/// Every setting, in display order.
pub fn all() -> &'static [SettingDef] {
    defs::ALL
}

/// Looks a setting up by key path.
pub fn find(id: &str) -> Option<&'static SettingDef> {
    all().iter().find(|def| def.id == id)
}

/// The settings of one page, in display order.
pub fn for_page(page: Page) -> impl Iterator<Item = &'static SettingDef> {
    all().iter().filter(move |def| def.page == page)
}

/// A card: consecutive settings of one page that share a section title.
pub struct Section {
    /// Card title.
    pub title: &'static str,
    /// Its settings.
    pub settings: Vec<&'static SettingDef>,
}

/// Groups a page into cards.
pub fn sections(page: Page) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for def in for_page(page) {
        match out.last_mut() {
            Some(last) if last.title == def.section => last.settings.push(def),
            _ => out.push(Section {
                title: def.section,
                settings: vec![def],
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn ids_are_unique_and_found() {
        let mut seen = HashSet::new();
        for def in all() {
            assert!(seen.insert(def.id), "duplicate id {}", def.id);
            assert_eq!(find(def.id).map(|found| found.id), Some(def.id));
        }
    }

    #[test]
    fn every_setting_has_texts_and_a_default_that_validates() {
        for def in all() {
            assert!(
                !def.title.is_empty() && !def.description.is_empty(),
                "{}",
                def.id
            );
            if def.is_informational() {
                continue;
            }
            let default = def.default_value();
            assert_eq!(def.normalize(default.clone()), Ok(default), "{}", def.id);
        }
    }

    #[test]
    fn set_then_get_round_trips_and_reset_restores_the_default() {
        let mut config = DesiliumConfig::default();
        for def in all().iter().filter(|d| !d.is_informational()) {
            let default = def.default_value();
            (def.set)(&mut config, &default);
            assert!(def.is_default(&config), "{}", def.id);
        }
        assert_eq!(config, DesiliumConfig::default());
    }

    #[test]
    fn sections_group_consecutive_settings() {
        let cards = sections(Page::Appearance);
        let titles: Vec<_> = cards.iter().map(|card| card.title).collect();
        assert_eq!(
            titles,
            ["Style", "Accent color", "Wallpaper", "Motion", "Pointer"]
        );
        assert_eq!(cards[2].settings.len(), 2);
    }
}
