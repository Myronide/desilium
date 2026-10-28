//! Turns a query into launcher results: calculator, applications, commands and web.

use desilium_apps::{Catalog, Search, UsageStats, search::calc};

use crate::model::launcher::{
    Activation, CalcCard, Category, Results, Row, RowIcon, Section, terminal_activation,
};

/// Rows shown for an empty query (most used apps).
const SUGGESTED: usize = 5;
/// Application rows for a query; one fewer when the calculator card is shown.
const APP_ROWS: usize = 6;

/// Everything a query needs besides the text.
pub struct Context<'a> {
    /// Installed applications.
    pub catalog: &'a Catalog,
    /// Launch history for ranking.
    pub usage: &'a UsageStats,
    /// Seconds since the epoch (usage decay).
    pub now_secs: u64,
}

fn app_row(
    catalog: &Catalog,
    id: &str,
    section: Section,
    highlights: Vec<std::ops::Range<usize>>,
) -> Option<Row> {
    let entry = catalog.get(id)?;
    let subtitle = entry
        .comment
        .clone()
        .or_else(|| entry.generic_name.clone())
        .unwrap_or_else(|| "Application".to_owned());
    Some(Row {
        section,
        activation: Activation::LaunchApp {
            desktop_id: id.to_owned(),
        },
        title: entry.name.clone(),
        subtitle,
        highlights,
        icon: RowIcon::App {
            desktop_id: id.to_owned(),
            icon: entry.icon.clone(),
        },
    })
}

fn suggested(ctx: &Context<'_>) -> Vec<Row> {
    let mut ids: Vec<(&str, f64)> = ctx
        .catalog
        .listed()
        .map(|entry| {
            let id = entry.id.as_str();
            (id, ctx.usage.score(id, ctx.now_secs))
        })
        .collect();
    ids.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    ids.into_iter()
        .take(SUGGESTED)
        .filter_map(|(id, _)| app_row(ctx.catalog, id, Section::Suggested, Vec::new()))
        .collect()
}

fn matching_apps(ctx: &Context<'_>, text: &str, limit: usize) -> Vec<Row> {
    Search::query(ctx.catalog, ctx.usage, text, ctx.now_secs, limit)
        .into_iter()
        .filter_map(|hit| {
            let name_hit = hit.field == desilium_apps::search::Field::Name;
            let highlights = if name_hit { hit.highlights } else { Vec::new() };
            app_row(ctx.catalog, &hit.id, Section::Applications, highlights)
        })
        .collect()
}

fn command_row(ctx: &Context<'_>, text: &str, category: Category) -> Option<Row> {
    let shell = text.starts_with('>');
    let first_word = text.split_whitespace().next().unwrap_or_default();
    let runnable = ctx.catalog.environment().find_program(first_word).is_some();
    let wanted = shell || category == Category::Commands || (runnable && category == Category::All);
    if !wanted {
        return None;
    }
    let activation = terminal_activation(text);
    let command = match &activation {
        Activation::RunShell(line) => line.clone(),
        _ => text.to_owned(),
    };
    if command.is_empty() {
        return None;
    }
    let prefix = "Run ";
    Some(Row {
        section: Section::Commands,
        activation,
        highlights: std::iter::once(prefix.len()..prefix.len() + command.len()).collect(),
        title: format!("{prefix}{command} in Terminal"),
        subtitle: if shell { "Shell command" } else { "Command" }.to_owned(),
        icon: RowIcon::Command,
    })
}

fn web_row(text: &str) -> Row {
    Row {
        section: Section::Web,
        activation: Activation::WebSearch(text.to_owned()),
        title: format!("Search the web for \u{201c}{text}\u{201d}"),
        subtitle: "Default search engine".to_owned(),
        highlights: Vec::new(),
        icon: RowIcon::Web,
    }
}

/// `24*7` shown as `24 × 7`.
pub fn display_expression(text: &str) -> String {
    let mut out = String::new();
    for c in text.trim().chars() {
        match c {
            '*' => out.push_str(" \u{d7} "),
            '/' => out.push_str(" \u{f7} "),
            '+' | '-' => {
                out.push(' ');
                out.push(c);
                out.push(' ');
            }
            other => out.push(other),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// All results for `query` in `category`.
pub fn compute(ctx: &Context<'_>, query: &str, category: Category) -> Results {
    let text = query.trim();
    let apps_wanted = matches!(category, Category::All | Category::Apps);
    if text.is_empty() {
        let rows = if apps_wanted {
            suggested(ctx)
        } else {
            Vec::new()
        };
        return Results { calc: None, rows };
    }
    let calc = (category == Category::All)
        .then(|| calc::answer(text))
        .flatten()
        .map(|result| CalcCard {
            expression: display_expression(text),
            result,
        });
    let limit = if calc.is_some() {
        APP_ROWS - 1
    } else {
        APP_ROWS
    };
    let mut rows = Vec::new();
    if apps_wanted {
        rows.extend(matching_apps(ctx, text, limit));
    }
    rows.extend(command_row(ctx, text, category));
    if matches!(category, Category::All | Category::Web) {
        rows.push(web_row(text));
    }
    Results { calc, rows }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_apps::Environment;
    use std::{collections::HashMap, fs};

    struct Fixture {
        _dir: tempfile::TempDir,
        catalog: Catalog,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path().join("share/applications");
        fs::create_dir_all(&apps).unwrap();
        let entries = [
            ("firefox", "Firefox", "Browse the web", "WebBrowser;"),
            (
                "calculator",
                "Calculator",
                "Scientific and programmer modes",
                "Utility;",
            ),
            ("files", "Files", "Browse your files", "FileManager;"),
        ];
        for (stem, name, comment, categories) in entries {
            fs::write(
                apps.join(format!("{stem}.desktop")),
                format!("[Desktop Entry]\nType=Application\nName={name}\nComment={comment}\nExec={stem}\nCategories={categories}\n"),
            )
            .unwrap();
        }
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let tool = bin.join("htop");
        fs::write(&tool, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&tool, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let vars: HashMap<&str, String> = HashMap::from([
            ("HOME", dir.path().display().to_string()),
            (
                "XDG_DATA_HOME",
                dir.path().join("none").display().to_string(),
            ),
            (
                "XDG_DATA_DIRS",
                dir.path().join("share").display().to_string(),
            ),
            ("PATH", bin.display().to_string()),
        ]);
        let env = Environment::from_lookup(|key| vars.get(key).cloned());
        let catalog = Catalog::load(&env);
        Fixture { _dir: dir, catalog }
    }

    fn run(fixture: &Fixture, usage: &UsageStats, query: &str, category: Category) -> Results {
        let ctx = Context {
            catalog: &fixture.catalog,
            usage,
            now_secs: 1_000_000,
        };
        compute(&ctx, query, category)
    }

    fn titles(results: &Results) -> Vec<&str> {
        results.rows.iter().map(|r| r.title.as_str()).collect()
    }

    #[test]
    fn app_query_finds_the_app_and_highlights_the_name() {
        let f = fixture();
        let results = run(&f, &UsageStats::in_memory(), "fire", Category::All);
        assert_eq!(results.rows[0].title, "Firefox");
        assert_eq!(results.rows[0].highlights, vec![0..4]);
        assert_eq!(results.rows[0].section, Section::Applications);
        let last = results.rows.last().unwrap();
        assert_eq!(last.section, Section::Web);
        assert_eq!(last.activation, Activation::WebSearch("fire".into()));
        assert!(results.calc.is_none());
    }

    #[test]
    fn arithmetic_shows_the_calculator_card() {
        let f = fixture();
        let results = run(&f, &UsageStats::in_memory(), "24*7", Category::All);
        let card = results.calc.unwrap();
        assert_eq!(card.result, "168");
        assert_eq!(card.expression, "24 \u{d7} 7");
        assert!(results.rows.iter().any(|r| r.section == Section::Web));
    }

    #[test]
    fn programs_on_the_path_offer_run_in_terminal() {
        let f = fixture();
        let results = run(&f, &UsageStats::in_memory(), "htop -d 5", Category::All);
        let row = results
            .rows
            .iter()
            .find(|r| r.section == Section::Commands)
            .unwrap();
        assert_eq!(
            row.activation,
            Activation::RunInTerminal("htop -d 5".into())
        );
        assert_eq!(row.title, "Run htop -d 5 in Terminal");
        assert_eq!(row.highlights, vec![4..13]);
    }

    #[test]
    fn plain_words_do_not_offer_a_command_but_chevron_prefix_always_does() {
        let f = fixture();
        let none = run(&f, &UsageStats::in_memory(), "fire", Category::All);
        assert!(none.rows.iter().all(|r| r.section != Section::Commands));
        let shell = run(&f, &UsageStats::in_memory(), "> ls -la", Category::All);
        let row = shell
            .rows
            .iter()
            .find(|r| r.section == Section::Commands)
            .unwrap();
        assert_eq!(row.activation, Activation::RunShell("ls -la".into()));
        assert_eq!(row.title, "Run ls -la in Terminal");
    }

    #[test]
    fn categories_filter_the_sections() {
        let f = fixture();
        let usage = UsageStats::in_memory();
        let apps = run(&f, &usage, "fi", Category::Apps);
        assert!(apps.rows.iter().all(|r| r.section == Section::Applications));
        let web = run(&f, &usage, "fi", Category::Web);
        assert_eq!(titles(&web), ["Search the web for \u{201c}fi\u{201d}"]);
        let commands = run(&f, &usage, "fi", Category::Commands);
        assert_eq!(commands.rows.len(), 1);
        assert_eq!(commands.rows[0].section, Section::Commands);
        assert!(run(&f, &usage, "24*7", Category::Apps).calc.is_none());
    }

    #[test]
    fn empty_query_suggests_the_most_used_apps_first() {
        let f = fixture();
        let mut usage = UsageStats::in_memory();
        usage.record("files.desktop", 999_000);
        usage.record("files.desktop", 999_001);
        usage.record("firefox.desktop", 999_002);
        let results = run(&f, &usage, "", Category::All);
        assert_eq!(titles(&results)[..2], ["Files", "Firefox"]);
        assert!(results.rows.iter().all(|r| r.section == Section::Suggested));
        assert!(run(&f, &usage, "", Category::Web).rows.is_empty());
    }

    #[test]
    fn usage_boosts_the_ranking() {
        let f = fixture();
        let mut usage = UsageStats::in_memory();
        let plain = run(&f, &usage, "f", Category::Apps);
        let before = plain.rows.iter().position(|r| r.title == "Files").unwrap();
        for second in 0..20 {
            usage.record("files.desktop", 999_000 + second);
        }
        let boosted = run(&f, &usage, "f", Category::Apps);
        let after = boosted
            .rows
            .iter()
            .position(|r| r.title == "Files")
            .unwrap();
        assert!(after <= before);
        assert_eq!(after, 0);
    }

    #[test]
    fn expression_display() {
        assert_eq!(display_expression("24*7"), "24 \u{d7} 7");
        assert_eq!(display_expression(" (1+2)/3 "), "(1 + 2) \u{f7} 3");
    }
}
