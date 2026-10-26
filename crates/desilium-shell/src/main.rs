//! `desilium-shell`: the desktop shell process (top bar, dock, launcher).
//!
//! One thread, one event loop: the Wayland connection, timers and file watches
//! are polled together, and an idle shell only wakes for its clock.

mod app;
mod bar;
mod cli;
mod dock;
mod error;
mod launcher;
mod model;
mod quick;
mod services;
mod slot;
#[cfg(test)]
mod snapshot_tests;
mod tooltip;
mod view;

use std::process::ExitCode;

use tracing_subscriber::EnvFilter;

use cli::{Args, USAGE};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("desilium-shell: {error}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if args.help {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match app::run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("desilium-shell: {error}");
            ExitCode::FAILURE
        }
    }
}
