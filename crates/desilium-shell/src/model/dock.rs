//! Dock content: pinned apps merged with running windows, indicators and click behaviour.

use super::windows::{WindowId, WindowInfo, WindowModel};

/// Most window dots drawn under one dock icon.
pub const MAX_DOTS: usize = 4;

/// What the dock needs to know about an installed application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    /// Desktop file id (`firefox.desktop`).
    pub desktop_id: String,
    /// Display name.
    pub name: String,
    /// Icon name or absolute path from the desktop file.
    pub icon: Option<String>,
    /// `Categories=` of the desktop file.
    pub categories: Vec<String>,
}

/// Application lookups, implemented by the real catalog and by test fakes.
pub trait AppLookup {
    /// The app installed under this desktop file id.
    fn by_desktop_id(&self, desktop_id: &str) -> Option<AppInfo>;
    /// The app a window with this Wayland app id belongs to.
    fn by_app_id(&self, app_id: &str) -> Option<AppInfo>;
}

/// One icon in the dock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockItem {
    /// Stable key: the desktop id, or `app:<app_id>` for windows without a desktop file.
    pub key: String,
    /// Desktop file id when the app is installed.
    pub desktop_id: Option<String>,
    /// Name shown in the tooltip.
    pub name: String,
    /// Icon name or path: the desktop file's, or for a window without one the name KWin reports.
    pub icon: Option<String>,
    /// Whether the user pinned it.
    pub pinned: bool,
    /// Its windows in creation order.
    pub windows: Vec<WindowId>,
}

/// One dot under an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dot {
    /// A window that is open but not focused.
    Open,
    /// The focused window.
    Focused,
}

/// What a click on an icon should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockClick {
    /// Not running: start the application.
    Launch,
    /// Bring this window up (restores it when minimized).
    Activate(WindowId),
    /// It is the only focused window: minimize it.
    Minimize(WindowId),
}

impl DockItem {
    /// Indicator dots: one per window, at most [`MAX_DOTS`], the focused one marked.
    pub fn dots(&self, model: &WindowModel) -> Vec<Dot> {
        self.windows
            .iter()
            .take(MAX_DOTS)
            .map(|id| match model.get(*id) {
                Some(window) if window.activated => Dot::Focused,
                _ => Dot::Open,
            })
            .collect()
    }

    /// Click behaviour; repeated clicks cycle through several windows in creation order.
    pub fn click(&self, model: &WindowModel) -> DockClick {
        let Some(newest_used) = self.most_recent(model) else {
            return DockClick::Launch;
        };
        let focused = self
            .windows
            .iter()
            .position(|id| model.get(*id).is_some_and(|w| w.activated));
        match (focused, self.windows.len()) {
            (Some(_), 1) => DockClick::Minimize(newest_used),
            (Some(at), count) => {
                let next = self.windows.get((at + 1) % count).copied();
                DockClick::Activate(next.unwrap_or(newest_used))
            }
            (None, _) => DockClick::Activate(newest_used),
        }
    }

    /// The most recently used window of this app.
    pub fn most_recent(&self, model: &WindowModel) -> Option<WindowId> {
        self.windows
            .iter()
            .min_by_key(|id| model.recency(**id).unwrap_or(usize::MAX))
            .copied()
    }
}

fn item_from_app(app: AppInfo, pinned: bool) -> DockItem {
    DockItem {
        key: app.desktop_id.clone(),
        desktop_id: Some(app.desktop_id),
        name: app.name,
        icon: app.icon,
        pinned,
        windows: Vec::new(),
    }
}

fn item_for_unknown(window: &WindowInfo) -> DockItem {
    let name = if window.app_id.is_empty() {
        window.title.clone()
    } else {
        window.app_id.clone()
    };
    DockItem {
        key: format!("app:{}", window.app_id),
        desktop_id: None,
        name,
        icon: Some(window.icon.clone()).filter(|icon| !icon.is_empty()),
        pinned: false,
        windows: Vec::new(),
    }
}

/// Merges pinned apps (in pinned order; uninstalled ones are skipped) with running windows
/// (apps without a pin follow in order of their oldest window).
pub fn build(pinned: &[String], model: &WindowModel, lookup: &dyn AppLookup) -> Vec<DockItem> {
    let mut items: Vec<DockItem> = pinned
        .iter()
        .filter_map(|id| lookup.by_desktop_id(id))
        .map(|app| item_from_app(app, true))
        .collect();
    for window in model.windows() {
        let key = match lookup.by_app_id(&window.app_id) {
            Some(app) => app.desktop_id,
            None => format!("app:{}", window.app_id),
        };
        let at = items.iter().position(|item| item.key == key);
        let index = match at {
            Some(index) => index,
            None => {
                let fresh = match lookup.by_app_id(&window.app_id) {
                    Some(app) => item_from_app(app, false),
                    None => item_for_unknown(window),
                };
                items.push(fresh);
                items.len() - 1
            }
        };
        if let Some(item) = items.get_mut(index) {
            item.windows.push(window.id);
        }
    }
    items
}

/// The pinned desktop ids after the app `moving` was dropped before slot `slot` of the dock.
///
/// `visible` are the pinned apps the dock shows, in order; `pinned` is the stored list, which may
/// also hold apps that are not installed. Those are not shown and keep their order behind the
/// shown ones. `slot` counts items of the dock, so it may point past the pinned ones (an
/// unpinned running app dropped there is pinned at the end). A pinned app that is moved leaves
/// its old place first, so `slot` after it counts one less.
pub fn reordered_pins(
    pinned: &[String],
    visible: &[String],
    moving: &str,
    slot: usize,
) -> Vec<String> {
    let old = visible.iter().position(|id| id == moving);
    let mut shown: Vec<String> = visible.iter().filter(|id| *id != moving).cloned().collect();
    let at = match old {
        Some(old) if old < slot => slot - 1,
        _ => slot,
    };
    shown.insert(at.min(shown.len()), moving.to_owned());
    shown.extend(
        pinned
            .iter()
            .filter(|id| *id != moving && !visible.contains(id))
            .cloned(),
    );
    shown
}

/// Index of the first running-but-unpinned item (where the separator goes).
pub fn separator_index(items: &[DockItem]) -> Option<usize> {
    let first = items.iter().position(|item| !item.pinned)?;
    (first > 0).then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn dropping_a_pinned_app_before_another_moves_it() {
        let pins = ids(&["a", "b", "c", "d"]);
        // Drag d before b (slot 1).
        assert_eq!(
            reordered_pins(&pins, &pins, "d", 1),
            ids(&["a", "d", "b", "c"])
        );
        // Drag a behind c (slot 3, counted with a still in place).
        assert_eq!(
            reordered_pins(&pins, &pins, "a", 3),
            ids(&["b", "c", "a", "d"])
        );
        // Dropped on its own slot or the one after: nothing changes.
        assert_eq!(reordered_pins(&pins, &pins, "b", 1), pins);
        assert_eq!(reordered_pins(&pins, &pins, "b", 2), pins);
        // Behind the last one.
        assert_eq!(
            reordered_pins(&pins, &pins, "a", 4),
            ids(&["b", "c", "d", "a"])
        );
    }

    #[test]
    fn a_running_app_dropped_among_the_pins_gets_pinned_there() {
        let pins = ids(&["a", "b"]);
        assert_eq!(reordered_pins(&pins, &pins, "z", 1), ids(&["a", "z", "b"]));
        assert_eq!(
            reordered_pins(&pins, &pins, "z", 7),
            ids(&["a", "b", "z"]),
            "a slot beyond the pinned apps pins it last"
        );
        assert_eq!(reordered_pins(&[], &[], "z", 0), ids(&["z"]));
    }

    #[test]
    fn pinned_apps_that_are_not_installed_keep_their_place_behind_the_shown_ones() {
        let pins = ids(&["a", "gone", "b"]);
        let shown = ids(&["a", "b"]);
        assert_eq!(
            reordered_pins(&pins, &shown, "b", 0),
            ids(&["b", "a", "gone"])
        );
        assert_eq!(
            reordered_pins(&pins, &shown, "gone", 0),
            ids(&["gone", "a", "b"])
        );
    }

    struct Fake(Vec<AppInfo>);

    impl AppLookup for Fake {
        fn by_desktop_id(&self, desktop_id: &str) -> Option<AppInfo> {
            self.0.iter().find(|a| a.desktop_id == desktop_id).cloned()
        }
        fn by_app_id(&self, app_id: &str) -> Option<AppInfo> {
            let stem = |a: &AppInfo| a.desktop_id.trim_end_matches(".desktop").to_owned();
            self.0.iter().find(|a| stem(a) == app_id).cloned()
        }
    }

    fn app(stem: &str) -> AppInfo {
        AppInfo {
            desktop_id: format!("{stem}.desktop"),
            name: stem.to_uppercase(),
            icon: None,
            categories: Vec::new(),
        }
    }

    fn window(id: u32, app_id: &str, activated: bool) -> WindowInfo {
        WindowInfo {
            app_id: app_id.to_owned(),
            activated,
            ..WindowInfo::new(WindowId(id))
        }
    }

    fn model(windows: Vec<WindowInfo>) -> WindowModel {
        let mut model = WindowModel::default();
        model.replace(windows);
        model
    }

    fn lookup() -> Fake {
        Fake(vec![app("foot"), app("firefox"), app("code")])
    }

    fn pins(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn pinned_come_first_then_running_in_creation_order() {
        let windows = model(vec![
            window(1, "code", false),
            window(2, "mystery", false),
            window(3, "foot", true),
        ]);
        let items = build(
            &pins(&["firefox.desktop", "foot.desktop"]),
            &windows,
            &lookup(),
        );
        let keys: Vec<_> = items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "firefox.desktop",
                "foot.desktop",
                "code.desktop",
                "app:mystery"
            ]
        );
        assert_eq!(separator_index(&items), Some(2));
        assert!(items[0].pinned && items[0].windows.is_empty());
        assert_eq!(items[1].windows, [WindowId(3)]);
        assert_eq!(items[3].name, "mystery");
        assert_eq!(items[3].desktop_id, None);
    }

    #[test]
    fn uninstalled_pins_are_skipped() {
        let items = build(
            &pins(&["gone.desktop", "foot.desktop"]),
            &model(vec![]),
            &lookup(),
        );
        assert_eq!(items.len(), 1);
        assert_eq!(separator_index(&items), None);
    }

    #[test]
    fn windows_of_one_app_share_an_item() {
        let windows = model(vec![window(1, "foot", false), window(2, "foot", true)]);
        let items = build(&[], &windows, &lookup());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].windows, [WindowId(1), WindowId(2)]);
        assert_eq!(items[0].dots(&windows), [Dot::Open, Dot::Focused]);
        assert_eq!(
            separator_index(&items),
            None,
            "no pinned items, no separator"
        );
    }

    #[test]
    fn dots_are_capped() {
        let windows = model((1..=7).map(|i| window(i, "foot", false)).collect());
        let items = build(&[], &windows, &lookup());
        assert_eq!(items[0].dots(&windows).len(), MAX_DOTS);
    }

    #[test]
    fn click_launches_when_not_running() {
        let windows = model(vec![]);
        let items = build(&pins(&["foot.desktop"]), &windows, &lookup());
        assert_eq!(items[0].click(&windows), DockClick::Launch);
    }

    #[test]
    fn click_activates_an_unfocused_or_minimized_window() {
        let mut minimized = window(1, "foot", false);
        minimized.minimized = true;
        let windows = model(vec![minimized, window(2, "code", true)]);
        let items = build(&[], &windows, &lookup());
        assert_eq!(items[0].click(&windows), DockClick::Activate(WindowId(1)));
    }

    #[test]
    fn click_on_the_only_focused_window_minimizes_it() {
        let windows = model(vec![window(1, "foot", true)]);
        let items = build(&[], &windows, &lookup());
        assert_eq!(items[0].click(&windows), DockClick::Minimize(WindowId(1)));
    }

    #[test]
    fn repeated_clicks_cycle_through_all_windows() {
        let ids = [1, 2, 3];
        let at = |focused: u32| {
            model(
                ids.iter()
                    .map(|i| window(*i, "foot", *i == focused))
                    .collect(),
            )
        };
        let expect = [(1, 2), (2, 3), (3, 1)];
        for (focused, next) in expect {
            let windows = at(focused);
            let items = build(&[], &windows, &lookup());
            assert_eq!(
                items[0].click(&windows),
                DockClick::Activate(WindowId(next))
            );
        }
    }

    #[test]
    fn unfocused_app_with_several_windows_activates_the_most_recent() {
        let mut windows = model(vec![
            window(1, "foot", true),
            window(2, "foot", false),
            window(3, "code", false),
        ]);
        windows.replace(vec![
            window(1, "foot", false),
            window(2, "foot", false),
            window(3, "code", true),
        ]);
        let items = build(&[], &windows, &lookup());
        assert_eq!(items[0].click(&windows), DockClick::Activate(WindowId(1)));
    }
}
