//! Terminal emulator selection for `Terminal=true` entries and typed shell commands.

use crate::env::Environment;

/// Probe order for auto-detection, each with the flag that introduces its command.
/// `None` means the emulator takes the command as trailing arguments.
const CANDIDATES: [(&str, Option<&str>); 5] = [
    ("kitty", None),
    ("xdg-terminal-exec", None),
    ("foot", None),
    ("konsole", Some("-e")),
    ("alacritty", Some("-e")),
];
/// Flag used for a terminal named by the user.
const DEFAULT_EXEC_FLAG: &str = "-e";

/// How to run a command inside a terminal emulator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSpec {
    /// Terminal program, resolved through `PATH` at launch time.
    pub program: String,
    /// Flag placed before the command, if the emulator needs one.
    pub exec_flag: Option<String>,
}

impl TerminalSpec {
    /// The first installed emulator in the probe order, or `None` if none is installed.
    pub fn detect(env: &Environment) -> Option<Self> {
        CANDIDATES
            .iter()
            .find(|(program, _)| env.find_program(program).is_some())
            .map(|(program, flag)| Self {
                program: (*program).to_owned(),
                exec_flag: flag.map(str::to_owned),
            })
    }

    /// A user-chosen emulator, assumed to accept `-e` before the command.
    pub fn custom(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            exec_flag: Some(DEFAULT_EXEC_FLAG.to_owned()),
        }
    }

    /// Prefixes `words` with the emulator and its flag, yielding a new argv.
    pub fn wrap(&self, words: Vec<String>) -> Vec<String> {
        let mut argv = vec![self.program.clone()];
        argv.extend(self.exec_flag.clone());
        argv.extend(words);
        argv
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, env_with_data_dirs};
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn wrap_inserts_flag_only_when_needed() {
        let konsole = TerminalSpec {
            program: "konsole".into(),
            exec_flag: Some("-e".into()),
        };
        let foot = TerminalSpec {
            program: "foot".into(),
            exec_flag: None,
        };
        let words = vec!["htop".to_owned()];
        assert_eq!(konsole.wrap(words.clone()), vec!["konsole", "-e", "htop"]);
        assert_eq!(foot.wrap(words), vec!["foot", "htop"]);
    }

    #[test]
    fn detect_follows_probe_order() {
        let dir = TempDir::new("terminal");
        for name in ["kitty", "foot", "xdg-terminal-exec", "alacritty"] {
            let path = dir.write(name, "#!/bin/sh\n");
            let _ = std::fs::set_permissions(&path, Permissions::from_mode(0o755));
        }
        let mut env = env_with_data_dirs(&[]);
        env.path_dirs = vec![dir.path().to_path_buf()];
        let found = TerminalSpec::detect(&env).expect("a terminal is installed");
        assert_eq!(found.program, "kitty");
        assert_eq!(found.exec_flag, None);
    }

    #[test]
    fn detect_returns_none_without_terminals() {
        let env = env_with_data_dirs(&[]);
        assert_eq!(TerminalSpec::detect(&env), None);
    }

    #[test]
    fn custom_terminal_uses_dash_e() {
        assert_eq!(
            TerminalSpec::custom("wezterm").wrap(vec!["x".into()]),
            vec!["wezterm", "-e", "x"]
        );
    }
}
