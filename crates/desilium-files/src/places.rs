use crate::location::Location;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    pub uri: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Places {
    pub pins: Vec<Place>,
}

impl Places {

    pub fn load(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(raw) => {
                let places: Self = toml::from_str(&raw).map_err(|error| {
                    format!("Saved places could not be read: {error}. The file was left unchanged.")
                })?;
                for pin in &places.pins {
                    Location::parse(&pin.uri, Path::new("/"))?;
                }
                Ok(places)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn toggle(&mut self, location: &Location, name: String) {
        if let Some(index) = self.pins.iter().position(|pin| pin.uri == location.0) {
            self.pins.remove(index);
            return;
        }
        self.pins.push(Place {
            name,
            uri: location.0.clone(),
        });
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let parent = path.parent().ok_or("Places path has no parent")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;

        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| error.to_string())?;

        let result = (|| {
            let raw = toml::to_string_pretty(self).map_err(|error| error.to_string())?;
            file.write_all(raw.as_bytes())
                .map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            fs::rename(&temporary, path).map_err(|error| error.to_string())
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }

        result
    }

    pub fn default_path() -> PathBuf {
        desilium_config::ConfigStore::default_dir().join("doria/places.toml")
    }
}
