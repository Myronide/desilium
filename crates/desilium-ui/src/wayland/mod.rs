//! Layer-shell host: puts [`UiSurface`](crate::UiSurface)s on screen and feeds them input.
//!
//! # Threading model
//!
//! Everything is single-threaded. A [`LayerHost`] owns the Wayland connection,
//! the `wl_shm` pools and all documents; none of it is `Sync`. The host has no
//! event loop of its own. The application registers [`LayerHost::poll_fd`] in
//! its loop and calls [`LayerHost::dispatch`] when it is readable:
//!
//! ```ignore
//! use std::time::Instant;
//! use desilium_ui::{wayland::*, *};
//!
//! let mut host = LayerHost::connect()?;
//! let mut ui = UiSurface::new(BAR_HTML, &[BAR_CSS], SurfaceOptions::new(1920, 32))?;
//! let mut config = LayerConfig::new("desilium-bar", (0, 32));
//! config.anchor = Anchor::TOP_BAR;
//! config.exclusive_zone = 32;
//! let bar = host.add_surface(config, ui)?;
//!
//! // calloop: Generic::new(host.poll_fd(), Interest::READ, Mode::Level) ... then
//! host.dispatch(Instant::now())?;
//! for event in host.take_events() {
//!     if let HostEvent::Ui { event: UiEvent::Action(action), .. } = event { /* act */ }
//! }
//! // Clock tick from a timer source:
//! host.update(bar, |ui| ui.set_text("#clock", "14:32"));
//! ```
//!
//! Rendering is demand driven: a surface is drawn only when its document is
//! dirty, it is configured, and the previous frame callback has fired. Identical
//! frames are not committed. Fractional scale (`wp_fractional_scale` plus
//! `wp_viewporter`) is used when the compositor offers it, otherwise the integer
//! output scale. The cursor shape follows CSS `cursor` through
//! `wp_cursor_shape_v1` when available (otherwise the compositor's cursor stays).

mod config;
mod handlers;
mod host;
mod keys;
mod state;

pub use config::{Anchor, HostEvent, KeyboardMode, Layer, LayerConfig, Margins, SurfaceId};
pub use host::LayerHost;
