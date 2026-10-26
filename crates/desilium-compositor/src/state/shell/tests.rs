//! End-to-end test of the shell protocol: a real client talks to a real `DesiliumState`
//! over a socket pair, driven step by step on one thread.

use crate::state::{ClientState, DesiliumState};
use desilium_config_types::CompositorSettings;
use desilium_protocols::client::desilium_shell_manager_v1::{
    self as manager, DesiliumShellManagerV1,
};
use desilium_protocols::client::desilium_toplevel_handle_v1::{
    self as handle, DesiliumToplevelHandleV1,
};
use smithay::desktop::layer_map_for_output;
use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::Display;
use smithay::utils::Transform;
use smithay_client_toolkit::reexports::client::protocol::wl_compositor::WlCompositor;
use smithay_client_toolkit::reexports::client::protocol::wl_registry::{self, WlRegistry};
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop,
};
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_surface::XdgSurface;
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::XdgToplevel;
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_wm_base::XdgWmBase;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{
    Layer, ZwlrLayerShellV1,
};
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    Anchor, ZwlrLayerSurfaceV1,
};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

const OUTPUT_SIZE: (i32, i32) = (1280, 800);
const PUMP_ROUNDS: usize = 6;

/// Records what the client saw, one line per event.
#[derive(Default)]
struct Client {
    log: Vec<String>,
    /// Advertised globals: name, interface, version.
    globals: Vec<(u32, String, u32)>,
    handles: Vec<DesiliumToplevelHandleV1>,
}

impl Dispatch<WlRegistry, ()> for Client {
    fn event(
        client: &mut Self,
        _: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            client.globals.push((name, interface, version));
        }
    }
}

impl Dispatch<DesiliumShellManagerV1, ()> for Client {
    fn event(
        client: &mut Self,
        _: &DesiliumShellManagerV1,
        event: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let line = match event {
            manager::Event::Toplevel { handle } => {
                client.handles.push(handle);
                "toplevel".to_owned()
            }
            manager::Event::Workspaces { count, active } => format!("workspaces {count} {active}"),
            manager::Event::OutputGeometry {
                width,
                height,
                work_width,
                work_height,
                scale_120ths,
                ..
            } => format!("output {width}x{height} work {work_width}x{work_height} @{scale_120ths}"),
            manager::Event::Shortcut { name } => format!("shortcut {name}"),
            manager::Event::Done => "done".to_owned(),
            _ => "unknown".to_owned(),
        };
        client.log.push(line);
    }

    smithay_client_toolkit::reexports::client::event_created_child!(Client, DesiliumShellManagerV1, [
        manager::EVT_TOPLEVEL_OPCODE => (DesiliumToplevelHandleV1, ()),
    ]);
}

impl Dispatch<DesiliumToplevelHandleV1, ()> for Client {
    fn event(
        client: &mut Self,
        _: &DesiliumToplevelHandleV1,
        event: handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let line = match event {
            handle::Event::WindowId { id } => format!("window {id}"),
            handle::Event::Title { title } => format!("title {title}"),
            handle::Event::AppId { app_id } => format!("app_id {app_id}"),
            handle::Event::State { state } => format!("state {}", state.len() / 4),
            handle::Event::Workspace { index } => format!("workspace {index}"),
            handle::Event::Closed => "closed".to_owned(),
            handle::Event::Done => "handle_done".to_owned(),
            _ => "unknown".to_owned(),
        };
        client.log.push(line);
    }
}

impl Dispatch<XdgWmBase, ()> for Client {
    fn event(
        _: &mut Self,
        _: &XdgWmBase,
        _: <XdgWmBase as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for Client {
    fn event(
        _: &mut Self,
        _: &ZwlrLayerSurfaceV1,
        _: <ZwlrLayerSurfaceV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Client: WlCompositor);
delegate_noop!(Client: ignore WlSurface);
delegate_noop!(Client: ignore XdgSurface);
delegate_noop!(Client: ignore XdgToplevel);
delegate_noop!(Client: ZwlrLayerShellV1);

/// A compositor state, its loop and one connected client.
struct Harness {
    event_loop: EventLoop<'static, DesiliumState>,
    state: DesiliumState,
    queue: EventQueue<Client>,
    client: Client,
    registry: WlRegistry,
}

impl Harness {
    /// `None` when no Wayland socket can be created (no `XDG_RUNTIME_DIR`).
    fn new(trusted: bool) -> Option<Self> {
        let event_loop: EventLoop<'static, DesiliumState> = EventLoop::try_new().ok()?;
        let display = Display::<DesiliumState>::new().ok()?;
        let mut state = DesiliumState::new(
            &event_loop.handle(),
            event_loop.get_signal(),
            display,
            CompositorSettings::default(),
        )
        .ok()?;
        add_output(&mut state);
        let (server_end, client_end) = UnixStream::pair().ok()?;
        client_end.set_nonblocking(true).ok()?;
        let server_client = state
            .display_handle
            .insert_client(server_end, Arc::new(ClientState::default()))
            .ok()?;
        server_client
            .get_data::<ClientState>()?
            .set_shell_trusted(trusted);
        let conn = Connection::from_socket(client_end).ok()?;
        let queue = conn.new_event_queue::<Client>();
        let registry = conn.display().get_registry(&queue.handle(), ());
        let mut harness = Self {
            event_loop,
            state,
            queue,
            client: Client::default(),
            registry,
        };
        harness.pump();
        Some(harness)
    }

    fn qh(&self) -> QueueHandle<Client> {
        self.queue.handle()
    }

    /// Moves messages both ways a few times so that request/event chains settle.
    fn pump(&mut self) {
        for _ in 0..PUMP_ROUNDS {
            let _ = self.queue.flush();
            let _ = self
                .event_loop
                .dispatch(Some(Duration::ZERO), &mut self.state);
            let _ = self.state.display_handle.flush_clients();
            if let Some(guard) = self.queue.prepare_read() {
                let _ = guard.read();
            }
            let _ = self.queue.dispatch_pending(&mut self.client);
        }
    }

    fn bind<T: Proxy + 'static>(&mut self, interface: &str) -> Option<T>
    where
        Client: Dispatch<T, ()>,
    {
        let (name, _, version) = self
            .client
            .globals
            .iter()
            .find(|(_, advertised, _)| advertised == interface)?
            .clone();
        Some(self.registry.bind::<T, _, _>(name, version, &self.qh(), ()))
    }

    fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.client.log)
    }
}

fn add_output(state: &mut DesiliumState) {
    let mode = Mode {
        size: OUTPUT_SIZE.into(),
        refresh: 60_000,
    };
    let output = Output::new(
        "test".to_owned(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Desilium".into(),
            model: "Test".into(),
        },
    );
    output.create_global::<DesiliumState>(&state.display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(Scale::Integer(1)),
        Some((0, 0).into()),
    );
    state.space.map_output(&output, (0, 0));
    state
        .wm
        .set_area(desilium_wm::Rect::new(0, 0, OUTPUT_SIZE.0, OUTPUT_SIZE.1));
}

fn trusted_harness() -> Option<Harness> {
    let harness = Harness::new(true);
    if harness.is_none() {
        eprintln!("skipped: no Wayland socket available");
    }
    harness
}

#[test]
fn untrusted_clients_do_not_see_the_global() {
    let Some(harness) = Harness::new(false) else {
        return;
    };
    let names: Vec<&str> = harness
        .client
        .globals
        .iter()
        .map(|g| g.1.as_str())
        .collect();
    assert!(names.contains(&"wl_compositor"));
    assert!(!names.contains(&"desilium_shell_manager_v1"));
}

#[test]
fn bind_sends_the_full_state_then_only_diffs() {
    let Some(mut harness) = trusted_harness() else {
        return;
    };
    let Some(compositor) = harness.bind::<WlCompositor>("wl_compositor") else {
        panic!("no compositor");
    };
    let wm_base = harness.bind::<XdgWmBase>("xdg_wm_base").unwrap();
    let shell = harness
        .bind::<DesiliumShellManagerV1>("desilium_shell_manager_v1")
        .unwrap();
    harness.pump();
    assert_eq!(
        harness.take_log(),
        vec![
            "workspaces 4 0",
            "output 1280x800 work 1280x800 @120",
            "done"
        ]
    );

    // A new window arrives with all its properties in one batch.
    let qh = harness.qh();
    let surface = compositor.create_surface(&qh, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg_surface.get_toplevel(&qh, ());
    toplevel.set_title("hello".into());
    harness.pump();
    let log = harness.take_log();
    assert_eq!(log.first().map(String::as_str), Some("toplevel"));
    assert!(log.contains(&"title hello".to_owned()), "{log:?}");
    assert_eq!(log.last().map(String::as_str), Some("done"));

    // A title change sends the title alone.
    toplevel.set_title("bye".into());
    harness.pump();
    assert_eq!(harness.take_log(), vec!["title bye", "handle_done", "done"]);

    // Nothing changed: nothing is sent.
    harness.pump();
    assert!(harness.take_log().is_empty());

    // Workspace switching goes through the request and comes back as a diff.
    shell.switch_workspace(2);
    harness.pump();
    let log = harness.take_log();
    assert!(log.contains(&"workspaces 4 2".to_owned()), "{log:?}");
    assert_eq!(log.last().map(String::as_str), Some("done"));
    assert_eq!(harness.state.desktop_overview().active, 2);
}

#[test]
fn shortcuts_reach_the_shell() {
    let Some(mut harness) = trusted_harness() else {
        return;
    };
    let _shell = harness
        .bind::<DesiliumShellManagerV1>("desilium_shell_manager_v1")
        .unwrap();
    harness.pump();
    harness.take_log();
    assert!(harness.state.shell_shortcut("toggle_launcher"));
    harness.pump();
    assert_eq!(harness.take_log(), vec!["shortcut toggle_launcher"]);
}

#[test]
fn layer_surface_effects_are_clamped_and_minimize_target_follows_the_surface() {
    let Some(mut harness) = trusted_harness() else {
        return;
    };
    let compositor = harness.bind::<WlCompositor>("wl_compositor").unwrap();
    let wm_base = harness.bind::<XdgWmBase>("xdg_wm_base").unwrap();
    let layer_shell = harness
        .bind::<ZwlrLayerShellV1>("zwlr_layer_shell_v1")
        .unwrap();
    let shell = harness
        .bind::<DesiliumShellManagerV1>("desilium_shell_manager_v1")
        .unwrap();
    let qh = harness.qh();

    let window = compositor.create_surface(&qh, ());
    wm_base
        .get_xdg_surface(&window, &qh, ())
        .get_toplevel(&qh, ());
    let dock = compositor.create_surface(&qh, ());
    let layer = layer_shell.get_layer_surface(&dock, None, Layer::Top, "dock".into(), &qh, ());
    layer.set_anchor(Anchor::Bottom | Anchor::Left | Anchor::Right);
    layer.set_size(0, 48);
    dock.commit();
    harness.pump();

    shell.set_surface_effects(&dock, 500, 12, 12, 0, 1000);
    harness.pump();
    let root = {
        let output = harness.state.layer_output().unwrap();
        let map = layer_map_for_output(&output);
        map.layers().next().unwrap().wl_surface().clone()
    };
    let effects = harness.state.layer_surface_effects(&root).unwrap();
    assert_eq!(effects.blur_radius, 64.0);
    assert_eq!(effects.corner_radii, [12.0, 12.0, 0.0, 64.0]);

    shell.set_surface_effects(&dock, 0, 0, 0, 0, 0);
    harness.pump();
    assert!(harness.state.layer_surface_effects(&root).is_none());

    let (id, _) = harness.state.wm.all_windows()[0];
    assert!(harness.state.minimize_target(id).is_none());

    harness.client.handles[0].set_minimize_rectangle(Some(&dock), 10, 5, 40, 30);
    harness.pump();
    let target = harness.state.minimize_target(id).unwrap();
    let dock_top = OUTPUT_SIZE.1 - 48;
    assert_eq!(target, desilium_wm::Rect::new(10, dock_top + 5, 40, 30));

    harness.client.handles[0].set_minimize_rectangle(None, 0, 0, 0, 0);
    harness.pump();
    assert!(harness.state.minimize_target(id).is_none());
}

#[test]
fn window_controls_and_closing_are_reported() {
    let Some(mut harness) = trusted_harness() else {
        return;
    };
    let compositor = harness.bind::<WlCompositor>("wl_compositor").unwrap();
    let wm_base = harness.bind::<XdgWmBase>("xdg_wm_base").unwrap();
    let _shell = harness
        .bind::<DesiliumShellManagerV1>("desilium_shell_manager_v1")
        .unwrap();
    let qh = harness.qh();
    let surface = compositor.create_surface(&qh, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg_surface.get_toplevel(&qh, ());
    harness.pump();
    harness.take_log();

    harness.client.handles[0].set_minimized(1);
    harness.pump();
    let log = harness.take_log();
    assert_eq!(log, vec!["state 1", "handle_done", "done"], "{log:?}");

    harness.client.handles[0].activate();
    harness.pump();
    assert_eq!(harness.take_log(), vec!["state 1", "handle_done", "done"]);

    toplevel.destroy();
    harness.pump();
    assert_eq!(harness.take_log(), vec!["closed", "handle_done", "done"]);
}

use smithay_client_toolkit::reexports::client::protocol::{
    wl_keyboard::{self, WlKeyboard},
    wl_seat::WlSeat,
};
delegate_noop!(Client: ignore WlSeat);

impl Dispatch<WlKeyboard, ()> for Client {
    fn event(
        client: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::RepeatInfo { rate, delay } = event {
            client.log.push(format!("repeat {rate} {delay}"));
        }
    }
}

#[test]
fn keyboard_repeat_changes_reach_an_existing_wayland_keyboard() {
    let Some(mut harness) = trusted_harness() else {
        return;
    };
    let seat = harness.bind::<WlSeat>("wl_seat").expect("seat");
    let _keyboard = seat.get_keyboard(&harness.qh(), ());
    harness.pump();
    assert!(harness.take_log().contains(&"repeat 25 200".to_owned()));
    let mut config = desilium_config_types::DesiliumConfig::default();
    config.compositor.keyboard_repeat_rate = 30;
    config.compositor.keyboard_repeat_delay = 300;
    harness.state.apply_config(&config);
    harness.pump();
    assert!(harness.take_log().contains(&"repeat 30 300".to_owned()));
}

#[test]
fn pointer_focus_honors_click_mode_and_raise_preference() {
    let Some(mut harness) = trusted_harness() else {
        return;
    };
    let state = &mut harness.state;
    let size = desilium_wm::Size { w: 200, h: 200 };
    let bottom = state.wm.open_window(size);
    let top = state.wm.open_window(size);
    state.wm.move_to(bottom, desilium_wm::Point { x: 0, y: 0 });
    state.wm.move_to(top, desilium_wm::Point { x: 400, y: 0 });
    state.focus_on_pointer_motion((100.0, 100.0).into());
    assert_eq!(state.wm.focused(), Some(top));
    state.settings.focus = desilium_config_types::FocusPolicy::Sloppy;
    state.settings.raise_on_focus = false;
    let stack = state.wm.placements();
    state.focus_on_pointer_motion((100.0, 100.0).into());
    assert_eq!(state.wm.focused(), Some(bottom));
    assert_eq!(state.wm.placements(), stack);
    state.focus_on_pointer_motion((500.0, 100.0).into());
    state.settings.raise_on_focus = true;
    state.focus_on_pointer_motion((100.0, 100.0).into());
    assert_eq!(
        state.wm.placements().last().map(|window| window.id),
        Some(bottom)
    );
    state.focus_on_pointer_motion((900.0, 700.0).into());
    assert_eq!(state.wm.focused(), Some(bottom));
}

#[test]
fn live_snap_controls_change_the_preview_and_disable_it_immediately() {
    let Some(mut harness) = trusted_harness() else {
        return;
    };
    let state = &mut harness.state;
    let id = state.wm.open_window(desilium_wm::Size { w: 200, h: 200 });
    let point = desilium_wm::Point { x: 20, y: 400 };
    assert!(state.update_snap_preview(id, point).is_none());
    let mut config = desilium_config_types::DesiliumConfig::default();
    config.compositor.snap_distance = 24;
    state.apply_config(&config);
    assert_eq!(
        state.update_snap_preview(id, point),
        Some(desilium_wm::SnapZone::Left)
    );
    config.compositor.snap_enabled = false;
    state.apply_config(&config);
    assert!(state.snap_preview().is_none());
    assert!(state.update_snap_preview(id, point).is_none());
}
