//! Unified search over the settings registry.
//!
//! Every whitespace-separated word of the query must match somewhere in a
//! setting (AND); the score of a word is the best field it hits, so a title hit
//! beats a keyword hit beats a description hit. Ties keep registry order.

use crate::registry::{self, SettingDef};

const SCORE_TITLE_WORD_PREFIX: u32 = 100;
const SCORE_TITLE_CONTAINS: u32 = 70;
const SCORE_KEYWORD_WORD: u32 = 80;
const SCORE_KEYWORD_CONTAINS: u32 = 50;
const SCORE_KEY_PATH: u32 = 60;
const SCORE_SECTION: u32 = 40;
const SCORE_DESCRIPTION: u32 = 25;
const SCORE_PAGE: u32 = 20;
const BONUS_TITLE_STARTS_WITH_QUERY: u32 = 30;

/// A setting that matched, with its relevance.
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    /// The matching setting.
    pub def: &'static SettingDef,
    /// Higher is more relevant.
    pub score: u32,
}

/// Searches every setting. An empty or blank query returns nothing.
pub fn search(query: &str) -> Vec<Hit> {
    search_in(registry::all(), query)
}

fn search_in(defs: &'static [SettingDef], query: &str) -> Vec<Hit> {
    let query = query.trim().to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<Hit> = defs
        .iter()
        .filter_map(|def| score(def, &words, &query).map(|score| Hit { def, score }))
        .collect();
    // `sort_by` is stable, so equal scores keep the registry's display order.
    hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
    hits
}

fn score(def: &SettingDef, words: &[&str], query: &str) -> Option<u32> {
    let mut total = 0;
    for word in words {
        total += word_score(def, word)?;
    }
    if def.title_text().to_lowercase().starts_with(query) {
        total += BONUS_TITLE_STARTS_WITH_QUERY;
    }
    Some(total)
}

/// Best score of one query word over all fields; `None` when it matches nothing.
///
/// Fields are searched in the language on screen, so a German user finds "Akzentfarbe".
fn word_score(def: &SettingDef, word: &str) -> Option<u32> {
    let title = def.title_text().to_lowercase();
    let candidates = [
        field_score(&title, word, SCORE_TITLE_WORD_PREFIX, SCORE_TITLE_CONTAINS),
        keyword_score(def, word),
        contains_score(def.id, word, SCORE_KEY_PATH),
        contains_score(&def.section_text().to_lowercase(), word, SCORE_SECTION),
        contains_score(
            &def.description_text().to_lowercase(),
            word,
            SCORE_DESCRIPTION,
        ),
        contains_score(&def.page.title().to_lowercase(), word, SCORE_PAGE),
    ];
    candidates.into_iter().flatten().max()
}

fn field_score(text: &str, word: &str, prefix_score: u32, contains_score: u32) -> Option<u32> {
    if text
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w.starts_with(word))
    {
        return Some(prefix_score);
    }
    text.contains(word).then_some(contains_score)
}

fn contains_score(text: &str, word: &str, score: u32) -> Option<u32> {
    text.contains(word).then_some(score)
}

fn keyword_score(def: &SettingDef, word: &str) -> Option<u32> {
    def.search_keywords()
        .iter()
        .filter_map(|keyword| {
            field_score(
                &keyword.to_lowercase(),
                word,
                SCORE_KEYWORD_WORD,
                SCORE_KEYWORD_CONTAINS,
            )
        })
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(query: &str) -> Vec<&'static str> {
        search(query).iter().map(|hit| hit.def.id).collect()
    }

    #[test]
    fn blank_query_finds_nothing() {
        assert!(search("").is_empty());
        assert!(search("   ").is_empty());
    }

    #[test]
    fn title_prefix_ranks_first() {
        assert_eq!(ids("accent").first(), Some(&"appearance.accent"));
        assert_eq!(ids("cursor si").first(), Some(&"appearance.cursor_size"));
    }

    #[test]
    fn keywords_find_settings_by_synonym() {
        assert!(ids("dark mode").contains(&"appearance.theme"));
        assert!(ids("workspaces").contains(&"compositor.desktops"));
        assert!(ids("hotkeys").contains(&"compositor.shortcuts"));
    }

    #[test]
    fn all_words_must_match() {
        assert!(ids("accent zebra").is_empty());
        assert!(ids("cursor theme").contains(&"appearance.cursor_theme"));
        assert!(!ids("cursor theme").contains(&"appearance.accent"));
    }

    #[test]
    fn key_paths_are_searchable() {
        assert_eq!(
            ids("appearance.wallpaper_mode").first(),
            Some(&"appearance.wallpaper_mode")
        );
    }

    #[test]
    fn title_beats_description_hits() {
        // "speed" is a title word of the animation row and only in the description elsewhere.
        let hits = search("speed");
        assert_eq!(hits[0].def.id, "appearance.animation_speed");
    }

    #[test]
    fn search_is_case_insensitive_and_trims() {
        assert_eq!(ids("  ACCENT "), ids("accent"));
    }

    #[test]
    fn equal_scores_keep_registry_order() {
        let hits = search("pointer");
        let positions: Vec<_> = hits
            .iter()
            .filter(|h| h.score == hits[0].score)
            .map(|h| h.def.id)
            .collect();
        let mut sorted = positions.clone();
        sorted.sort_by_key(|id| registry::all().iter().position(|d| d.id == *id));
        assert_eq!(positions, sorted);
    }
}
