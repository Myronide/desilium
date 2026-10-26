//! Development switches for reproducible animation captures (`--freeze-time`, `--demo`).
//!
//! Both are inert unless given on the command line, so normal runs pay nothing for them.

use super::timing::{Freeze, Kind, KindSet};
use std::time::Duration;

/// A scripted window action performed once the first window has been drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Demo {
    /// Maximizes the focused window, to capture the geometry transition.
    Maximize,
    /// Closes the focused window, to capture the close animation.
    Close,
}

impl Demo {
    /// Parses the `--demo` value.
    pub fn parse(text: &str) -> Option<Demo> {
        match text {
            "maximize" => Some(Demo::Maximize),
            "close" => Some(Demo::Close),
            _ => None,
        }
    }
}

/// Parses `MS` or `MS:KIND[,KIND...]` (kinds: open, close, geometry, focus, hover).
pub fn parse_freeze(text: &str) -> Option<Freeze> {
    let (millis, kinds) = match text.split_once(':') {
        Some((millis, kinds)) => (millis, Some(kinds)),
        None => (text, None),
    };
    let at = Duration::from_millis(millis.trim().parse::<u64>().ok()?);
    let kinds = match kinds {
        None => KindSet::ALL,
        Some(list) => parse_kinds(list)?,
    };
    Some(Freeze { at, kinds })
}

fn parse_kinds(list: &str) -> Option<KindSet> {
    let mut set: Option<KindSet> = None;
    for name in list.split(',') {
        let kind: Kind = KindSet::parse_kind(name.trim())?;
        let one = KindSet::only(kind);
        set = Some(set.map_or(one, |existing| existing.with(one)));
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_milliseconds_freeze_every_kind() {
        let freeze = parse_freeze("120").expect("valid");
        assert_eq!(freeze.at, Duration::from_millis(120));
        assert_eq!(freeze.kinds, KindSet::ALL);
    }

    #[test]
    fn kinds_can_be_selected() {
        let freeze = parse_freeze("60:open, focus").expect("valid");
        assert!(freeze.kinds.contains(Kind::Open) && freeze.kinds.contains(Kind::Focus));
        assert!(!freeze.kinds.contains(Kind::Geometry));
    }

    #[test]
    fn malformed_values_are_rejected() {
        for text in ["", "abc", "10:", "10:bogus", "-5", "10:open,"] {
            assert!(parse_freeze(text).is_none(), "{text}");
        }
    }

    #[test]
    fn demo_names() {
        assert_eq!(Demo::parse("maximize"), Some(Demo::Maximize));
        assert_eq!(Demo::parse("close"), Some(Demo::Close));
        assert_eq!(Demo::parse("dance"), None);
    }
}
