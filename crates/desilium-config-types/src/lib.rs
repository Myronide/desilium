//! User-facing settings types for the Desilium compositor.
//!
//! Pure data: serde types with `Default` values and no I/O. TOML is only the
//! storage format.

mod action;
mod root;
mod settings;
mod shell;
mod shortcut;
pub use shell::{PanelEdge, PanelMode, ShellSettings, Visibility};

pub use root::*;

pub use action::Action;
pub use settings::{
    CompositorSettings, DEFAULT_DESKTOPS, FocusPolicy, KeyboardLayout, MAX_DESKTOPS, MIN_DESKTOPS,
};
pub use shortcut::{Modifiers, Shortcut};
