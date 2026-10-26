//! Window chrome geometry and hit testing (pure logic, no Smithay types).
//!
//! The window manager places whole *frames* (title bar and border included). This module turns a
//! frame rectangle into the title bar, button, client and resize-zone rectangles, converts
//! between frame and client sizes, and decides what a pointer position hits.

mod geometry;
mod glyph;
mod hit;
mod interaction;
mod metrics;

pub use geometry::{Button, Frame};
pub use glyph::{GlyphKind, rasterize as rasterize_glyph};
pub use hit::{Hit, hit_test};
pub use interaction::{ButtonState, Interaction};
pub use metrics::{Chrome, Metrics};
