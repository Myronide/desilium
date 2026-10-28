//! A message catalog: flat dotted keys to translated text.

use std::collections::BTreeMap;

/// Why a catalog file could not be used.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    /// Not valid TOML.
    #[error("catalog is not valid TOML: {0}")]
    Syntax(#[from] toml::de::Error),
    /// A key holds something other than text (a table, a number).
    #[error("catalog key {0:?} is not text")]
    NotText(String),
}

/// Translated texts by key. Keys are flat: the dots belong to the key, not to TOML tables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    messages: BTreeMap<String, String>,
}

impl Catalog {
    /// Reads a catalog from TOML text of the form `"key.with.dots" = "text"`.
    pub fn parse(source: &str) -> Result<Self, CatalogError> {
        let table: toml::Table = source.parse()?;
        let mut messages = BTreeMap::new();
        for (key, value) in table {
            match value {
                toml::Value::String(text) => {
                    messages.insert(key, text);
                }
                _ => return Err(CatalogError::NotText(key)),
            }
        }
        Ok(Self { messages })
    }

    /// The translation of `key`, if the catalog has one.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.messages.get(key).map(String::as_str)
    }

    /// Every key, sorted.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.messages.keys().map(String::as_str)
    }

    /// Number of messages.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// True when the catalog has no messages.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_quoted_keys_stay_flat() {
        let catalog = Catalog::parse("\"page.about\" = \"Info\"\n\"ui.a.b\" = \"x\"").expect("ok");
        assert_eq!(catalog.get("page.about"), Some("Info"));
        assert_eq!(catalog.get("page"), None);
        assert_eq!(catalog.len(), 2);
    }

    #[test]
    fn bad_catalogs_are_typed_errors() {
        assert!(matches!(
            Catalog::parse("not toml ="),
            Err(CatalogError::Syntax(_))
        ));
        assert!(matches!(
            Catalog::parse("\"a.b\" = 3"),
            Err(CatalogError::NotText(key)) if key == "a.b"
        ));
        assert!(Catalog::parse("\"a.b\" = \"x\"\n\"a.b\" = \"y\"").is_err());
    }
}
