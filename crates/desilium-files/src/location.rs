use std::path::{Path, PathBuf};
use url::Url;

/// Canonical URI. Passwords are rejected before a URI can enter history or saved places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location(pub String);
impl Location {
    pub fn parse(text: &str, home: &Path) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("Enter a folder path or server address.".into());
        }

        if !text.contains("://") {
            let path = match text.strip_prefix("~/") {
                Some(tail) => home.join(tail),
                None if text == "~" => home.to_path_buf(),
                None => PathBuf::from(text),
            };
            let path = if path.is_absolute() {
                path
            } else {
                home.join(path)
            };
            return Self::from_path(&path);
        }

        let uri = Url::parse(text).map_err(|_| "The address is not valid.".to_owned())?;
        if !matches!(
            uri.scheme(),
            "file" | "smb" | "sftp" | "ftp" | "dav" | "davs" | "network" | "trash"
        ) {
            return Err(
                "Use a folder path, smb://, sftp://, ftp://, dav:// or davs:// address.".into(),
            );
        }

        if uri.password().is_some() {
            return Err("Enter passwords in the sign-in dialog, never in an address.".into());
        }

        if uri.scheme() == "file" && uri.to_file_path().is_err() {
            return Err("This file address is not a local path.".into());
        }

        if matches!(uri.scheme(), "smb" | "sftp" | "ftp" | "dav" | "davs")
            && uri.host_str().is_none()
        {
            return Err("The server address needs a hostname.".into());
        }

        Ok(Self(uri.to_string()))
    }

    pub fn from_path(path: &Path) -> Result<Self, String> {
        Url::from_file_path(path)
            .map(|uri| Self(uri.to_string()))
            .map_err(|_| "The folder path must be absolute.".into())
    }

    pub fn path(&self) -> Option<PathBuf> {
        Url::parse(&self.0).ok()?.to_file_path().ok()
    }

    pub fn label(&self) -> String {
        self.path()
            .map_or_else(|| self.0.clone(), |path| path.display().to_string())
    }

    pub fn parent(&self) -> Option<Self> {
        let file = gio::File::for_uri(&self.0);
        use gio::prelude::*;
        file.parent().map(|parent| Self(parent.uri().into()))
    }
}

pub fn valid_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || matches!(name, "." | "..") || name.contains(['/', '\0']) {
        return Err("Choose a name without slashes; . and .. are reserved.".into());
    }
    
    Ok(())
}
