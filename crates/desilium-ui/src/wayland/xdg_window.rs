//! Toplevel (xdg-shell) window host: puts [`UiSurface`](crate::UiSurface)s into normal
//! application windows.
//!
//! Same model as [`LayerHost`](super::LayerHost): single-threaded, no event loop of
//! its own. Register [`XdgWindowHost::poll_fd`] in your loop and call
//! [`XdgWindowHost::dispatch`] when it is readable or [`XdgWindowHost::next_timeout`]
//! expires; then drain [`XdgWindowHost::take_events`].
//!
//! ```ignore
//! use std::time::Instant;
//! use desilium_ui::{wayland::*, *};
//!
//! let mut host = XdgWindowHost::connect()?;
//! let ui = UiSurface::new(HTML, &[CSS], SurfaceOptions::new(1100, 720))?;
//! let mut config = WindowConfig::new("Settings", "org.desilium.Settings", (1100, 720));
//! config.min_size = Some((720, 520));
//! let window = host.add_window(config, ui)?;
//! loop {
//!     host.dispatch(Instant::now())?;
//!     for event in host.take_events() {
//!         if let WindowEvent::CloseRequested(_) = event { return Ok(()); }
//!     }
//! }
//! ```
//!
//! The title bar is requested from the compositor through `xdg-decoration`
//! (server side); [`XdgWindowHost::decorations`] tells what was negotiated. The
//! host never draws chrome itself. Rendering is demand driven (dirty document,
//! configured window, previous frame callback fired). Fractional scale and the
//! cursor shape work as in the layer host. Not supported yet: text-input (IME)
//! and clipboard.

mod handlers;
mod state;

use std::{
    os::fd::{AsFd, BorrowedFd},
    time::{Duration, Instant},
};

use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    reexports::client::{Connection, EventQueue, QueueHandle, globals::registry_queue_init},
    registry::RegistryState,
    seat::{SeatState, pointer::cursor_shape::CursorShapeManager},
    shell::{
        WaylandSurface,
        xdg::{XdgShell, window::WindowDecorations},
    },
    shm::Shm,
};

use super::config::SurfaceId;
use crate::{HoverTarget, KeyEvent, UiError, UiEvent, UiSurface};
use state::{Entry, InputState, State};

pub use state::Decorations;

/// Handle of a window managed by an [`XdgWindowHost`].
pub type WindowId = SurfaceId;

fn wayland_error(error: impl std::fmt::Display) -> UiError {
    UiError::Wayland(error.to_string())
}

/// Identity and size constraints of a toplevel window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowConfig {
    /// Window title shown in the title bar and task lists.
    pub title: String,
    /// Application id (matches the `.desktop` file), e.g. `org.desilium.Settings`.
    pub app_id: String,
    /// Initial logical size; the compositor may choose another one.
    pub size: (u32, u32),
    /// Smallest logical size the layout supports.
    pub min_size: Option<(u32, u32)>,
}

impl WindowConfig {
    /// A resizable window with no size limits.
    pub fn new(title: &str, app_id: &str, size: (u32, u32)) -> Self {
        Self {
            title: title.to_owned(),
            app_id: app_id.to_owned(),
            size,
            min_size: None,
        }
    }
}

/// Something the host wants the application to know.
#[derive(Debug, Clone, PartialEq)]
pub enum WindowEvent {
    /// A document event (click action, text change, submit).
    Ui {
        /// Window it happened in.
        window: WindowId,
        /// The event.
        event: UiEvent,
    },
    /// The hovered tooltip/action element changed (`None` = left it).
    Hover {
        /// Window it happened in.
        window: WindowId,
        /// New hover target.
        target: Option<HoverTarget>,
    },
    /// The compositor or the title bar asked to close; the window stays until
    /// [`XdgWindowHost::remove_window`] is called.
    CloseRequested(WindowId),
    /// A key was pressed (not repeated) while the window had keyboard focus.
    ///
    /// Sent in addition to normal delivery to the document, unless
    /// [`XdgWindowHost::set_key_capture`] is on, in which case it is the only delivery.
    Key {
        /// The focused window.
        window: WindowId,
        /// The key with its modifiers.
        event: KeyEvent,
    },
    /// The compositor resized the window (the document was already resized).
    Resized {
        /// The window.
        window: WindowId,
        /// New logical size.
        size: (u32, u32),
    },
}

/// Owns the Wayland connection and any number of toplevel [`UiSurface`] windows.
pub struct XdgWindowHost {
    conn: Connection,
    queue: EventQueue<State>,
    qh: QueueHandle<State>,
    state: State,
}

impl XdgWindowHost {
    /// Connects to `$WAYLAND_DISPLAY`.
    pub fn connect() -> Result<Self, UiError> {
        Self::from_connection(Connection::connect_to_env().map_err(wayland_error)?)
    }

    /// Uses an existing connection.
    pub fn from_connection(conn: Connection) -> Result<Self, UiError> {
        let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(wayland_error)?;
        let qh = queue.handle();
        let mut state = State {
            registry: RegistryState::new(&globals),
            outputs: OutputState::new(&globals, &qh),
            seats: SeatState::new(&globals, &qh),
            compositor: CompositorState::bind(&globals, &qh).map_err(wayland_error)?,
            xdg_shell: XdgShell::bind(&globals, &qh).map_err(wayland_error)?,
            shm: Shm::bind(&globals, &qh).map_err(wayland_error)?,
            viewporter: globals.bind(&qh, 1..=1, ()).ok(),
            fractional_manager: globals.bind(&qh, 1..=1, ()).ok(),
            cursor_shapes: CursorShapeManager::bind(&globals, &qh).ok(),
            entries: Vec::new(),
            next_id: 0,
            events: Vec::new(),
            input: InputState::default(),
            now: Instant::now(),
            capture_keys: false,
            frames: 0,
        };
        // Two round trips: globals first, then their initial state (seat capabilities).
        queue.roundtrip(&mut state).map_err(wayland_error)?;
        queue.roundtrip(&mut state).map_err(wayland_error)?;
        Ok(Self {
            conn,
            queue,
            qh,
            state,
        })
    }

    /// Creates a window showing `ui`, asking for server-side decorations.
    ///
    /// Nothing is drawn before the first configure, which arrives during a later
    /// [`dispatch`](Self::dispatch).
    pub fn add_window(
        &mut self,
        config: WindowConfig,
        mut ui: UiSurface,
    ) -> Result<WindowId, UiError> {
        ui.resize(config.size.0, config.size.1)?;
        let surface = self.state.compositor.create_surface(&self.qh);
        let window =
            self.state
                .xdg_shell
                .create_window(surface, WindowDecorations::RequestServer, &self.qh);
        window.set_title(config.title.clone());
        window.set_app_id(config.app_id.clone());
        window.set_min_size(config.min_size);
        window.commit();

        let id = SurfaceId(self.state.next_id);
        self.state.next_id += 1;
        let mut entry = Entry::new(id, window, ui, &config, &self.state.shm)?;
        let wl_surface = entry.window.wl_surface().clone();
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

    /// Destroys a window.
    pub fn remove_window(&mut self, id: WindowId) {
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

    /// Changes the window title.
    pub fn set_title(&mut self, id: WindowId, title: &str) -> Result<(), UiError> {
        let Some(entry) = self.state.entry_mut(id) else {
            return Ok(());
        };
        entry.window.set_title(title);
        self.conn.flush().map_err(wayland_error)
    }

    /// Routes key presses to the application only (`true`) or also to the document (`false`).
    ///
    /// Used while the user records a shortcut: the pressed keys must not type into fields.
    pub fn set_key_capture(&mut self, capture: bool) {
        self.state.capture_keys = capture;
        if capture {
            self.state.input.repeat.release(None);
        }
    }

    /// Who draws the title bar of a window, once it has been configured.
    pub fn decorations(&self, id: WindowId) -> Option<Decorations> {
        let entry = self.state.entries.iter().find(|e| e.id == id)?;
        entry.configured.then_some(entry.decorations)
    }

    /// Read access to a window's document.
    pub fn surface(&self, id: WindowId) -> Option<&UiSurface> {
        self.state
            .entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| &e.ui)
    }

    /// Mutable access to a document. Call [`render_pending`](Self::render_pending)
    /// afterwards (or use [`update`](Self::update), which does it for you).
    pub fn surface_mut(&mut self, id: WindowId) -> Option<&mut UiSurface> {
        self.state.entry_mut(id).map(|e| &mut e.ui)
    }

    /// Runs `change` on the document, then draws it if the change made it dirty.
    pub fn update<R>(
        &mut self,
        id: WindowId,
        change: impl FnOnce(&mut UiSurface) -> R,
    ) -> Option<R> {
        let result = change(self.surface_mut(id)?);
        if let Err(error) = self.render_pending() {
            tracing::warn!(%error, "flush after update failed");
        }
        Some(result)
    }

    /// File descriptor to poll for readability; call [`dispatch`](Self::dispatch) when it fires.
    pub fn poll_fd(&self) -> BorrowedFd<'_> {
        self.conn.as_fd()
    }

    /// Time until [`dispatch`](Self::dispatch) must run even without socket data
    /// (key repeat). `None` means: sleep until the fd is readable.
    pub fn next_timeout(&self, now: Instant) -> Option<Duration> {
        self.state.input.repeat.next_timeout(now)
    }

    /// Processes incoming events, key repeats and frame callbacks, then draws what is dirty.
    pub fn dispatch(&mut self, now: Instant) -> Result<(), UiError> {
        self.state.now = now;
        self.conn.flush().map_err(wayland_error)?;
        self.read_socket()?;
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(wayland_error)?;
        for (id, key) in self.state.input.repeat.due(now) {
            if let Some(entry) = self.state.entry_mut(id) {
                entry.ui.key_down(&key);
            }
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

    /// Draws every dirty window that is ready for a frame. Cheap when nothing changed.
    pub fn render_pending(&mut self) -> Result<(), UiError> {
        self.state.collect_ui_events();
        self.state.draw_ready(&self.qh);
        self.conn.flush().map_err(wayland_error)
    }

    /// Drains events for the application.
    pub fn take_events(&mut self) -> Vec<WindowEvent> {
        std::mem::take(&mut self.state.events)
    }

    /// Number of buffers committed so far (diagnostics and idle-cost checks).
    pub fn frame_count(&self) -> u64 {
        self.state.frames
    }

    /// True if any window still waits for its first configure or a frame callback.
    pub fn is_busy(&self) -> bool {
        self.state
            .entries
            .iter()
            .any(|e| !e.configured || e.frame_pending)
    }
}
