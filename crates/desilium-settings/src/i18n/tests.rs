//! Catalog coverage: the keys the code uses, the keys the catalog has, and the scheme between.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use super::*;
use crate::{
    app::tests::app_in,
    palette::ACCENT_PRESETS,
    registry::{self, Control, NAV, NavEntry, Page},
    shortcuts::ACTIONS,
    view,
};

fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_owned())
    }
}

/// Every key the registry, the pages, the sidebar, the actions and the swatches imply.
fn structural_keys() -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for page in Page::ALL {
        keys.insert(keys::page(page.slug()));
        keys.insert(keys::page_subtitle(page.slug()));
    }
    for entry in NAV {
        if let NavEntry::Planned { slug, .. } = entry {
            keys.insert(keys::page(slug));
        }
    }
    for def in registry::all() {
        keys.insert(keys::section(def.page.slug(), def.section));
        keys.insert(keys::setting(def.id, "title"));
        keys.insert(keys::setting(def.id, "description"));
        if !def.keywords.is_empty() {
            keys.insert(keys::setting(def.id, "keywords"));
        }
        if matches!(def.control, Control::Text { .. }) {
            keys.insert(keys::setting(def.id, "placeholder"));
        }
        if let Control::Segmented(choices) | Control::Select(choices) = def.control {
            for choice in choices {
                keys.insert(keys::choice(def.id, choice.value));
            }
        }
    }
    for preset in ACCENT_PRESETS {
        keys.insert(keys::choice(
            "appearance.accent",
            &preset.name.to_ascii_lowercase(),
        ));
    }
    for (_, name, _) in ACTIONS {
        keys.insert(keys::action(name));
    }
    keys
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "i18n") {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Every `"ui.…"` string literal in the sources: the chrome keys the code uses.
fn used_ui_keys() -> BTreeSet<String> {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(files.len() > 20, "found the sources");
    let mut keys = BTreeSet::new();
    for file in files {
        let source = fs::read_to_string(&file).expect("readable source");
        for part in source.split("\"ui.").skip(1) {
            let Some(end) = part.find('"') else {
                continue;
            };
            keys.insert(format!("ui.{}", &part[..end]));
        }
    }
    keys
}

fn placeholders(text: &str) -> BTreeSet<String> {
    text.split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}').map(|(name, _)| name.to_owned()))
        .collect()
}

#[test]
fn the_shipped_german_catalog_parses_and_is_not_empty() {
    assert!(Catalog::parse(GERMAN_SOURCE).is_ok());
    assert!(german().len() > 100);
}

#[test]
fn german_is_chosen_for_german_locales_only() {
    for value in [
        "de_DE.UTF-8",
        "de",
        "de_AT",
        "de_CH.utf8",
        "DE_de",
        "de@euro",
    ] {
        assert_eq!(
            locale_from(env(&[("LANG", value)])),
            Locale::German,
            "{value}"
        );
    }
    for value in ["en_US.UTF-8", "fr_FR", "C", "POSIX", "dev_XX", "", "deu"] {
        assert_eq!(
            locale_from(env(&[("LANG", value)])),
            Locale::English,
            "{value}"
        );
    }
    assert_eq!(locale_from(env(&[])), Locale::English);
}

#[test]
fn the_variables_are_read_in_gettext_order() {
    let de = "de_DE.UTF-8";
    assert_eq!(
        locale_from(env(&[("LANGUAGE", "de:en"), ("LANG", "en_US")])),
        Locale::German
    );
    assert_eq!(
        locale_from(env(&[("LANGUAGE", "en:de"), ("LANG", de)])),
        Locale::English
    );
    assert_eq!(
        locale_from(env(&[
            ("LC_ALL", de),
            ("LC_MESSAGES", "en_US"),
            ("LANG", "en_US")
        ])),
        Locale::German
    );
    assert_eq!(
        locale_from(env(&[("LC_MESSAGES", de), ("LANG", "en_US")])),
        Locale::German
    );
    assert_eq!(
        locale_from(env(&[("LANGUAGE", ""), ("LANG", de)])),
        Locale::German
    );
}

#[test]
fn a_missing_key_falls_back_to_the_english_text() {
    with_locale(Locale::German, || {
        assert_eq!(tr("ui.no.such_key", "Plain English"), "Plain English");
        assert_eq!(
            tr_args("ui.no.such_key", "{n} items", &[("n", "3")]),
            "3 items"
        );
    });
    assert_eq!(tr("page.about", "About"), "About");
}

#[test]
fn german_texts_replace_the_english_ones_and_fill_placeholders() {
    with_locale(Locale::German, || {
        assert_eq!(tr("page.about", "About"), "Info");
        assert_eq!(
            tr_args("ui.count.settings", "{count} settings", &[("count", "3")]),
            "3 Einstellungen"
        );
    });
}

#[test]
fn the_forced_locale_is_restored() {
    with_locale(Locale::German, || {
        assert_eq!(locale(), Locale::German);
    });
    assert_eq!(locale(), Locale::English);
}

#[test]
fn page_titles_are_german_in_the_model_and_in_the_document() {
    with_locale(Locale::German, || {
        assert_eq!(Page::Appearance.title(), "Erscheinungsbild");
        assert_eq!(Page::Keyboard.title(), "Tastatur");
        let (_dir, app) = app_in();
        let html = view::document(&app.ctx(Instant::now()));
        assert!(html.contains("Erscheinungsbild"), "heading and sidebar");
        assert!(html.contains("Tastatur"));
        assert!(html.contains("Einstellungen suchen"));
        assert!(!html.contains("Appearance"));
    });
    assert_eq!(Page::Appearance.title(), "Appearance");
}

#[test]
fn no_german_page_shows_the_english_source_texts() {
    with_locale(Locale::German, || {
        let (_dir, mut app) = app_in();
        for page in Page::ALL {
            app.model_mut().page = page;
            let html = view::document(&app.ctx(Instant::now()));
            for def in registry::for_page(page) {
                for english in [def.title, def.description] {
                    let german_text = if english == def.title {
                        def.title_text()
                    } else {
                        def.description_text()
                    };
                    if german_text != english {
                        assert!(
                            !html.contains(&view::esc(english)),
                            "{:?} shows the English text {english:?}",
                            def.id
                        );
                    }
                }
                // Cards like the shortcut list show their section title, not a row title.
                assert!(
                    def.fills_card() || html.contains(&view::esc(&def.title_text())),
                    "{:?} is missing",
                    def.id
                );
            }
        }
    });
}

#[test]
fn every_key_the_code_uses_is_in_the_german_catalog() {
    let used: BTreeSet<String> = structural_keys().union(&used_ui_keys()).cloned().collect();
    let missing: Vec<&String> = used
        .iter()
        .filter(|key| german().get(key).is_none())
        .collect();
    assert!(missing.is_empty(), "missing in de.toml: {missing:#?}");
}

#[test]
fn the_german_catalog_has_no_unknown_keys() {
    let used: BTreeSet<String> = structural_keys().union(&used_ui_keys()).cloned().collect();
    let unknown: Vec<&str> = german().keys().filter(|key| !used.contains(*key)).collect();
    assert!(unknown.is_empty(), "unknown keys in de.toml: {unknown:#?}");
}

#[test]
fn all_keys_follow_the_naming_scheme() {
    let all: BTreeSet<String> = structural_keys().union(&used_ui_keys()).cloned().collect();
    for key in all.iter().map(String::as_str).chain(german().keys()) {
        assert_eq!(keys::check(key), Ok(()), "{key}");
    }
}

#[test]
fn german_texts_are_filled_in_and_use_known_placeholders() {
    const KNOWN: [&str; 18] = [
        "count", "query", "hex", "name", "value", "what", "reason", "path", "text", "chord",
        "action", "error", "detail", "tool", "entry", "names", "page", "pages",
    ];
    for key in german().keys() {
        let text = german().get(key).expect("key from the catalog");
        assert!(!text.trim().is_empty(), "{key} is empty");
        for name in placeholders(text) {
            assert!(
                KNOWN.contains(&name.as_str()),
                "{key} uses the unknown placeholder {{{name}}}"
            );
        }
    }
}

#[test]
fn action_names_are_the_ones_of_the_configuration_file() {
    for (action, name, _) in ACTIONS {
        let written = toml::Value::try_from(action).expect("serializable");
        assert_eq!(written.as_str(), Some(name), "{action:?}");
    }
}

#[test]
fn the_german_search_finds_german_words() {
    with_locale(Locale::German, || {
        let hits = crate::search::search("Akzentfarbe");
        assert_eq!(
            hits.first().map(|hit| hit.def.id),
            Some("appearance.accent")
        );
        let english = crate::search::search("accent");
        assert!(english.iter().any(|hit| hit.def.id == "appearance.accent"));
    });
}
