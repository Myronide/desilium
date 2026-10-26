//! `--set KEY=VALUE`: settings applied before the window opens, through the normal change path.

use desilium_config::ConfigStore;

use crate::{changes, registry};

/// Applies every `KEY=VALUE`; the first problem aborts with a readable message.
pub fn apply_sets(store: &mut ConfigStore, sets: &[String]) -> Result<(), String> {
    for item in sets {
        let (id, text) = item
            .split_once('=')
            .ok_or_else(|| format!("--set {item}: expected KEY=VALUE"))?;
        let def =
            registry::find(id).ok_or_else(|| format!("--set {item}: unknown setting \"{id}\""))?;
        let value = if text.is_empty() {
            registry::Value::Unset
        } else {
            def.value_from_text(text)
                .ok_or_else(|| format!("--set {item}: \"{text}\" is not a valid value"))?
        };
        changes::set_value(store, def, value).map_err(|error| format!("--set {item}: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ConfigStore) {
        let dir = tempfile::tempdir().expect("dir");
        let store = ConfigStore::open(dir.path()).store;
        (dir, store)
    }

    #[test]
    fn sets_go_through_validation() {
        let (_dir, mut store) = store();
        let sets = [
            "appearance.theme=desilium-light".to_owned(),
            "appearance.accent=e8348a".to_owned(),
        ];
        apply_sets(&mut store, &sets).expect("apply");
        assert_eq!(store.get().appearance.theme, "desilium-light");
        assert_eq!(store.get().appearance.accent.as_deref(), Some("#E8348A"));
    }

    #[test]
    fn empty_value_clears_an_optional_setting() {
        let (_dir, mut store) = store();
        apply_sets(&mut store, &["appearance.accent=#FF0000".into()]).expect("set");
        apply_sets(&mut store, &["appearance.accent=".into()]).expect("clear");
        assert_eq!(store.get().appearance.accent, None);
    }

    #[test]
    fn problems_are_reported_by_name() {
        let (_dir, mut store) = store();
        for bad in [
            "nokey",
            "nope.x=1",
            "appearance.cursor_size=big",
            "appearance.theme=neon",
        ] {
            let error = apply_sets(&mut store, &[bad.to_owned()]).expect_err(bad);
            assert!(error.contains(bad), "{error}");
        }
    }
}
