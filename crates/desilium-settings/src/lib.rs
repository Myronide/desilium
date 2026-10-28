//! The Desilium settings application.
//!
//! Pure logic (registry, search, changes) is separate from the Wayland window so it can be
//! tested without a display; the UI is HTML/CSS rendered by `desilium-ui`.

pub mod app;
pub mod changes;
pub mod i18n;
pub mod model;
pub mod palette;
pub mod refresh;
pub mod registry;
pub mod runtime;
pub mod search;
pub mod shortcuts;
pub mod store;
pub mod surface;
pub mod theme;
pub mod view;
pub mod wallpapers;
