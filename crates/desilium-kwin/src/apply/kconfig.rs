//! Writes KDE configuration keys with `kwriteconfig6`.
//!
//! The program is started with a fixed argument vector, never through a shell, and is killed when
//! it takes longer than [`TIMEOUT`]. `kwriteconfig6` is the supported writer for KConfig files:
//! it keeps the file format, escaping and `[$i]` immutability rules that a hand-written INI
//! writer would have to repeat.

use std::ffi::OsString;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::KwinError;
use crate::plan::{ConfigEntry, Value};

/// Program name looked up on `PATH`.
pub const PROGRAM: &str = "kwriteconfig6";
/// Longest a single write may take.
const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(10);
/// How much of the error output goes into an error message.
const STDERR_LIMIT: usize = 300;

/// Writer for KConfig files.
#[derive(Debug, Clone)]
pub struct KConfigWriter {
    program: OsString,
    /// Arguments placed before the entry's own; only tests use them, to run a script through `sh`.
    leading: Vec<OsString>,
}

impl Default for KConfigWriter {
    fn default() -> Self {
        Self::with_program(PROGRAM)
    }
}

impl KConfigWriter {
    /// A writer that runs `program`; tests use a stand-in script.
    pub fn with_program(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            leading: Vec::new(),
        }
    }

    /// Runs `script` through `sh`, which avoids "text file busy" when tests write and run
    /// scripts from parallel threads.
    #[cfg(test)]
    pub(crate) fn with_script(script: impl Into<OsString>) -> Self {
        Self {
            program: OsString::from("sh"),
            leading: vec![script.into()],
        }
    }

    /// Writes one entry and asks KDE applications to re-read the file.
    ///
    /// `--notify` is tried first. When this `kwriteconfig6` does not know the option, the entry is
    /// written without it, so a missing notification never loses a setting.
    pub fn write(&self, entry: &ConfigEntry) -> Result<(), KwinError> {
        match self.run(&argv(entry, true)) {
            Err(KwinError::ConfigWrite { .. }) => self.run(&argv(entry, false)),
            other => other,
        }
    }

    fn run(&self, args: &[String]) -> Result<(), KwinError> {
        let program = self.program.to_string_lossy().into_owned();
        let mut child = Command::new(&self.program)
            .args(&self.leading)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| KwinError::Spawn {
                program: program.clone(),
                source,
            })?;
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => {
                    return Err(KwinError::ConfigWrite {
                        program,
                        status: status.to_string(),
                        stderr: read_stderr(&mut child),
                    });
                }
                Ok(None) if started.elapsed() >= TIMEOUT => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(KwinError::Timeout {
                        program,
                        seconds: TIMEOUT.as_secs(),
                    });
                }
                Ok(None) => thread::sleep(POLL),
                Err(source) => return Err(KwinError::Spawn { program, source }),
            }
        }
    }
}

fn read_stderr(child: &mut std::process::Child) -> String {
    let mut text = String::new();
    if let Some(stderr) = child.stderr.as_mut() {
        let _ = stderr.take(STDERR_LIMIT as u64).read_to_string(&mut text);
    }
    text.trim().to_owned()
}

/// The arguments for writing `entry`.
pub fn argv(entry: &ConfigEntry, notify: bool) -> Vec<String> {
    let mut args = vec![
        "--file".to_owned(),
        entry.id.file.to_owned(),
        "--group".to_owned(),
        entry.id.group.clone(),
        "--key".to_owned(),
        entry.id.key.clone(),
    ];
    if matches!(entry.value, Value::Bool(_)) {
        args.extend(["--type".to_owned(), "bool".to_owned()]);
    }
    if notify {
        args.push("--notify".to_owned());
    }
    let value = entry.value.render();
    if value.starts_with('-') {
        // The command line parser would take the value for an option.
        args.push("--".to_owned());
    }
    args.push(value.to_owned());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
        let path = dir.join("kwriteconfig-stand-in");
        std::fs::write(&path, format!("{body}\n")).expect("write script");
        path
    }

    #[test]
    fn text_entries_carry_file_group_key_and_value() {
        let entry = ConfigEntry::text("kwinrc", "Windows", "FocusPolicy", "ClickToFocus");
        assert_eq!(
            argv(&entry, false),
            [
                "--file",
                "kwinrc",
                "--group",
                "Windows",
                "--key",
                "FocusPolicy",
                "ClickToFocus"
            ]
        );
    }

    #[test]
    fn booleans_are_typed_and_notification_comes_before_the_value() {
        let entry = ConfigEntry::flag("kwinrc", "Plugins", "blurEnabled", true);
        assert_eq!(
            argv(&entry, true),
            [
                "--file",
                "kwinrc",
                "--group",
                "Plugins",
                "--key",
                "blurEnabled",
                "--type",
                "bool",
                "--notify",
                "true"
            ]
        );
    }

    #[test]
    fn a_value_that_looks_like_an_option_stays_one_argument() {
        let entry = ConfigEntry::text("kdeglobals", "General", "Name", "--delete");
        let args = argv(&entry, false);
        assert_eq!(args.last().map(String::as_str), Some("--delete"));
        assert_eq!(args[args.len() - 2], "--", "options end before the value");
    }

    #[test]
    fn success_is_ok() {
        let dir = tempfile::tempdir().expect("temp dir");
        let writer = KConfigWriter::with_script(script(dir.path(), "exit 0"));
        let entry = ConfigEntry::text("kwinrc", "Windows", "FocusPolicy", "ClickToFocus");
        assert!(writer.write(&entry).is_ok());
    }

    #[test]
    fn unknown_notify_option_falls_back_to_a_plain_write() {
        let dir = tempfile::tempdir().expect("temp dir");
        let body = "for a in \"$@\"; do [ \"$a\" = --notify ] && { echo 'unknown option' >&2; exit 1; }; done; exit 0";
        let writer = KConfigWriter::with_script(script(dir.path(), body));
        let entry = ConfigEntry::text("kwinrc", "Windows", "FocusPolicy", "ClickToFocus");
        assert!(writer.write(&entry).is_ok());
    }

    #[test]
    fn failure_reports_status_and_error_output() {
        let dir = tempfile::tempdir().expect("temp dir");
        let writer = KConfigWriter::with_script(script(dir.path(), "echo broken >&2; exit 3"));
        let entry = ConfigEntry::text("kwinrc", "Windows", "FocusPolicy", "ClickToFocus");
        match writer.write(&entry) {
            Err(KwinError::ConfigWrite { stderr, .. }) => assert_eq!(stderr, "broken"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_missing_program_is_a_spawn_error() {
        let writer = KConfigWriter::with_program("/nonexistent/kwriteconfig6");
        let entry = ConfigEntry::text("kwinrc", "Windows", "FocusPolicy", "ClickToFocus");
        assert!(matches!(writer.write(&entry), Err(KwinError::Spawn { .. })));
    }
}
