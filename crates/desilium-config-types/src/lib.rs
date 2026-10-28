//! User-facing settings types for Desilium and, through `desilium-kwin`, for KWin.
//!
//! Pure data: serde types with `Default` values and no I/O. TOML is only the
//! storage format.

mod action;
mod mouse;
mod root;
mod settings;
mod shell;
mod shortcut;
pub use mouse::{
    AccelProfile, DOUBLE_CLICK_RANGE, MouseSettings, POINTER_SPEED_RANGE, SCROLL_SPEED_RANGE,
};
pub use shell::{PanelEdge, PanelMode, ShellSettings, Visibility};

pub use root::*;

pub use action::Action;
pub use settings::{
    CompositorSettings, DEFAULT_DESKTOPS, FocusPolicy, KeyboardLayout, MAX_DESKTOPS, MIN_DESKTOPS,
};
pub use shortcut::{Modifiers, Shortcut};
