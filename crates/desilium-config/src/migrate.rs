//! Schema migrations, applied as a chain of single-version steps.
//!
//! To add a version: bump `CURRENT_VERSION` in `desilium-config-types`, add a
//! `vN_to_vN+1` function, register it in [`step`], and test it. Documents with an
//! unknown *newer* version are returned unchanged; the store loads them best-effort.

use std::fmt;

use desilium_config_types::CURRENT_VERSION;
use toml::{Table, Value};

const VERSION_KEY: &str = "version";
const COMPOSITOR_KEY: &str = "compositor";
/// Keys that v0 stored at the top level instead of inside `[compositor]`.
const V0_COMPOSITOR_KEYS: [&str; 2] = ["terminal", "shortcuts"];

/// One applied migration step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationNote {
    pub from: u32,
    pub to: u32,
}

impl fmt::Display for MigrationNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "configuration upgraded from version {} to {}",
            self.from, self.to
        )
    }
}

/// A document that cannot be migrated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MigrationError {
    #[error("version must be a non-negative integer, found {0}")]
    InvalidVersion(String),
    #[error("cannot migrate from version {from}: {reason}")]
    Step { from: u32, reason: String },
}

/// Returns the document's version. A missing `version` key means version 0.
pub fn stored_version(table: &Table) -> Result<u32, MigrationError> {
    match table.get(VERSION_KEY) {
        None => Ok(0),
        Some(Value::Integer(number)) => {
            u32::try_from(*number).map_err(|_| MigrationError::InvalidVersion(number.to_string()))
        }
        Some(other) => Err(MigrationError::InvalidVersion(other.to_string())),
    }
}

/// Upgrades `table` to [`CURRENT_VERSION`], returning the steps that ran.
///
/// Documents already at or above the current version are returned unchanged.
pub fn migrate(mut table: Table) -> Result<(Table, Vec<MigrationNote>), MigrationError> {
    let mut version = stored_version(&table)?;
    let mut notes = Vec::new();
    while version < CURRENT_VERSION {
        let next = version.saturating_add(1);
        table = step(version, table)?;
        notes.push(MigrationNote {
            from: version,
            to: next,
        });
        version = next;
    }
    Ok((table, notes))
}

fn step(from: u32, table: Table) -> Result<Table, MigrationError> {
    match from {
        0 => v0_to_v1(table),
        _ => Err(MigrationError::Step {
            from,
            reason: "no migration step is registered for this version".to_owned(),
        }),
    }
}

/// v0 had `terminal` and `shortcuts` at the top level; v1 nests them in `[compositor]`.
fn v0_to_v1(mut table: Table) -> Result<Table, MigrationError> {
    let mut compositor = take_compositor_section(&mut table)?;
    for key in V0_COMPOSITOR_KEYS {
        if let Some(value) = table.remove(key) {
            compositor.insert(key.to_owned(), value);
        }
    }
    table.insert(COMPOSITOR_KEY.to_owned(), Value::Table(compositor));
    table.insert(VERSION_KEY.to_owned(), version_value(1));
    Ok(table)
}

fn take_compositor_section(table: &mut Table) -> Result<Table, MigrationError> {
    match table.remove(COMPOSITOR_KEY) {
        None => Ok(Table::new()),
        Some(Value::Table(section)) => Ok(section),
        Some(_) => Err(MigrationError::Step {
            from: 0,
            reason: "`compositor` must be a table".to_owned(),
        }),
    }
}

fn version_value(version: u32) -> Value {
    Value::Integer(i64::from(version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Table {
        text.parse().expect("test document parses")
    }

    #[test]
    fn v0_top_level_keys_move_into_compositor_section() {
        let v0 = parse(
            r#"
            terminal = "foot"
            [[shortcuts]]
            key = "t"
            action = "launch-terminal"
            "#,
        );
        let (v1, notes) = migrate(v0).expect("migrates");
        assert_eq!(notes, vec![MigrationNote { from: 0, to: 1 }]);
        assert_eq!(stored_version(&v1), Ok(1));
        assert!(!v1.contains_key("terminal"));
        assert!(!v1.contains_key("shortcuts"));
        let compositor = v1["compositor"].as_table().expect("compositor table");
        assert_eq!(compositor["terminal"].as_str(), Some("foot"));
        assert_eq!(compositor["shortcuts"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn current_version_is_returned_unchanged() {
        let v1 = parse("version = 1\n[appearance]\ntheme = \"x\"\n");
        let (out, notes) = migrate(v1.clone()).expect("no-op");
        assert!(notes.is_empty());
        assert_eq!(out, v1);
    }

    #[test]
    fn future_version_is_left_alone() {
        let future = parse("version = 7\n[mystery]\nflag = true\n");
        let (out, notes) = migrate(future.clone()).expect("best effort");
        assert!(notes.is_empty());
        assert_eq!(out, future);
        assert_eq!(stored_version(&out), Ok(7));
    }

    #[test]
    fn empty_document_migrates_from_zero() {
        let (out, notes) = migrate(Table::new()).expect("migrates");
        assert_eq!(notes.len(), 1);
        assert_eq!(stored_version(&out), Ok(CURRENT_VERSION));
        assert!(out["compositor"].as_table().is_some_and(Table::is_empty));
    }

    #[test]
    fn non_integer_version_is_rejected() {
        let bad = parse("version = \"one\"\n");
        assert!(matches!(
            migrate(bad),
            Err(MigrationError::InvalidVersion(_))
        ));
    }

    #[test]
    fn negative_version_is_rejected() {
        let bad = parse("version = -3\n");
        assert!(matches!(
            stored_version(&bad),
            Err(MigrationError::InvalidVersion(_))
        ));
    }

    #[test]
    fn non_table_compositor_fails_without_panicking() {
        let bad = parse("compositor = 4\n");
        assert!(matches!(
            migrate(bad),
            Err(MigrationError::Step { from: 0, .. })
        ));
    }
}
