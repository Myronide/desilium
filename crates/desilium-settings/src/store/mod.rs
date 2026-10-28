//! The theme store: browsing the KDE Store, downloading and installing icon themes, cursor
//! themes and colour schemes.
//!
//! Layout: [`ocs`] speaks the store's API (pure parsing and URL building), [`process`] runs the
//! external tools with limits, [`install`] unpacks archives safely and puts themes where
//! applications look for them, [`worker`] runs all of that on a background thread, and
//! [`state`] is what the page shows. Nothing here blocks the window.
//!
//! Network and archive work go through `curl`, `tar` and `unzip` with fixed arguments, like the
//! rest of Desilium talks to `nmcli` and `kwriteconfig6`. Nothing of this has been tried against
//! the live store; see `docs/theme-store.md`.

pub mod install;
pub mod ocs;
mod process;
pub mod state;
pub mod worker;

use std::{
    env,
    path::{Path, PathBuf},
};

pub use ocs::{Entry, Kind, Listing, RemoteFile, Sort};
pub use state::{Load, StoreState};
pub use worker::{Done, Job, StoreWorker};

use crate::i18n::{tr, tr_args};

/// Why a store action failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// A tool (`curl`, `tar`, `unzip`) is not installed.
    #[error("the tool `{0}` is not installed")]
    MissingTool(&'static str),
    /// The network or the store did not answer.
    #[error("no network, or the store did not answer: {0}")]
    Network(String),
    /// The answer could not be read.
    #[error("the answer of the store could not be read: {0}")]
    BadResponse(String),
    /// The store refused the request.
    #[error("the store reported an error: {0}")]
    Rejected(String),
    /// The entry has no file this app can install.
    #[error("this entry has no download Desilium can install")]
    NoDownload,
    /// The file is not an archive (or a colour scheme) this app can read.
    #[error("the download is not an archive Desilium can read")]
    NotAnArchive,
    /// The archive holds something that must not be unpacked.
    #[error("the archive contains an unsafe entry ({0}); nothing was installed")]
    Unsafe(String),
    /// The archive holds no theme of the wanted kind.
    #[error("the archive contains no {0}")]
    NothingFound(&'static str),
    /// A directory of that name exists and did not come from the store.
    #[error("\"{0}\" already exists and was not installed from the store")]
    Foreign(String),
    /// Output, download or unpacked size over the limit.
    #[error("the download is too large")]
    TooLarge,
    /// There is no home directory to install into.
    #[error("there is no home directory to install into")]
    NoHome,
    /// A file system call failed.
    #[error("file system: {0}")]
    Io(#[from] std::io::Error),
}

impl StoreError {
    /// The sentence shown to the user, in their language.
    pub fn message(&self) -> String {
        match self {
            Self::MissingTool(tool) => tr_args(
                "ui.store.err_tool",
                "The tool \"{tool}\" is not installed.",
                &[("tool", tool)],
            ),
            Self::Network(detail) => tr_args(
                "ui.store.err_network",
                "No network, or the store did not answer ({detail}).",
                &[("detail", detail)],
            ),
            Self::BadResponse(_) | Self::Rejected(_) => tr(
                "ui.store.err_response",
                "The store sent an answer that could not be used.",
            ),
            Self::NoDownload => tr(
                "ui.store.err_no_download",
                "This entry has no download that can be installed.",
            ),
            Self::NotAnArchive => tr(
                "ui.store.err_archive",
                "The download is not an archive that can be read.",
            ),
            Self::Unsafe(entry) => tr_args(
                "ui.store.err_unsafe",
                "The archive contains an unsafe entry ({entry}). Nothing was installed.",
                &[("entry", entry)],
            ),
            Self::NothingFound(what) => tr_args(
                "ui.store.err_nothing",
                "The archive contains no {what}.",
                &[("what", &tr_what(what))],
            ),
            Self::Foreign(name) => tr_args(
                "ui.store.err_foreign",
                "\"{name}\" already exists and was not installed from the store. Remove it first.",
                &[("name", name)],
            ),
            Self::TooLarge => tr("ui.store.err_large", "The download is too large."),
            Self::NoHome => tr(
                "ui.store.err_home",
                "There is no home directory to install into.",
            ),
            Self::Io(error) => tr_args(
                "ui.store.err_io",
                "A file could not be written ({error}).",
                &[("error", &error.to_string())],
            ),
        }
    }
}

/// The noun of a kind inside a sentence ("the archive contains no icon theme").
fn tr_what(what: &str) -> String {
    match what {
        "icon theme" => tr("ui.store.what_icons", "icon theme"),
        "cursor theme" => tr("ui.store.what_cursors", "cursor theme"),
        _ => tr("ui.store.what_colours", "colour scheme"),
    }
}

/// Where the store looks for installed themes, installs new ones and keeps scratch files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreDirs {
    /// Where icon and cursor themes are installed (`~/.local/share/icons`); empty without a home.
    pub icons: PathBuf,
    /// Where colour schemes are installed (`~/.local/share/color-schemes`).
    pub schemes: PathBuf,
    /// Scratch space for downloads and unpacking.
    pub work: PathBuf,
    /// Every directory that may hold installed icon or cursor themes.
    pub icon_search: Vec<PathBuf>,
    /// Every directory that may hold colour schemes.
    pub scheme_search: Vec<PathBuf>,
}

impl StoreDirs {
    /// The XDG directories of the running process.
    pub fn from_environment() -> Self {
        let var = |name: &str| {
            env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let home = var("HOME");
        let data_home =
            var("XDG_DATA_HOME").or_else(|| home.as_ref().map(|h| h.join(".local/share")));
        let cache_home = var("XDG_CACHE_HOME").or_else(|| home.as_ref().map(|h| h.join(".cache")));
        let under = |base: &Option<PathBuf>, leaf: &str| {
            base.as_ref().map(|b| b.join(leaf)).unwrap_or_default()
        };
        let kde = desilium_theme::kde::KdeDirs::from_env();
        Self {
            icons: under(&data_home, "icons"),
            schemes: under(&data_home, "color-schemes"),
            work: under(&cache_home, "desilium/store"),
            icon_search: crate::runtime::cursors::icon_dirs(),
            scheme_search: kde
                .data_dirs
                .iter()
                .map(|dir| dir.join("color-schemes"))
                .collect(),
        }
    }

    /// Everything below `root`, for tests.
    pub fn under(root: &Path) -> Self {
        Self {
            icons: root.join("data/icons"),
            schemes: root.join("data/color-schemes"),
            work: root.join("cache/store"),
            icon_search: vec![root.join("data/icons")],
            scheme_search: vec![root.join("data/color-schemes")],
        }
    }

    /// Whether installing is possible at all.
    pub fn can_install(&self) -> bool {
        self.icons.is_absolute() && self.schemes.is_absolute() && self.work.is_absolute()
    }
}
