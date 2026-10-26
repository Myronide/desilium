use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use tracing::{info, warn};

const TERMINAL_CANDIDATES: [&str; 5] = ["foot", "kitty", "konsole", "alacritty", "weston-terminal"];

/// The configured terminal, or the first known emulator found on `PATH`.
pub fn find_terminal(configured: Option<&str>) -> Option<String> {
    if let Some(program) = configured {
        return Some(program.to_owned());
    }
    TERMINAL_CANDIDATES
        .iter()
        .find(|program| is_on_path(program))
        .map(|program| (*program).to_owned())
}

fn is_on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| is_executable(&dir.join(program)))
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Starts a client that talks to this compositor, never to the host session.
pub fn spawn(program: &str, socket_name: &OsStr) {
    let words = split_words(program);
    let mut parts = words.iter();
    let Some(binary) = parts.next() else {
        return;
    };
    let mut command = Command::new(binary);
    command
        .args(parts)
        .env("WAYLAND_DISPLAY", socket_name)
        .env("XDG_SESSION_TYPE", "wayland")
        .env_remove("DISPLAY");
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            warn!(%program, %error, "cannot start client");
            return;
        }
    };
    info!(%program, pid = child.id(), "client started");
    // A small reaper thread keeps exited clients from lingering as zombies.
    std::thread::spawn(move || child.wait());
}

/// Splits a command line into words, honouring single and double quotes.
fn split_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    for ch in line.chars() {
        match (quote, ch) {
            (Some(open), _) if ch == open => quote = None,
            (Some(_), _) => current.push(ch),
            (None, '\'' | '"') => {
                quote = Some(ch);
                in_word = true;
            }
            (None, _) if ch.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            (None, _) => {
                current.push(ch);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::split_words;

    #[test]
    fn splits_plain_words() {
        assert_eq!(split_words("foot  -e  htop"), ["foot", "-e", "htop"]);
    }

    #[test]
    fn keeps_quoted_groups_together() {
        assert_eq!(
            split_words("foot -e sh -c 'echo hi; sleep 5'"),
            ["foot", "-e", "sh", "-c", "echo hi; sleep 5"]
        );
        assert_eq!(split_words(r#"a "b c" d"#), ["a", "b c", "d"]);
    }

    #[test]
    fn empty_quotes_make_an_empty_word() {
        assert_eq!(split_words("a '' b"), ["a", "", "b"]);
        assert!(split_words("   ").is_empty());
    }
}
