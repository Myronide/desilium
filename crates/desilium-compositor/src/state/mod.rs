mod animation;
mod chrome;
mod handlers;
mod input;
pub(crate) mod layers;
mod settings;
mod shell;
mod windows;

pub use chrome::CursorSource;
pub use handlers::ClientState;
use handlers::ProtocolStates;
pub use windows::{ClosingWindow, SceneWindow, WindowRequest};

use crate::error::CompositorError;
use desilium_config_types::CompositorSettings;
use desilium_theme::Theme;
use desilium_wm::{Desktops, Rect, WindowId};
use input::ShortcutTable;
use smithay::desktop::{PopupManager, Space, Window};
use smithay::input::{Seat, SeatState};
use smithay::reexports::calloop::ping::Ping;
use smithay::reexports::calloop::{LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::wayland::compositor::CompositorState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::wlr_layer::WlrLayerShellState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shm::ShmState;
use std::collections::HashMap;
use std::ffi::OsString;
use std::time::Instant;

const KEY_REPEAT_DELAY_MS: i32 = 200;
const KEY_REPEAT_RATE_HZ: i32 = 25;
const SEAT_NAME: &str = "desilium";

/// Wakes the render loop; the compositor only draws when something changed.
#[derive(Clone)]
pub struct RedrawHandle(Ping);

impl RedrawHandle {
    pub fn new(ping: Ping) -> Self {
        Self(ping)
    }

    fn request(&self) {
        self.0.ping();
    }
}

pub struct DesiliumState {
    pub start_time: Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub loop_signal: LoopSignal,
    pub settings: CompositorSettings,
    theme: Theme,
    decoration: chrome::DecorationState,
    cursor: chrome::CursorState,
    #[allow(dead_code)] // filled in by the settings work
    settings_runtime: settings::SettingsRuntime,
    shortcuts: ShortcutTable,
    /// Virtual desktops; window operations act on the active one (see `Desktops`' `Deref`).
    pub wm: Desktops,
    windows: HashMap<WindowId, Window>,
    pub space: Space<Window>,
    popups: PopupManager,
    pub seat: Seat<Self>,
    redraw: Option<RedrawHandle>,
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    // Held only to keep the xdg-decoration global alive.
    _xdg_decoration_state: XdgDecorationState,
    shm_state: ShmState,
    // Held only to keep the xdg-output globals alive.
    _output_manager_state: OutputManagerState,
    seat_state: SeatState<Self>,
    data_device_state: DataDeviceState,
    protocols: ProtocolStates,
    layer_shell_state: WlrLayerShellState,
    /// Window animation clock and per-window motion state.
    motion: animation::MotionRuntime,
    /// Private protocol for the desktop shell.
    shell: shell::ShellState,
}

impl DesiliumState {
    pub fn new(
        handle: &LoopHandle<'static, Self>,
        signal: LoopSignal,
        display: Display<Self>,
        settings: CompositorSettings,
    ) -> Result<Self, CompositorError> {
        let dh = display.handle();
        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(&dh, SEAT_NAME);
        seat.add_keyboard(Default::default(), KEY_REPEAT_DELAY_MS, KEY_REPEAT_RATE_HZ)
            .map_err(|error| CompositorError::Keyboard(error.to_string()))?;
        seat.add_pointer();

        let socket_name = handlers::init_listener(handle, display)?;
        let start_time = Instant::now();
        let wm = Desktops::new(Rect::new(0, 0, 0, 0), usize::from(settings.desktops));
        Ok(Self {
            start_time,
            socket_name,
            display_handle: dh.clone(),
            loop_signal: signal,
            shortcuts: ShortcutTable::from_settings(&settings),
            settings,
            theme: Theme::dark(),
            decoration: Default::default(),
            cursor: chrome::CursorState::new(handle.clone(), &dh),
            settings_runtime: Default::default(),
            wm,
            windows: HashMap::new(),
            space: Space::default(),
            popups: PopupManager::default(),
            seat,
            redraw: None,
            compositor_state: CompositorState::new::<Self>(&dh),
            xdg_shell_state: XdgShellState::new::<Self>(&dh),
            _xdg_decoration_state: XdgDecorationState::new::<Self>(&dh),
            shm_state: ShmState::new::<Self>(&dh, vec![]),
            _output_manager_state: OutputManagerState::new_with_xdg_output::<Self>(&dh),
            seat_state,
            data_device_state: DataDeviceState::new::<Self>(&dh),
            protocols: ProtocolStates::new(handle, &dh),
            layer_shell_state: WlrLayerShellState::new::<Self>(&dh),
            motion: animation::MotionRuntime::new(start_time),
            shell: shell::ShellState::new(&dh),
        })
    }

    pub fn set_redraw_handle(&mut self, handle: RedrawHandle) {
        self.redraw = Some(handle);
    }

    pub fn request_redraw(&self) {
        if let Some(handle) = &self.redraw {
            handle.request();
        }
    }
}
