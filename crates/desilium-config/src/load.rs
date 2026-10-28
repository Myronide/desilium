//! Turns raw bytes on disk into a valid [`DesiliumConfig`].
//!
//! Whole-document failures (not UTF-8, not TOML, bad version) are errors, so the
//! caller can quarantine the file. Field-level failures never fail the load: the
//! bad field falls back to its default and the rest of the document is kept.

use desilium_config_types::{CURRENT_VERSION, DesiliumConfig};
use toml::{Table, Value};

use crate::error::ConfigError;
use crate::migrate;
use crate::recovery::Recovery;

/// A parsed document plus the repairs that were needed to build it.
pub(crate) struct Loaded {
    pub config: DesiliumConfig,
    pub recovery: Vec<Recovery>,
}

/// Parses, migrates and field-repairs a document.
pub(crate) fn load_document(bytes: &[u8]) -> Result<Loaded, ConfigError> {
    let text =
        std::str::from_utf8(bytes).map_err(|error| ConfigError::Corrupt(error.to_string()))?;
    let table: Table = text
        .parse()
        .map_err(|error: toml::de::Error| ConfigError::Corrupt(error.to_string()))?;
    let found = migrate::stored_version(&table)?;
    let mut recovery = Vec::new();
    if found > CURRENT_VERSION {
        recovery.push(Recovery::NewerVersion { found });
    }
    let (table, notes) = migrate::migrate(table)?;
    recovery.extend(notes.into_iter().map(Recovery::Migrated));
    let (config, field_recovery) = repair_fields(table);
    recovery.extend(field_recovery);
    Ok(Loaded { config, recovery })
}

/// Builds the config from `table`, replacing each unusable leaf value by its default.
fn repair_fields(table: Table) -> (DesiliumConfig, Vec<Recovery>) {
    if let Ok(config) = to_config(&table) {
        return (config, Vec::new());
    }
    let mut accepted = default_table();
    let mut recovery = Vec::new();
    let mut leaves = Vec::new();
    flatten(Vec::new(), table, &mut leaves);
    for (path, value) in leaves {
        let previous = get_at(&accepted, &path);
        set_at(&mut accepted, &path, Some(value));
        let Err(error) = to_config(&accepted) else {
            continue;
        };
        set_at(&mut accepted, &path, previous);
        recovery.push(Recovery::InvalidField {
            field: path.join("."),
            reason: error.to_string(),
        });
    }
    (to_config(&accepted).unwrap_or_default(), recovery)
}

fn to_config(table: &Table) -> Result<DesiliumConfig, toml::de::Error> {
    Value::Table(table.clone()).try_into()
}

fn default_table() -> Table {
    Value::try_from(DesiliumConfig::default())
        .ok()
        .and_then(|value| value.as_table().cloned())
        .unwrap_or_default()
}

/// Collects every non-table value with its dotted key path.
fn flatten(prefix: Vec<String>, table: Table, out: &mut Vec<(Vec<String>, Value)>) {
    for (key, value) in table {
        let mut path = prefix.clone();
        path.push(key);
        match value {
            Value::Table(child) => flatten(path, child, out),
            leaf => out.push((path, leaf)),
        }
    }
}

fn get_at(table: &Table, path: &[String]) -> Option<Value> {
    match path {
        [] => None,
        [last] => table.get(last).cloned(),
        [head, rest @ ..] => match table.get(head) {
            Some(Value::Table(child)) => get_at(child, rest),
            _ => None,
        },
    }
}

/// Sets or removes (`None`) the value at `path`, creating intermediate tables.
fn set_at(table: &mut Table, path: &[String], value: Option<Value>) {
    match path {
        [] => {}
        [last] => match value {
            Some(value) => {
                table.insert(last.clone(), value);
            }
            None => {
                table.remove(last);
            }
        },
        [head, rest @ ..] => {
            let entry = table
                .entry(head.clone())
                .or_insert_with(|| Value::Table(Table::new()));
            if let Value::Table(child) = entry {
                set_at(child, rest, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::validate;

    #[test]
    fn corrupt_toml_is_an_error() {
        assert!(matches!(
            load_document(b"[appearance\ntheme ="),
            Err(ConfigError::Corrupt(_))
        ));
    }

    #[test]
    fn invalid_utf8_is_an_error() {
        assert!(matches!(
            load_document(&[0xff, 0xfe, 0x00]),
            Err(ConfigError::Corrupt(_))
        ));
    }

    #[test]
    fn truncated_file_is_an_error() {
        let full = "version = 1\n[appearance]\ntheme = \"desilium-dark\"\nanimation_speed = 1.5\n";
        let cut = &full.as_bytes()[..full.len() - 9];
        assert!(load_document(cut).is_err());
    }

    #[test]
    fn empty_file_loads_as_migrated_defaults() {
        let loaded = load_document(b"").expect("loads");
        assert_eq!(loaded.config, DesiliumConfig::default());
        assert_eq!(loaded.recovery.len(), 1);
    }

    #[test]
    fn bad_field_is_defaulted_and_others_kept() {
        let text = "version = 1\n[appearance]\nanimation_speed = \"fast\"\ncursor_size = 40\n";
        let loaded = load_document(text.as_bytes()).expect("loads");
        assert_eq!(loaded.config.appearance.animation_speed, 1.0);
        assert_eq!(loaded.config.appearance.cursor_size, 40);
        assert!(loaded.recovery.iter().any(|r| matches!(
            r,
            Recovery::InvalidField { field, .. } if field == "appearance.animation_speed"
        )));
    }

    #[test]
    fn bad_shortcut_list_falls_back_without_losing_other_sections() {
        let text = "version = 1\n[appearance]\ncursor_size = 40\n[compositor]\nshortcuts = 5\n";
        let loaded = load_document(text.as_bytes()).expect("loads");
        assert_eq!(loaded.config.appearance.cursor_size, 40);
        assert_eq!(
            loaded.config.compositor.shortcuts,
            DesiliumConfig::default().compositor.shortcuts
        );
    }

    #[test]
    fn newer_version_is_loaded_and_reported() {
        let loaded =
            load_document(b"version = 9\n[appearance]\ncursor_size = 32\n").expect("best effort");
        assert_eq!(loaded.config.appearance.cursor_size, 32);
        assert!(
            loaded
                .recovery
                .contains(&Recovery::NewerVersion { found: 9 })
        );
    }

    #[test]
    fn v0_document_is_migrated_and_reported() {
        let loaded = load_document(b"terminal = \"foot\"\n").expect("loads");
        assert_eq!(loaded.config.compositor.terminal.as_deref(), Some("foot"));
        assert!(
            loaded
                .recovery
                .iter()
                .any(|r| matches!(r, Recovery::Migrated(_)))
        );
    }

    #[test]
    fn repaired_document_passes_validation_clean_after_load() {
        let text = "version = 1\n[appearance]\nanimation_speed = 100.0\n";
        let mut loaded = load_document(text.as_bytes()).expect("loads").config;
        validate(&mut loaded);
        assert!(validate(&mut loaded).is_clean());
    }
}
