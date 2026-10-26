use std::time::Duration;

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker, element::surface::WaylandSurfaceRenderElement,
            gles::GlesRenderer,
        },
        winit::{self, WinitEvent},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
    utils::{Rectangle, Transform},
};

use crate::Smallvil;

pub fn init_winit(
    event_loop: &mut EventLoop<Smallvil>,
    state: &mut Smallvil,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, winit) = winit::init_from_attributes::<GlesRenderer>(
        smithay::reexports::winit::window::WindowAttributes::default()
            .with_title("WC · Floating Wayland Desktop")
            .with_surface_size(smithay::reexports::winit::dpi::LogicalSize::new(
                1280.0, 800.0,
            )),
    )?;

    use smithay::backend::{
        egl::EGLDevice,
        renderer::{ImportDma, ImportMemWl},
    };
    use smithay::wayland::dmabuf::DmabufFeedbackBuilder;
    let formats = backend.renderer().dmabuf_formats();
    let feedback = EGLDevice::device_for_display(backend.renderer().egl_context().display())
        .and_then(|device| device.try_get_render_node())
        .ok()
        .flatten()
        .and_then(|node| {
            DmabufFeedbackBuilder::new(node.dev_id(), formats.clone())
                .build()
                .ok()
        });
    state.dmabuf_global = Some(if let Some(feedback) = feedback {
        state
            .dmabuf_state
            .create_global_with_default_feedback::<Smallvil>(&state.display_handle, &feedback)
    } else {
        tracing::warn!("Kein Render-Node: DMA-BUF v3, GPU-Clients eventuell eingeschränkt");
        state
            .dmabuf_state
            .create_global::<Smallvil>(&state.display_handle, formats)
    });
    state
        .shm_state
        .update_formats(backend.renderer().shm_formats());

    let mode = Mode {
        size: backend.window_size(),
        refresh: 60_000,
    };

    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "Winit".into(),
            serial_number: "Unknown".into(),
        },
    );
    let _global = output.create_global::<Smallvil>(&state.display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    state.space.map_output(&output, (0, 0));

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    state.backend = Some(backend);
    event_loop
        .handle()
        .insert_source(winit, move |event, _, state| {
            match event {
                WinitEvent::Resized { size, .. } => {
                    output.change_current_state(
                        Some(Mode {
                            size,
                            refresh: 60_000,
                        }),
                        None,
                        None,
                        None,
                    );
                    state.reflow();
                }
                WinitEvent::Input(event) => state.process_input_event(event),
                WinitEvent::Redraw => {
                    let mut backend = state.backend.take().expect("winit backend initialized");
                    let size = backend.window_size();
                    let damage = Rectangle::from_size(size);

                    {
                        use smithay::backend::renderer::element::{
                            Kind, surface::render_elements_from_surface_tree,
                        };
                        use smithay::input::pointer::{CursorImageAttributes, CursorImageStatus};
                        use smithay::wayland::compositor::with_states;
                        let status = state.cursor_status.clone();
                        backend
                            .window()
                            .set_cursor_visible(matches!(status, CursorImageStatus::Named(_)));
                        if let CursorImageStatus::Named(icon) = &status {
                            backend.window().set_cursor((*icon).into());
                        }
                        let (renderer, mut framebuffer) = backend.bind().unwrap();
                        let cursor: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                            if let CursorImageStatus::Surface(surface) = &status {
                                let hotspot = with_states(surface, |states| {
                                    states
                                        .data_map
                                        .get::<std::sync::Mutex<CursorImageAttributes>>()
                                        .unwrap()
                                        .lock()
                                        .unwrap()
                                        .hotspot
                                });
                                let pos = state.seat.get_pointer().unwrap().current_location()
                                    - hotspot.to_f64();
                                render_elements_from_surface_tree(
                                    renderer,
                                    surface,
                                    pos.to_physical_precise_round(1.0),
                                    1.0,
                                    1.0,
                                    Kind::Cursor,
                                )
                            } else {
                                Vec::new()
                            };
                        smithay::desktop::space::render_output::<
                            _,
                            WaylandSurfaceRenderElement<GlesRenderer>,
                            _,
                            _,
                        >(
                            &output,
                            renderer,
                            &mut framebuffer,
                            1.0,
                            0,
                            [&state.space],
                            &cursor,
                            &mut damage_tracker,
                            state.policy.settings.rgba(),
                        )
                        .unwrap();
                    }
                    backend.submit(Some(&[damage])).unwrap();

                    state.space.elements().for_each(|window| {
                        window.send_frame(
                            &output,
                            state.start_time.elapsed(),
                            Some(Duration::ZERO),
                            |_, _| Some(output.clone()),
                        )
                    });

                    state.space.refresh();
                    state.housekeeping();
                    state.popups.cleanup();
                    let _ = state.display_handle.flush_clients();

                    // Ask for redraw to schedule new frame.
                    backend.window().request_redraw();
                    state.backend = Some(backend);
                }
                WinitEvent::CloseRequested => {
                    state.loop_signal.stop();
                }
                _ => (),
            };
        })?;

    Ok(())
}
