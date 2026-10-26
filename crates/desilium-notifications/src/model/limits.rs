//! Size caps applied to everything a client sends, so one sender cannot exhaust memory.

/// Summary length in characters.
pub const SUMMARY_MAX_CHARS: usize = 256;
/// Body length in characters after markup is stripped.
pub const BODY_MAX_CHARS: usize = 4096;
pub const APP_NAME_MAX_CHARS: usize = 128;
pub const ACTIONS_MAX_PAIRS: usize = 8;
pub const ACTION_KEY_MAX_CHARS: usize = 64;
pub const ACTION_LABEL_MAX_CHARS: usize = 64;
pub const ICON_REF_MAX_CHARS: usize = 1024;
pub const CATEGORY_MAX_CHARS: usize = 64;
/// Hints beyond this count are ignored.
pub const HINTS_MAX: usize = 32;
/// Longest auto-expiry a client may request.
pub const MAX_EXPIRE_MS: u64 = 600_000;
/// Number of notifications kept in the history before the oldest are dropped.
pub const HISTORY_CAPACITY: usize = 200;
/// Toasts shown at once; older ones stay in the history only.
pub const MAX_VISIBLE_TOASTS: usize = 3;

/// Keeps at most `max` characters of `text`.
pub fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Removes control characters and surrounding whitespace for single-line fields.
pub fn sanitize_line(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

/// Sanitises and clips a single-line field, returning `None` when nothing is left.
pub fn line_or_none(text: &str, max: usize) -> Option<String> {
    let cleaned = clip(&sanitize_line(text), max);
    (!cleaned.is_empty()).then_some(cleaned)
}
