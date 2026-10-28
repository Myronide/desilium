//! One key of a KDE configuration file, as the bridge writes it.

/// A value as `kwriteconfig6` takes it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    /// Written as is; numbers go through here too, KConfig parses them on read.
    Text(String),
    /// Written with `--type bool`.
    Bool(bool),
}

impl Value {
    /// The text handed to `kwriteconfig6`.
    pub fn render(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Bool(true) => "true",
            Self::Bool(false) => "false",
        }
    }
}

/// File, group and key of an entry; two entries with the same identity replace each other.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntryId {
    /// Configuration file name below `$XDG_CONFIG_HOME`, e.g. `kwinrc`.
    pub file: &'static str,
    /// Group header without brackets, e.g. `Windows` or `Colors:View`.
    pub group: String,
    /// Key inside the group.
    pub key: String,
}

/// One setting to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEntry {
    /// Where it goes.
    pub id: EntryId,
    /// What to write.
    pub value: Value,
}

impl ConfigEntry {
    /// A text entry.
    pub fn text(file: &'static str, group: &str, key: &str, value: impl Into<String>) -> Self {
        Self::new(file, group, key, Value::Text(value.into()))
    }

    /// A boolean entry.
    pub fn flag(file: &'static str, group: &str, key: &str, value: bool) -> Self {
        Self::new(file, group, key, Value::Bool(value))
    }

    fn new(file: &'static str, group: &str, key: &str, value: Value) -> Self {
        Self {
            id: EntryId {
                file,
                group: group.to_owned(),
                key: key.to_owned(),
            },
            value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_render_the_way_kconfig_reads_them() {
        assert_eq!(Value::Bool(true).render(), "true");
        assert_eq!(Value::Bool(false).render(), "false");
        assert_eq!(Value::Text("0.5".to_owned()).render(), "0.5");
    }

    #[test]
    fn identity_ignores_the_value() {
        let a = ConfigEntry::text("kwinrc", "Windows", "FocusPolicy", "ClickToFocus");
        let b = ConfigEntry::text("kwinrc", "Windows", "FocusPolicy", "FocusFollowsMouse");
        assert_eq!(a.id, b.id);
        assert_ne!(a, b);
    }
}
