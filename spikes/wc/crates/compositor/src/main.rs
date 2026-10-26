#![allow(irrefutable_let_patterns)]
mod control;
mod grabs;
mod handlers;
mod input;
mod policy;
mod state;
mod winit;
use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
pub use state::Smallvil;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "wc_compositor=info,warn".into()),
        )
        .init();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|s| s == "--help" || s == "-h") {
        println!(
            "WC — floating-first Wayland compositor (nested)\n\nwc-compositor [--settings] [--terminal] [--command PROGRAM]\n\nSuper+, Einstellungen · Alt+Tab Wechseln · Alt+F4 Schließen\nSuper+Pfeile Einrasten/Maximieren/Wiederherstellen\nSuper+Shift+Escape Beenden\n\nWC_SOCKET: eigener Control-Socket; RUST_LOG: Logging"
        );
        return Ok(());
    }
    let mut program = None;
    let mut launch_settings = false;
    let mut terminal = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--settings" => launch_settings = true,
            "--terminal" => terminal = true,
            "--command" | "-c" => {
                program = Some(args.next().ok_or("--command braucht einen Programmpfad")?)
            }
            _ => return Err(format!("Unbekannte Option: {arg}").into()),
        }
    }
    let settings = wc_core::load()
        .map_err(|e| format!("Konfiguration ungültig (unverändert gelassen): {e}"))?;
    let mut event_loop: EventLoop<Smallvil> = EventLoop::try_new()?;
    let display: Display<Smallvil> = Display::new()?;
    let mut state = Smallvil::new(&mut event_loop, display, settings);
    let _socket = control::install(&mut event_loop)?;
    winit::init_winit(&mut event_loop, &mut state)?;
    println!(
        "WC läuft auf WAYLAND_DISPLAY={}",
        state.socket_name.to_string_lossy()
    );
    println!("Control: {}", wc_core::socket_path()?.display());
    if launch_settings {
        state.spawn_settings();
    }
    if terminal {
        state.spawn_terminal();
    }
    if let Some(program) = program {
        let mut command = std::process::Command::new(program);
        command
            .env("WAYLAND_DISPLAY", &state.socket_name)
            .env_remove("DISPLAY");
        state.spawn(command);
    }
    event_loop.run(None, &mut state, |_| {})?;
    Ok(())
}
