//! Maps a Wayland `app_id` (or X11 class) onto a catalog entry.

use std::collections::BTreeMap;
use std::path::Path;

use super::Item;
use crate::desktop::DesktopEntry;
use crate::launch::expand::program_of;

/// Finds the entry a window with `app_id` belongs to, trying the usual heuristics in order.
pub(crate) fn find<'a>(
    items: &'a BTreeMap<String, Item>,
    app_id: &str,
) -> Option<&'a DesktopEntry> {
    let wanted = app_id.trim();
    if wanted.is_empty() {
        return None;
    }
    if let Some(item) = items
        .get(wanted)
        .or_else(|| items.get(&format!("{wanted}.desktop")))
    {
        return Some(&item.entry);
    }
    let lowered = wanted.to_lowercase();
    first(items, |entry| id_matches_lowercase(entry, &lowered))
        .or_else(|| {
            first(items, |entry| {
                entry.startup_wm_class.as_deref() == Some(wanted)
            })
        })
        .or_else(|| {
            first(items, |entry| {
                entry
                    .startup_wm_class
                    .as_deref()
                    .is_some_and(|class| class.to_lowercase() == lowered)
            })
        })
        .or_else(|| {
            first(items, |entry| {
                exec_basename(entry).as_deref() == Some(lowered.as_str())
            })
        })
        .or_else(|| first(items, |entry| last_id_segment(entry) == lowered))
}

fn first(
    items: &BTreeMap<String, Item>,
    matches: impl Fn(&DesktopEntry) -> bool,
) -> Option<&DesktopEntry> {
    items
        .values()
        .map(|item| &item.entry)
        .find(|entry| matches(entry))
}

fn id_matches_lowercase(entry: &DesktopEntry, lowered: &str) -> bool {
    let id = entry.id.as_str().to_lowercase();
    id == lowered || id == format!("{lowered}.desktop")
}

fn exec_basename(entry: &DesktopEntry) -> Option<String> {
    let program = program_of(&entry.exec)?;
    let name = Path::new(&program).file_name()?.to_str()?;
    Some(name.to_lowercase())
}

/// `org.gnome.Nautilus.desktop` yields `nautilus`.
fn last_id_segment(entry: &DesktopEntry) -> String {
    entry
        .id
        .stem()
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::{DesktopEntry, DesktopId};
    use std::path::PathBuf;

    fn item(id: &str, exec: &str, wm_class: Option<&str>) -> (String, Item) {
        let entry = DesktopEntry {
            id: DesktopId::new(id).expect("valid id"),
            path: PathBuf::from(id),
            name: id.to_owned(),
            generic_name: None,
            comment: None,
            keywords: Vec::new(),
            icon: None,
            exec: exec.to_owned(),
            try_exec: None,
            working_dir: None,
            terminal: false,
            categories: Vec::new(),
            mime_types: Vec::new(),
            no_display: false,
            hidden: false,
            only_show_in: Vec::new(),
            not_show_in: Vec::new(),
            startup_wm_class: wm_class.map(str::to_owned),
            actions: Vec::new(),
        };
        (
            id.to_owned(),
            Item {
                entry,
                listed: true,
            },
        )
    }

    fn table() -> BTreeMap<String, Item> {
        [
            item(
                "org.gnome.Nautilus.desktop",
                "nautilus --new-window %U",
                None,
            ),
            item("code.desktop", "/usr/bin/code %F", Some("Code")),
            item("foot.desktop", "foot", None),
        ]
        .into_iter()
        .collect()
    }

    fn matched(app_id: &str) -> Option<String> {
        find(&table(), app_id).map(|entry| entry.id.as_str().to_owned())
    }

    #[test]
    fn heuristic_table() {
        let cases: &[(&str, Option<&str>)] = &[
            ("foot", Some("foot.desktop")),
            ("foot.desktop", Some("foot.desktop")),
            ("FOOT", Some("foot.desktop")),
            ("Code", Some("code.desktop")),
            ("nautilus", Some("org.gnome.Nautilus.desktop")),
            ("org.gnome.Nautilus", Some("org.gnome.Nautilus.desktop")),
            ("unknown-app", None),
            ("  ", None),
        ];
        for (app_id, expected) in cases {
            assert_eq!(matched(app_id).as_deref(), *expected, "app_id {app_id:?}");
        }
    }
}
