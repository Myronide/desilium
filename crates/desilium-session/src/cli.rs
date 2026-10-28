//! Command-line parsing for `desilium-session`. Pure: the caller supplies the arguments.

use std::ffi::OsString;
use std::path::PathBuf;

use desilium_session::PlanRequest;

/// How the binary should behave after parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Start and supervise the session.
    Run,
    /// Print the resolved plan and exit without starting anything.
    DryRun(DryRunFormat),
    /// Print the shell exports of the session environment and exit.
    PrintEnv,
    /// Print usage and exit.
    Help,
}

/// Output format of `--dry-run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DryRunFormat {
    /// Readable text.
    Text,
    /// One JSON document.
    Json,
}

/// Everything the command line controls.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CliArgs {
    /// Behaviour selected by the flags.
    pub mode: Option<Mode>,
    /// Component overrides and compositor arguments.
    pub request: PlanRequest,
}

impl CliArgs {
    /// The selected mode; running the session is the default.
    pub fn mode(&self) -> Mode {
        self.mode.unwrap_or(Mode::Run)
    }
}

/// Usage errors on the command line.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UsageError {
    /// A flag that needs a value was the last argument.
    #[error("`{0}` requires a value")]
    MissingValue(&'static str),
    /// `--dry-run=` with a format other than `text` or `json`.
    #[error("unknown dry-run format `{0}` (use text or json)")]
    UnknownFormat(String),
}

/// Usage text printed for `--help` and after usage errors.
pub const USAGE: &str = "usage: desilium-session [--dry-run[=text|json]] [--print-env] [--allow-any-client]\n\
                         \x20                      [--compositor <kwin_wayland path>] [--shell <path>]\n\
                         \x20                      [--kwin-bridge <path>] [--notifications <path>]\n\
                         \x20                      [--] [kwin_wayland args...]";

const DRY_RUN_PREFIX: &str = "--dry-run=";

/// Parses the arguments after the program name.
///
/// The first argument that is not a session flag starts the compositor's own arguments.
pub fn parse_args<I>(args: I) -> Result<CliArgs, UsageError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut parsed = CliArgs::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let flag = arg.to_string_lossy().into_owned();
        if flag == "--" {
            parsed.request.compositor_args.extend(args);
            break;
        }
        if !is_session_flag(&flag) {
            parsed.request.compositor_args.push(arg);
            parsed.request.compositor_args.extend(args);
            break;
        }
        apply_flag(&mut parsed, &flag, &mut args)?;
    }
    Ok(parsed)
}

fn is_session_flag(flag: &str) -> bool {
    matches!(
        flag,
        "--compositor"
            | "--shell"
            | "--kwin-bridge"
            | "--allow-any-client"
            | "--notifications"
            | "--dry-run"
            | "--print-env"
            | "-h"
            | "--help"
    ) || flag.starts_with(DRY_RUN_PREFIX)
}

fn apply_flag(
    parsed: &mut CliArgs,
    flag: &str,
    args: &mut impl Iterator<Item = OsString>,
) -> Result<(), UsageError> {
    match flag {
        "--compositor" => parsed.request.compositor = Some(path_value(args, "--compositor")?),
        "--shell" => parsed.request.shell = Some(path_value(args, "--shell")?),
        "--kwin-bridge" => parsed.request.kwin_bridge = Some(path_value(args, "--kwin-bridge")?),
        "--allow-any-client" => parsed.request.allow_any_client = true,
        "--notifications" => {
            parsed.request.notifications = Some(path_value(args, "--notifications")?);
        }
        "--dry-run" => parsed.mode = Some(Mode::DryRun(DryRunFormat::Text)),
        "--print-env" => parsed.mode = Some(Mode::PrintEnv),
        "-h" | "--help" => parsed.mode = Some(Mode::Help),
        other => parsed.mode = Some(Mode::DryRun(parse_format(other)?)),
    }
    Ok(())
}

fn path_value(
    args: &mut impl Iterator<Item = OsString>,
    flag: &'static str,
) -> Result<PathBuf, UsageError> {
    args.next()
        .map(PathBuf::from)
        .ok_or(UsageError::MissingValue(flag))
}

fn parse_format(flag: &str) -> Result<DryRunFormat, UsageError> {
    let value = flag.strip_prefix(DRY_RUN_PREFIX).unwrap_or_default();
    match value {
        "text" => Ok(DryRunFormat::Text),
        "json" => Ok(DryRunFormat::Json),
        other => Err(UsageError::UnknownFormat(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn empty_command_line_runs_the_session_with_defaults() {
        let parsed = parse_args(args(&[])).expect("no flags is valid");
        assert_eq!(parsed.mode(), Mode::Run);
        assert_eq!(parsed.request, PlanRequest::default());
    }

    #[test]
    fn component_paths_and_separator_are_parsed() {
        let parsed = parse_args(args(&[
            "--compositor",
            "/bin/comp",
            "--shell",
            "/bin/shell",
            "--notifications",
            "/bin/notify",
            "--kwin-bridge",
            "/bin/bridge",
            "--allow-any-client",
            "--",
            "--virtual",
            "--width",
            "800",
        ]))
        .expect("valid flags");
        assert_eq!(parsed.request.compositor, Some(PathBuf::from("/bin/comp")));
        assert_eq!(parsed.request.shell, Some(PathBuf::from("/bin/shell")));
        assert_eq!(
            parsed.request.kwin_bridge,
            Some(PathBuf::from("/bin/bridge"))
        );
        assert!(parsed.request.allow_any_client);
        assert_eq!(
            parsed.request.notifications,
            Some(PathBuf::from("/bin/notify"))
        );
        assert_eq!(
            parsed.request.compositor_args,
            args(&["--virtual", "--width", "800"])
        );
    }

    #[test]
    fn unknown_argument_starts_compositor_arguments() {
        let parsed = parse_args(args(&["--virtual", "--dry-run"])).expect("valid flags");
        assert_eq!(
            parsed.request.compositor_args,
            args(&["--virtual", "--dry-run"])
        );
        assert_eq!(parsed.mode, None);
    }

    #[test]
    fn dry_run_defaults_to_text_and_accepts_json() {
        let text = parse_args(args(&["--dry-run"])).expect("valid flag");
        assert_eq!(text.mode(), Mode::DryRun(DryRunFormat::Text));
        let json = parse_args(args(&["--dry-run=json"])).expect("valid flag");
        assert_eq!(json.mode(), Mode::DryRun(DryRunFormat::Json));
    }

    #[test]
    fn unknown_dry_run_format_is_a_usage_error() {
        let error = parse_args(args(&["--dry-run=yaml"])).expect_err("format is unknown");
        assert_eq!(error, UsageError::UnknownFormat("yaml".to_owned()));
    }

    #[test]
    fn print_env_and_help_select_their_modes() {
        assert_eq!(
            parse_args(args(&["--print-env"])).expect("valid").mode(),
            Mode::PrintEnv
        );
        assert_eq!(parse_args(args(&["-h"])).expect("valid").mode(), Mode::Help);
    }

    #[test]
    fn flag_without_value_is_an_error() {
        let error = parse_args(args(&["--shell"])).expect_err("value is missing");
        assert_eq!(error, UsageError::MissingValue("--shell"));
    }
}
