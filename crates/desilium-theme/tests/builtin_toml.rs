//! The committed TOML files must never drift from the Rust built-ins.

use desilium_theme::{BUILTIN_THEMES, Mode, Theme};

#[test]
fn toml_files_equal_rust_builtins() {
    let expected = [Theme::dark(), Theme::light()];
    assert_eq!(BUILTIN_THEMES.len(), expected.len());
    for (builtin, theme) in BUILTIN_THEMES.iter().zip(expected) {
        let parsed = Theme::from_toml_str(builtin.toml).unwrap();
        assert_eq!(
            parsed, theme,
            "{} drifted from the Rust built-in",
            builtin.id
        );
    }
}

#[test]
fn toml_files_are_complete_documents() {
    for builtin in BUILTIN_THEMES {
        let table: toml::Table = builtin.toml.parse().unwrap();
        let regenerated: toml::Table = Theme::from_toml_str(builtin.toml)
            .unwrap()
            .to_toml_string()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(
            table, regenerated,
            "{} is missing keys or has extras",
            builtin.id
        );
    }
}

#[test]
fn builtin_ids_and_modes_match() {
    assert_eq!(BUILTIN_THEMES[0].id, "desilium-dark");
    assert_eq!(Theme::default().mode, Mode::Dark);
    let light = Theme::from_toml_str(BUILTIN_THEMES[1].toml).unwrap();
    assert_eq!(light.mode, Mode::Light);
}

#[test]
fn builtins_pass_validation_untouched() {
    for builtin in BUILTIN_THEMES {
        let (_, issues) = Theme::from_toml_str_validated(builtin.toml).unwrap();
        assert!(issues.is_empty(), "{}: {issues:?}", builtin.id);
    }
}
