//! Exec tokenizer and field-code expansion, following the Desktop Entry spec.

use super::{Documents, LaunchError};

/// Values substituted for field codes.
pub(crate) struct Context<'a> {
    /// Localized application name for `%c`.
    pub(crate) name: &'a str,
    /// Icon name for `%i`.
    pub(crate) icon: Option<&'a str>,
    /// Location of the desktop file for `%k`.
    pub(crate) location: &'a str,
    /// Files and URLs for the file and URL field codes.
    pub(crate) docs: &'a Documents,
}

/// A whitespace-separated word after quote removal.
#[derive(Debug, Default)]
struct Word {
    text: String,
    quoted: bool,
}

/// Splits an Exec line into words with quotes removed. Used for user-typed commands,
/// where no field codes are expanded.
pub(crate) fn tokenize(exec: &str) -> Result<Vec<String>, LaunchError> {
    Ok(words(exec)?.into_iter().map(|word| word.text).collect())
}

/// Program name of an Exec line, the first word after quote removal.
pub(crate) fn program_of(exec: &str) -> Option<String> {
    words(exec).ok()?.into_iter().next().map(|word| word.text)
}

/// Expands an Exec line into argv: tokenizes, then replaces field codes.
pub(crate) fn expand(exec: &str, context: &Context<'_>) -> Result<Vec<String>, LaunchError> {
    let mut argv = Vec::new();
    for word in words(exec)? {
        let special = if word.quoted {
            None
        } else {
            standalone(&word.text, context)
        };
        if let Some(values) = special {
            argv.extend(values);
            continue;
        }
        argv.push(embedded(&word.text, context)?);
    }
    Ok(argv)
}

/// Field codes that form a whole argument of their own and may expand to several.
fn standalone(text: &str, context: &Context<'_>) -> Option<Vec<String>> {
    let docs = context.docs;
    match text {
        "%f" => Some(docs.files.first().cloned().into_iter().collect()),
        "%F" => Some(docs.files.clone()),
        "%u" => Some(docs.urls.first().cloned().into_iter().collect()),
        "%U" => Some(docs.urls.clone()),
        "%i" => Some(
            context
                .icon
                .map(|icon| vec!["--icon".to_owned(), icon.to_owned()])
                .unwrap_or_default(),
        ),
        _ => None,
    }
}

/// Expands field codes inside a word. File and URL codes vanish here, because they only
/// carry meaning as whole arguments.
fn embedded(text: &str, context: &Context<'_>) -> Result<String, LaunchError> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let Some(code) = chars.next() else {
            return Err(LaunchError::FieldCode("%".to_owned()));
        };
        match code {
            '%' => out.push('%'),
            'c' => out.push_str(context.name),
            'k' => out.push_str(context.location),
            'f' | 'F' | 'u' | 'U' | 'i' | 'd' | 'D' | 'n' | 'N' | 'v' | 'm' => {}
            other => return Err(LaunchError::FieldCode(format!("%{other}"))),
        }
    }
    Ok(out)
}

/// Splits an Exec line into words. Double quotes group text and allow `\" \` \$ \\`
/// escapes; an unquoted backslash and other reserved characters are taken literally.
fn words(exec: &str) -> Result<Vec<Word>, LaunchError> {
    let mut words: Vec<Word> = Vec::new();
    let mut current: Option<Word> = None;
    let mut chars = exec.chars().peekable();
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => in_quotes = false,
                '\\' => match chars.peek().copied() {
                    Some(escaped @ ('"' | '`' | '$' | '\\')) => {
                        chars.next();
                        push(&mut current, escaped);
                    }
                    _ => push(&mut current, '\\'),
                },
                other => push(&mut current, other),
            }
            continue;
        }
        match c {
            ' ' | '\t' => finish(&mut current, &mut words),
            '"' => {
                in_quotes = true;
                current.get_or_insert_with(Word::default).quoted = true;
            }
            other => push(&mut current, other),
        }
    }
    if in_quotes {
        return Err(LaunchError::UnterminatedQuote);
    }
    finish(&mut current, &mut words);
    Ok(words)
}

fn push(current: &mut Option<Word>, c: char) {
    current.get_or_insert_with(Word::default).text.push(c);
}

fn finish(current: &mut Option<Word>, words: &mut Vec<Word>) {
    if let Some(word) = current.take() {
        words.push(word);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(exec: &str, docs: &Documents) -> Result<Vec<String>, LaunchError> {
        let context = Context {
            name: "Demo App",
            icon: Some("demo-icon"),
            location: "/apps/demo.desktop",
            docs,
        };
        expand(exec, &context)
    }

    fn files(list: &[&str]) -> Documents {
        Documents {
            files: list.iter().map(|f| (*f).to_owned()).collect(),
            urls: vec![
                "https://example.org/a".to_owned(),
                "https://example.org/b".to_owned(),
            ],
        }
    }

    #[test]
    fn expansion_table() {
        let none = Documents::default();
        let one = files(&["/tmp/a b.txt"]);
        let many = files(&["/tmp/one", "/tmp/two"]);
        let cases: Vec<(&str, &Documents, Vec<&str>)> = vec![
            ("app", &none, vec!["app"]),
            ("app %F", &none, vec!["app"]),
            ("app %f", &one, vec!["app", "/tmp/a b.txt"]),
            ("app %F", &many, vec!["app", "/tmp/one", "/tmp/two"]),
            (
                "app %U",
                &one,
                vec!["app", "https://example.org/a", "https://example.org/b"],
            ),
            ("app %u", &one, vec!["app", "https://example.org/a"]),
            ("app %i", &none, vec!["app", "--icon", "demo-icon"]),
            ("app '%c'", &none, vec!["app", "'Demo App'"]),
            ("app %c", &none, vec!["app", "Demo App"]),
            ("app %k", &none, vec!["app", "/apps/demo.desktop"]),
            ("app 100%%", &none, vec!["app", "100%"]),
            ("app --flag=%f", &one, vec!["app", "--flag="]),
            (
                r#""/opt/my app/bin" --x"#,
                &none,
                vec!["/opt/my app/bin", "--x"],
            ),
            (
                r#"app "a \"quoted\" \$HOME \\ \` end""#,
                &none,
                vec!["app", r#"a "quoted" $HOME \ ` end"#],
            ),
            ("app \"\"", &none, vec!["app", ""]),
            ("app   spaced\tout", &none, vec!["app", "spaced", "out"]),
            (r"app back\slash", &none, vec!["app", r"back\slash"]),
            ("sh -c \"echo hi\"", &none, vec!["sh", "-c", "echo hi"]),
            ("sh -c 'echo hi'", &none, vec!["sh", "-c", "'echo", "hi'"]),
        ];
        for (exec, docs, expected) in cases {
            let got = run(exec, docs).unwrap_or_else(|err| panic!("{exec}: {err}"));
            assert_eq!(got, expected, "exec {exec:?}");
        }
    }

    #[test]
    fn rejects_bad_percent_and_quotes() {
        let none = Documents::default();
        assert!(matches!(run("app %z", &none), Err(LaunchError::FieldCode(code)) if code == "%z"));
        assert!(matches!(
            run("app 100%", &none),
            Err(LaunchError::FieldCode(_))
        ));
        assert!(matches!(
            run("app \"open", &none),
            Err(LaunchError::UnterminatedQuote)
        ));
    }

    #[test]
    fn tokenize_keeps_percent_text_literal() {
        assert_eq!(
            tokenize("run %U \"a b\"").expect("valid"),
            vec!["run", "%U", "a b"]
        );
        assert_eq!(
            program_of("  /usr/bin/app --x").as_deref(),
            Some("/usr/bin/app")
        );
        assert_eq!(program_of("   "), None);
    }
}
