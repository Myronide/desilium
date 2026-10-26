mod actions;
mod desktops;
mod scene;

pub use actions::WindowRequest;
pub use scene::{ClosingWindow, SceneWindow};

use crate::state::DesiliumState;
use desilium_wm::{Size, SizeLimits, WindowId};
use smithay::desktop::Window;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::SERIAL_COUNTER;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{SurfaceCachedState, XdgToplevelSurfaceData};
use std::time::Duration;

impl DesiliumState {
    pub fn window_id_for_surface(&self, surface: &WlSurface) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|(_, window)| {
                window
                    .toplevel()
                    .is_some_and(|toplevel| toplevel.wl_surface() == surface)
            })
            .map(|(id, _)| *id)
    }

    pub fn window_id_of(&self, window: &Window) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|(_, candidate)| *candidate == window)
            .map(|(id, _)| *id)
    }

    /// Pushes the window manager's stacking order and positions into the render space.
    pub fn sync_space(&mut self) {
        for placement in self.wm.placements() {
            let Some(window) = self.windows.get(&placement.id) else {
                continue;
            };
            // The space maps the client surface, which sits inside the chrome.
            let client = self.frame_of(placement.id, placement.rect).client;
            self.space
                .map_element(window.clone(), (client.loc.x, client.loc.y), false);
            self.space.raise_element(window, false);
        }
        self.request_redraw();
    }

    /// Mirrors the window manager's focus into window activation and keyboard focus.
    pub fn apply_focus(&mut self) {
        let focused = self.wm.focused();
        for (id, window) in &self.windows {
            window.set_activated(Some(*id) == focused);
            if let Some(toplevel) = window.toplevel() {
                toplevel.send_pending_configure();
            }
        }
        // An exclusive layer surface (launcher, lock screen) keeps the keyboard until it lets go.
        let surface = self.exclusive_layer_surface().or_else(|| {
            focused
                .and_then(|id| self.windows.get(&id))
                .and_then(|window| window.toplevel())
                .map(|toplevel| toplevel.wl_surface().clone())
        });
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        keyboard.set_focus(self, surface, SERIAL_COUNTER.next_serial());
        self.request_redraw();
    }

    pub fn close_focused_window(&self) {
        let Some(id) = self.wm.focused() else {
            return;
        };
        self.request_close_animated(id);
    }

    pub fn size_limits(&self, id: WindowId) -> SizeLimits {
        let Some(toplevel) = self.windows.get(&id).and_then(|window| window.toplevel()) else {
            return SizeLimits::default();
        };
        with_states(toplevel.wl_surface(), |states| {
            let mut guard = states.cached_state.get::<SurfaceCachedState>();
            let current = guard.current();
            SizeLimits {
                min: Size {
                    w: current.min_size.w,
                    h: current.min_size.h,
                },
                max: Size {
                    w: current.max_size.w,
                    h: current.max_size.h,
                },
            }
        })
    }

    /// Called for commits of a window's root surface.
    pub fn handle_window_commit(&mut self, root: &WlSurface) {
        let Some(id) = self.window_id_for_surface(root) else {
            return;
        };
        let Some(window) = self.windows.get(&id).cloned() else {
            return;
        };
        window.on_commit();
        self.send_initial_configure(root, &window);

        let size = window.geometry().size;
        if size.w <= 0 || size.h <= 0 {
            return;
        }
        let frame = self.frame_size_for_client(
            id,
            Size {
                w: size.w,
                h: size.h,
            },
        );
        if self.wm.on_commit(id, frame) {
            self.sync_space();
        }
    }

    fn send_initial_configure(&self, root: &WlSurface, window: &Window) {
        let Some(toplevel) = window.toplevel() else {
            return;
        };
        let sent = with_states(root, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .is_some_and(|data| data.lock().is_ok_and(|data| data.initial_configure_sent))
        });
        if !sent {
            toplevel.send_configure();
        }
    }

    /// Tells clients a frame was presented so they can draw the next one.
    pub fn send_frame_callbacks(&self, output: &smithay::output::Output) {
        let elapsed = self.start_time.elapsed();
        self.send_layer_frame_callbacks(output, elapsed);
        for window in self.space.elements() {
            window.send_frame(output, elapsed, Some(Duration::ZERO), |_, _| {
                Some(output.clone())
            });
        }
    }
}

impl DesiliumState {
    /// Housekeeping after a presented frame.
    pub fn finish_frame(&mut self) {
        self.space.refresh();
        self.popups.cleanup();
        self.advance_animations();
        if let Err(error) = self.display_handle.flush_clients() {
            tracing::warn!(%error, "flushing clients failed");
        }
    }
}
