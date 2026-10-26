//! Desktop Entry specification 1.5: the INI-style file format, locale-aware keys,
//! typed entries and the main menu categories.

pub mod category;
pub mod entry;
pub mod ini;
pub mod locale;

pub use category::MainCategory;
pub use entry::{DesktopAction, DesktopEntry, DesktopId, EntryError};
