//! Cursor state: which cursor image is shown at the pointer, and why.

use crate::state::DesiliumState;
use desilium_cursor::CursorKind;
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::input::pointer::{CursorIcon, CursorImageStatus};
use smithay::output::Output;
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{DisplayHandle, Resource};
use smithay::utils::{Logical, Point};
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
use tracing::warn;

/// What the compositor draws under the pointer this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorSource {
    /// Nothing: the client hid the cursor or the pointer was never placed.
    Hidden,
    /// A themed cursor.
    Themed(CursorKind),
    /// A client-provided surface; its hotspot lives in the surface data.
    Surface(WlSurface),
}

/// Runtime cursor state.
pub struct CursorState {
    status: CursorImageStatus,
    override_kind: Option<CursorKind>,
    /// Stays false until the pointer moved once, so untouched screenshots show no cursor.
    pointer_seen: Cell<bool>,
    wake_armed: Rc<Cell<bool>>,
    handle: LoopHandle<'static, DesiliumState>,
    // Held only to keep the cursor-shape global alive.
    _shape_manager: CursorShapeManagerState,
}

impl CursorState {
    /// Creates the state and registers `wp_cursor_shape_manager_v1`.
    pub fn new(handle: LoopHandle<'static, DesiliumState>, display: &DisplayHandle) -> Self {
        Self {
            status: CursorImageStatus::default_named(),
            override_kind: None,
            pointer_seen: Cell::new(false),
            wake_armed: Rc::new(Cell::new(false)),
            handle,
            _shape_manager: CursorShapeManagerState::new::<DesiliumState>(display),
        }
    }
}

/// Maps a smithay icon (also what cursor-shape-v1 resolves to) onto a themed kind.
///
/// Icons without a dedicated kind try their aliases, then fall back to the arrow.
pub fn kind_for_icon(icon: CursorIcon) -> CursorKind {
    std::iter::once(icon.name())
        .chain(icon.alt_names().iter().copied())
        .find_map(CursorKind::from_css_name)
        .unwrap_or(CursorKind::Default)
}

/// Priority: chrome override, then the client's surface, then its named shape; hidden hides.
pub fn resolve(
    override_kind: Option<CursorKind>,
    status: &CursorImageStatus,
    pointer_seen: bool,
) -> CursorSource {
    if !pointer_seen {
        return CursorSource::Hidden;
    }
    if let Some(kind) = override_kind {
        return CursorSource::Themed(kind);
    }
    match status {
        CursorImageStatus::Hidden => CursorSource::Hidden,
        CursorImageStatus::Named(icon) => CursorSource::Themed(kind_for_icon(*icon)),
        CursorImageStatus::Surface(surface) if surface.is_alive() => {
            CursorSource::Surface(surface.clone())
        }
        CursorImageStatus::Surface(_) => CursorSource::Themed(CursorKind::Default),
    }
}

impl DesiliumState {
    /// Chrome (resize edges, buttons) overrides the client's cursor while hovered; `None` clears it.
    pub fn set_cursor_override(&mut self, kind: Option<CursorKind>) {
        if self.cursor.override_kind == kind {
            return;
        }
        self.cursor.override_kind = kind;
        self.request_redraw();
    }

    /// Stores the cursor a client asked for (`wl_pointer.set_cursor` or cursor-shape-v1).
    pub fn set_client_cursor(&mut self, status: CursorImageStatus) {
        if self.cursor.status == status {
            return;
        }
        self.cursor.status = status;
        self.request_redraw();
    }

    /// Puts the pointer at `location` without any focus change (headless screenshots).
    pub fn place_pointer(&mut self, location: Point<f64, Logical>) {
        if let Some(pointer) = self.seat.get_pointer() {
            pointer.set_location(location);
        }
        self.cursor.pointer_seen.set(true);
        self.request_redraw();
    }

    /// The cursor to draw for the current frame.
    pub fn cursor_source(&self) -> CursorSource {
        let seen = self.pointer_has_moved();
        resolve(self.cursor.override_kind, &self.cursor.status, seen)
    }

    fn pointer_has_moved(&self) -> bool {
        if self.cursor.pointer_seen.get() {
            return true;
        }
        let moved = self
            .seat
            .get_pointer()
            .is_some_and(|pointer| pointer.current_location() != Point::default());
        self.cursor.pointer_seen.set(moved);
        moved
    }

    /// Redraws once after `delay`; repeated calls while a wake-up is pending do nothing.
    pub fn schedule_cursor_redraw(&self, delay: Duration) {
        if self.cursor.wake_armed.replace(true) {
            return;
        }
        let armed = Rc::clone(&self.cursor.wake_armed);
        let inserted =
            self.cursor
                .handle
                .insert_source(Timer::from_duration(delay), move |_, _, state| {
                    armed.set(false);
                    state.request_redraw();
                    TimeoutAction::Drop
                });
        if let Err(error) = inserted {
            self.cursor.wake_armed.set(false);
            warn!(%error, "cursor animation timer failed");
        }
    }

    /// Lets a client-provided cursor surface present its next frame.
    pub fn send_cursor_frame_callbacks(&self, output: &Output) {
        let CursorImageStatus::Surface(surface) = &self.cursor.status else {
            return;
        };
        if !surface.is_alive() {
            return;
        }
        let elapsed = self.start_time.elapsed();
        send_frames_surface_tree(surface, output, elapsed, Some(Duration::ZERO), |_, _| {
            Some(output.clone())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_map_to_kinds() {
        assert_eq!(kind_for_icon(CursorIcon::Text), CursorKind::Text);
        assert_eq!(kind_for_icon(CursorIcon::Pointer), CursorKind::Pointer);
        assert_eq!(kind_for_icon(CursorIcon::NwseResize), CursorKind::ResizeNw);
        assert_eq!(kind_for_icon(CursorIcon::ColResize), CursorKind::ResizeEw);
        assert_eq!(kind_for_icon(CursorIcon::Default), CursorKind::Default);
    }

    #[test]
    fn unknown_icons_fall_back_to_the_arrow() {
        assert_eq!(kind_for_icon(CursorIcon::ZoomIn), CursorKind::Default);
    }

    #[test]
    fn unseen_pointer_hides_everything() {
        let status = CursorImageStatus::Named(CursorIcon::Text);
        let source = resolve(Some(CursorKind::Move), &status, false);
        assert_eq!(source, CursorSource::Hidden);
    }

    #[test]
    fn chrome_override_beats_the_client() {
        let status = CursorImageStatus::Named(CursorIcon::Text);
        let source = resolve(Some(CursorKind::ResizeE), &status, true);
        assert_eq!(source, CursorSource::Themed(CursorKind::ResizeE));
    }

    #[test]
    fn named_status_resolves_to_its_kind() {
        let status = CursorImageStatus::Named(CursorIcon::Text);
        assert_eq!(
            resolve(None, &status, true),
            CursorSource::Themed(CursorKind::Text)
        );
    }

    #[test]
    fn hidden_status_hides_but_chrome_still_shows() {
        let status = CursorImageStatus::Hidden;
        assert_eq!(resolve(None, &status, true), CursorSource::Hidden);
        assert_eq!(
            resolve(Some(CursorKind::Move), &status, true),
            CursorSource::Themed(CursorKind::Move)
        );
    }
}
