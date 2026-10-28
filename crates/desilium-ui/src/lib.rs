//! HTML/CSS UI host for Desilium shell surfaces: HTML in, pixels and actions out.
//!
//! The rendering engine (Blitz + vello_cpu) stays behind [`UiSurface`]; no
//! engine type appears in the public API, so the backend can be replaced.

//!
//! ```
//! use desilium_ui::{SurfaceOptions, UiSurface, icon_svg};
//!
//! let html = format!(
//!     "<div class=bar><span id=clock>14:32</span><button data-action=settings>{}</button></div>",
//!     icon_svg("settings", 16, "")
//! );
//! let css = ".bar{display:flex;gap:8px;color:var(--text)} button{border:0;background:var(--hover)}";
//! let mut ui = UiSurface::new(&html, &[css], SurfaceOptions::new(240, 32))?;
//! ui.set_text("#clock", "14:33")?;
//!
//! // premultiplied BGRA, the byte order of wl_shm Argb8888
//! let stride = ui.min_stride();
//! let mut pixels = vec![0u8; stride * ui.buffer_size().1 as usize];
//! let stats = ui.render(&mut pixels, stride)?;
//! assert!(stats.rendered && !ui.needs_render());
//!
//! // pointer in, actions out
//! ui.pointer_down(200.0, 10.0, desilium_ui::PointerButton::Primary);
//! ui.pointer_up(200.0, 10.0, desilium_ui::PointerButton::Primary);
//! for action in ui.take_actions() {
//!     println!("{} {:?}", action.action, action.arg);
//! }
//! # Ok::<(), desilium_ui::UiError>(())
//! ```

mod error;
mod icons;
mod input;
mod selector;
mod surface;
mod theme;
#[cfg(feature = "wayland")]
pub mod wayland;

pub use error::UiError;
pub use icons::{app_icon_html, has_icon, icon_names, icon_svg, inline_sprite_uses};
pub use input::{
    CursorHint, DragPhase, HoverTarget, Key, KeyEvent, Modifiers, PixelRect, PointerButton, Rect,
    RenderStats, UiAction, UiDrag, UiEvent, WheelDelta,
};
pub use surface::{SurfaceOptions, UiSurface, Updates, bgra_premultiplied_to_rgba};
pub use theme::{blitz_fixups_css, tokens_css};
