//! Starts a [`LaunchCommand`] in its own process group and reaps it when it exits.

use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;

use tracing::{debug, warn};

use super::{LaunchCommand, LaunchError};

const WAYLAND_DISPLAY: &str = "WAYLAND_DISPLAY";
const ACTIVATION_TOKEN: &str = "XDG_ACTIVATION_TOKEN";
const STARTUP_ID: &str = "DESKTOP_STARTUP_ID";
/// Process group id meaning "use the child's pid", as `process_group(0)` requires.
const NEW_GROUP: i32 = 0;
const REAPER_THREAD: &str = "desilium-reap";

/// Starts `command` detached from the caller's process group and returns its pid.
///
/// `wayland_display` is set explicitly as `WAYLAND_DISPLAY` when given, so the child
/// talks to the right compositor; `None` leaves the inherited value alone. Stdin and
/// stdout are closed, stderr is inherited for diagnostics. A background thread waits
/// for the child so that no zombies accumulate.
pub fn spawn(command: &LaunchCommand, wayland_display: Option<&OsStr>) -> Result<u32, LaunchError> {
    if command.program.is_empty() {
        return Err(LaunchError::Empty);
    }
    let mut process = Command::new(&command.program);
    process
        .args(&command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .process_group(NEW_GROUP);
    apply_cwd(&mut process, command.cwd.as_deref());
    for (key, value) in &command.env {
        process.env(key, value);
    }
    if let Some(display) = wayland_display {
        process.env(WAYLAND_DISPLAY, display);
    }
    if let Some(token) = &command.activation_token {
        process.env(ACTIVATION_TOKEN, token);
    }
    if let Some(startup_id) = &command.startup_id {
        process.env(STARTUP_ID, startup_id);
    }
    let child = process.spawn().map_err(|source| LaunchError::Spawn {
        program: command.program.clone(),
        source,
    })?;
    let pid = child.id();
    reap(child);
    debug!(program = %command.program, pid, "started application");
    Ok(pid)
}

fn apply_cwd(process: &mut Command, cwd: Option<&Path>) {
    let Some(dir) = cwd else {
        return;
    };
    if dir.is_dir() {
        process.current_dir(dir);
    } else {
        warn!(dir = %dir.display(), "working directory is missing; starting without it");
    }
}

fn reap(mut child: Child) {
    let spawned = thread::Builder::new()
        .name(REAPER_THREAD.to_owned())
        .spawn(move || {
            let _ = child.wait();
        });
    if let Err(err) = spawned {
        warn!(%err, "cannot start reaper thread; child may linger as a zombie");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn empty_program_is_rejected() {
        let command = LaunchCommand::default();
        assert!(matches!(spawn(&command, None), Err(LaunchError::Empty)));
    }

    #[test]
    fn missing_program_reports_spawn_error() {
        let command = LaunchCommand::new("/nonexistent/desilium-test-binary", Vec::new());
        assert!(matches!(
            spawn(&command, None),
            Err(LaunchError::Spawn { .. })
        ));
    }

    #[test]
    fn child_receives_explicit_display_and_activation_env() {
        let dir = TempDir::new("spawn-env");
        let out = dir.path().join("env.txt");
        let script = format!(
            "printf '%s|%s|%s' \"$WAYLAND_DISPLAY\" \"$XDG_ACTIVATION_TOKEN\" \"$DESKTOP_STARTUP_ID\" > '{}'",
            out.display()
        );
        let command = LaunchCommand::new("sh", vec!["-c".into(), script])
            .with_activation(Some("tok".into()), Some("id-1".into()))
            .with_cwd(dir.path().to_path_buf());
        let pid = spawn(&command, Some(OsStr::new("wayland-test"))).expect("spawns");
        assert!(pid > 0);
        for _ in 0..200 {
            let text = std::fs::read_to_string(&out).unwrap_or_default();
            if !text.is_empty() {
                assert_eq!(text, "wayland-test|tok|id-1");
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("child did not write its environment in time");
    }
}
