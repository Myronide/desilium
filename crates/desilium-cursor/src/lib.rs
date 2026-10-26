//! Cursor theme loading (XCursor), animation frame selection and a built-in fallback.
//!
//! This is a pure CPU crate: every lookup yields premultiplied RGBA frames that the
//! compositor turns into textures. Loading never fails; missing themes fall back to
//! procedurally drawn cursors.

mod builtin;
mod file;
mod frames;
mod image;
mod kind;
mod theme;

#[cfg(test)]
mod test_support;

pub use builtin::builtin_cursor;
pub use image::{CursorImage, premultiply_argb};
pub use kind::CursorKind;
pub use theme::CursorTheme;
