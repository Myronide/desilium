use crate::error::CompositorError;
use crate::render::{self, RenderResources, SceneContext};
use crate::state::{DesiliumState, RedrawHandle};
use desilium_wm::Rect;
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::calloop::ping::make_ping;
use smithay::utils::{Physical, Size, Transform};
use std::cell::RefCell;
use std::rc::Rc;
use tracing::{error, info};

/// Nominal refresh in millihertz; the host compositor paces the real frames.
const REFRESH_MHZ: i32 = 60_000;

struct WinitSession {
    backend: WinitGraphicsBackend<GlesRenderer>,
    output: Output,
    damage_tracker: OutputDamageTracker,
    resources: RenderResources,
}

/// Opens a window on the host compositor and renders the desktop into it.
pub fn init(
    handle: &LoopHandle<'static, DesiliumState>,
    state: &mut DesiliumState,
) -> Result<(), CompositorError> {
    let (mut backend, events) = winit::init::<GlesRenderer>()?;
    // The compositor draws its own cursor, so the host one would be a duplicate.
    backend.window().set_cursor_visible(false);
    let resources = RenderResources::new(backend.renderer());
    state.init_dmabuf_global(backend.renderer().dmabuf_formats());
    let size = backend.window_size();
    let mode = Mode {
        size,
        refresh: REFRESH_MHZ,
    };

    let output = Output::new(
        "winit".to_owned(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Desilium".into(),
            model: "Winit".into(),
        },
    );
    output.create_global::<DesiliumState>(&state.display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));
    state.wm.set_area(area_of(size));

    let (ping, ping_source) = make_ping()?;
    state.set_redraw_handle(RedrawHandle::new(ping));

    let damage_tracker = OutputDamageTracker::from_output(&output);
    let session = Rc::new(RefCell::new(WinitSession {
        backend,
        output,
        damage_tracker,
        resources,
    }));

    let wake = Rc::clone(&session);
    handle
        .insert_source(ping_source, move |_, _, _| {
            wake.borrow().backend.window().request_redraw()
        })
        .map_err(|e| e.error)?;
    handle
        .insert_source(events, move |event, _, state| {
            on_event(&mut session.borrow_mut(), state, event)
        })
        .map_err(|e| e.error)?;

    state.request_redraw();
    info!(width = size.w, height = size.h, "winit output ready");
    Ok(())
}

fn area_of(size: Size<i32, Physical>) -> Rect {
    Rect::new(0, 0, size.w, size.h)
}

fn on_event(session: &mut WinitSession, state: &mut DesiliumState, event: WinitEvent) {
    match event {
        WinitEvent::Resized { size, .. } => {
            session.output.change_current_state(
                Some(Mode {
                    size,
                    refresh: REFRESH_MHZ,
                }),
                None,
                None,
                None,
            );
            state.wm.set_area(area_of(size));
            state.arrange_layers();
            state.request_redraw();
        }
        WinitEvent::Input(input) => state.process_input_event(input),
        WinitEvent::Redraw => redraw(session, state),
        WinitEvent::CloseRequested => state.loop_signal.stop(),
        WinitEvent::Focus(_) => {}
    }
}

fn redraw(session: &mut WinitSession, state: &mut DesiliumState) {
    let WinitSession {
        backend,
        output,
        damage_tracker,
        resources,
    } = session;
    state.import_pending_dmabufs(backend.renderer());
    let ctx = SceneContext::new(resources, state, output);
    if let Err(error) = render::draw(backend, damage_tracker, &ctx) {
        error!(%error, "frame failed");
        return;
    }
    state.send_frame_callbacks(output);
    state.send_cursor_frame_callbacks(output);
    state.finish_frame();
}
