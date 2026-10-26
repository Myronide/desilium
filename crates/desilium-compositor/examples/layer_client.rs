//! Test client for the layer-shell work: a 32 px top bar that reserves its space and a
//! floating 64 px dock. Both are solid, so screenshots show where the compositor put them.
//!
//! With `--maximized` it also opens a plain xdg window and asks for maximized, which shows
//! whether maximize respects the bar's exclusive zone. Exits when a layer surface is closed.
//!
//! Visual-proof flags for the glass work (see `docs/headless.md`):
//!
//! * `--glass` makes bar and dock translucent and paints the window as a detailed checker, so a
//!   backdrop blur behind the panels is easy to see.
//! * `--popup` opens a plain window (no maximize needed) with an xdg popup on it: an opaque
//!   dark menu, or with `--popup-glass` a half transparent one.

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    delegate_xdg_popup, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::client::{
        Connection, QueueHandle,
        globals::registry_queue_init,
        protocol::{wl_output, wl_shm, wl_surface},
    },
    reexports::protocols::xdg::shell::client::xdg_positioner::Gravity,
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        xdg::{
            XdgPositioner, XdgShell, XdgSurface,
            popup::{ConfigureKind, Popup, PopupConfigure, PopupHandler},
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
/// Translucent bar and dock for `--glass` (premultiplied ARGB: about 50 % and 60 % alpha).
const GLASS_BAR_COLOR: u32 = 0x801E_417B;
const GLASS_DOCK_COLOR: u32 = 0x9913_1921;
const GLASS_DOCK_CORNER: f32 = 16.0;
/// The two checker colours of the window in `--glass` mode.
const CHECKER_A: u32 = 0xFFE5_E7EB;
const CHECKER_B: u32 = 0xFF6D_28D9;
const CHECKER_CELL: u32 = 28;
/// Opaque and half transparent (premultiplied) popup fills.
const POPUP_OPAQUE: u32 = 0xFF1B_1E2A;
const POPUP_GLASS: u32 = 0xB814_1722;
const POPUP_SIZE: (u32, u32) = (240, 180);
const POPUP_ANCHOR: (i32, i32) = (120, 90);
/// Light grey for the plain window, so it reads differently from the bar.
const WINDOW_COLOR: u32 = 0xFFE5_E7EB;
const BAR_HEIGHT: u32 = 32;
const DOCK_WIDTH: u32 = 400;
const DOCK_HEIGHT: u32 = 64;
const DOCK_BOTTOM_MARGIN: i32 = 16;
const BYTES_PER_PIXEL: usize = 4;
/// Used when the compositor sends a zero size (it lets the client pick).
const FALLBACK_WIDTH: u32 = 640;
const FALLBACK_HEIGHT: u32 = 420;
const MAXIMIZED_FLAG: &str = "--maximized";
const GLASS_FLAG: &str = "--glass";
const POPUP_FLAG: &str = "--popup";
const POPUP_GLASS_FLAG: &str = "--popup-glass";

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

    let has_flag = |flag: &str| env::args().any(|arg| arg == flag);
    let glass = has_flag(GLASS_FLAG);
    let popup_glass = has_flag(POPUP_GLASS_FLAG);
    let wants_popup = has_flag(POPUP_FLAG) || popup_glass;
    let bar = create_panel(&compositor, &layer_shell, &shm, &qh, Panel::bar(glass))?;
    let dock = create_panel(&compositor, &layer_shell, &shm, &qh, Panel::dock(glass))?;
    let maximize = has_flag(MAXIMIZED_FLAG);
    let xdg_shell = if maximize || wants_popup {
        Some(XdgShell::bind(&globals, &qh).map_err(|error| error.to_string())?)
    } else {
        None
    };
    let window = match xdg_shell.as_ref() {
        Some(xdg_shell) => Some(create_window(&compositor, xdg_shell, &shm, &qh, maximize)?),
        None => None,
    };

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        panels: vec![bar, dock],
        window,
        xdg_shell,
        compositor,
        glass,
        popup: wants_popup.then_some(if popup_glass {
            POPUP_GLASS
        } else {
            POPUP_OPAQUE
        }),
        popup_surface: None,
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
    /// Corner radius of the painted shape in px (0 = square); lets `--glass` show rounded glass.
    corner: f32,
}

impl Panel {
    fn bar(glass: bool) -> Self {
        Self {
            namespace: "layer-client-bar",
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            size: (0, BAR_HEIGHT),
            margin_bottom: 0,
            exclusive_zone: BAR_HEIGHT as i32,
            color: if glass { GLASS_BAR_COLOR } else { BAR_COLOR },
            corner: 0.0,
        }
    }

    fn dock(glass: bool) -> Self {
        Self {
            namespace: "layer-client-dock",
            layer: Layer::Top,
            anchor: Anchor::BOTTOM,
            size: (DOCK_WIDTH, DOCK_HEIGHT),
            margin_bottom: DOCK_BOTTOM_MARGIN,
            exclusive_zone: 0,
            color: if glass { GLASS_DOCK_COLOR } else { DOCK_COLOR },
            corner: if glass { GLASS_DOCK_CORNER } else { 0.0 },
        }
    }
}

/// A committed layer surface together with the memory its pixels are drawn into.
struct PanelSurface {
    layer: LayerSurface,
    pool: SlotPool,
    color: u32,
    corner: f32,
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
        corner: panel.corner,
        width: panel.size.0,
        height: panel.size.1,
    })
}

/// A plain xdg toplevel, optionally asking to be maximized.
struct MaximizedWindow {
    window: Window,
    pool: SlotPool,
    width: u32,
    height: u32,
}

fn create_window(
    compositor: &CompositorState,
    xdg_shell: &XdgShell,
    shm: &Shm,
    qh: &QueueHandle<App>,
    maximize: bool,
) -> Result<MaximizedWindow, String> {
    let surface = compositor.create_surface(qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, qh);
    window.set_title("layer-client window");
    window.set_app_id("layer-client");
    if maximize {
        window.set_maximized();
    }
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
    xdg_shell: Option<XdgShell>,
    compositor: CompositorState,
    glass: bool,
    /// Fill of the popup to open once the window is configured.
    popup: Option<u32>,
    /// The open popup and its memory.
    popup_surface: Option<(Popup, SlotPool, u32)>,
    exit: bool,
}

impl PanelSurface {
    /// Fills the surface with its colour and commits the buffer.
    fn draw(&mut self, qh: &QueueHandle<App>) -> Result<(), String> {
        let (color, corner, size) = (self.color, self.corner, (self.width, self.height));
        paint(
            self.layer.wl_surface(),
            &mut self.pool,
            size,
            |x, y| rounded(color, corner, size, (x, y)),
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
    color_at: impl Fn(u32, u32) -> u32,
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
    for (index, pixel) in pixels.iter_mut().enumerate() {
        let (row, column) = (index as u32 / width, index as u32 % width);
        *pixel = color_at(column, row).to_le_bytes();
    }
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
        let glass = self.glass;
        let painted = paint(
            app_window.window.wl_surface(),
            &mut app_window.pool,
            (app_window.width, app_window.height),
            |x, y| window_color(glass, x, y),
            qh,
        );
        if let Err(message) = painted {
            eprintln!("layer-client: drawing the window failed: {message}");
            self.exit = true;
            return;
        }
        app_window.window.commit();
        if let Err(message) = self.open_popup(qh) {
            eprintln!("layer-client: opening the popup failed: {message}");
            self.exit = true;
        }
    }
}

/// `color` (premultiplied ARGB) inside a rounded rectangle of `size`, anti-aliased at the edge.
fn rounded(color: u32, corner: f32, size: (u32, u32), at: (u32, u32)) -> u32 {
    if corner <= 0.0 {
        return color;
    }
    let (width, height) = (size.0 as f32, size.1 as f32);
    let (px, py) = (at.0 as f32 + 0.5, at.1 as f32 + 0.5);
    let dx = (corner - px).max(px - (width - corner)).max(0.0);
    let dy = (corner - py).max(py - (height - corner)).max(0.0);
    let coverage = (corner - dx.hypot(dy) + 0.5).clamp(0.0, 1.0);
    let scale = |shift: u32| (((color >> shift) & 0xFF) as f32 * coverage).round() as u32;
    (scale(24) << 24) | (scale(16) << 16) | (scale(8) << 8) | scale(0)
}

/// Plain light grey, or a two colour checker in `--glass` mode so blur is visible.
fn window_color(glass: bool, x: u32, y: u32) -> u32 {
    if !glass {
        return WINDOW_COLOR;
    }
    if (x / CHECKER_CELL + y / CHECKER_CELL).is_multiple_of(2) {
        CHECKER_A
    } else {
        CHECKER_B
    }
}

impl App {
    /// Opens the requested popup on the window, once.
    fn open_popup(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
        let (Some(fill), Some(xdg_shell), Some(window)) = (
            self.popup.take(),
            self.xdg_shell.as_ref(),
            self.window.as_ref(),
        ) else {
            return Ok(());
        };
        let positioner = XdgPositioner::new(xdg_shell).map_err(|error| error.to_string())?;
        positioner.set_size(POPUP_SIZE.0 as i32, POPUP_SIZE.1 as i32);
        positioner.set_anchor_rect(POPUP_ANCHOR.0, POPUP_ANCHOR.1, 1, 1);
        positioner.set_gravity(Gravity::BottomRight);
        let surface = self.compositor.create_surface(qh);
        let popup = Popup::from_surface(
            Some(window.window.xdg_surface()),
            &positioner,
            qh,
            surface,
            xdg_shell,
        )
        .map_err(|error| error.to_string())?;
        popup.wl_surface().commit();
        let pool = SlotPool::new(
            POPUP_SIZE.0 as usize * POPUP_SIZE.1 as usize * BYTES_PER_PIXEL,
            &self.shm,
        )
        .map_err(|error| error.to_string())?;
        self.popup_surface = Some((popup, pool, fill));
        Ok(())
    }
}

impl PopupHandler for App {
    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        popup: &Popup,
        configure: PopupConfigure,
    ) {
        let Some((open, pool, fill)) = self.popup_surface.as_mut() else {
            return;
        };
        if open.wl_surface() != popup.wl_surface()
            || matches!(configure.kind, ConfigureKind::Reposition { .. })
        {
            return;
        }
        let size = (
            configure.width.max(1) as u32,
            configure.height.max(1) as u32,
        );
        let color = *fill;
        let painted = paint(popup.wl_surface(), pool, size, |_, _| color, qh);
        match painted {
            Ok(()) => popup.wl_surface().commit(),
            Err(message) => eprintln!("layer-client: drawing the popup failed: {message}"),
        }
    }

    fn done(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _popup: &Popup) {
        self.popup_surface = None;
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
delegate_xdg_popup!(App);
delegate_registry!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}
