//! Reads from the operating system: wall clock and the pinned-apps file.

use std::{fs, io, path::Path};

use chrono::{Datelike, Local, Timelike};
use desilium_apps::Catalog;

use crate::model::{
    clock::LocalTime,
    pins::{DOCK_FILE_NAME, DockConfig},
};

/// The current local time.
pub fn local_now() -> LocalTime {
    let now = Local::now();
    let small = |value: u32| u8::try_from(value).unwrap_or(0);
    LocalTime {
        weekday: small(now.weekday().num_days_from_monday()),
        day: small(now.day()),
        month: small(now.month()),
        hour: small(now.hour()),
        minute: small(now.minute()),
        second: small(now.second()),
        nanos: now.nanosecond(),
    }
}

/// Loads `dock.toml`; the first run derives the default from the catalog and saves it.
///
/// A damaged file is left alone (the user may be mid-edit) and the default is used in memory.
pub fn load_pins(dir: &Path, catalog: &Catalog) -> DockConfig {
    let path = dir.join(DOCK_FILE_NAME);
    match fs::read_to_string(&path) {
        Ok(text) => DockConfig::parse(&text).unwrap_or_else(|error| {
            tracing::warn!(%error, path = %path.display(), "using default dock pins");
            first_run(catalog)
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let config = first_run(catalog);
            save_pins(dir, &config);
            config
        }
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "cannot read dock pins");
            first_run(catalog)
        }
    }
}

fn first_run(catalog: &Catalog) -> DockConfig {
    DockConfig::first_run(|id| catalog.is_listed(id))
}

fn save_pins(dir: &Path, config: &DockConfig) {
    let text = match config.render() {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!(%error, "cannot serialise dock pins");
            return;
        }
    };
    let written = fs::create_dir_all(dir).and_then(|()| fs::write(dir.join(DOCK_FILE_NAME), text));
    if let Err(error) = written {
        tracing::warn!(%error, "cannot save dock pins");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_apps::Environment;

    fn catalog_with(dir: &Path, ids: &[&str]) -> Catalog {
        let apps = dir.join("share/applications");
        fs::create_dir_all(&apps).unwrap();
        for id in ids {
            let stem = id.trim_end_matches(".desktop");
            fs::write(
                apps.join(id),
                format!("[Desktop Entry]\nType=Application\nName={stem}\nExec={stem}\n"),
            )
            .unwrap();
        }
        let share = dir.join("share").display().to_string();
        let home = dir.display().to_string();
        let env = Environment::from_lookup(|key| match key {
            "HOME" => Some(home.clone()),
            "XDG_DATA_HOME" => Some(format!("{home}/none")),
            "XDG_DATA_DIRS" => Some(share.clone()),
            _ => None,
        });
        Catalog::load(&env)
    }

    #[test]
    fn first_run_derives_and_saves_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = catalog_with(dir.path(), &["foot.desktop", "firefox.desktop"]);
        let config_dir = dir.path().join("config");
        let pins = load_pins(&config_dir, &catalog);
        assert_eq!(pins.pinned, ["firefox.desktop", "foot.desktop"]);
        let saved = fs::read_to_string(config_dir.join(DOCK_FILE_NAME)).unwrap();
        assert!(saved.contains("firefox.desktop"));
    }

    #[test]
    fn existing_file_wins_over_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = catalog_with(dir.path(), &["foot.desktop"]);
        let config_dir = dir.path().join("config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join(DOCK_FILE_NAME), "pinned = ['x.desktop']").unwrap();
        assert_eq!(load_pins(&config_dir, &catalog).pinned, ["x.desktop"]);
    }

    #[test]
    fn damaged_file_falls_back_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = catalog_with(dir.path(), &["foot.desktop"]);
        let config_dir = dir.path().join("config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join(DOCK_FILE_NAME), "pinned = [[[").unwrap();
        assert_eq!(load_pins(&config_dir, &catalog).pinned, ["foot.desktop"]);
        assert_eq!(
            fs::read_to_string(config_dir.join(DOCK_FILE_NAME)).unwrap(),
            "pinned = [[["
        );
    }

    #[test]
    fn local_time_is_plausible() {
        let now = local_now();
        assert!((1..=31).contains(&now.day));
        assert!((1..=12).contains(&now.month));
        assert!(now.hour < 24 && now.minute < 60);
    }
}

/// GUI pin edits are atomic and never replace a file before a complete write.
pub(super) fn persist_pins(dir: &Path, config: &DockConfig) -> io::Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    fs::create_dir_all(dir)?;
    let temporary = dir.join(format!("dock.{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(config.render().map_err(io::Error::other)?.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, dir.join(DOCK_FILE_NAME))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod pin_write_tests {
    use super::*;
    #[test]
    fn gui_pins_save_atomically_and_refuse_an_existing_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let config = DockConfig {
            pinned: vec!["kitty.desktop".into()],
            ..Default::default()
        };
        persist_pins(directory.path(), &config).unwrap();
        assert_eq!(
            DockConfig::parse(&fs::read_to_string(directory.path().join(DOCK_FILE_NAME)).unwrap())
                .unwrap(),
            config
        );
        fs::write(
            directory
                .path()
                .join(format!("dock.{}.tmp", std::process::id())),
            "another writer",
        )
        .unwrap();
        let before = fs::read(directory.path().join(DOCK_FILE_NAME)).unwrap();
        assert!(persist_pins(directory.path(), &Default::default()).is_err());
        assert_eq!(
            fs::read(directory.path().join(DOCK_FILE_NAME)).unwrap(),
            before
        );
    }
}
