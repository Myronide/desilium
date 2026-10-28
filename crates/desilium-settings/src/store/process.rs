//! Running the external tools (`curl`, `tar`, `unzip`) with fixed arguments and hard limits.
//!
//! No shell is involved; every argument is a separate `OsStr`. A run ends when the tool exits,
//! when it overruns its time, when it writes more than the allowed output, or when a watched
//! file grows past its limit, and the tool is killed in the last three cases.

use std::{
    ffi::OsStr,
    fs,
    io::{self, Read},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use super::StoreError;

const POLL: Duration = Duration::from_millis(25);
/// How much of a tool's error output is kept for messages.
const STDERR_LIMIT: u64 = 2048;

/// Limits of one run.
pub(super) struct Limits {
    /// Longest run.
    pub timeout: Duration,
    /// Most bytes of standard output kept; more ends the run.
    pub max_stdout: u64,
    /// A file that must not grow beyond the given size while the tool runs.
    pub watch: Option<(PathBuf, u64)>,
}

impl Limits {
    /// A run that only needs its exit status and a little text.
    pub(super) fn quick(timeout: Duration) -> Self {
        Self {
            timeout,
            max_stdout: 64 * 1024,
            watch: None,
        }
    }
}

/// What a finished tool left.
pub(super) struct Finished {
    /// Whether it exited with status 0.
    pub success: bool,
    /// Standard output.
    pub stdout: Vec<u8>,
    /// The start of its error output, for messages.
    pub stderr: String,
}

fn spawn_error(program: &'static str, error: io::Error) -> StoreError {
    if error.kind() == io::ErrorKind::NotFound {
        StoreError::MissingTool(program)
    } else {
        StoreError::Io(error)
    }
}

/// Runs `program` with `args` under `limits`.
pub(super) fn run(
    program: &'static str,
    args: &[&OsStr],
    limits: &Limits,
) -> Result<Finished, StoreError> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| spawn_error(program, error))?;
    let (Some(mut out), Some(mut err)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        return Err(StoreError::Io(io::Error::other("the tool has no pipes")));
    };
    let max = limits.max_stdout;
    let overflow = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&overflow);
    let out_thread = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = out.by_ref().take(max + 1).read_to_end(&mut buffer);
        if buffer.len() as u64 > max {
            flag.store(true, Ordering::SeqCst);
        }
        // After an overflow the tool is killed; otherwise the pipe is at its end anyway.
        buffer
    });
    let err_thread = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = err.by_ref().take(STDERR_LIMIT).read_to_end(&mut buffer);
        // Keep emptying the pipe so that the tool never blocks on it.
        let _ = io::copy(&mut err, &mut io::sink());
        buffer
    });
    let started = Instant::now();
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status.success()),
            Ok(None) => {}
            Err(error) => break Err(StoreError::Io(error)),
        }
        let too_slow = started.elapsed() > limits.timeout;
        let too_much_output = overflow.load(Ordering::SeqCst);
        let too_big = limits
            .watch
            .as_ref()
            .is_some_and(|(path, limit)| fs::metadata(path).is_ok_and(|meta| meta.len() > *limit));
        if too_slow || too_much_output || too_big {
            let _ = child.kill();
            let _ = child.wait();
            break Err(if too_slow {
                StoreError::Network("timed out".to_owned())
            } else {
                StoreError::TooLarge
            });
        }
        thread::sleep(POLL);
    };
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    let success = outcome?;
    if stdout.len() as u64 > max {
        return Err(StoreError::TooLarge);
    }
    Ok(Finished {
        success,
        stdout,
        stderr: String::from_utf8_lossy(&stderr).trim().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick() -> Limits {
        Limits::quick(Duration::from_secs(10))
    }

    #[test]
    fn output_and_status_are_captured() {
        let done = run("echo", &[OsStr::new("hello")], &quick()).expect("echo");
        assert!(done.success);
        assert_eq!(done.stdout, b"hello\n");
        let failed = run("false", &[], &quick()).expect("false");
        assert!(!failed.success);
    }

    #[test]
    fn a_missing_tool_is_reported_by_name() {
        let error = run("desilium-no-such-tool", &[], &quick())
            .err()
            .expect("error");
        assert!(matches!(
            error,
            StoreError::MissingTool("desilium-no-such-tool")
        ));
    }

    #[test]
    fn a_slow_tool_is_killed() {
        let limits = Limits::quick(Duration::from_millis(150));
        let error = run("sleep", &[OsStr::new("5")], &limits)
            .err()
            .expect("timeout");
        assert!(matches!(error, StoreError::Network(_)), "{error:?}");
    }

    #[test]
    fn too_much_output_ends_the_run() {
        let limits = Limits {
            timeout: Duration::from_secs(10),
            max_stdout: 100,
            watch: None,
        };
        let error = run("yes", &[], &limits).err().expect("overflow");
        assert!(matches!(error, StoreError::TooLarge), "{error:?}");
    }
}
