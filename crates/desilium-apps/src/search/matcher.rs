//! Scores one folded query token against one folded field.

const EXACT: u32 = 1000;
const PREFIX: u32 = 800;
const WORD_START: u32 = 600;
const SUBSTRING: u32 = 400;
const SUBSEQUENCE_MAX: u32 = 200;
const SUBSEQUENCE_MIN: u32 = 60;
const SUBSEQUENCE_GAP_PENALTY: u32 = 10;
const TYPO: u32 = 150;
/// Tokens shorter than this must match exactly; typos are only tolerated above it.
const TYPO_MIN_CHARS: usize = 4;
/// Words longer than this are not compared for typos, bounding the work.
const MAX_TYPO_WORD: usize = 64;

/// A successful match: its strength and the field positions it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Match {
    pub(crate) score: u32,
    /// Indices into the folded field, ascending for every kind except subsequence gaps.
    pub(crate) positions: Vec<usize>,
}

/// Best match kind of `token` inside `field`, or `None` when nothing matches.
/// Order of preference: exact, prefix, word start, substring, subsequence, typo.
pub(crate) fn match_token(token: &[char], field: &[char]) -> Option<Match> {
    let len = token.len();
    if len == 0 || field.is_empty() {
        return None;
    }
    if field == token {
        return Some(span(EXACT, 0, len));
    }
    if field.starts_with(token) {
        return Some(span(PREFIX, 0, len));
    }
    if let Some(start) = word_start(token, field) {
        return Some(span(WORD_START, start, len));
    }
    if let Some(start) = field.windows(len).position(|window| window == token) {
        return Some(span(SUBSTRING, start, len));
    }
    subsequence(token, field).or_else(|| typo(token, field))
}

fn span(score: u32, start: usize, len: usize) -> Match {
    Match {
        score,
        positions: (start..start + len).collect(),
    }
}

/// Start of the first occurrence of `token` that begins a word.
fn word_start(token: &[char], field: &[char]) -> Option<usize> {
    field
        .windows(token.len())
        .enumerate()
        .find_map(|(index, window)| {
            let at_boundary = index == 0
                || index
                    .checked_sub(1)
                    .and_then(|previous| field.get(previous))
                    .is_some_and(|c| !c.is_alphanumeric());
            (window == token && at_boundary).then_some(index)
        })
}

/// Every character of `token` appears in `field` in order; fewer gaps score higher.
fn subsequence(token: &[char], field: &[char]) -> Option<Match> {
    let mut positions = Vec::with_capacity(token.len());
    let mut remaining = field.iter().enumerate();
    for wanted in token {
        let (index, _) = remaining.find(|(_, c)| **c == *wanted)?;
        positions.push(index);
    }
    let first = *positions.first()?;
    let last = *positions.last()?;
    let gaps = last
        .saturating_sub(first)
        .saturating_add(1)
        .saturating_sub(token.len());
    let penalty = u32::try_from(gaps)
        .unwrap_or(u32::MAX)
        .saturating_mul(SUBSEQUENCE_GAP_PENALTY);
    let score = SUBSEQUENCE_MAX.saturating_sub(penalty).max(SUBSEQUENCE_MIN);
    Some(Match { score, positions })
}

/// One edit allowed against any whole word of the field, for tokens of enough length.
fn typo(token: &[char], field: &[char]) -> Option<Match> {
    if token.len() < TYPO_MIN_CHARS {
        return None;
    }
    word_ranges(field)
        .into_iter()
        .find(|range| {
            field
                .get(range.clone())
                .is_some_and(|word| word.len() <= MAX_TYPO_WORD && within_one_edit(token, word))
        })
        .map(|range| Match {
            score: TYPO,
            positions: range.collect(),
        })
}

/// Index ranges of maximal alphanumeric runs.
fn word_ranges(field: &[char]) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut start: Option<usize> = None;
    for (index, c) in field.iter().enumerate() {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(index),
            (false, Some(begin)) => {
                ranges.push(begin..index);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(begin) = start {
        ranges.push(begin..field.len());
    }
    ranges
}

/// Levenshtein distance is at most one. Two-row dynamic programming over short words.
fn within_one_edit(a: &[char], b: &[char]) -> bool {
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (row, ca) in a.iter().enumerate() {
        let mut current = Vec::with_capacity(b.len() + 1);
        current.push(row + 1);
        for (col, cb) in b.iter().enumerate() {
            let diagonal = previous.get(col).copied().unwrap_or(usize::MAX);
            let above = previous.get(col + 1).copied().unwrap_or(usize::MAX);
            let left = current.last().copied().unwrap_or(usize::MAX);
            let cost = usize::from(ca != cb);
            current.push(
                diagonal
                    .saturating_add(cost)
                    .min(above.saturating_add(1))
                    .min(left.saturating_add(1)),
            );
        }
        previous = current;
    }
    previous.last().is_some_and(|distance| *distance <= 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    fn score(token: &str, field: &str) -> Option<u32> {
        match_token(&chars(token), &chars(field)).map(|m| m.score)
    }

    #[test]
    fn match_kinds_rank_in_order() {
        assert_eq!(score("term", "term"), Some(EXACT));
        assert_eq!(score("ter", "terminal"), Some(PREFIX));
        assert_eq!(score("nal", "terminal emulator"), Some(SUBSTRING));
        assert_eq!(score("emu", "terminal emulator"), Some(WORD_START));
        assert_eq!(
            score("tml", "terminal"),
            Some(SUBSEQUENCE_MAX - 5 * SUBSEQUENCE_GAP_PENALTY)
        );
        assert_eq!(score("zz", "terminal"), None);
    }

    #[test]
    fn typo_tolerance_needs_four_characters() {
        assert_eq!(score("firebox", "firefox"), Some(TYPO));
        assert_eq!(
            score("ffx", "firefox"),
            Some(SUBSEQUENCE_MAX - 4 * SUBSEQUENCE_GAP_PENALTY)
        );
        assert_eq!(score("fir", "fer"), None);
        assert_eq!(score("abcd", "abxd"), Some(TYPO));
        assert_eq!(score("abcdx", "zzzzz"), None);
    }

    #[test]
    fn positions_cover_the_matched_characters() {
        let found = match_token(&chars("emu"), &chars("terminal emulator")).expect("match");
        assert_eq!(found.positions, vec![9, 10, 11]);
    }

    #[test]
    fn within_one_edit_table() {
        let cases = [
            ("kitten", "kitten", true),
            ("kitten", "sitten", true),
            ("kitten", "kittn", true),
            ("kitten", "kittens", true),
            ("kitten", "sittin", false),
            ("", "a", true),
            ("ab", "", false),
        ];
        for (a, b, expected) in cases {
            assert_eq!(
                within_one_edit(&chars(a), &chars(b)),
                expected,
                "{a} vs {b}"
            );
        }
    }
}
