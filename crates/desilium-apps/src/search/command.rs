//! Interprets launcher input that is not an application: `> line` shell commands and
//! plain text offered as a program to run in a terminal.

/// What the launcher should offer for free-form input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// `> line`: run through `sh -c`, because the user asked for a shell command.
    Shell(String),
    /// Anything else: split into words (no shell) and run in a terminal, if chosen.
    RunInTerminal(String),
}

/// Classifies free-form input. Blank input and a bare `>` yield `None`.
pub fn detect(input: &str) -> Option<Intent> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.strip_prefix('>') {
        Some(rest) => {
            let line = rest.trim();
            (!line.is_empty()).then(|| Intent::Shell(line.to_owned()))
        }
        None => Some(Intent::RunInTerminal(trimmed.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_table() {
        let cases: &[(&str, Option<Intent>)] = &[
            ("> ls -la", Some(Intent::Shell("ls -la".into()))),
            (">ls", Some(Intent::Shell("ls".into()))),
            (">   ", None),
            ("   ", None),
            ("htop", Some(Intent::RunInTerminal("htop".into()))),
            (
                "  make build  ",
                Some(Intent::RunInTerminal("make build".into())),
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(detect(input), *expected, "input {input:?}");
        }
    }
}
