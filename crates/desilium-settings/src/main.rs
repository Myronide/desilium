//! `desilium-settings`: the graphical settings application.

use std::process::ExitCode;

use desilium_settings::runtime::{
    cli::{self, Parsed, USAGE},
    run,
};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();
    match cli::parse(std::env::args().skip(1)) {
        Ok(Parsed::Help) => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Parsed::Version) => {
            println!("desilium-settings {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Parsed::Run(options)) => match run(options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("desilium-settings: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("desilium-settings: {error}");
            ExitCode::from(2)
        }
    }
}
