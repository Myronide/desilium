//! The tiny selector subset the DOM update API understands.
//!
//! `#id`, `.class`, `[data-key]` and `[data-key=value]` (value optionally quoted).
//! Anything else is rejected instead of silently matching nothing; the shell
//! addresses elements by stable ids and data keys, not by structure.

use crate::UiError;

/// A parsed selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Selector {
    Id(String),
    Class(String),
    Attr { name: String, value: Option<String> },
}

fn is_ident(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn is_attr_value(text: &str) -> bool {
    !text.contains(['"', '\\', ']', '\n'])
}

impl Selector {
    /// Parses `source`, or reports it as unsupported.
    pub(crate) fn parse(source: &str) -> Result<Selector, UiError> {
        let invalid = || UiError::InvalidSelector(source.to_owned());
        let text = source.trim();
        if let Some(id) = text.strip_prefix('#') {
            return is_ident(id)
                .then(|| Selector::Id(id.to_owned()))
                .ok_or_else(invalid);
        }
        if let Some(class) = text.strip_prefix('.') {
            return is_ident(class)
                .then(|| Selector::Class(class.to_owned()))
                .ok_or_else(invalid);
        }
        let inner = text
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .ok_or_else(invalid)?;
        let (name, value) = match inner.split_once('=') {
            Some((name, value)) => (name.trim(), Some(unquote(value.trim()))),
            None => (inner.trim(), None),
        };
        let value_ok = value.is_none_or(is_attr_value);
        if !is_ident(name) || !value_ok {
            return Err(invalid());
        }
        Ok(Selector::Attr {
            name: name.to_owned(),
            value: value.map(str::to_owned),
        })
    }

    /// CSS text equivalent, safe to hand to the backend's selector engine.
    pub(crate) fn to_css(&self) -> String {
        match self {
            Selector::Id(id) => format!("#{id}"),
            Selector::Class(class) => format!(".{class}"),
            Selector::Attr { name, value: None } => format!("[{name}]"),
            Selector::Attr {
                name,
                value: Some(value),
            } => format!("[{name}=\"{value}\"]"),
        }
    }
}

fn unquote(value: &str) -> &str {
    let quoted = |q: char| value.len() >= 2 && value.starts_with(q) && value.ends_with(q);
    if quoted('"') || quoted('\'') {
        return &value[1..value.len() - 1];
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_supported_forms() {
        assert_eq!(
            Selector::parse("#clock").unwrap(),
            Selector::Id("clock".into())
        );
        assert_eq!(
            Selector::parse(".on").unwrap(),
            Selector::Class("on".into())
        );
        assert_eq!(
            Selector::parse("[data-key=wifi]").unwrap().to_css(),
            "[data-key=\"wifi\"]"
        );
        assert_eq!(
            Selector::parse("[data-key='a b']").unwrap().to_css(),
            "[data-key=\"a b\"]"
        );
        assert_eq!(Selector::parse("[hidden]").unwrap().to_css(), "[hidden]");
    }

    #[test]
    fn rejects_everything_else() {
        for bad in [
            "div",
            "#a .b",
            ".a.b",
            "#",
            "[=x]",
            "[data-x=\"]",
            "a > b",
            "",
        ] {
            assert!(Selector::parse(bad).is_err(), "{bad}");
        }
    }
}
