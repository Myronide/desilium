//! Bridge between the typed Desilium configuration and the KWin compositor.
//!
//! Desilium does not ship a compositor; KWin is it. This crate is the one place that knows how to
//! express Desilium's settings in KWin's terms:
//!
//! - [`plan`]: the configuration as KWin/KDE config keys (pure, tested without KWin),
//! - [`apply`]: writing the difference with `kwriteconfig6` and asking KWin to reconfigure,
//! - [`shortcuts`]: global shortcuts through KGlobalAccel, and a listener for Desilium's own,
//! - [`windows`]: exact window placement through a one-shot KWin script,
//! - [`bridge`]: the `desilium-kwin` process that keeps all of it current.
//!
//! The shell depends on this crate for [`windows::set_geometry`] and
//! [`shortcuts::ShortcutListener`]; everything else is used by the bridge binary.

pub mod apply;
pub mod bridge;
mod error;
pub mod plan;
pub mod shortcuts;
pub mod windows;

pub use error::KwinError;
