//! HTML of the launcher panel.

use std::fmt::Write as _;

use desilium_ui::icon_svg;

use crate::{
    model::launcher::{
        Activation, CalcCard, Category, LauncherModel, PINNED_SLOTS, Row, RowIcon, Section,
    },
    view::{
        esc,
        icon::{IconSpec, icon_html},
    },
};

const ENTER_PX: u32 = 16;

/// A pinned app in the bottom strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PinView {
    /// Label.
    pub name: String,
    /// Icon.
    pub icon: IconSpec,
}

/// Supplies row icons; the controller resolves them against the icon theme.
pub(crate) trait RowIcons {
    /// Icon of an application row at `px` logical pixels.
    fn app_icon(&mut self, icon: &RowIcon, px: u32) -> IconSpec;
}

fn enter_kbd() -> String {
    format!(
        "<span class=kbd>{}</span>",
        icon_svg("enter", ENTER_PX, "s16")
    )
}

fn kbd(text: &str) -> String {
    format!("<span class=kbd>{text}</span>")
}

/// Title with the highlighted byte ranges wrapped in `<b class=hl>`.
pub(super) fn title_html(row: &Row) -> String {
    let mut out = String::new();
    let mut at = 0;
    for range in &row.highlights {
        let valid = range.start >= at
            && range.end <= row.title.len()
            && row.title.is_char_boundary(range.start)
            && row.title.is_char_boundary(range.end);
        if !valid {
            continue;
        }
        out.push_str(&esc(&row.title[at..range.start]));
        let _ = write!(out, "<b class=hl>{}</b>", esc(&row.title[range.clone()]));
        at = range.end;
    }
    out.push_str(&esc(&row.title[at..]));
    out
}

fn meta_html(row: &Row, selected: bool, pinned: &[String]) -> String {
    if selected {
        let verb = match row.activation {
            Activation::LaunchApp { .. } => "Open",
            Activation::RunShell(_) | Activation::RunInTerminal(_) => "Run",
            Activation::WebSearch(_) => "Search",
        };
        return format!("{verb}{}", enter_kbd());
    }
    match &row.activation {
        Activation::LaunchApp { desktop_id } => {
            let slot = pinned
                .iter()
                .take(PINNED_SLOTS)
                .position(|id| id == desktop_id);
            slot.map(|slot| format!("{}{}", kbd("Alt"), kbd(&(slot + 1).to_string())))
                .unwrap_or_default()
        }
        Activation::RunShell(_) | Activation::RunInTerminal(_) => {
            format!("{}{}", kbd("Ctrl"), enter_kbd())
        }
        Activation::WebSearch(_) => format!("{}{}", kbd("Shift"), enter_kbd()),
    }
}

fn row_icon_html(row: &Row, icons: &mut dyn RowIcons) -> String {
    match &row.icon {
        RowIcon::App { .. } => {
            let spec = icons.app_icon(&row.icon, 32);
            icon_html(&spec, "md", 32)
        }
        RowIcon::Command => format!(
            "<span class=\"tile cmd\">{}</span>",
            icon_svg("terminal", 20, "")
        ),
        RowIcon::Web => format!(
            "<span class=\"tile web\">{}</span>",
            icon_svg("globe", 20, "")
        ),
    }
}

fn row_html(
    index: usize,
    row: &Row,
    model: &LauncherModel,
    pinned: &[String],
    icons: &mut dyn RowIcons,
) -> String {
    let selected = model.selected() == index;
    format!(
        "<div class=\"r{sel}\" id=r{index} data-action=activate data-arg={index}>{icon}\
         <div class=txt><span class=ti>{title}</span><span class=su>{sub}</span></div>\
         <div class=meta>{meta}</div></div>",
        sel = if selected { " sel" } else { "" },
        icon = row_icon_html(row, icons),
        title = title_html(row),
        sub = esc(&row.subtitle),
        meta = meta_html(row, selected, pinned),
    )
}

fn calc_html(card: &CalcCard) -> String {
    format!(
        "<div class=sec><span class=t-caption>Calculator</span></div>\
         <div class=calc-card><div class=half><span class=t-caption>Expression</span><span class=expr>{}</span></div>\
         <span class=eq>=</span>\
         <div class=half><span class=t-caption>Result</span><span class=res>{}</span></div></div>",
        esc(&card.expression),
        esc(&card.result)
    )
}

fn empty_message(model: &LauncherModel) -> &'static str {
    if model.query().trim().is_empty() {
        "Type to search apps, run commands or calculate"
    } else {
        "Nothing found"
    }
}

/// The result list: calculator card, then rows grouped under section headings.
pub(super) fn results_html(
    model: &LauncherModel,
    pinned: &[String],
    icons: &mut dyn RowIcons,
) -> String {
    let results = model.results();
    let mut out = String::new();
    if let Some(card) = &results.calc {
        out.push_str(&calc_html(card));
    }
    let mut section: Option<Section> = None;
    for (index, row) in results.rows.iter().enumerate() {
        if section != Some(row.section) {
            section = Some(row.section);
            let _ = write!(
                out,
                "<div class=sec><span class=t-caption>{}</span></div>",
                row.section.title()
            );
        }
        out.push_str(&row_html(index, row, model, pinned, icons));
    }
    if results.rows.is_empty() && results.calc.is_none() {
        let _ = write!(out, "<div class=empty>{}</div>", empty_message(model));
    }
    out
}

/// The category chips and the Tab hint.
pub(super) fn chips_html(active: Category) -> String {
    let mut out = String::new();
    for category in Category::ALL {
        let icon = category
            .icon()
            .map(|name| icon_svg(name, 16, "s16"))
            .unwrap_or_default();
        let state = if category == active {
            " on"
        } else if !category.enabled() {
            " dis"
        } else {
            ""
        };
        let attrs = if category.enabled() {
            format!("data-action=category data-arg={}", category.id())
        } else {
            "data-tooltip=\"Coming soon\"".to_owned()
        };
        let _ = write!(
            out,
            "<span class=\"cat{state}\" {attrs}>{icon}{}</span>",
            category.label()
        );
    }
    let _ = write!(
        out,
        "<span class=grow></span><span class=\"row t-small t-3\" style=\"gap:6px\">{}switch category</span>",
        kbd("Tab")
    );
    out
}

/// The pinned strip contents.
pub(super) fn pins_html(pins: &[PinView]) -> String {
    let mut out = String::from("<div class=\"lbl t-caption\">Pinned</div>");
    for (slot, pin) in pins.iter().take(PINNED_SLOTS).enumerate() {
        let _ = write!(
            out,
            "<div class=pin data-action=pin data-arg={slot}>{}<span>{}</span><span class=k>Alt {}</span></div>",
            icon_html(&pin.icon, "lg", 56),
            esc(&pin.name),
            slot + 1
        );
    }
    out
}

/// The complete launcher document body.
pub(super) fn launcher_html(
    model: &LauncherModel,
    pins: &[PinView],
    pinned: &[String],
    icons: &mut dyn RowIcons,
) -> String {
    format!(
        "<div class=launcher id=panel>\
         <div class=l-search>{search}<input id=q data-key=query type=text placeholder=\"Search apps, run commands, calculate\" value=\"{query}\">{esc_kbd}</div>\
         <div class=l-chips id=chips>{chips}</div>\
         <div class=l-results id=results>{results}</div>\
         <div class=pins id=pins>{pins}</div>\
         <div class=hints>\
           <span class=h>{up}{down}navigate</span><span class=h>{enter}open</span><span class=h>{tab}category</span>\
           <span class=h>{alt}{one}pinned</span><span class=grow></span>\
           <span class=h>{sparkles}Everything here is also in Settings</span></div></div>",
        search = icon_svg("search", 24, "s24"),
        query = esc(model.query()),
        esc_kbd = kbd("Esc"),
        chips = chips_html(model.category()),
        results = results_html(model, pinned, icons),
        pins = pins_html(pins),
        up = kbd("\u{2191}"),
        down = kbd("\u{2193}"),
        enter = enter_kbd(),
        tab = kbd("Tab"),
        alt = kbd("Alt"),
        one = kbd("1\u{2013}6"),
        sparkles = icon_svg("sparkles", 16, "s16"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::launcher::Results;

    struct Tiles;

    impl RowIcons for Tiles {
        fn app_icon(&mut self, _: &RowIcon, _: u32) -> IconSpec {
            IconSpec::NEUTRAL
        }
    }

    fn app_row(title: &str, highlights: Vec<std::ops::Range<usize>>) -> Row {
        Row {
            section: Section::Applications,
            activation: Activation::LaunchApp {
                desktop_id: format!("{title}.desktop"),
            },
            title: title.to_owned(),
            subtitle: "Sub".into(),
            highlights,
            icon: RowIcon::App {
                desktop_id: format!("{title}.desktop"),
                icon: None,
            },
        }
    }

    fn model(rows: Vec<Row>, calc: Option<CalcCard>) -> LauncherModel {
        let mut model = LauncherModel::default();
        model.set_results(Results { calc, rows });
        model
    }

    #[test]
    fn highlights_wrap_the_matched_text_and_survive_bad_ranges() {
        let row = app_row("Firefox", vec![0..4, 90..95]);
        assert_eq!(title_html(&row), "<b class=hl>Fire</b>fox");
        let multi = app_row("Files & Fonts", vec![0..1, 8..9]);
        assert_eq!(
            title_html(&multi),
            "<b class=hl>F</b>iles &amp; <b class=hl>F</b>onts"
        );
        let unicode = app_row("Caf\u{e9}", std::iter::once(3..4).collect());
        assert_eq!(
            title_html(&unicode),
            "Caf\u{e9}",
            "range inside a char is skipped"
        );
    }

    #[test]
    fn exactly_one_row_is_selected_and_shows_open() {
        let m = model(vec![app_row("A", vec![]), app_row("B", vec![])], None);
        let html = results_html(&m, &[], &mut Tiles);
        assert_eq!(html.matches("class=\"r sel\"").count(), 1);
        assert!(html.contains("Open"));
        assert_eq!(
            html.matches("class=sec").count(),
            1,
            "one Applications heading"
        );
    }

    #[test]
    fn pinned_apps_show_their_alt_slot() {
        let m = model(vec![app_row("A", vec![]), app_row("B", vec![])], None);
        let pinned = vec!["A.desktop".to_owned(), "B.desktop".to_owned()];
        let html = results_html(&m, &pinned, &mut Tiles);
        assert!(html.contains(">Alt<"));
        assert!(
            html.contains(">2<"),
            "row B is slot 2 (row A is selected and shows Open)"
        );
    }

    #[test]
    fn calculator_card_and_empty_state() {
        let card = CalcCard {
            expression: "24 \u{d7} 7".into(),
            result: "168".into(),
        };
        let html = results_html(&model(vec![], Some(card)), &[], &mut Tiles);
        assert!(html.contains("168") && html.contains("calc-card"));
        let empty = results_html(&model(vec![], None), &[], &mut Tiles);
        assert!(empty.contains("class=empty"));
    }

    #[test]
    fn chips_mark_the_active_and_disable_unfinished_ones() {
        let html = chips_html(Category::Apps);
        assert_eq!(html.matches("cat on").count(), 1);
        assert_eq!(html.matches("cat dis").count(), 2);
        assert_eq!(html.matches("Coming soon").count(), 2);
        assert!(html.contains("data-arg=web"));
        assert!(!html.contains("data-arg=files"));
    }

    #[test]
    fn query_is_escaped_into_the_input() {
        let mut m = LauncherModel::default();
        m.set_query("\"><script>");
        let html = launcher_html(&m, &[], &[], &mut Tiles);
        assert!(!html.contains("\"><script>"));
        assert!(html.contains("&quot;&gt;&lt;script&gt;"));
    }

    #[test]
    fn pins_are_limited_to_six_slots() {
        let pin = PinView {
            name: "X".into(),
            icon: IconSpec::NEUTRAL,
        };
        let html = pins_html(&vec![pin; 9]);
        assert_eq!(html.matches("class=pin ").count(), 6);
        assert!(html.contains("Alt 6") && !html.contains("Alt 7"));
    }
}
