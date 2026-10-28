//! Desktop icons: the state of the icon grid (selection, gestures, menu, rename) and its HTML.
//!
//! The icons live in the wallpaper's own layer surface, which covers the screen, so the shell
//! needs no extra window. What the icons are and what can be done to them is in
//! `model::desktop` and `services::desktop`, where they sit is `model::grid`; reading input is
//! `app::desktop`.

mod view;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::model::{
    desktop::{DeskItem, Kind},
    grid::{self, Cell, Grid},
};

pub use view::html;

/// Two clicks on the same icon within this time open it.
const DOUBLE_CLICK: Duration = Duration::from_millis(450);
/// How far the selection rectangle must overlap an icon's cell to select it, per side.
const BAND_INSET: i32 = 8;

/// A command of the desktop's context menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Open the icon (start a launcher, open a folder or file).
    Open,
    /// Let a double click start an untrusted launcher.
    AllowLaunching,
    /// Start renaming the icon.
    Rename,
    /// Move the selected icons to the trash.
    Trash,
    /// Create an empty folder where the menu was opened.
    NewFolder,
    /// Create an empty text file where the menu was opened.
    NewFile,
    /// Open the desktop folder in the file manager.
    OpenFolder,
    /// Open the desktop settings.
    Settings,
}

impl Command {
    /// The name carried by the menu button.
    pub fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::AllowLaunching => "allow",
            Self::Rename => "rename",
            Self::Trash => "trash",
            Self::NewFolder => "new-folder",
            Self::NewFile => "new-file",
            Self::OpenFolder => "open-folder",
            Self::Settings => "settings",
        }
    }

    /// The command behind a menu button's name.
    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Open,
            Self::AllowLaunching,
            Self::Rename,
            Self::Trash,
            Self::NewFolder,
            Self::NewFile,
            Self::OpenFolder,
            Self::Settings,
        ]
        .into_iter()
        .find(|command| command.name() == name)
    }

    /// The menu text.
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::AllowLaunching => "Allow launching",
            Self::Rename => "Rename",
            Self::Trash => "Move to Trash",
            Self::NewFolder => "New folder",
            Self::NewFile => "New file",
            Self::OpenFolder => "Open desktop folder",
            Self::Settings => "Desktop settings",
        }
    }
}

/// An open context menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    /// File name of the icon it belongs to; `None` for the desktop itself.
    pub target: Option<String>,
    /// Where it opened, in surface pixels. New icons are created in the cell under this point.
    pub at: (i32, i32),
}

/// What a click on an icon asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// Only select it.
    Select,
    /// It was the second click: open it.
    Open,
}

/// A pointer gesture in progress.
#[derive(Debug, Clone, PartialEq)]
pub enum Gesture {
    /// Icons are being carried. `anchor` is the one under the pointer; the selected ones follow it.
    Icons {
        /// File name of the icon the user holds.
        anchor: String,
        /// Where the button went down.
        origin: (f64, f64),
        /// Where the pointer is.
        pointer: (f64, f64),
        /// The cell the anchor would land in.
        target: Cell,
    },
    /// A rectangle is being drawn to select the icons it touches.
    Band {
        /// Where the button went down.
        from: (f64, f64),
        /// Where the pointer is.
        to: (f64, f64),
    },
}

/// The desktop's changing state.
#[derive(Debug, Default)]
pub struct Desktop {
    /// The desktop folder, or `None` when there is none.
    pub dir: Option<PathBuf>,
    /// What is on the desktop, in listing order.
    pub items: Vec<DeskItem>,
    /// File names of the selected icons.
    pub selected: BTreeSet<String>,
    /// The open menu, if any.
    pub menu: Option<Menu>,
    /// File name of the icon being renamed.
    pub renaming: Option<String>,
    /// File name of an icon that was just created, to be renamed once it shows up.
    pub rename_when_listed: Option<String>,
    /// The last problem, shown until the next action.
    pub notice: String,
    /// Which cell each icon sits in. Saved between runs; icons without an entry take free cells.
    pub layout: BTreeMap<String, Cell>,
    /// `layout` changed since it was last saved.
    pub layout_dirty: bool,
    /// The drag or selection rectangle in progress.
    pub gesture: Option<Gesture>,
    last_click: Option<(String, Instant)>,
}

impl Desktop {
    /// The icon at `index` of the listing.
    pub fn item(&self, index: usize) -> Option<&DeskItem> {
        self.items.get(index)
    }

    fn names(&self) -> Vec<&str> {
        self.items
            .iter()
            .map(|item| item.file_name.as_str())
            .collect()
    }

    /// Takes a new listing. Selections and renames of icons that are gone are dropped, and an
    /// icon waiting to be renamed enters rename mode as soon as it is there. Saved positions of
    /// icons that vanished are forgotten, unless the listing is empty (a failed read must not
    /// erase the layout).
    pub fn set_items(&mut self, items: Vec<DeskItem>) {
        self.items = items;
        let present: BTreeSet<String> = self.items.iter().map(|i| i.file_name.clone()).collect();
        self.selected.retain(|name| present.contains(name));
        if self
            .renaming
            .as_ref()
            .is_some_and(|name| !present.contains(name))
        {
            self.renaming = None;
        }
        if !present.is_empty() {
            let before = self.layout.len();
            self.layout.retain(|name, _| present.contains(name));
            self.layout_dirty |= self.layout.len() != before;
        }
        if let Some(name) = self.rename_when_listed.take() {
            if present.contains(&name) {
                self.selected = BTreeSet::from([name.clone()]);
                self.renaming = Some(name);
            } else {
                self.rename_when_listed = Some(name);
            }
        }
    }

    /// The cell of every icon, in listing order, and remembers the cells of icons that had none.
    pub fn cells(&mut self, grid: &Grid) -> Vec<Cell> {
        let placed = grid.place(&self.names(), &self.layout);
        for (index, cell) in placed.iter().enumerate() {
            let name = self.items[index].file_name.clone();
            if let std::collections::btree_map::Entry::Vacant(slot) = self.layout.entry(name) {
                slot.insert(*cell);
                self.layout_dirty = true;
            }
        }
        placed
    }

    fn placed(&self, grid: &Grid) -> Vec<Cell> {
        grid.place(&self.names(), &self.layout)
    }

    /// Puts a not yet listed icon into the free cell nearest to `wanted`.
    pub fn reserve_near(&mut self, name: &str, wanted: Cell, grid: &Grid) {
        let taken = self.placed(grid).into_iter().collect();
        let cell = grid.nearest_free(wanted, &taken);
        self.layout.insert(name.to_owned(), cell);
        self.layout_dirty = true;
    }

    /// An icon was renamed: it keeps its place.
    pub fn renamed(&mut self, old: &str, new: &str) {
        if let Some(cell) = self.layout.remove(old) {
            self.layout.insert(new.to_owned(), cell);
            self.layout_dirty = true;
        }
    }

    /// Selects the icon at `index` and says whether this click was the second of a double click.
    pub fn click(&mut self, index: usize, now: Instant) -> Click {
        self.menu = None;
        self.notice.clear();
        let Some(name) = self.item(index).map(|item| item.file_name.clone()) else {
            return Click::Select;
        };
        if self.renaming.as_deref() != Some(name.as_str()) {
            self.renaming = None;
        }
        let double = self
            .last_click
            .take()
            .is_some_and(|(last, at)| last == name && now.duration_since(at) <= DOUBLE_CLICK);
        self.selected = BTreeSet::from([name.clone()]);
        if double {
            return Click::Open;
        }
        self.last_click = Some((name, now));
        Click::Select
    }

    /// Right click on the icon at `index`: it is selected unless it already belongs to the
    /// selection (then the menu applies to the whole selection).
    pub fn select_for_menu(&mut self, index: usize) {
        self.last_click = None;
        self.notice.clear();
        let Some(name) = self.item(index).map(|item| item.file_name.clone()) else {
            return;
        };
        if !self.selected.contains(&name) {
            self.selected = BTreeSet::from([name]);
        }
    }

    /// A click on empty desktop: nothing is selected, no menu or rename stays open.
    pub fn clear(&mut self) {
        self.selected.clear();
        self.menu = None;
        self.renaming = None;
        self.last_click = None;
        self.gesture = None;
        self.notice.clear();
    }

    /// The commands of the menu of `target`.
    pub fn commands(&self, target: Option<&str>) -> Vec<Command> {
        let item = target.and_then(|name| self.items.iter().find(|item| item.file_name == name));
        let Some(item) = item else {
            return vec![
                Command::NewFolder,
                Command::NewFile,
                Command::OpenFolder,
                Command::Settings,
            ];
        };
        if self.selected.len() > 1 && self.selected.contains(&item.file_name) {
            return vec![Command::Trash];
        }
        let mut commands = vec![Command::Open];
        if item.kind == (Kind::Launcher { trusted: false }) {
            commands.push(Command::AllowLaunching);
        }
        commands.extend([Command::Rename, Command::Trash]);
        commands
    }

    /// The icons a command on `target` applies to: the whole selection when `target` is part of
    /// it, else `target` alone.
    pub fn affected(&self, target: &str) -> Vec<String> {
        if self.selected.contains(target) {
            return self
                .items
                .iter()
                .filter(|item| self.selected.contains(&item.file_name))
                .map(|item| item.file_name.clone())
                .collect();
        }
        vec![target.to_owned()]
    }

    /// The button went down on the icon at `index` and the pointer moved: the icons are carried.
    /// The selection becomes that icon alone unless it was part of it.
    pub fn begin_icon_drag(&mut self, index: usize, origin: (f64, f64), grid: &Grid) {
        let Some(name) = self.item(index).map(|item| item.file_name.clone()) else {
            return;
        };
        self.menu = None;
        self.renaming = None;
        self.last_click = None;
        if !self.selected.contains(&name) {
            self.selected = BTreeSet::from([name.clone()]);
        }
        let target = grid.cell_at(origin.0, origin.1);
        self.gesture = Some(Gesture::Icons {
            anchor: name,
            origin,
            pointer: origin,
            target,
        });
    }

    /// The pointer moved while icons are carried.
    pub fn drag_icons(&mut self, pointer: (f64, f64), grid: &Grid) {
        if let Some(Gesture::Icons {
            pointer: held,
            target,
            ..
        }) = &mut self.gesture
        {
            *held = pointer;
            *target = grid.cell_at(pointer.0, pointer.1);
        }
    }

    /// The button went up: the carried icons land around the cell under the pointer.
    pub fn drop_icons(&mut self, grid: &Grid) {
        let Some(Gesture::Icons { anchor, target, .. }) = self.gesture.take() else {
            return;
        };
        let placed = self.placed(grid);
        let mut current: BTreeMap<String, Cell> = self
            .items
            .iter()
            .zip(placed)
            .map(|(item, cell)| (item.file_name.clone(), cell))
            .collect();
        let moving = self.affected(&anchor);
        grid.move_group(&mut current, &moving, &anchor, target);
        for name in moving {
            if let Some(cell) = current.get(&name) {
                self.layout.insert(name, *cell);
            }
        }
        self.layout_dirty = true;
    }

    /// The button went down on empty desktop and the pointer moved: a selection rectangle starts.
    pub fn begin_band(&mut self, origin: (f64, f64)) {
        self.menu = None;
        self.renaming = None;
        self.selected.clear();
        self.last_click = None;
        self.notice.clear();
        self.gesture = Some(Gesture::Band {
            from: origin,
            to: origin,
        });
    }

    /// The pointer moved while the rectangle is drawn: it selects the icons it touches.
    pub fn drag_band(&mut self, pointer: (f64, f64), grid: &Grid) {
        let Some(Gesture::Band { from, to }) = &mut self.gesture else {
            return;
        };
        *to = pointer;
        let band = grid::spanned(*from, pointer);
        let hit: BTreeSet<String> = self
            .items
            .iter()
            .zip(self.placed(grid))
            .filter(|(_, cell)| {
                let (x, y, w, h) = grid.rect(*cell);
                let inner = (
                    x + BAND_INSET,
                    y + BAND_INSET,
                    w - 2 * BAND_INSET,
                    h - 2 * BAND_INSET,
                );
                grid::intersects(band, inner)
            })
            .map(|(item, _)| item.file_name.clone())
            .collect();
        self.selected = hit;
    }

    /// A gesture ended or was cancelled; what it selected stays.
    pub fn end_gesture(&mut self) {
        self.gesture = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, kind: Kind) -> DeskItem {
        DeskItem {
            path: PathBuf::from(format!("/d/{name}")),
            file_name: name.to_owned(),
            name: name.to_owned(),
            kind,
            icon: None,
        }
    }

    fn grid() -> Grid {
        Grid::new((1280, 720), 32, 100)
    }

    fn desktop() -> Desktop {
        let mut desktop = Desktop::default();
        desktop.set_items(vec![
            item("Docs", Kind::Folder),
            item("a.txt", Kind::File),
            item("x.desktop", Kind::Launcher { trusted: false }),
        ]);
        desktop
    }

    fn selection(desktop: &Desktop) -> Vec<&str> {
        desktop.selected.iter().map(String::as_str).collect()
    }

    #[test]
    fn a_second_click_on_the_same_icon_within_the_time_opens_it() {
        let mut desktop = desktop();
        let start = Instant::now();
        assert_eq!(desktop.click(1, start), Click::Select);
        assert_eq!(selection(&desktop), ["a.txt"]);
        assert_eq!(
            desktop.click(1, start + Duration::from_millis(200)),
            Click::Open
        );
        assert_eq!(
            desktop.click(1, start + Duration::from_millis(300)),
            Click::Select,
            "a double click is used up"
        );
    }

    #[test]
    fn slow_clicks_and_clicks_on_other_icons_do_not_open() {
        let mut desktop = desktop();
        let start = Instant::now();
        desktop.click(1, start);
        assert_eq!(
            desktop.click(1, start + Duration::from_secs(2)),
            Click::Select
        );
        desktop.click(0, start + Duration::from_secs(3));
        assert_eq!(
            desktop.click(1, start + Duration::from_secs(3)),
            Click::Select
        );
    }

    #[test]
    fn clicking_the_empty_desktop_clears_everything() {
        let mut desktop = desktop();
        desktop.select_for_menu(0);
        desktop.menu = Some(Menu {
            target: None,
            at: (1, 1),
        });
        desktop.renaming = Some("Docs".into());
        desktop.notice = "x".into();
        desktop.begin_band((1.0, 1.0));
        desktop.clear();
        assert!(
            desktop.selected.is_empty() && desktop.menu.is_none() && desktop.renaming.is_none()
        );
        assert!(desktop.notice.is_empty() && desktop.gesture.is_none());
    }

    #[test]
    fn a_selection_that_disappears_is_dropped() {
        let mut desktop = desktop();
        desktop.select_for_menu(1);
        desktop.renaming = Some("a.txt".into());
        desktop.set_items(vec![item("Docs", Kind::Folder)]);
        assert!(desktop.selected.is_empty() && desktop.renaming.is_none());
    }

    #[test]
    fn a_new_icon_enters_rename_mode_once_it_is_listed() {
        let mut desktop = desktop();
        desktop.rename_when_listed = Some("New Folder".into());
        desktop.set_items(vec![item("Docs", Kind::Folder)]);
        assert!(desktop.renaming.is_none(), "not there yet");
        assert_eq!(desktop.rename_when_listed.as_deref(), Some("New Folder"));
        desktop.set_items(vec![
            item("Docs", Kind::Folder),
            item("New Folder", Kind::Folder),
        ]);
        assert_eq!(desktop.renaming.as_deref(), Some("New Folder"));
        assert_eq!(selection(&desktop), ["New Folder"]);
        assert!(desktop.rename_when_listed.is_none());
    }

    #[test]
    fn menus_offer_what_fits_the_target() {
        let mut desktop = desktop();
        assert_eq!(
            desktop.commands(None),
            [
                Command::NewFolder,
                Command::NewFile,
                Command::OpenFolder,
                Command::Settings
            ]
        );
        assert_eq!(
            desktop.commands(Some("a.txt")),
            [Command::Open, Command::Rename, Command::Trash]
        );
        assert!(
            desktop
                .commands(Some("x.desktop"))
                .contains(&Command::AllowLaunching)
        );
        assert_eq!(desktop.commands(Some("gone")), desktop.commands(None));
        desktop.selected = BTreeSet::from(["a.txt".to_owned(), "Docs".to_owned()]);
        assert_eq!(
            desktop.commands(Some("a.txt")),
            [Command::Trash],
            "a menu on a selection of several only offers what works on all of them"
        );
    }

    #[test]
    fn a_menu_acts_on_the_whole_selection_only_when_the_icon_belongs_to_it() {
        let mut desktop = desktop();
        desktop.selected = BTreeSet::from(["a.txt".to_owned(), "Docs".to_owned()]);
        desktop.select_for_menu(1);
        assert_eq!(desktop.affected("a.txt"), ["Docs", "a.txt"]);
        desktop.select_for_menu(2);
        assert_eq!(
            selection(&desktop),
            ["x.desktop"],
            "an outside icon replaces the selection"
        );
        assert_eq!(desktop.affected("x.desktop"), ["x.desktop"]);
        assert_eq!(desktop.affected("other"), ["other"]);
    }

    #[test]
    fn command_names_round_trip() {
        for command in [
            Command::Open,
            Command::AllowLaunching,
            Command::Rename,
            Command::Trash,
            Command::NewFolder,
            Command::NewFile,
            Command::OpenFolder,
            Command::Settings,
        ] {
            assert_eq!(Command::from_name(command.name()), Some(command));
            assert!(!command.label().is_empty());
        }
        assert_eq!(Command::from_name("rm -rf"), None);
    }

    #[test]
    fn icons_get_cells_and_the_layout_remembers_them() {
        let mut desktop = desktop();
        let cells = desktop.cells(&grid());
        assert_eq!(cells, [(0, 0), (0, 1), (0, 2)]);
        assert_eq!(desktop.layout["a.txt"], (0, 1));
        assert!(desktop.layout_dirty);
        // A new icon takes the first free cell instead of shifting the others.
        desktop.layout_dirty = false;
        desktop.set_items(vec![
            item("0-first.txt", Kind::File),
            item("Docs", Kind::Folder),
            item("a.txt", Kind::File),
            item("x.desktop", Kind::Launcher { trusted: false }),
        ]);
        let cells = desktop.cells(&grid());
        assert_eq!(cells, [(0, 3), (0, 0), (0, 1), (0, 2)]);
    }

    #[test]
    fn an_icon_dragged_to_another_cell_stays_there() {
        let mut desktop = desktop();
        let grid = grid();
        desktop.cells(&grid);
        desktop.layout_dirty = false;
        let (x, y, w, h) = grid.rect((5, 2));
        let point = (f64::from(x + w / 2), f64::from(y + h / 2));
        desktop.begin_icon_drag(1, (30.0, 150.0), &grid);
        assert_eq!(selection(&desktop), ["a.txt"]);
        desktop.drag_icons(point, &grid);
        assert!(matches!(
            desktop.gesture,
            Some(Gesture::Icons { target: (5, 2), .. })
        ));
        desktop.drop_icons(&grid);
        assert_eq!(desktop.layout["a.txt"], (5, 2));
        assert!(desktop.layout_dirty && desktop.gesture.is_none());
        assert_eq!(desktop.cells(&grid)[1], (5, 2));
    }

    #[test]
    fn dragging_a_selected_icon_carries_the_whole_selection() {
        let mut desktop = desktop();
        let grid = grid();
        desktop.cells(&grid);
        desktop.selected = BTreeSet::from(["Docs".to_owned(), "a.txt".to_owned()]);
        desktop.begin_icon_drag(0, (30.0, 60.0), &grid);
        assert_eq!(selection(&desktop), ["Docs", "a.txt"], "kept");
        let (x, y, w, h) = grid.rect((4, 0));
        desktop.drag_icons((f64::from(x + w / 2), f64::from(y + h / 2)), &grid);
        desktop.drop_icons(&grid);
        assert_eq!(
            (desktop.layout["Docs"], desktop.layout["a.txt"]),
            ((4, 0), (4, 1))
        );
        assert_eq!(desktop.layout["x.desktop"], (0, 2), "the others stay");
    }

    #[test]
    fn a_selection_rectangle_selects_the_icons_it_touches() {
        let mut desktop = desktop();
        let grid = grid();
        desktop.cells(&grid);
        desktop.selected = BTreeSet::from(["x.desktop".to_owned()]);
        let (x0, y0, w, h) = grid.rect((0, 0));
        let (_, y1, _, h1) = grid.rect((0, 1));
        desktop.begin_band((f64::from(x0 + w + 40), f64::from(y1 + h1 - 4)));
        assert!(
            desktop.selected.is_empty(),
            "a new rectangle starts from nothing"
        );
        desktop.drag_band((f64::from(x0 + 2), f64::from(y0 + h / 2)), &grid);
        assert_eq!(selection(&desktop), ["Docs", "a.txt"]);
        desktop.drag_band((f64::from(x0 + 2), f64::from(y1 + h1 / 2)), &grid);
        assert_eq!(
            selection(&desktop),
            ["a.txt"],
            "shrinking the rectangle unselects"
        );
        desktop.end_gesture();
        assert!(desktop.gesture.is_none());
        assert_eq!(selection(&desktop), ["a.txt"], "what it selected stays");
    }

    #[test]
    fn creating_at_a_point_reserves_the_nearest_free_cell_and_renaming_keeps_the_place() {
        let mut desktop = desktop();
        let grid = grid();
        desktop.cells(&grid);
        desktop.reserve_near("New Folder", (0, 1), &grid);
        assert_eq!(
            desktop.layout["New Folder"],
            (1, 1),
            "the wanted cell is taken"
        );
        desktop.reserve_near("Far", (7, 3), &grid);
        assert_eq!(desktop.layout["Far"], (7, 3));
        desktop.renamed("Far", "Near");
        assert_eq!(desktop.layout["Near"], (7, 3));
        assert!(!desktop.layout.contains_key("Far"));
    }

    #[test]
    fn a_failed_read_does_not_erase_the_saved_layout_but_a_removed_icon_is_forgotten() {
        let mut desktop = desktop();
        desktop.cells(&grid());
        desktop.set_items(Vec::new());
        assert_eq!(desktop.layout.len(), 3, "an empty listing is not trusted");
        desktop.set_items(vec![item("Docs", Kind::Folder)]);
        assert_eq!(desktop.layout.len(), 1);
    }
}
