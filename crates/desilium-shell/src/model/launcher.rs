//! Launcher state machine: query, category, results and keyboard selection.
//!
//! Pure: results are computed elsewhere and handed in; key presses return an [`Outcome`].

use std::ops::Range;

use desilium_ui::{Key, KeyEvent};

/// Number of pinned apps reachable with `Alt+1..6`.
pub(crate) const PINNED_SLOTS: usize = 6;

/// Result filter chips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Category {
    /// Everything.
    All,
    /// Applications only.
    Apps,
    /// Files (not available yet).
    Files,
    /// Settings (not available yet).
    Settings,
    /// Run-in-terminal commands.
    Commands,
    /// Web search.
    Web,
}

impl Category {
    /// All chips in display order.
    pub(crate) const ALL: [Category; 6] = [
        Category::All,
        Category::Apps,
        Category::Files,
        Category::Settings,
        Category::Commands,
        Category::Web,
    ];

    /// Chip label.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Category::All => "All",
            Category::Apps => "Apps",
            Category::Files => "Files",
            Category::Settings => "Settings",
            Category::Commands => "Commands",
            Category::Web => "Web",
        }
    }

    /// Sprite icon of the chip; `All` has none.
    pub(crate) fn icon(self) -> Option<&'static str> {
        match self {
            Category::All => None,
            Category::Apps => Some("grid"),
            Category::Files => Some("file"),
            Category::Settings => Some("settings"),
            Category::Commands => Some("terminal"),
            Category::Web => Some("globe"),
        }
    }

    /// False for chips that are shown but not implemented yet.
    pub(crate) fn enabled(self) -> bool {
        !matches!(self, Category::Files | Category::Settings)
    }

    /// Stable name used in `data-arg`.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Category::All => "all",
            Category::Apps => "apps",
            Category::Files => "files",
            Category::Settings => "settings",
            Category::Commands => "commands",
            Category::Web => "web",
        }
    }

    /// Inverse of [`Category::id`].
    pub(crate) fn from_id(id: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.id() == id)
    }
}

/// Group heading of a result row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    /// Most used apps shown for an empty query.
    Suggested,
    /// Matching applications.
    Applications,
    /// Terminal commands.
    Commands,
    /// Web search.
    Web,
}

impl Section {
    /// Heading text.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Section::Suggested => "Suggested",
            Section::Applications => "Applications",
            Section::Commands => "Commands",
            Section::Web => "Web",
        }
    }
}

/// What Enter does on a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Activation {
    /// Start an installed application.
    LaunchApp {
        /// Desktop file id.
        desktop_id: String,
    },
    /// `> line`: run through `sh -c` in a terminal.
    RunShell(String),
    /// Run the words in a terminal without a shell.
    RunInTerminal(String),
    /// Open the default search engine with this text.
    WebSearch(String),
}

/// Icon of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RowIcon {
    /// An application icon.
    App {
        /// Desktop file id (its stem is tried as an icon name when the entry names none).
        desktop_id: String,
        /// Icon name or path.
        icon: Option<String>,
    },
    /// Terminal command tile.
    Command,
    /// Web search tile.
    Web,
}

/// One selectable result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    /// Heading it appears under.
    pub section: Section,
    /// Effect of Enter.
    pub activation: Activation,
    /// First line.
    pub title: String,
    /// Second line.
    pub subtitle: String,
    /// Byte ranges of `title` to emphasise.
    pub highlights: Vec<Range<usize>>,
    /// Leading icon.
    pub icon: RowIcon,
}

/// The calculator answer card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CalcCard {
    /// The expression as typed.
    pub expression: String,
    /// The formatted result.
    pub result: String,
}

/// Everything a query produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Results {
    /// Calculator answer, if the query is arithmetic.
    pub calc: Option<CalcCard>,
    /// Selectable rows in display order.
    pub rows: Vec<Row>,
}

/// What the caller must do after a key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Nothing changed.
    Ignored,
    /// Selection moved: redraw only.
    SelectionChanged,
    /// The category changed: recompute results.
    CategoryChanged,
    /// Close without acting.
    Close,
    /// Do this, then close.
    Activate(Activation),
}

/// Launcher state while it is open.
#[derive(Debug, Clone)]
pub(crate) struct LauncherModel {
    query: String,
    category: Category,
    results: Results,
    selected: usize,
}

impl Default for LauncherModel {
    fn default() -> Self {
        Self {
            query: String::new(),
            category: Category::All,
            results: Results::default(),
            selected: 0,
        }
    }
}

impl LauncherModel {
    /// The text in the search field.
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    /// Active chip.
    pub(crate) fn category(&self) -> Category {
        self.category
    }

    /// Current results.
    pub(crate) fn results(&self) -> &Results {
        &self.results
    }

    /// Index of the selected row.
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    /// The selected row, if there are any.
    pub(crate) fn selected_row(&self) -> Option<&Row> {
        self.results.rows.get(self.selected)
    }

    /// Stores the new query text. Call [`set_results`](Self::set_results) afterwards.
    pub(crate) fn set_query(&mut self, query: &str) {
        self.query = query.to_owned();
    }

    /// Replaces the results; the selection returns to the first row.
    pub(crate) fn set_results(&mut self, results: Results) {
        self.results = results;
        self.selected = 0;
    }

    /// Back to the empty state for the next opening.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Selects a category (disabled ones are refused).
    pub(crate) fn set_category(&mut self, category: Category) -> bool {
        let changed = category.enabled() && category != self.category;
        if changed {
            self.category = category;
        }
        changed
    }

    fn step_category(&mut self, forward: bool) -> bool {
        let all = Category::ALL;
        let count = all.len();
        let current = all.iter().position(|c| *c == self.category).unwrap_or(0);
        for offset in 1..=count {
            let at = if forward {
                (current + offset) % count
            } else {
                (current + count - offset % count) % count
            };
            let Some(candidate) = all.get(at).copied().filter(|c| c.enabled()) else {
                continue;
            };
            self.category = candidate;
            return true;
        }
        false
    }

    fn move_selection(&mut self, delta: isize) -> bool {
        let count = self.results.rows.len();
        if count == 0 {
            return false;
        }
        let count = count as isize;
        let next = (self.selected as isize + delta).rem_euclid(count);
        let moved = next as usize != self.selected;
        self.selected = next as usize;
        moved
    }

    /// Interprets a key press. `pinned` are the desktop ids behind `Alt+1..6`.
    pub(crate) fn handle_key(&mut self, event: &KeyEvent, pinned: &[String]) -> Outcome {
        let mods = event.modifiers;
        if mods.alt {
            return alt_pinned(event, pinned);
        }
        match &event.key {
            Key::Escape => Outcome::Close,
            Key::ArrowDown => selection_outcome(self.move_selection(1)),
            Key::ArrowUp => selection_outcome(self.move_selection(-1)),
            Key::PageDown => selection_outcome(self.move_selection(PAGE_STEP)),
            Key::PageUp => selection_outcome(self.move_selection(-PAGE_STEP)),
            Key::Tab => {
                let changed = self.step_category(!mods.shift);
                if changed {
                    Outcome::CategoryChanged
                } else {
                    Outcome::Ignored
                }
            }
            Key::Enter => self.enter(mods.shift, mods.ctrl),
            _ => Outcome::Ignored,
        }
    }

    fn enter(&self, shift: bool, ctrl: bool) -> Outcome {
        let text = self.query.trim();
        if shift && !text.is_empty() {
            return Outcome::Activate(Activation::WebSearch(text.to_owned()));
        }
        if ctrl && !text.is_empty() {
            return Outcome::Activate(terminal_activation(text));
        }
        match self.selected_row() {
            Some(row) => Outcome::Activate(row.activation.clone()),
            None => Outcome::Ignored,
        }
    }
}

const PAGE_STEP: isize = 5;

fn selection_outcome(moved: bool) -> Outcome {
    if moved {
        Outcome::SelectionChanged
    } else {
        Outcome::Ignored
    }
}

/// The activation for text that is not an app: `>` means a shell line.
pub(crate) fn terminal_activation(text: &str) -> Activation {
    match text.strip_prefix('>') {
        Some(line) => Activation::RunShell(line.trim().to_owned()),
        None => Activation::RunInTerminal(text.to_owned()),
    }
}

fn alt_pinned(event: &KeyEvent, pinned: &[String]) -> Outcome {
    let Key::Character(text) = &event.key else {
        return Outcome::Ignored;
    };
    let slot = text
        .chars()
        .next()
        .and_then(|c| c.to_digit(10))
        .and_then(|digit| usize::try_from(digit).ok())
        .and_then(|digit| digit.checked_sub(1))
        .filter(|slot| *slot < PINNED_SLOTS);
    let id = slot.and_then(|slot| pinned.get(slot));
    match id {
        Some(desktop_id) => Outcome::Activate(Activation::LaunchApp {
            desktop_id: desktop_id.clone(),
        }),
        None => Outcome::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_ui::Modifiers;

    fn row(name: &str) -> Row {
        Row {
            section: Section::Applications,
            activation: Activation::LaunchApp {
                desktop_id: format!("{name}.desktop"),
            },
            title: name.to_owned(),
            subtitle: String::new(),
            highlights: Vec::new(),
            icon: RowIcon::Command,
        }
    }

    fn model_with(names: &[&str]) -> LauncherModel {
        let mut model = LauncherModel::default();
        model.set_results(Results {
            calc: None,
            rows: names.iter().map(|n| row(n)).collect(),
        });
        model
    }

    fn key(key: Key) -> KeyEvent {
        KeyEvent::named(key)
    }

    fn with(mut event: KeyEvent, modifiers: Modifiers) -> KeyEvent {
        event.modifiers = modifiers;
        event
    }

    #[test]
    fn selection_wraps_in_both_directions() {
        let mut model = model_with(&["a", "b", "c"]);
        assert_eq!(
            model.handle_key(&key(Key::ArrowUp), &[]),
            Outcome::SelectionChanged
        );
        assert_eq!(model.selected(), 2);
        assert_eq!(
            model.handle_key(&key(Key::ArrowDown), &[]),
            Outcome::SelectionChanged
        );
        assert_eq!(model.selected(), 0);
    }

    #[test]
    fn arrows_without_rows_do_nothing() {
        let mut model = LauncherModel::default();
        assert_eq!(
            model.handle_key(&key(Key::ArrowDown), &[]),
            Outcome::Ignored
        );
        assert_eq!(model.handle_key(&key(Key::Enter), &[]), Outcome::Ignored);
    }

    #[test]
    fn single_row_selection_reports_no_change() {
        let mut model = model_with(&["only"]);
        assert_eq!(
            model.handle_key(&key(Key::ArrowDown), &[]),
            Outcome::Ignored
        );
    }

    #[test]
    fn enter_activates_the_selected_row() {
        let mut model = model_with(&["a", "b"]);
        model.handle_key(&key(Key::ArrowDown), &[]);
        assert_eq!(
            model.handle_key(&key(Key::Enter), &[]),
            Outcome::Activate(Activation::LaunchApp {
                desktop_id: "b.desktop".into()
            })
        );
    }

    #[test]
    fn shift_and_ctrl_enter_use_the_query_text() {
        let mut model = model_with(&["a"]);
        model.set_query("  rust book ");
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        assert_eq!(
            model.handle_key(&with(key(Key::Enter), shift), &[]),
            Outcome::Activate(Activation::WebSearch("rust book".into()))
        );
        assert_eq!(
            model.handle_key(&with(key(Key::Enter), ctrl), &[]),
            Outcome::Activate(Activation::RunInTerminal("rust book".into()))
        );
        model.set_query("> ls -la");
        assert_eq!(
            model.handle_key(&with(key(Key::Enter), ctrl), &[]),
            Outcome::Activate(Activation::RunShell("ls -la".into()))
        );
    }

    #[test]
    fn escape_closes() {
        let mut model = LauncherModel::default();
        assert_eq!(model.handle_key(&key(Key::Escape), &[]), Outcome::Close);
    }

    #[test]
    fn tab_skips_disabled_categories_and_wraps() {
        let mut model = LauncherModel::default();
        let mut seen = Vec::new();
        for _ in 0..5 {
            assert_eq!(
                model.handle_key(&key(Key::Tab), &[]),
                Outcome::CategoryChanged
            );
            seen.push(model.category());
        }
        assert_eq!(
            seen,
            [
                Category::Apps,
                Category::Commands,
                Category::Web,
                Category::All,
                Category::Apps
            ]
        );
        let back = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        model.handle_key(&with(key(Key::Tab), back), &[]);
        assert_eq!(model.category(), Category::All);
        model.handle_key(&with(key(Key::Tab), back), &[]);
        assert_eq!(model.category(), Category::Web);
    }

    #[test]
    fn disabled_categories_cannot_be_chosen() {
        let mut model = LauncherModel::default();
        assert!(!model.set_category(Category::Files));
        assert!(!model.set_category(Category::Settings));
        assert!(model.set_category(Category::Web));
        assert!(!model.set_category(Category::Web));
    }

    #[test]
    fn alt_digits_launch_pinned_apps() {
        let mut model = LauncherModel::default();
        let pins: Vec<String> = ["a.desktop", "b.desktop"].map(String::from).to_vec();
        let alt = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        let press = |c: &str| with(KeyEvent::character(c), alt);
        assert_eq!(
            model.handle_key(&press("2"), &pins),
            Outcome::Activate(Activation::LaunchApp {
                desktop_id: "b.desktop".into()
            })
        );
        assert_eq!(model.handle_key(&press("3"), &pins), Outcome::Ignored);
        assert_eq!(model.handle_key(&press("0"), &pins), Outcome::Ignored);
        assert_eq!(model.handle_key(&press("x"), &pins), Outcome::Ignored);
    }

    #[test]
    fn new_results_reset_the_selection() {
        let mut model = model_with(&["a", "b", "c"]);
        model.handle_key(&key(Key::ArrowDown), &[]);
        assert_eq!(model.selected(), 1);
        model.set_results(Results::default());
        assert_eq!(model.selected(), 0);
    }

    #[test]
    fn category_ids_round_trip() {
        for category in Category::ALL {
            assert_eq!(Category::from_id(category.id()), Some(category));
        }
        assert_eq!(Category::from_id("nope"), None);
    }
}
