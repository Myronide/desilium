//! `desilium-kwin`: keeps KWin configured from the Desilium configuration.

use std::path::PathBuf;
use std::process::ExitCode;

use desilium_config::ConfigStore;
use desilium_kwin::bridge::{self, Options};
use desilium_theme::Mode;

const USAGE: &str =
    "usage: desilium-kwin [--config-dir DIR] [--theme dark|light] [--once] [--print-plan]

  --config-dir DIR   directory holding config.toml (default: the Desilium config directory)
  --theme dark|light replace the configured theme, as the shell's --theme does
  --once             apply the configuration once and exit instead of watching it
  --print-plan       print the KWin settings the configuration maps to and exit
  -h, --help         show this help";

enum Parsed {
    Run(Options, bool),
    Help,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let (options, print_plan) = match parse(std::env::args().skip(1)) {
        Ok(Parsed::Run(options, print_plan)) => (options, print_plan),
        Ok(Parsed::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("desilium-kwin: {message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if print_plan {
        print_plan_text(&options);
        return ExitCode::SUCCESS;
    }
    match bridge::run(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "the bridge stopped");
            ExitCode::FAILURE
        }
    }
}

fn parse(args: impl Iterator<Item = String>) -> Result<Parsed, String> {
    let mut args = args;
    let mut options = Options {
        config_dir: ConfigStore::default_dir(),
        theme: None,
        once: false,
    };
    let mut print_plan = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Parsed::Help),
            "--once" => options.once = true,
            "--print-plan" => print_plan = true,
            "--config-dir" => {
                let value = args.next().ok_or("--config-dir needs a directory")?;
                options.config_dir = PathBuf::from(value);
            }
            "--theme" => {
                options.theme = Some(match args.next().as_deref() {
                    Some("dark") => Mode::Dark,
                    Some("light") => Mode::Light,
                    _ => return Err("--theme needs dark or light".to_owned()),
                });
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Parsed::Run(options, print_plan))
}

fn print_plan_text(options: &Options) {
    let store = ConfigStore::open(&options.config_dir).store;
    let (plan, warnings) = desilium_kwin::plan::plan(store.get(), options.theme);
    for entry in &plan.entries {
        println!(
            "{}\t[{}]\t{}={}",
            entry.id.file,
            entry.id.group,
            entry.id.key,
            entry.value.render()
        );
    }
    println!("virtual-desktops\t{}", plan.desktops);
    let bindings = desilium_kwin::shortcuts::bindings(&store.get().compositor);
    for binding in &bindings.list {
        let (component, action) = binding.target.names();
        println!("shortcut\t{component}/{action}\t{:x?}", binding.keys);
    }
    for warning in warnings.iter().chain(&bindings.skipped) {
        eprintln!("warning: {warning}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter()
            .map(|s| (*s).to_owned())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn flags_are_parsed() {
        let Ok(Parsed::Run(options, print)) = parse(args(&[
            "--once",
            "--theme",
            "light",
            "--config-dir",
            "/tmp/x",
            "--print-plan",
        ])) else {
            panic!("should parse");
        };
        assert!(options.once && print);
        assert_eq!(options.theme, Some(Mode::Light));
        assert_eq!(options.config_dir, PathBuf::from("/tmp/x"));
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert!(parse(args(&["--theme", "blue"])).is_err());
        assert!(parse(args(&["--config-dir"])).is_err());
        assert!(parse(args(&["--frobnicate"])).is_err());
        assert!(matches!(parse(args(&["--help"])), Ok(Parsed::Help)));
    }
}
