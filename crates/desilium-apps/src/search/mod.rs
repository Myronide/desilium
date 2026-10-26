//! Launcher ranking: fuzzy matching over catalog fields, boosted by usage.
//!
//! The launcher also reuses the pure providers in [`calc`] and [`command`].

pub mod calc;
pub mod command;
mod fold;
mod matcher;
pub mod usage;

use std::ops::Range;
use std::path::Path;

use crate::catalog::Catalog;
use crate::desktop::DesktopEntry;
use crate::launch::expand::program_of;

pub use usage::UsageStats;

use fold::{Folded, fold};

/// The part of an application that matched the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// Localized display name.
    Name,
    /// Localized generic name such as "Web Browser".
    GenericName,
    /// One localized keyword.
    Keyword,
    /// Basename of the Exec program.
    Exec,
    /// One freedesktop category.
    Category,
    /// Localized comment.
    Comment,
}

impl Field {
    /// Importance of a match in this field, in percent of the raw match score.
    fn weight(self) -> u32 {
        match self {
            Self::Name => 100,
            Self::GenericName => 80,
            Self::Keyword => 70,
            Self::Exec => 60,
            Self::Category => 50,
            Self::Comment => 40,
        }
    }
}

/// One ranked application for a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Desktop file id of the application.
    pub id: String,
    /// Match score plus usage boost; higher ranks first.
    pub score: u32,
    /// Field that matched the first query word best.
    pub field: Field,
    /// Original text of that field, so highlights can be drawn over it.
    pub matched_text: String,
    /// Byte ranges into `matched_text` to emphasize.
    pub highlights: Vec<Range<usize>>,
}

/// Entry point for launcher searches.
pub struct Search;

impl Search {
    /// Ranks listed applications for `text`. Every query word must match some field.
    /// Ties are ordered by id. `now_secs` drives the usage decay.
    pub fn query(
        catalog: &Catalog,
        usage: &UsageStats,
        text: &str,
        now_secs: u64,
        limit: usize,
    ) -> Vec<Hit> {
        let tokens = query_tokens(text);
        if tokens.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut hits: Vec<Hit> = catalog
            .listed()
            .filter_map(|entry| score_entry(entry, &tokens))
            .collect();
        for hit in &mut hits {
            hit.score = hit.score.saturating_add(usage.boost(&hit.id, now_secs));
        }
        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
        hits.truncate(limit);
        hits
    }
}

/// A searchable string of an entry together with the field it came from.
struct FieldText {
    kind: Field,
    text: String,
    folded: Folded,
}

/// Best match of one query word across all fields.
struct TokenMatch {
    field: usize,
    score: u32,
    positions: Vec<usize>,
}

fn query_tokens(text: &str) -> Vec<Vec<char>> {
    fold(text)
        .chars
        .split(|c| c.is_whitespace())
        .filter(|word| !word.is_empty())
        .map(<[char]>::to_vec)
        .collect()
}

fn score_entry(entry: &DesktopEntry, tokens: &[Vec<char>]) -> Option<Hit> {
    let fields = fields_of(entry);
    let mut matches = Vec::with_capacity(tokens.len());
    for token in tokens {
        matches.push(best_match(token, &fields)?);
    }
    let total = matches
        .iter()
        .fold(0_u32, |sum, found| sum.saturating_add(found.score));
    let average = total.checked_div(u32::try_from(tokens.len()).ok()?)?;
    let lead_field = matches.first()?.field;
    let field = fields.get(lead_field)?;
    let mut positions: Vec<usize> = matches
        .iter()
        .filter(|found| found.field == lead_field)
        .flat_map(|found| found.positions.iter().copied())
        .collect();
    positions.sort_unstable();
    positions.dedup();
    Some(Hit {
        id: entry.id.as_str().to_owned(),
        score: average,
        field: field.kind,
        matched_text: field.text.clone(),
        highlights: highlight_ranges(&positions, &field.folded.spans),
    })
}

/// Strongest match among the fields. On equal scores the earlier field wins.
fn best_match(token: &[char], fields: &[FieldText]) -> Option<TokenMatch> {
    let mut best: Option<TokenMatch> = None;
    for (index, field) in fields.iter().enumerate() {
        let Some(found) = matcher::match_token(token, &field.folded.chars) else {
            continue;
        };
        let candidate = TokenMatch {
            field: index,
            score: found.score.saturating_mul(field.kind.weight()) / 100,
            positions: found.positions,
        };
        if best
            .as_ref()
            .is_none_or(|current| candidate.score > current.score)
        {
            best = Some(candidate);
        }
    }
    best
}

fn fields_of(entry: &DesktopEntry) -> Vec<FieldText> {
    let mut texts: Vec<(Field, String)> = vec![(Field::Name, entry.name.clone())];
    texts.extend(
        entry
            .generic_name
            .iter()
            .map(|text| (Field::GenericName, text.clone())),
    );
    texts.extend(
        entry
            .keywords
            .iter()
            .map(|text| (Field::Keyword, text.clone())),
    );
    texts.extend(exec_basename(&entry.exec).map(|text| (Field::Exec, text)));
    texts.extend(
        entry
            .categories
            .iter()
            .map(|text| (Field::Category, text.clone())),
    );
    texts.extend(
        entry
            .comment
            .iter()
            .map(|text| (Field::Comment, text.clone())),
    );
    texts
        .into_iter()
        .map(|(kind, text)| {
            let folded = fold(&text);
            FieldText { kind, text, folded }
        })
        .collect()
}

fn exec_basename(exec: &str) -> Option<String> {
    let program = program_of(exec)?;
    Path::new(&program)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}

/// Merges sorted character positions into byte ranges of the original text.
fn highlight_ranges(positions: &[usize], spans: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for position in positions {
        let Some(span) = spans.get(*position) else {
            continue;
        };
        match ranges.last_mut() {
            Some(last) if last.end == span.start => last.end = span.end,
            _ => ranges.push(span.clone()),
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, env_with_data_dirs};

    const APPS: &[(&str, &str)] = &[
        (
            "firefox.desktop",
            "[Desktop Entry]\nType=Application\nName=Firefox\nGenericName=Web Browser\n\
             Keywords=internet;www;browser;\nExec=firefox %u\nCategories=Network;WebBrowser;\n",
        ),
        (
            "org.gnome.Nautilus.desktop",
            "[Desktop Entry]\nType=Application\nName=Files\nGenericName=File Manager\n\
             Keywords=folder;explorer;\nExec=nautilus --new-window\nCategories=GNOME;Utility;\n",
        ),
        (
            "foot.desktop",
            "[Desktop Entry]\nType=Application\nName=Foot\nComment=Fast, lightweight terminal emulator\n\
             Keywords=shell;console;terminal;\nExec=foot\nCategories=System;TerminalEmulator;\n",
        ),
        (
            "calc.desktop",
            "[Desktop Entry]\nType=Application\nName=Calculator\nKeywords=math;arithmetic;\n\
             Exec=gnome-calculator\nCategories=Utility;Calculator;\n",
        ),
        (
            "gimp.desktop",
            "[Desktop Entry]\nType=Application\nName=GIMP\nGenericName=Image Editor\n\
             Exec=gimp-2.10 %U\nCategories=Graphics;\n",
        ),
        (
            "editor-a.desktop",
            "[Desktop Entry]\nType=Application\nName=Editor One\nExec=one\n",
        ),
        (
            "editor-b.desktop",
            "[Desktop Entry]\nType=Application\nName=Editor Two\nExec=two\n",
        ),
        (
            "quiet.desktop",
            "[Desktop Entry]\nType=Application\nName=Quiet Tool\nExec=quiet\nNoDisplay=true\n",
        ),
    ];

    fn catalog() -> (TempDir, Catalog) {
        let dir = TempDir::new("search");
        for (id, text) in APPS {
            dir.write(&format!("applications/{id}"), text);
        }
        let env = env_with_data_dirs(&[dir.path()]);
        let catalog = Catalog::load(&env);
        (dir, catalog)
    }

    fn top(query: &str) -> Option<String> {
        let (_dir, catalog) = catalog();
        Search::query(&catalog, &UsageStats::in_memory(), query, 0, 10)
            .first()
            .map(|hit| hit.id.clone())
    }

    #[test]
    fn ranking_examples() {
        let cases: &[(&str, Option<&str>)] = &[
            ("fire", Some("firefox.desktop")),
            ("firefox", Some("firefox.desktop")),
            ("files", Some("org.gnome.Nautilus.desktop")),
            ("term", Some("foot.desktop")),
            ("browser", Some("firefox.desktop")),
            ("calc", Some("calc.desktop")),
            ("math", Some("calc.desktop")),
            ("gimp", Some("gimp.desktop")),
            ("image editor", Some("gimp.desktop")),
            ("firefix", Some("firefox.desktop")),
            ("nautilus", Some("org.gnome.Nautilus.desktop")),
            ("quiet", None),
            ("xyz-nothing", None),
            ("", None),
        ];
        for (query, expected) in cases {
            assert_eq!(top(query).as_deref(), *expected, "query {query:?}");
        }
    }

    #[test]
    fn equal_scores_are_ordered_by_id() {
        let (_dir, catalog) = catalog();
        let hits = Search::query(&catalog, &UsageStats::in_memory(), "editor", 0, 10);
        let ids: Vec<&str> = hits.iter().take(2).map(|hit| hit.id.as_str()).collect();
        assert_eq!(ids, vec!["editor-a.desktop", "editor-b.desktop"]);
    }

    #[test]
    fn usage_boost_reorders_equal_matches() {
        let (_dir, catalog) = catalog();
        let mut usage = UsageStats::in_memory();
        usage.record("editor-b.desktop", 1_000);
        let hits = Search::query(&catalog, &usage, "editor", 1_000, 10);
        assert_eq!(hits[0].id, "editor-b.desktop");
    }

    #[test]
    fn highlights_point_into_the_matched_field() {
        let (_dir, catalog) = catalog();
        let hits = Search::query(&catalog, &UsageStats::in_memory(), "fire", 0, 1);
        let hit = hits.first().expect("firefox matches");
        assert_eq!(hit.field, Field::Name);
        assert_eq!(hit.matched_text, "Firefox");
        assert_eq!(hit.highlights, vec![0..4]);
    }

    #[test]
    fn limit_and_empty_input() {
        let (_dir, catalog) = catalog();
        let store = UsageStats::in_memory();
        assert_eq!(Search::query(&catalog, &store, "e", 0, 2).len(), 2);
        assert!(Search::query(&catalog, &store, "e", 0, 0).is_empty());
        assert!(Search::query(&catalog, &store, "   ", 0, 5).is_empty());
    }

    #[test]
    fn exec_basename_is_searchable() {
        assert_eq!(top("gnome-calc").as_deref(), Some("calc.desktop"));
    }
}
