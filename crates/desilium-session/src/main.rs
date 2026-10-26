//! `desilium-session`: starts the compositor and helpers and supervises them until the session ends.

use std::env;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use desilium_session::{ComponentSpec, Criticality, SessionError, SessionOutcome, Supervisor};
use tracing::Level;
use tracing_subscriber::EnvFilter;

const COMPOSITOR_BINARY: &str = "desilium-compositor";
const SHELL_BINARY: &str = "desilium-shell";
const SESSION_ENV: [(&str, &str); 2] = [
    ("XDG_SESSION_TYPE", "wayland"),
    ("XDG_CURRENT_DESKTOP", "Desilium"),
];
const USAGE: &str =
    "usage: desilium-session [--compositor <path>] [--shell <path>] [--] [compositor args...]";

#[derive(Debug, Default, PartialEq, Eq)]
struct CliArgs {
    compositor: Option<PathBuf>,
    shell: Option<PathBuf>,
    compositor_args: Vec<OsString>,
    help: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
enum UsageError {
    #[error("`{0}` requires a value")]
    MissingValue(&'static str),
}

#[derive(Debug, thiserror::Error)]
enum MainError {
    #[error(transparent)]
    Usage(#[from] UsageError),
    #[error(transparent)]
    Session(#[from] SessionError),
}

fn main() -> ExitCode {
    init_tracing();
    match run() {
        Ok(code) => code,
        Err(error) => {
            tracing::error!(error = %error, "session failed");
            ExitCode::FAILURE
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .from_env_lossy();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .init();
}

fn run() -> Result<ExitCode, MainError> {
    let cli = parse_args(env::args_os().skip(1))?;
    if cli.help {
        println!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    }
    let outcome = Supervisor::new(build_specs(cli)).run()?;
    tracing::info!(outcome = ?outcome, "session ended");
    Ok(exit_code(&outcome))
}

fn parse_args<I>(args: I) -> Result<CliArgs, UsageError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut parsed = CliArgs::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let flag = arg.to_string_lossy().into_owned();
        match flag.as_str() {
            "--compositor" => parsed.compositor = Some(value(&mut args, "--compositor")?.into()),
            "--shell" => parsed.shell = Some(value(&mut args, "--shell")?.into()),
            "-h" | "--help" => parsed.help = true,
            "--" => {
                parsed.compositor_args.extend(args);
                break;
            }
            _ => {
                // The first unknown argument starts the compositor's own arguments.
                parsed.compositor_args.push(arg);
                parsed.compositor_args.extend(args);
                break;
            }
        }
    }
    Ok(parsed)
}

fn value(
    args: &mut impl Iterator<Item = OsString>,
    flag: &'static str,
) -> Result<OsString, UsageError> {
    args.next().ok_or(UsageError::MissingValue(flag))
}

fn build_specs(cli: CliArgs) -> Vec<ComponentSpec> {
    let compositor_path = cli
        .compositor
        .unwrap_or_else(|| sibling_or_path(COMPOSITOR_BINARY));
    let mut compositor = ComponentSpec::new("compositor", compositor_path, Criticality::Critical);
    compositor.args = cli.compositor_args;
    let shell = match cli.shell {
        // An explicitly requested shell must exist; a discovered one is optional.
        Some(path) => ComponentSpec::new("shell", path, Criticality::Restartable),
        None => ComponentSpec::new(
            "shell",
            sibling_or_path(SHELL_BINARY),
            Criticality::Optional,
        ),
    };
    vec![with_session_env(compositor), with_session_env(shell)]
}

fn with_session_env(mut spec: ComponentSpec) -> ComponentSpec {
    spec.env = SESSION_ENV
        .iter()
        .map(|&(key, value)| (OsString::from(key), OsString::from(value)))
        .collect();
    spec
}

/// Prefers a binary installed next to this executable, falling back to a `PATH` lookup.
fn sibling_or_path(binary: &str) -> PathBuf {
    let sibling = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(binary)));
    match sibling {
        Some(path) if path.is_file() => path,
        _ => PathBuf::from(binary),
    }
}

fn exit_code(outcome: &SessionOutcome) -> ExitCode {
    match outcome {
        SessionOutcome::Normal | SessionOutcome::Signalled => ExitCode::SUCCESS,
        SessionOutcome::CompositorExited { code: Some(code) } => {
            u8::try_from(*code).map_or(ExitCode::FAILURE, ExitCode::from)
        }
        SessionOutcome::CompositorExited { code: None } => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn empty_command_line_uses_defaults() {
        let parsed = parse_args(args(&[])).expect("no flags is valid");
        assert_eq!(parsed, CliArgs::default());
    }

    #[test]
    fn component_paths_and_separator_are_parsed() {
        let parsed = parse_args(args(&[
            "--compositor",
            "/bin/comp",
            "--shell",
            "/bin/shell",
            "--",
            "--backend",
            "winit",
        ]))
        .expect("valid flags");
        assert_eq!(parsed.compositor, Some(PathBuf::from("/bin/comp")));
        assert_eq!(parsed.shell, Some(PathBuf::from("/bin/shell")));
        assert_eq!(parsed.compositor_args, args(&["--backend", "winit"]));
    }

    #[test]
    fn unknown_argument_starts_compositor_arguments() {
        let parsed = parse_args(args(&["--nested", "--verbose"])).expect("valid flags");
        assert_eq!(parsed.compositor_args, args(&["--nested", "--verbose"]));
    }

    #[test]
    fn flag_without_value_is_an_error() {
        let error = parse_args(args(&["--shell"])).expect_err("value is missing");
        assert_eq!(error, UsageError::MissingValue("--shell"));
    }

    #[test]
    fn session_environment_is_set_on_every_component() {
        let spec = with_session_env(ComponentSpec::new("x", "x", Criticality::Optional));
        assert!(spec.env.contains(&(
            OsString::from("XDG_SESSION_TYPE"),
            OsString::from("wayland")
        )));
        assert!(spec.env.contains(&(
            OsString::from("XDG_CURRENT_DESKTOP"),
            OsString::from("Desilium")
        )));
    }

    #[test]
    fn exit_codes_follow_the_outcome() {
        assert_eq!(exit_code(&SessionOutcome::Normal), ExitCode::SUCCESS);
        assert_eq!(exit_code(&SessionOutcome::Signalled), ExitCode::SUCCESS);
        assert_eq!(
            exit_code(&SessionOutcome::CompositorExited { code: None }),
            ExitCode::FAILURE
        );
    }
}
