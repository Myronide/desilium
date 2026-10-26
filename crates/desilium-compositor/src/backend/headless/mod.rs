//! Display-less backend: renders the desktop into an offscreen GPU buffer.

mod clock;
mod gpu;
mod image;

pub use gpu::HeadlessGpu;
pub use image::Image;

use crate::error::CompositorError;
use crate::render::{self, RenderResources, SceneContext};
use crate::state::{DesiliumState, RedrawHandle};
use clock::{ShotClock, Verdict};
use desilium_wm::Rect;
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::calloop::ping::make_ping;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::utils::{Physical, Size, Transform};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

const REFRESH_MHZ: i32 = 60_000;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Settings of one headless run.
pub struct HeadlessOptions {
    pub size: Size<i32, Physical>,
    pub scale: f64,
    pub screenshot: Option<PathBuf>,
    pub delay: Duration,
}

/// How a headless run ended; polled by `main` for the exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Succeeded,
    Failed,
}

/// Shared status cell that the event loop callbacks update.
pub type SharedStatus = Rc<Cell<RunStatus>>;

struct Session {
    gpu: HeadlessGpu,
    output: Output,
    damage_tracker: OutputDamageTracker,
    resources: RenderResources,
    options: HeadlessOptions,
    clock: ShotClock,
    last_image: Option<Image>,
    last_activity: Option<Instant>,
    status: SharedStatus,
}

/// Creates the offscreen output and starts the redraw and screenshot sources.
pub fn init(
    handle: &LoopHandle<'static, DesiliumState>,
    state: &mut DesiliumState,
    options: HeadlessOptions,
) -> Result<SharedStatus, CompositorError> {
    let mut gpu = HeadlessGpu::new()?;
    let resources = RenderResources::new(gpu.renderer());
    state.init_dmabuf_global(gpu.renderer().dmabuf_formats());
    let output = create_output(state, &options);
    let (ping, ping_source) = make_ping()?;
    state.set_redraw_handle(RedrawHandle::new(ping));

    let status: SharedStatus = Rc::new(Cell::new(RunStatus::Running));
    let session = Rc::new(RefCell::new(Session {
        gpu,
        damage_tracker: OutputDamageTracker::from_output(&output),
        resources,
        output,
        clock: ShotClock::new(Instant::now(), options.delay),
        options,
        last_image: None,
        last_activity: None,
        status: Rc::clone(&status),
    }));

    let on_ping = Rc::clone(&session);
    handle
        .insert_source(ping_source, move |_, _, state| {
            on_ping.borrow_mut().redraw(state)
        })
        .map_err(|e| e.error)?;
    let timer = Timer::from_duration(POLL_INTERVAL);
    handle
        .insert_source(timer, move |now, _, state| {
            session.borrow_mut().poll(now, state)
        })
        .map_err(|e| e.error)?;

    state.request_redraw();
    Ok(status)
}

fn create_output(state: &mut DesiliumState, options: &HeadlessOptions) -> Output {
    let mode = Mode {
        size: options.size,
        refresh: REFRESH_MHZ,
    };
    let output = Output::new(
        "headless".to_owned(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Desilium".into(),
            model: "Headless".into(),
        },
    );
    output.create_global::<DesiliumState>(&state.display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(Scale::Fractional(options.scale)),
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));
    state.wm.set_area(logical_area(options));
    info!(
        width = options.size.w,
        height = options.size.h,
        scale = options.scale,
        "headless output ready"
    );
    output
}

fn logical_area(options: &HeadlessOptions) -> Rect {
    let logical = options
        .size
        .to_f64()
        .to_logical(options.scale)
        .to_i32_round();
    Rect::new(0, 0, logical.w, logical.h)
}

impl Session {
    fn redraw(&mut self, state: &mut DesiliumState) {
        let Self {
            gpu,
            output,
            damage_tracker,
            resources,
            options,
            ..
        } = self;
        state.import_pending_dmabufs(gpu.renderer());
        let rendered = gpu.render_with(options.size, |renderer, target| {
            let ctx = SceneContext::new(resources, state, output);
            render::render_full(renderer, target, damage_tracker, &ctx)
        });
        match rendered {
            Ok(image) => self.last_image = Some(image),
            Err(error) => {
                error!(%error, "frame failed");
                return;
            }
        }
        if state.space.elements().next().is_some() {
            self.last_activity = Some(Instant::now());
        }
        state.send_frame_callbacks(&self.output);
        state.send_cursor_frame_callbacks(&self.output);
        state.finish_frame();
    }

    fn poll(&mut self, now: Instant, state: &mut DesiliumState) -> TimeoutAction {
        let Some(path) = self.options.screenshot.clone() else {
            return TimeoutAction::Drop;
        };
        let verdict = self.clock.verdict(now, self.last_activity);
        if verdict == Verdict::Wait {
            return TimeoutAction::ToDuration(POLL_INTERVAL);
        }
        if verdict == Verdict::TimedOut {
            warn!("screenshot timeout reached; capturing the current state");
        }
        let saved = self.save(&path);
        // A picture without any client window is saved for debugging but counts as a failure.
        let shown = self.last_activity.is_some();
        if !shown {
            error!("no client window was ever shown");
        }
        let ok = saved && shown;
        self.status.set(if ok {
            RunStatus::Succeeded
        } else {
            RunStatus::Failed
        });
        state.loop_signal.stop();
        TimeoutAction::Drop
    }

    fn save(&self, path: &std::path::Path) -> bool {
        let Some(image) = &self.last_image else {
            error!("nothing was rendered");
            return false;
        };
        match image.save_png(path) {
            Ok(()) => {
                info!(path = %path.display(), "screenshot written");
                true
            }
            Err(error) => {
                error!(%error, "cannot write the screenshot");
                false
            }
        }
    }
}
