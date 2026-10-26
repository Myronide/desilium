//! Turns desktop entries and typed commands into process invocations, and starts them.
//!
//! Building a [`LaunchCommand`] is pure and testable. Only [`spawn`] touches processes.
//! No shell is involved unless the entry's own Exec line runs one, or the user typed a
//! `> line` command, which is an explicit request to run it through `sh -c`.

pub(crate) mod expand;
mod spawn;
mod terminal;

use std::path::PathBuf;

use crate::desktop::DesktopEntry;

pub use spawn::spawn;
pub use terminal::TerminalSpec;

/// A fully resolved process invocation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LaunchCommand {
    /// Program to execute; looked up through `PATH` when it has no slash.
    pub program: String,
    /// Arguments after the program, never re-split by a shell.
    pub args: Vec<String>,
    /// Working directory; ignored by [`spawn`] when it is not an existing directory.
    pub cwd: Option<PathBuf>,
    /// Extra environment variables for the child.
    pub env: Vec<(String, String)>,
    /// `XDG_ACTIVATION_TOKEN` to hand to the child for focus stealing prevention.
    pub activation_token: Option<String>,
    /// `DESKTOP_STARTUP_ID` for startup notification.
    pub startup_id: Option<String>,
}

impl LaunchCommand {
    /// A command with no working directory, extra environment or activation data.
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            ..Self::default()
        }
    }

    /// Sets the working directory.
    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }

    /// Sets the activation token and startup id the compositor or launcher provides.
    pub fn with_activation(mut self, token: Option<String>, startup_id: Option<String>) -> Self {
        self.activation_token = token;
        self.startup_id = startup_id;
        self
    }
}

/// Files and URLs the user opened with an application, substituted for `%f %F %u %U`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Documents {
    /// Local file paths.
    pub files: Vec<String>,
    /// URLs.
    pub urls: Vec<String>,
}

/// Why a command could not be built or started.
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    /// The entry (or action) has no Exec line.
    #[error("entry has no Exec line")]
    NoExec,
    /// The requested desktop action does not exist.
    #[error("entry has no action named {0}")]
    UnknownAction(String),
    /// The Exec line produced no program.
    #[error("command is empty")]
    Empty,
    /// A double quote was never closed.
    #[error("unterminated quote in Exec line")]
    UnterminatedQuote,
    /// A `%` is followed by something that is not a valid field code.
    #[error("invalid field code in Exec line: {0}")]
    FieldCode(String),
    /// The command needs a terminal and none is installed.
    #[error("command needs a terminal emulator but none is installed")]
    NoTerminal,
    /// The process could not be started.
    #[error("failed to start {program}: {source}")]
    Spawn {
        /// Program that failed to start.
        program: String,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },
}

/// Builds the command for an entry's main action (`action` is `None`) or a named action.
///
/// `terminal` is required only when the entry sets `Terminal=true`.
pub fn for_entry(
    entry: &DesktopEntry,
    action: Option<&str>,
    docs: &Documents,
    terminal: Option<&TerminalSpec>,
) -> Result<LaunchCommand, LaunchError> {
    let exec = match action {
        None => entry.exec.as_str(),
        Some(id) => entry
            .action(id)
            .map(|action| action.exec.as_str())
            .ok_or_else(|| LaunchError::UnknownAction(id.to_owned()))?,
    };
    let location = entry.path.to_string_lossy();
    let context = expand::Context {
        name: &entry.name,
        icon: entry.icon.as_deref(),
        location: &location,
        docs,
    };
    let mut argv = expand::expand(exec, &context)?;
    if entry.terminal {
        let emulator = terminal.ok_or(LaunchError::NoTerminal)?;
        argv = emulator.wrap(argv);
    }
    let command = into_command(argv)?;
    Ok(match &entry.working_dir {
        Some(dir) => command.with_cwd(dir.clone()),
        None => command,
    })
}

/// Builds the command for text typed into the launcher: split into words without field
/// code expansion, optionally run inside `terminal`. No shell is used.
pub fn user_command(
    text: &str,
    terminal: Option<&TerminalSpec>,
    in_terminal: bool,
) -> Result<LaunchCommand, LaunchError> {
    let mut argv = expand::tokenize(text)?;
    if in_terminal {
        let emulator = terminal.ok_or(LaunchError::NoTerminal)?;
        argv = emulator.wrap(argv);
    }
    into_command(argv)
}

/// Runs `line` through `sh -c` inside `terminal`. Only for an explicit `> line` request.
pub fn shell_command(line: &str, terminal: &TerminalSpec) -> LaunchCommand {
    let argv = terminal.wrap(vec!["sh".to_owned(), "-c".to_owned(), line.to_owned()]);
    LaunchCommand::new(
        argv.first().cloned().unwrap_or_default(),
        argv.into_iter().skip(1).collect(),
    )
}

fn into_command(argv: Vec<String>) -> Result<LaunchCommand, LaunchError> {
    let mut words = argv.into_iter();
    let program = words.next().ok_or(LaunchError::Empty)?;
    Ok(LaunchCommand::new(program, words.collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::{DesktopEntry, DesktopId, ini};
    use std::path::Path;

    fn entry(text: &str) -> DesktopEntry {
        let file = ini::parse(text).expect("valid ini");
        let id = DesktopId::new("demo.desktop").expect("valid id");
        DesktopEntry::from_ini(id, PathBuf::from("/apps/demo.desktop"), &file, &[])
            .expect("valid entry")
    }

    fn konsole() -> TerminalSpec {
        TerminalSpec {
            program: "konsole".into(),
            exec_flag: Some("-e".into()),
        }
    }

    #[test]
    fn entry_command_expands_fields_and_keeps_working_directory() {
        let demo = entry(
            "[Desktop Entry]\nType=Application\nName=Demo\nExec=demo --open %F\nPath=/work\n",
        );
        let docs = Documents {
            files: vec!["/a b".into(), "/c".into()],
            urls: vec![],
        };
        let command = for_entry(&demo, None, &docs, None).expect("builds");
        assert_eq!(command.program, "demo");
        assert_eq!(command.args, vec!["--open", "/a b", "/c"]);
        assert_eq!(command.cwd.as_deref(), Some(Path::new("/work")));
    }

    #[test]
    fn terminal_entries_are_wrapped_or_rejected() {
        let text = "[Desktop Entry]\nType=Application\nName=Top\nExec=htop\nTerminal=true\n";
        let top = entry(text);
        let command =
            for_entry(&top, None, &Documents::default(), Some(&konsole())).expect("builds");
        assert_eq!(command.program, "konsole");
        assert_eq!(command.args, vec!["-e", "htop"]);
        assert!(matches!(
            for_entry(&top, None, &Documents::default(), None),
            Err(LaunchError::NoTerminal)
        ));
    }

    #[test]
    fn named_actions_are_selected() {
        let demo = entry(
            "[Desktop Entry]\nType=Application\nName=Demo\nExec=demo\nActions=win;\n\
                          [Desktop Action win]\nName=Window\nExec=demo --window\n",
        );
        let command = for_entry(&demo, Some("win"), &Documents::default(), None).expect("builds");
        assert_eq!(command.args, vec!["--window"]);
        assert!(matches!(
            for_entry(&demo, Some("nope"), &Documents::default(), None),
            Err(LaunchError::UnknownAction(_))
        ));
    }

    #[test]
    fn user_text_is_split_without_a_shell() {
        let command = user_command("ls \"my dir\" $HOME", None, false).expect("builds");
        assert_eq!(command.program, "ls");
        assert_eq!(command.args, vec!["my dir", "$HOME"]);
        let wrapped = user_command("htop", Some(&konsole()), true).expect("builds");
        assert_eq!(wrapped.program, "konsole");
        assert_eq!(wrapped.args, vec!["-e", "htop"]);
        assert!(matches!(
            user_command("   ", None, false),
            Err(LaunchError::Empty)
        ));
    }

    #[test]
    fn shell_command_is_explicit_sh_dash_c() {
        let command = shell_command("echo hi | wc -c", &konsole());
        assert_eq!(command.program, "konsole");
        assert_eq!(command.args, vec!["-e", "sh", "-c", "echo hi | wc -c"]);
    }
}
