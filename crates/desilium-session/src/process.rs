//! The OS boundary: spawning components and signalling their process groups.

use std::ffi::c_int;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};

use crate::component::{ComponentSpec, EnvVar};
use crate::policy::ExitKind;

/// Starts `program` with the arguments and environment from `spec`, plus `extra_env` from the session.
pub fn spawn(spec: &ComponentSpec, program: &Path, extra_env: &[EnvVar]) -> io::Result<Child> {
    Command::new(program)
        .args(&spec.args)
        .envs(spec.env.iter().map(|(key, value)| (key, value)))
        .envs(extra_env.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        // Own process group: terminal signals reach only the supervisor, which forwards shutdown itself.
        .process_group(0)
        .spawn()
}

/// Maps a finished child's status onto the restart policy's view of it.
pub fn exit_kind(status: ExitStatus) -> ExitKind {
    if status.success() {
        return ExitKind::Success;
    }
    ExitKind::Failure
}

/// Sends `signal` to the process group whose leader is `pid`.
pub fn signal_group(pid: u32, signal: c_int) -> io::Result<()> {
    let group = i32::try_from(pid).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // A group id of 0 would address the supervisor's own group, which must never be signalled here.
    if group <= 0 {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    // SAFETY: killpg only delivers a signal to existing processes and touches no memory of ours.
    let result = unsafe { libc::killpg(group, signal) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
