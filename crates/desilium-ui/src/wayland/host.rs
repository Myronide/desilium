//! [`LayerHost`]: layer-shell surfaces driven from the application's own event loop.

use std::{
    os::fd::{AsFd, BorrowedFd},
    time::{Duration, Instant},
};

use smithay_client_toolkit::{
    compositor::{CompositorState, Region},
    output::OutputState,
    reexports::client::{
        Connection, EventQueue, QueueHandle,
        globals::registry_queue_init,
        protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    },
    registry::RegistryState,
    seat::{SeatState, pointer::cursor_shape::CursorShapeManager},
    shell::{WaylandSurface, wlr_layer::LayerShell},
    shm::Shm,
};

use super::{
    config::{HostEvent, LayerConfig, SurfaceId},
    state::{Entry, InputState, State},
};
use crate::{UiError, UiSurface};

fn wayland_error(error: impl std::fmt::Display) -> UiError {
    UiError::Wayland(error.to_string())
}

/// Owns the Wayland connection and any number of layer-shell [`UiSurface`]s.
///
/// Single-threaded: create and drive it from one thread. It does not own an
/// event loop; register [`poll_fd`](Self::poll_fd) with yours (calloop, poll,
/// ...) and call [`dispatch`](Self::dispatch) when it is readable or when
/// [`next_timeout`](Self::next_timeout) expires. With nothing to do the host
/// schedules no timers, so an idle shell costs no wakeups.
pub struct LayerHost {
    conn: Connection,
    queue: EventQueue<State>,
    qh: QueueHandle<State>,
    state: State,
}

impl LayerHost {
    /// Connects to `$WAYLAND_DISPLAY`.
    pub fn connect() -> Result<Self, UiError> {
        Self::from_connection(Connection::connect_to_env().map_err(wayland_error)?)
    }

    /// Uses an existing connection (the shell may share one with its protocol client).
    pub fn from_connection(conn: Connection) -> Result<Self, UiError> {
        let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(wayland_error)?;
        let qh = queue.handle();
        let mut state = State {
            registry: RegistryState::new(&globals),
            outputs: OutputState::new(&globals, &qh),
            seats: SeatState::new(&globals, &qh),
            compositor: CompositorState::bind(&globals, &qh).map_err(wayland_error)?,
            layer_shell: LayerShell::bind(&globals, &qh).map_err(wayland_error)?,
            shm: Shm::bind(&globals, &qh).map_err(wayland_error)?,
            viewporter: globals.bind(&qh, 1..=1, ()).ok(),
            fractional_manager: globals.bind(&qh, 1..=1, ()).ok(),
            cursor_shapes: CursorShapeManager::bind(&globals, &qh).ok(),
            entries: Vec::new(),
            next_id: 0,
            events: Vec::new(),
            input: InputState::default(),
            now: Instant::now(),
            frames: 0,
        };
        // Two round trips: the first delivers globals, the second their initial state
        // (outputs with names, seat capabilities).
        queue.roundtrip(&mut state).map_err(wayland_error)?;
        queue.roundtrip(&mut state).map_err(wayland_error)?;
        Ok(Self {
            conn,
            queue,
            qh,
            state,
        })
    }

    /// Names of the outputs the compositor advertises.
    pub fn output_names(&self) -> Vec<String> {
        self.state
            .outputs
            .outputs()
            .filter_map(|output| self.state.outputs.info(&output)?.name)
            .collect()
    }

    fn find_output(&self, name: &str) -> Option<WlOutput> {
        self.state.outputs.outputs().find(|output| {
            self.state
                .outputs
                .info(output)
                .and_then(|i| i.name)
                .as_deref()
                == Some(name)
        })
    }

    /// Creates a layer surface showing `ui`.
    ///
    /// When an axis of `config.size` is 0 the compositor decides that dimension
    /// at configure time and `ui` is resized to it; the size `ui` was created
    /// with is only a placeholder until then. Nothing is drawn before the first
    /// configure, which arrives during a later [`dispatch`](Self::dispatch).
    pub fn add_surface(
        &mut self,
        config: LayerConfig,
        mut ui: UiSurface,
    ) -> Result<SurfaceId, UiError> {
        let (width, height) = config.size;
        if width > 0 && height > 0 {
            ui.resize(width, height)?;
        }
        let output = match config.output.as_deref() {
            Some(name) => {
                let found = self.find_output(name);
                if found.is_none() {
                    tracing::warn!(
                        output = name,
                        "output not found, letting the compositor choose"
                    );
                }
                found
            }
            None => None,
        };
        let surface = self.state.compositor.create_surface(&self.qh);
        let layer = self.state.layer_shell.create_layer_surface(
            &self.qh,
            surface,
            config.layer.to_sctk(),
            Some(config.namespace.clone()),
            output.as_ref(),
        );
        apply_layer_config(&layer, &config);
        layer.commit();

        let id = SurfaceId(self.state.next_id);
        self.state.next_id += 1;
        let mut entry = Entry::new(id, layer, ui, config, &self.state.shm)?;
        let wl_surface = entry.layer.wl_surface().clone();
        entry.viewport = self
            .state
            .viewporter
            .as_ref()
            .map(|v| v.get_viewport(&wl_surface, &self.qh, ()));
        entry.fractional = self
            .state
            .fractional_manager
            .as_ref()
            .map(|m| m.get_fractional_scale(&wl_surface, &self.qh, wl_surface.clone()));
        self.state.entries.push(entry);
        self.conn.flush().map_err(wayland_error)?;
        Ok(id)
    }

    /// The Wayland connection, for protocol objects the application dispatches itself
    /// (create them on an own `EventQueue`; events are routed by the shared socket).
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// The `wl_surface` behind a layer surface, for protocols that take one as argument.
    pub fn wl_surface(&self, id: SurfaceId) -> Option<WlSurface> {
        let entry = self.state.entries.iter().find(|entry| entry.id == id)?;
        Some(entry.layer.wl_surface().clone())
    }

    /// Whether the compositor has configured the surface (it is mapped and can be drawn).
    pub fn is_configured(&self, id: SurfaceId) -> bool {
        self.state
            .entries
            .iter()
            .any(|entry| entry.id == id && entry.configured)
    }

    /// Logical size of the first output the compositor advertises, if it reported one.
    pub fn output_logical_size(&self) -> Option<(i32, i32)> {
        self.state
            .outputs
            .outputs()
            .find_map(|output| self.state.outputs.info(&output)?.logical_size)
    }

    /// Restricts where the surface receives pointer input.
    ///
    /// `Some(rect)` (logical pixels) accepts input only inside `rect`; `None` makes
    /// the whole surface transparent to the pointer (tooltips, decorations).
    /// Call [`clear_input_region`](Self::clear_input_region) to restore the default.
    pub fn set_input_region(&mut self, id: SurfaceId, rect: Option<crate::Rect>) {
        let Some(entry) = self.state.entries.iter().find(|entry| entry.id == id) else {
            return;
        };
        let region = Region::new(&self.state.compositor).ok();
        let Some(region) = region else {
            tracing::warn!("cannot create an input region");
            return;
        };
        if let Some(rect) = rect {
            region.add(
                rect.x.round() as i32,
                rect.y.round() as i32,
                rect.width.round() as i32,
                rect.height.round() as i32,
            );
        }
        entry
            .layer
            .wl_surface()
            .set_input_region(Some(region.wl_region()));
        entry.layer.commit();
        let _ = self.conn.flush();
    }

    /// Accepts pointer input on the whole surface again.
    pub fn clear_input_region(&mut self, id: SurfaceId) {
        let Some(entry) = self.state.entries.iter().find(|entry| entry.id == id) else {
            return;
        };
        entry.layer.wl_surface().set_input_region(None);
        entry.layer.commit();
        let _ = self.conn.flush();
    }

    /// Destroys a surface.
    pub fn remove_surface(&mut self, id: SurfaceId) {
        self.state.entries.retain(|entry| entry.id != id);
        if self.state.input.pointer_focus == Some(id) {
            self.state.input.pointer_focus = None;
        }
        if self.state.input.keyboard_focus == Some(id) {
            self.state.input.keyboard_focus = None;
        }
        // Best effort: a dead connection is reported by the next dispatch.
        let _ = self.conn.flush();
    }

    /// Destroys the layer surface but returns its placement and document.
    ///
    /// For surfaces that are hidden most of the time: the document stays in memory and
    /// [`add_surface`](Self::add_surface) maps it again with a fresh layer surface. This
    /// follows the layer-shell protocol exactly (an unmapped surface would have to wait
    /// for a new configure, which not every compositor sends).
    pub fn take_surface(&mut self, id: SurfaceId) -> Option<(LayerConfig, UiSurface)> {
        let position = self.state.entries.iter().position(|entry| entry.id == id)?;
        let entry = self.state.entries.remove(position);
        if self.state.input.pointer_focus == Some(id) {
            self.state.input.pointer_focus = None;
        }
        if self.state.input.keyboard_focus == Some(id) {
            self.state.input.keyboard_focus = None;
        }
        // Best effort: a dead connection is reported by the next dispatch.
        let _ = self.conn.flush();
        Some((entry.config, entry.ui))
    }

    /// Read access to a surface's document.
    pub fn surface(&self, id: SurfaceId) -> Option<&UiSurface> {
        self.state
            .entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| &e.ui)
    }

    /// Mutable access to a document. Call [`render_pending`](Self::render_pending)
    /// afterwards (or use [`update`](Self::update), which does it for you).
    pub fn surface_mut(&mut self, id: SurfaceId) -> Option<&mut UiSurface> {
        self.state.entry_mut(id).map(|e| &mut e.ui)
    }

    /// Runs `change` on the document, then draws it if the change made it dirty.
    pub fn update<R>(
        &mut self,
        id: SurfaceId,
        change: impl FnOnce(&mut UiSurface) -> R,
    ) -> Option<R> {
        let result = change(self.surface_mut(id)?);
        if let Err(error) = self.render_pending() {
            tracing::warn!(%error, "flush after update failed");
        }
        Some(result)
    }

    /// Changes placement parameters (margins, anchor, size, layer, keyboard mode) of a live surface.
    pub fn reconfigure(
        &mut self,
        id: SurfaceId,
        change: impl FnOnce(&mut LayerConfig),
    ) -> Result<(), UiError> {
        let Some(entry) = self.state.entry_mut(id) else {
            return Ok(());
        };
        change(&mut entry.config);
        apply_layer_config(&entry.layer, &entry.config);
        entry.layer.commit();
        self.conn.flush().map_err(wayland_error)
    }

    /// Unmaps (`false`) or maps (`true`) a surface without destroying its document.
    ///
    /// Mapping again waits for a new configure and redraws everything. Not every compositor
    /// sends one after an unmap (Smithay does not): for overlays prefer
    /// [`take_surface`](Self::take_surface) and [`add_surface`](Self::add_surface).
    pub fn set_visible(&mut self, id: SurfaceId, visible: bool) -> Result<(), UiError> {
        let Some(entry) = self.state.entry_mut(id) else {
            return Ok(());
        };
        if entry.visible == visible {
            return Ok(());
        }
        entry.visible = visible;
        entry.configured = false;
        entry.frame_pending = false;
        if !visible {
            entry.layer.wl_surface().attach(None, 0, 0);
        }
        entry.layer.commit();
        self.conn.flush().map_err(wayland_error)
    }

    /// File descriptor to poll for readability; call [`dispatch`](Self::dispatch) when it fires.
    pub fn poll_fd(&self) -> BorrowedFd<'_> {
        self.conn.as_fd()
    }

    /// Time until [`dispatch`](Self::dispatch) must run even without socket data
    /// (currently only key repeat). `None` means: sleep until the fd is readable.
    pub fn next_timeout(&self, now: Instant) -> Option<Duration> {
        self.state.input.repeat.next_timeout(now)
    }

    /// Processes incoming events, key repeats and frame callbacks, then draws what is dirty.
    ///
    /// `now` is passed in (instead of read internally) so the repeat timing is
    /// testable and driven by the caller's clock.
    pub fn dispatch(&mut self, now: Instant) -> Result<(), UiError> {
        self.state.now = now;
        self.conn.flush().map_err(wayland_error)?;
        self.read_socket()?;
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(wayland_error)?;
        for (id, key) in self.state.input.repeat.due(now) {
            self.state.route_key(id, &key);
        }
        self.state.sync_cursor();
        self.state.collect_ui_events();
        self.state.draw_ready(&self.qh);
        self.conn.flush().map_err(wayland_error)
    }

    fn read_socket(&mut self) -> Result<(), UiError> {
        use smithay_client_toolkit::reexports::client::backend::WaylandError;
        let Some(guard) = self.queue.prepare_read() else {
            return Ok(());
        };
        match guard.read() {
            Ok(_) => Ok(()),
            Err(WaylandError::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                Ok(())
            }
            Err(error) => Err(wayland_error(error)),
        }
    }

    /// Draws every dirty surface that is ready for a frame. Cheap when nothing changed.
    pub fn render_pending(&mut self) -> Result<(), UiError> {
        self.state.collect_ui_events();
        self.state.draw_ready(&self.qh);
        self.conn.flush().map_err(wayland_error)
    }

    /// Drains events for the application (actions, text changes, hover, closed surfaces).
    pub fn take_events(&mut self) -> Vec<HostEvent> {
        std::mem::take(&mut self.state.events)
    }

    /// Number of buffers committed so far (for diagnostics and idle-cost checks).
    pub fn frame_count(&self) -> u64 {
        self.state.frames
    }

    /// True if any surface still waits for its first configure or a frame callback.
    pub fn is_busy(&self) -> bool {
        self.state
            .entries
            .iter()
            .any(|e| !e.configured || e.frame_pending)
    }
}

fn apply_layer_config(
    layer: &smithay_client_toolkit::shell::wlr_layer::LayerSurface,
    config: &LayerConfig,
) {
    layer.set_layer(config.layer.to_sctk());
    layer.set_anchor(config.anchor.to_sctk());
    layer.set_size(config.size.0, config.size.1);
    layer.set_exclusive_zone(config.exclusive_zone);
    let m = config.margins;
    layer.set_margin(m.top, m.right, m.bottom, m.left);
    layer.set_keyboard_interactivity(config.keyboard.to_sctk());
}
