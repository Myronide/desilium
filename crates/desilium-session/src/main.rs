//! `desilium-session`: starts the compositor and helpers and supervises them until the session ends.

mod cli;

use std::env;
use std::ffi::OsString;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use desilium_session::startup::dbus::REEXEC_MARKER_VARIABLE;
use desilium_session::startup::render;
use desilium_session::{
    DbusAction, Environment, PlanError, PlanRequest, SessionError, SessionOutcome, SessionPlan,
    Supervisor, build_plan,
};
use tracing::{Level, info, warn};
use tracing_subscriber::EnvFilter;

use crate::cli::{DryRunFormat, Mode, USAGE, parse_args};

#[derive(Debug, thiserror::Error)]
enum MainError {
    #[error(transparent)]
    Usage(#[from] cli::UsageError),
    #[error(transparent)]
    Plan(#[from] PlanError),
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
    match cli.mode() {
        Mode::Help => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        Mode::PrintEnv => {
            print!("{}", render::render_exports(&resolve(&cli.request)?));
            Ok(ExitCode::SUCCESS)
        }
        Mode::DryRun(format) => {
            print_plan(&resolve(&cli.request)?, format);
            Ok(ExitCode::SUCCESS)
        }
        Mode::Run => run_session(&cli.request),
    }
}

/// Builds the plan from the real process environment and executable location.
fn resolve(request: &PlanRequest) -> Result<SessionPlan, PlanError> {
    build_plan(request, &Environment::from_process(), exe_dir().as_deref())
}

fn exe_dir() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
}

fn print_plan(plan: &SessionPlan, format: DryRunFormat) {
    match format {
        DryRunFormat::Text => print!("{}", render::render_text(plan)),
        DryRunFormat::Json => println!("{}", render::render_json(plan)),
    }
}

fn run_session(request: &PlanRequest) -> Result<ExitCode, MainError> {
    let plan = resolve(request)?;
    if let DbusAction::ReexecUnder { launcher } = &plan.dbus {
        reexec_under_bus(launcher);
    }
    if plan.dbus == DbusAction::Unavailable {
        warn!(
            "no D-Bus session bus and no dbus-run-session; portals and notifications will not work"
        );
    }
    for skipped in &plan.skipped {
        info!(
            component = skipped.name,
            binary = skipped.binary,
            "optional component not installed, skipping"
        );
    }
    let mut supervisor = Supervisor::new(plan.component_specs());
    if let Some(readiness) = plan.readiness() {
        supervisor = supervisor.with_wayland_readiness(readiness);
    }
    let outcome = supervisor.run()?;
    info!(outcome = ?outcome, "session ended");
    Ok(exit_code(&outcome))
}

/// Replaces this process with `dbus-run-session` running this same session. Returns only on failure.
fn reexec_under_bus(launcher: &Path) {
    let Some(exe) = env::current_exe().ok() else {
        warn!("cannot find own executable; continuing without a session bus");
        return;
    };
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let error = Command::new(launcher)
        .arg("--")
        .arg(exe)
        .args(args)
        .env(REEXEC_MARKER_VARIABLE, "1")
        .exec();
    warn!(error = %error, launcher = ?launcher, "re-exec under dbus-run-session failed; continuing without a session bus");
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
