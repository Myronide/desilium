//! Test client for the layer-shell work: a 32 px top bar that reserves its space and a
//! floating 64 px dock. Both are solid, so screenshots show where the compositor put them.
//!
//! With `--maximized` it also opens a plain xdg window and asks for maximized, which shows
//! whether maximize respects the bar's exclusive zone. Exits when a layer surface is closed.

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::client::{
        Connection, QueueHandle,
        globals::registry_queue_init,
        protocol::{wl_output, wl_shm, wl_surface},
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        xdg::{
            XdgShell,
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use std::env;
use std::process::ExitCode;

/// Accent colour for the bar, ARGB.
const BAR_COLOR: u32 = 0xFF3B_82F6;
/// Dock colour, ARGB.
const DOCK_COLOR: u32 = 0xFF1F_2937;
/// Light grey for the plain window, so it reads differently from the bar.
const WINDOW_COLOR: u32 = 0xFFE5_E7EB;
const BAR_HEIGHT: u32 = 32;
const DOCK_WIDTH: u32 = 400;
const DOCK_HEIGHT: u32 = 64;
const DOCK_BOTTOM_MARGIN: i32 = 16;
const BYTES_PER_PIXEL: usize = 4;
/// Used when the compositor sends a zero size (it lets the client pick).
const FALLBACK_WIDTH: u32 = 256;
const FALLBACK_HEIGHT: u32 = 256;
const MAXIMIZED_FLAG: &str = "--maximized";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("layer-client: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|error| error.to_string())?;
    let (globals, mut queue) = registry_queue_init(&conn).map_err(|error| error.to_string())?;
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).map_err(|error| error.to_string())?;
    let layer_shell = LayerShell::bind(&globals, &qh).map_err(|error| error.to_string())?;
    let shm = Shm::bind(&globals, &qh).map_err(|error| error.to_string())?;

    let bar = create_panel(&compositor, &layer_shell, &shm, &qh, Panel::bar())?;
    let dock = create_panel(&compositor, &layer_shell, &shm, &qh, Panel::dock())?;
    let window = if env::args().any(|arg| arg == MAXIMIZED_FLAG) {
        let xdg_shell = XdgShell::bind(&globals, &qh).map_err(|error| error.to_string())?;
        Some(create_maximized_window(&compositor, &xdg_shell, &shm, &qh)?)
    } else {
        None
    };

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        panels: vec![bar, dock],
        window,
        exit: false,
    };
    while !app.exit {
        queue
            .blocking_dispatch(&mut app)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Describes one layer surface this client creates.
struct Panel {
    namespace: &'static str,
    layer: Layer,
    anchor: Anchor,
    size: (u32, u32),
    margin_bottom: i32,
    exclusive_zone: i32,
    color: u32,
}

impl Panel {
    fn bar() -> Self {
        Self {
            namespace: "layer-client-bar",
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            size: (0, BAR_HEIGHT),
            margin_bottom: 0,
            exclusive_zone: BAR_HEIGHT as i32,
            color: BAR_COLOR,
        }
    }

    fn dock() -> Self {
        Self {
            namespace: "layer-client-dock",
            layer: Layer::Top,
            anchor: Anchor::BOTTOM,
            size: (DOCK_WIDTH, DOCK_HEIGHT),
            margin_bottom: DOCK_BOTTOM_MARGIN,
            exclusive_zone: 0,
            color: DOCK_COLOR,
        }
    }
}

/// A committed layer surface together with the memory its pixels are drawn into.
struct PanelSurface {
    layer: LayerSurface,
    pool: SlotPool,
    color: u32,
    width: u32,
    height: u32,
}

fn create_panel(
    compositor: &CompositorState,
    layer_shell: &LayerShell,
    shm: &Shm,
    qh: &QueueHandle<App>,
    panel: Panel,
) -> Result<PanelSurface, String> {
    let surface = compositor.create_surface(qh);
    let layer =
        layer_shell.create_layer_surface(qh, surface, panel.layer, Some(panel.namespace), None);
    layer.set_anchor(panel.anchor);
    layer.set_size(panel.size.0, panel.size.1);
    layer.set_exclusive_zone(panel.exclusive_zone);
    layer.set_margin(0, 0, panel.margin_bottom, 0);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    // The first commit without a buffer asks the compositor for the initial configure.
    layer.commit();
    let pool = SlotPool::new(
        FALLBACK_WIDTH as usize * FALLBACK_HEIGHT as usize * BYTES_PER_PIXEL,
        shm,
    )
    .map_err(|error| error.to_string())?;
    Ok(PanelSurface {
        layer,
        pool,
        color: panel.color,
        width: panel.size.0,
        height: panel.size.1,
    })
}

/// A plain xdg toplevel that asks to be maximized.
struct MaximizedWindow {
    window: Window,
    pool: SlotPool,
    width: u32,
    height: u32,
}

fn create_maximized_window(
    compositor: &CompositorState,
    xdg_shell: &XdgShell,
    shm: &Shm,
    qh: &QueueHandle<App>,
) -> Result<MaximizedWindow, String> {
    let surface = compositor.create_surface(qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, qh);
    window.set_title("layer-client window");
    window.set_app_id("layer-client");
    window.set_maximized();
    // The initial commit without a buffer triggers the configure that carries the size.
    window.commit();
    let pool = SlotPool::new(
        FALLBACK_WIDTH as usize * FALLBACK_HEIGHT as usize * BYTES_PER_PIXEL,
        shm,
    )
    .map_err(|error| error.to_string())?;
    Ok(MaximizedWindow {
        window,
        pool,
        width: FALLBACK_WIDTH,
        height: FALLBACK_HEIGHT,
    })
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    panels: Vec<PanelSurface>,
    window: Option<MaximizedWindow>,
    exit: bool,
}

impl PanelSurface {
    /// Fills the surface with its colour and commits the buffer.
    fn draw(&mut self, qh: &QueueHandle<App>) -> Result<(), String> {
        paint(
            self.layer.wl_surface(),
            &mut self.pool,
            (self.width, self.height),
            self.color,
            qh,
        )?;
        self.layer.commit();
        Ok(())
    }
}

/// Fills a buffer of `size` with one colour and attaches it to `surface`; the caller commits.
fn paint(
    surface: &wl_surface::WlSurface,
    pool: &mut SlotPool,
    size: (u32, u32),
    color: u32,
    qh: &QueueHandle<App>,
) -> Result<(), String> {
    let (width, height) = size;
    let stride = width as i32 * BYTES_PER_PIXEL as i32;
    let (buffer, canvas) = pool
        .create_buffer(
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Argb8888,
        )
        .map_err(|error| error.to_string())?;
    let (pixels, _) = canvas.as_chunks_mut::<BYTES_PER_PIXEL>();
    pixels.fill(color.to_le_bytes());
    surface.damage_buffer(0, 0, width as i32, height as i32);
    surface.frame(qh, surface.clone());
    buffer.attach_to(surface).map_err(|error| error.to_string())
}

impl App {
    fn panel_for(&mut self, surface: &wl_surface::WlSurface) -> Option<&mut PanelSurface> {
        self.panels
            .iter_mut()
            .find(|panel| panel.layer.wl_surface() == surface)
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let surface = layer.wl_surface().clone();
        let Some(panel) = self.panel_for(&surface) else {
            return;
        };
        let (width, height) = configure.new_size;
        if width > 0 {
            panel.width = width;
        }
        if height > 0 {
            panel.height = height;
        }
        if let Err(message) = panel.draw(qh) {
            eprintln!("layer-client: drawing failed: {message}");
            self.exit = true;
        }
    }
}

impl WindowHandler for App {
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let Some(app_window) = self.window.as_mut() else {
            return;
        };
        if app_window.window.wl_surface() != window.wl_surface() {
            return;
        }
        let (width, height) = configure.new_size;
        app_window.width = width.map_or(FALLBACK_WIDTH, |value| value.get());
        app_window.height = height.map_or(FALLBACK_HEIGHT, |value| value.get());
        let painted = paint(
            app_window.window.wl_surface(),
            &mut app_window.pool,
            (app_window.width, app_window.height),
            WINDOW_COLOR,
            qh,
        );
        if let Err(message) = painted {
            eprintln!("layer-client: drawing the window failed: {message}");
            self.exit = true;
            return;
        }
        app_window.window.commit();
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_layer!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
delegate_registry!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}
