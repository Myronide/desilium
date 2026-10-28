//! Strict parser for the INI-like syntax shared by desktop entries and icon themes.

/// Largest file accepted; bigger files are rejected before any parsing.
pub const MAX_FILE_BYTES: usize = 512 * 1024;
/// Longest accepted line, including the key and value.
const MAX_LINE_BYTES: usize = 16 * 1024;
/// Most groups accepted in one file.
const MAX_GROUPS: usize = 256;

/// One `Key[locale]=value` assignment; the value is still escaped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    /// Key name, ASCII letters, digits and `-`.
    pub name: String,
    /// Locale suffix such as `de_DE`, if the key was localized.
    pub locale: Option<String>,
    /// Value exactly as written after the `=`, trimmed of outer whitespace.
    pub raw: String,
}

/// A `[Group]` header and the assignments below it, in file order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Group {
    /// Group name between the brackets.
    pub name: String,
    /// Assignments in file order; duplicates are kept and the first one wins.
    pub keys: Vec<Key>,
}

impl Group {
    /// Unlocalized raw value of `key`; the first occurrence wins.
    pub fn raw(&self, key: &str) -> Option<&str> {
        self.keys
            .iter()
            .find(|entry| entry.locale.is_none() && entry.name == key)
            .map(|entry| entry.raw.as_str())
    }

    /// Raw value for the first locale in `locales` that has one, else the unlocalized value.
    pub fn localized(&self, key: &str, locales: &[String]) -> Option<&str> {
        for locale in locales {
            let found = self.keys.iter().find(|entry| {
                entry.name == key && entry.locale.as_deref() == Some(locale.as_str())
            });
            if let Some(entry) = found {
                return Some(entry.raw.as_str());
            }
        }
        self.raw(key)
    }
}

/// A parsed file: its groups in order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IniFile {
    /// Groups in file order.
    pub groups: Vec<Group>,
}

impl IniFile {
    /// First group with exactly this name.
    pub fn group(&self, name: &str) -> Option<&Group> {
        self.groups.iter().find(|group| group.name == name)
    }
}

/// Reasons a file is rejected as a whole.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IniError {
    /// The file exceeds [`MAX_FILE_BYTES`].
    #[error("file is too large")]
    TooLarge,
    /// A line is neither blank, a comment, a group header nor a key assignment.
    #[error("line {0} is malformed")]
    Malformed(usize),
    /// A key assignment appears before the first group header.
    #[error("line {0} has a key outside any group")]
    KeyOutsideGroup(usize),
    /// The file has more groups than the parser accepts.
    #[error("file has too many groups")]
    TooManyGroups,
}

/// How much a file may contain before it is rejected as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Largest file in bytes.
    pub max_bytes: usize,
    /// Most groups.
    pub max_groups: usize,
}

/// Desktop entries: small files with a handful of groups.
pub const DESKTOP_LIMITS: Limits = Limits {
    max_bytes: MAX_FILE_BYTES,
    max_groups: MAX_GROUPS,
};

/// Icon theme indexes: one group per icon directory. hicolor already has more than 256, and
/// large themes list several hundred, so these limits are far above any real theme.
pub const THEME_LIMITS: Limits = Limits {
    max_bytes: 2 * 1024 * 1024,
    max_groups: 4096,
};

/// Parses the text of a desktop-style file. Blank lines and `#` comments are skipped.
pub fn parse(text: &str) -> Result<IniFile, IniError> {
    parse_with(text, DESKTOP_LIMITS)
}

/// Like [`parse`], with the limits of `limits`.
pub fn parse_with(text: &str, limits: Limits) -> Result<IniFile, IniError> {
    if text.len() > limits.max_bytes {
        return Err(IniError::TooLarge);
    }
    let mut file = IniFile::default();
    for (index, raw_line) in text.lines().enumerate() {
        let line_no = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.len() > MAX_LINE_BYTES {
            return Err(IniError::Malformed(line_no));
        }
        if let Some(header) = line.strip_prefix('[') {
            let name = header
                .strip_suffix(']')
                .filter(|name| !name.is_empty() && !name.contains(['[', ']']))
                .ok_or(IniError::Malformed(line_no))?;
            if file.groups.len() >= limits.max_groups {
                return Err(IniError::TooManyGroups);
            }
            file.groups.push(Group {
                name: name.to_owned(),
                keys: Vec::new(),
            });
            continue;
        }
        let key = parse_assignment(line).ok_or(IniError::Malformed(line_no))?;
        let group = file
            .groups
            .last_mut()
            .ok_or(IniError::KeyOutsideGroup(line_no))?;
        group.keys.push(key);
    }
    Ok(file)
}

/// Splits `Name[locale] = value`; returns `None` for anything that is not an assignment.
fn parse_assignment(line: &str) -> Option<Key> {
    let (lhs, value) = line.split_once('=')?;
    let lhs = lhs.trim_end();
    let (name, locale) = match lhs.split_once('[') {
        Some((name, rest)) => (name, Some(rest.strip_suffix(']')?)),
        None => (lhs, None),
    };
    let valid_name =
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let valid_locale = locale.is_none_or(|l| !l.is_empty() && !l.contains(['[', ']']));
    if !valid_name || !valid_locale {
        return None;
    }
    Some(Key {
        name: name.to_owned(),
        locale: locale.map(str::to_owned),
        raw: value.trim().to_owned(),
    })
}

/// Decodes the string escapes `\s \n \t \r \\`. Unknown escapes are kept verbatim.
pub fn unescape(raw: &str) -> String {
    decode(raw, false)
}

/// Splits a `;`-separated list, honouring `\;` and `\\`, and drops empty items.
pub fn split_list(raw: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                current.push('\\');
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ';' => items.push(std::mem::take(&mut current)),
            other => current.push(other),
        }
    }
    items.push(current);
    items
        .iter()
        .map(|item| decode(item, true))
        .filter(|item| !item.is_empty())
        .collect()
}

/// Splits a comma- or semicolon-separated list used by icon theme files.
pub fn split_csv(raw: &str) -> Vec<String> {
    raw.split([',', ';'])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn decode(raw: &str, list_item: bool) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(';') if list_item => out.push(';'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_groups_keys_and_locales() {
        let file = parse("[A]\nName=Plain\nName[de]=Deutsch\n# note\n\n[B]\nX=1\n").expect("valid");
        assert_eq!(file.groups.len(), 2);
        let group = file.group("A").expect("group A");
        assert_eq!(group.raw("Name"), Some("Plain"));
        assert_eq!(group.localized("Name", &["de".to_owned()]), Some("Deutsch"));
        assert_eq!(group.localized("Name", &["fr".to_owned()]), Some("Plain"));
    }

    #[test]
    fn first_duplicate_wins_and_spaces_around_equals_are_ignored() {
        let file = parse("[A]\nName = first\nName=second\n").expect("valid");
        assert_eq!(file.group("A").and_then(|g| g.raw("Name")), Some("first"));
    }

    #[test]
    fn rejects_garbage_lines_and_orphan_keys() {
        assert_eq!(parse("Name=x\n"), Err(IniError::KeyOutsideGroup(1)));
        assert_eq!(parse("[A]\njust words\n"), Err(IniError::Malformed(2)));
        assert_eq!(parse("[]\n"), Err(IniError::Malformed(1)));
        assert_eq!(parse("[A]\nbad key!=1\n"), Err(IniError::Malformed(2)));
        assert_eq!(parse("[A]\nK[=1\n"), Err(IniError::Malformed(2)));
    }

    #[test]
    fn enforces_size_caps() {
        let huge = "a".repeat(MAX_FILE_BYTES + 1);
        assert_eq!(parse(&huge), Err(IniError::TooLarge));
        let long_line = format!("[A]\nK={}\n", "v".repeat(MAX_LINE_BYTES));
        assert_eq!(parse(&long_line), Err(IniError::Malformed(2)));
        let many: String = (0..=MAX_GROUPS).map(|i| format!("[G{i}]\n")).collect();
        assert_eq!(parse(&many), Err(IniError::TooManyGroups));
    }

    #[test]
    fn icon_theme_indexes_may_have_many_more_groups_than_desktop_entries() {
        // The real hicolor index lists a group per size and context, well over 256.
        let theme: String = (0..=MAX_GROUPS + 100)
            .map(|i| format!("[{i}x{i}/apps]\nSize={i}\n"))
            .collect();
        assert_eq!(parse(&theme), Err(IniError::TooManyGroups));
        let parsed = parse_with(&theme, THEME_LIMITS).expect("theme limits are generous");
        assert_eq!(parsed.groups.len(), MAX_GROUPS + 101);
        let beyond: String = (0..=THEME_LIMITS.max_groups)
            .map(|i| format!("[G{i}]\n"))
            .collect();
        assert_eq!(
            parse_with(&beyond, THEME_LIMITS),
            Err(IniError::TooManyGroups),
            "there is still a cap"
        );
    }

    #[test]
    fn unescape_handles_spec_sequences() {
        assert_eq!(unescape(r"a\sb\nc\td\re\\f"), "a b\nc\td\re\\f");
        assert_eq!(unescape(r"keep\q"), r"keep\q");
        assert_eq!(unescape("trailing\\"), "trailing\\");
    }

    #[test]
    fn split_list_respects_escaped_separators() {
        assert_eq!(split_list("a;b;;c;"), vec!["a", "b", "c"]);
        assert_eq!(split_list(r"x\;y;z"), vec!["x;y", "z"]);
        assert_eq!(split_list(r"back\\;next"), vec![r"back\", "next"]);
        assert!(split_list("").is_empty());
    }

    #[test]
    fn split_csv_accepts_commas() {
        assert_eq!(split_csv("hicolor, breeze ,"), vec!["hicolor", "breeze"]);
    }
}
