//! The application catalog seen through the dock's lookup trait, plus launching.

use std::ffi::OsString;

use desilium_apps::{
    Catalog, DesktopEntry,
    launch::{self, Documents, LaunchCommand, LaunchError, TerminalSpec},
};

use crate::model::dock::{AppInfo, AppLookup};

/// Search engine used by the launcher's web entry; `{}` is the percent-encoded query.
pub const SEARCH_URL: &str = "https://duckduckgo.com/?q={}";

/// Adapter: [`Catalog`] as an [`AppLookup`].
pub struct CatalogLookup<'a>(pub &'a Catalog);

pub(crate) fn app_info(entry: &DesktopEntry) -> AppInfo {
    AppInfo {
        desktop_id: entry.id.as_str().to_owned(),
        name: entry.name.clone(),
        icon: entry.icon.clone(),
        categories: entry.categories.clone(),
    }
}

impl AppLookup for CatalogLookup<'_> {
    fn by_desktop_id(&self, desktop_id: &str) -> Option<AppInfo> {
        if !self.0.is_listed(desktop_id) {
            return None;
        }
        self.0.get(desktop_id).map(app_info)
    }

    fn by_app_id(&self, app_id: &str) -> Option<AppInfo> {
        if app_id.is_empty() {
            return None;
        }
        self.0.match_app_id(app_id).map(app_info)
    }
}

/// Percent-encodes `text` for a URL query component.
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The search URL for `query`.
pub fn search_url(query: &str) -> String {
    SEARCH_URL.replace("{}", &percent_encode(query.trim()))
}

/// Failure to start something from the shell.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// The desktop id is not in the catalog.
    #[error("unknown application {0}")]
    UnknownApp(String),
    /// Building or spawning the command failed.
    #[error(transparent)]
    Launch(#[from] LaunchError),
    /// Commands need a terminal emulator and none is installed.
    #[error("no terminal emulator found")]
    NoTerminal,
}

/// Starts processes on behalf of the dock and launcher, pointing them at our compositor.
pub struct Starter {
    display: Option<OsString>,
    terminal: Option<TerminalSpec>,
}

impl Starter {
    /// `terminal` is the emulator for commands (`None` disables them).
    pub fn new(display: Option<OsString>, terminal: Option<TerminalSpec>) -> Self {
        Self { display, terminal }
    }

    fn run(&self, command: &LaunchCommand) -> Result<(), StartError> {
        launch::spawn(command, self.display.as_deref())?;
        Ok(())
    }

    /// Starts an installed application (a new instance when it already runs).
    pub fn launch_app(&self, catalog: &Catalog, desktop_id: &str) -> Result<(), StartError> {
        let entry = catalog
            .get(desktop_id)
            .ok_or_else(|| StartError::UnknownApp(desktop_id.to_owned()))?;
        let command =
            launch::for_entry(entry, None, &Documents::default(), self.terminal.as_ref())?;
        self.run(&command)
    }

    /// Runs `line` through `sh -c` inside the terminal.
    pub fn run_shell(&self, line: &str) -> Result<(), StartError> {
        let terminal = self.terminal.as_ref().ok_or(StartError::NoTerminal)?;
        self.run(&launch::shell_command(line, terminal))
    }

    /// Runs the words of `text` inside the terminal.
    pub fn run_in_terminal(&self, text: &str) -> Result<(), StartError> {
        let command = launch::user_command(text, self.terminal.as_ref(), true)?;
        self.run(&command)
    }

    /// Opens the default search engine with `query`.
    pub fn web_search(&self, query: &str) -> Result<(), StartError> {
        let command = LaunchCommand::new("xdg-open", vec![search_url(query)]);
        self.run(&command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_encoding() {
        assert_eq!(percent_encode("rust book"), "rust%20book");
        assert_eq!(percent_encode("a&b=c/é"), "a%26b%3Dc%2F%C3%A9");
        assert_eq!(percent_encode("safe-_.~"), "safe-_.~");
    }

    #[test]
    fn search_url_trims_and_encodes() {
        assert_eq!(
            search_url("  24 * 7 "),
            "https://duckduckgo.com/?q=24%20%2A%207"
        );
    }
}
