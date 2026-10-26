mod animation;
mod backend;
mod cli;
mod decoration;
mod error;
mod launch;
mod render;
mod state;

use backend::headless::{self, HeadlessOptions, RunStatus};
use calloop::signals::{Signal, Signals};
use cli::Args;
use desilium_config::ConfigStore;
use desilium_config_types::CompositorSettings;
use error::CompositorError;
use smithay::reexports::calloop::{EventLoop, LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::Display;
use state::DesiliumState;
use std::process::ExitCode;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error!(%error, "compositor stopped with an error");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), CompositorError> {
    let args = cli::parse_args(std::env::args().skip(1))?;
    let mut event_loop: EventLoop<'static, DesiliumState> = EventLoop::try_new()?;
    let display = Display::<DesiliumState>::new()?;
    let settings = CompositorSettings::default();
    let mut state = DesiliumState::new(
        &event_loop.handle(),
        event_loop.get_signal(),
        display,
        settings,
    )?;

    let config_dir = args.resolved_config_dir(ConfigStore::default_dir());
    state.init_settings(config_dir, args.theme);
    state.watch_config(&event_loop.handle());
    state.configure_animation_dev(args.freeze_time, args.demo);
    stop_on_signals(&event_loop.handle(), event_loop.get_signal())?;
    let status = init_backend(&event_loop.handle(), &mut state, &args)?;
    place_pointer(&mut state, &args);
    autostart(&state, &args.commands);

    info!("desilium compositor running");
    event_loop.run(None, &mut state, |state| {
        if let Err(error) = state.display_handle.flush_clients() {
            warn!(%error, "flushing clients failed");
        }
    })?;
    match status.map(|status| status.get()) {
        Some(RunStatus::Failed | RunStatus::Running) => Err(CompositorError::Screenshot(
            "headless run ended without a screenshot".into(),
        )),
        _ => Ok(()),
    }
}

/// Picks the backend; only the headless one reports a run status.
fn init_backend(
    handle: &LoopHandle<'static, DesiliumState>,
    state: &mut DesiliumState,
    args: &Args,
) -> Result<Option<headless::SharedStatus>, CompositorError> {
    if !args.headless {
        backend::winit::init(handle, state)?;
        return Ok(None);
    }
    let options = HeadlessOptions {
        size: args.size.into(),
        scale: args.scale,
        screenshot: args.screenshot.clone(),
        delay: args.screenshot_delay,
    };
    headless::init(handle, state, options).map(Some)
}

/// Applies `--pointer`: places the pointer and optionally forces a cursor shape.
fn place_pointer(state: &mut DesiliumState, args: &Args) {
    let Some(start) = args.pointer else {
        return;
    };
    state.place_pointer((start.x, start.y).into());
    state.set_cursor_override(start.shape);
}

fn autostart(state: &DesiliumState, commands: &[String]) {
    if !commands.is_empty() {
        for program in commands {
            launch::spawn(program, &state.socket_name);
        }
        return;
    }
    match launch::find_terminal(state.settings.terminal.as_deref()) {
        Some(program) => launch::spawn(&program, &state.socket_name),
        None => warn!(
            "no client to autostart; connect one to WAYLAND_DISPLAY={:?}",
            state.socket_name
        ),
    }
}

/// Lets `docker stop`, Ctrl+C and the session manager end the compositor cleanly.
fn stop_on_signals(
    handle: &LoopHandle<'static, DesiliumState>,
    stop: LoopSignal,
) -> Result<(), CompositorError> {
    let signals = Signals::new(&[Signal::SIGINT, Signal::SIGTERM])?;
    handle
        .insert_source(signals, move |event, _, _| {
            info!(signal = ?event.signal(), "shutdown requested");
            stop.stop();
        })
        .map_err(|error| error.error)?;
    Ok(())
}
