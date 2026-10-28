//! Translations of the settings app.
//!
//! English is the source language and lives in code: every call site passes its English text
//! as the fallback, so a missing key never breaks a screen. German lives in
//! `assets/i18n/de.toml`, one flat `"dotted.key" = "text"` line per message. The language
//! follows the environment ([`locale_from`]): `de*` selects German, anything else English.
//!
//! # Key naming scheme
//!
//! Keys are lowercase; segments are `a-z`, `0-9`, `_` or `-`, separated by dots. Multi-word
//! names use `_`, option values keep the spelling of the stored value (`auto-hide`).
//!
//! | Key | Text |
//! |---|---|
//! | `page.<slug>` | sidebar label and heading of a page (`page.panel`) |
//! | `page.<slug>.subtitle` | sentence under the heading |
//! | `section.<page>.<slug>` | card title; slug is the English title in snake case (`section.appearance.accent_color`) |
//! | `setting.<setting id>.title` / `.description` | row title and explanation; the id is the registry id |
//! | `setting.<setting id>.keywords` | extra search words, comma separated |
//! | `setting.<setting id>.placeholder` | hint of an empty text field |
//! | `choice.<setting id>.<value>` | label of one option of a segmented control or dropdown |
//! | `action.<action name>` | label of a shortcut action, named as in the config file (`action.snap-left`) |
//! | `ui.<area>.<name>` | everything else: buttons, hints, notices, errors (`ui.keyboard.swap`) |
//!
//! Texts with variable parts use `{name}` placeholders, filled by [`tr_args`].

mod catalog;
pub mod keys;

#[cfg(test)]
mod tests;

use std::{cell::Cell, sync::OnceLock};

pub use catalog::{Catalog, CatalogError};

/// The German catalog, compiled in.
pub const GERMAN_SOURCE: &str = include_str!("../../assets/i18n/de.toml");

/// A language the app can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    /// The source language; texts come from the code.
    #[default]
    English,
    /// Texts come from `de.toml`.
    German,
}

thread_local! {
    static OVERRIDE: Cell<Option<Locale>> = const { Cell::new(None) };
}

/// Picks the language from environment variables, read through `lookup`.
///
/// Order as in gettext: `LANGUAGE` (a colon separated preference list), `LC_ALL`,
/// `LC_MESSAGES`, `LANG`. The first non-empty entry decides; `C` and `POSIX` mean English.
pub fn locale_from(lookup: impl Fn(&str) -> Option<String>) -> Locale {
    for variable in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        let Some(value) = lookup(variable) else {
            continue;
        };
        if let Some(entry) = value.split(':').find(|entry| !entry.trim().is_empty()) {
            return classify(entry.trim());
        }
    }
    Locale::English
}

fn classify(entry: &str) -> Locale {
    let lower = entry.to_ascii_lowercase();
    let language = lower.split(['_', '.', '@', '-']).next().unwrap_or_default();
    if language == "de" {
        Locale::German
    } else {
        Locale::English
    }
}

/// The language in use: the thread's override if set, otherwise the process environment.
///
/// Unit tests of this crate always see English unless they override it, so a German
/// developer machine does not change their outcome.
pub fn locale() -> Locale {
    if let Some(forced) = OVERRIDE.with(Cell::get) {
        return forced;
    }
    #[cfg(test)]
    {
        Locale::English
    }
    #[cfg(not(test))]
    {
        static FROM_ENVIRONMENT: OnceLock<Locale> = OnceLock::new();
        *FROM_ENVIRONMENT.get_or_init(|| locale_from(|name| std::env::var(name).ok()))
    }
}

/// Forces the language on the current thread (`None` goes back to the environment).
///
/// For screenshots and tests; the running app never calls it.
pub fn set_locale(locale: Option<Locale>) {
    OVERRIDE.with(|slot| slot.set(locale));
}

/// Runs `body` with `locale` forced on this thread, then restores the previous setting.
pub fn with_locale<T>(locale: Locale, body: impl FnOnce() -> T) -> T {
    let previous = OVERRIDE.with(Cell::get);
    set_locale(Some(locale));
    let result = body();
    set_locale(previous);
    result
}

/// The German catalog. A broken file is reported once and treated as empty, so the app
/// still starts in English; a test keeps the shipped file valid.
pub fn german() -> &'static Catalog {
    static GERMAN: OnceLock<Catalog> = OnceLock::new();
    GERMAN.get_or_init(|| {
        Catalog::parse(GERMAN_SOURCE).unwrap_or_else(|error| {
            tracing::error!(%error, "the German catalog is unusable; showing English");
            Catalog::default()
        })
    })
}

/// The text for `key` in the current language; `english` when there is no translation.
pub fn tr(key: &str, english: &str) -> String {
    translated(locale(), key).unwrap_or(english).to_owned()
}

/// Like [`tr`], then replaces every `{name}` with the matching value of `args`.
pub fn tr_args(key: &str, english: &str, args: &[(&str, &str)]) -> String {
    let mut text = tr(key, english);
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// The catalog entry of `key` for `locale`; English has none because it lives in code.
pub fn translated(locale: Locale, key: &str) -> Option<&'static str> {
    match locale {
        Locale::English => None,
        Locale::German => german().get(key),
    }
}
