//! Desktop icons in the running shell: listing the folder, drawing it onto the wallpaper, and
//! what clicks, drags, menus and renaming do to the files.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use desilium_apps::Environment;
use desilium_ui::{DragPhase, UiDrag, wayland::SurfaceId};

use super::{Shell, timers::Due};
use crate::{
    desktop::{self, Click, Command, Menu},
    model::{
        desktop::{self as model, DeskItem, ICON_PX, Kind},
        grid::Grid,
    },
    services::desktop as files,
    view::icon::{IconSpec, candidate_names, resolve_icon},
};

/// Quiet time after the folder changed before it is listed again.
pub(super) const DESKTOP_DEBOUNCE: Duration = Duration::from_millis(150);
/// Settings page that holds the desktop options.
const SETTINGS_PAGE: &str = "desktop";

/// Icon names to try for one item, best first.
fn icon_names(item: &DeskItem) -> Vec<String> {
    match item.kind {
        Kind::Folder => vec!["folder".to_owned(), "inode-directory".to_owned()],
        Kind::Launcher { .. } => candidate_names(item.icon.as_deref(), &item.file_name),
        Kind::File => model::file_icon_names(&item.file_name)
            .into_iter()
            .map(str::to_owned)
            .collect(),
    }
}

impl Shell {
    /// The user wants the desktop folder shown.
    pub(super) fn desktop_enabled(&self) -> bool {
        self.store.get().shell.desktop_icons && self.desktop.dir.is_some()
    }

    /// Finds the desktop folder, loads the saved icon positions, starts watching the folder and
    /// lists it. The folder is created if it is missing, as other desktops do, so that it can
    /// always be watched.
    pub(super) fn start_desktop(&mut self) {
        let env = Environment::from_env();
        let config_home = desilium_theme::kde::KdeDirs::from_env().config_home;
        self.desktop.dir = files::desktop_dir(&env.home, &config_home);
        let Some(dir) = self.desktop.dir.clone() else {
            return;
        };
        self.desktop.layout = files::load_layout(&files::layout_path(&env.state_home));
        self.desktop_watch = super::DirWatch::new(&dir)
            .map_err(|error| tracing::warn!(%error, "changes on the desktop are not watched"))
            .ok();
        self.rescan_desktop();
    }

    /// Writes the icon positions when they changed.
    fn persist_layout(&mut self) {
        if !std::mem::take(&mut self.desktop.layout_dirty) {
            return;
        }
        let path = files::layout_path(&Environment::from_env().state_home);
        if let Err(error) = files::save_layout(&path, &self.desktop.layout) {
            tracing::warn!(%error, "the desktop layout could not be saved");
        }
    }

    /// Size of the primary wallpaper surface and the grid that fits it between the panels.
    fn desktop_grid(&self) -> Option<((u32, u32), Grid)> {
        let primary = self.wallpaper.primary()?;
        let size = self.host.surface(primary).map(|ui| ui.size())?;
        let insets = self.popup_insets();
        Some((size, Grid::new(size, insets.top, insets.bottom)))
    }

    /// Lists the folder again and redraws.
    pub(super) fn rescan_desktop(&mut self) {
        let Some(dir) = self.desktop.dir.clone() else {
            return;
        };
        let items = if self.store.get().shell.desktop_icons {
            files::scan(&dir, &Environment::from_env().locales)
        } else {
            Vec::new()
        };
        self.desktop.set_items(items);
        self.refresh_desktop();
    }

    /// Draws the icons onto the primary wallpaper surface, or clears it when they are off.
    pub(super) fn refresh_desktop(&mut self) {
        let Some(primary) = self.wallpaper.primary() else {
            return;
        };
        let Some((size, grid)) = self.desktop_grid() else {
            return;
        };
        let html = if self.desktop_enabled() {
            let cells = self.desktop.cells(&grid);
            let scale = self.icon_scale;
            let icons: Vec<IconSpec> = self
                .desktop
                .items
                .iter()
                .map(icon_names)
                .map(|names| resolve_icon(&mut self.icons, &names, ICON_PX, scale))
                .collect();
            desktop::html(&self.desktop, &icons, &grid, &cells, size)
        } else {
            String::new()
        };
        let renaming = self.desktop.renaming.is_some();
        self.host.update(primary, |ui| {
            if let Err(error) = ui.replace_children("#desk", &html) {
                tracing::warn!(%error, "the desktop icons could not be drawn");
            }
            if renaming && let Err(error) = ui.focus("#desk-rename") {
                tracing::debug!(%error, "the rename field could not be focused");
            }
        });
        let insets = self.popup_insets();
        self.desktop_drawn = Some((size, insets.top, insets.bottom));
        self.persist_layout();
    }

    /// Redraws when the surface or the space the panels take changed since the last drawing.
    pub(super) fn check_desktop(&mut self) {
        let Some(primary) = self.wallpaper.primary() else {
            return;
        };
        if !self.host.is_configured(primary) || !self.desktop_enabled() {
            return;
        }
        let Some((size, _)) = self.desktop_grid() else {
            return;
        };
        let insets = self.popup_insets();
        if self.desktop_drawn != Some((size, insets.top, insets.bottom)) {
            self.refresh_desktop();
        }
    }

    /// Draws what the last pointer movements changed, once per round of events.
    pub(super) fn flush_desktop(&mut self) {
        if std::mem::take(&mut self.desktop_dirty) {
            self.refresh_desktop();
        }
    }

    /// The pointer left a surface: a menu on the desktop closes.
    pub(super) fn desktop_pointer(&mut self, surface: SurfaceId, inside: bool) {
        if !inside && self.wallpaper.is_primary(surface) && self.desktop.menu.take().is_some() {
            self.refresh_desktop();
        }
    }

    /// Where the pointer is on `surface`, for opening a menu there.
    fn pointer_at(&self, surface: SurfaceId) -> (i32, i32) {
        self.host
            .surface(surface)
            .and_then(|ui| ui.pointer_position())
            .map_or((0, 0), |(x, y)| (x as i32, y as i32))
    }

    /// A drag on the desktop: carrying icons or drawing a selection rectangle.
    pub(super) fn desktop_drag(&mut self, surface: SurfaceId, drag: &UiDrag) {
        if !self.wallpaper.is_primary(surface) || !self.desktop_enabled() {
            return;
        }
        let Some((_, grid)) = self.desktop_grid() else {
            return;
        };
        match (drag.name.as_str(), drag.phase) {
            ("desk-icon", DragPhase::Start) => {
                if let Some(index) = drag.arg.as_deref().and_then(|arg| arg.parse().ok()) {
                    self.desktop.begin_icon_drag(index, drag.origin, &grid);
                    self.desktop.drag_icons(drag.pos, &grid);
                }
            }
            ("desk-icon", DragPhase::Move) => self.desktop.drag_icons(drag.pos, &grid),
            ("desk-icon", DragPhase::End) => {
                self.desktop.drag_icons(drag.pos, &grid);
                self.desktop.drop_icons(&grid);
            }
            ("desk-band", DragPhase::Start) => {
                self.desktop.begin_band(drag.origin);
                self.desktop.drag_band(drag.pos, &grid);
            }
            ("desk-band", DragPhase::Move) => self.desktop.drag_band(drag.pos, &grid),
            ("desk-band", DragPhase::End) => {
                self.desktop.drag_band(drag.pos, &grid);
                self.desktop.end_gesture();
            }
            _ => return,
        }
        self.desktop_dirty = true;
    }

    /// A click on the desktop: `action` and `arg` come from the markup in `desktop::html`.
    pub(super) fn desktop_action(&mut self, surface: SurfaceId, action: &str, arg: &str) {
        if !self.wallpaper.is_primary(surface) || !self.desktop_enabled() {
            return;
        }
        let index = arg.parse::<usize>().ok();
        match (action, index) {
            ("desk-bg", _) => self.desktop.clear(),
            ("desk-menu", _) => {
                self.desktop.clear();
                let at = self.pointer_at(surface);
                self.desktop.menu = Some(Menu { target: None, at });
            }
            ("desk-item", Some(index)) => {
                if self.desktop.click(index, Instant::now()) == Click::Open {
                    self.open_desktop_item(index);
                }
            }
            ("desk-item-menu", Some(index)) => {
                self.desktop.menu = None;
                self.desktop.select_for_menu(index);
                let at = self.pointer_at(surface);
                let target = self.desktop.item(index).map(|item| item.file_name.clone());
                self.desktop.menu = Some(Menu { target, at });
            }
            ("desk-cmd", _) => {
                if let Some(command) = Command::from_name(arg) {
                    self.desktop_command(command);
                }
            }
            _ => return,
        }
        self.refresh_desktop();
    }

    /// Enter in the rename field.
    pub(super) fn desktop_rename_submitted(&mut self, value: &str) {
        let (Some(old), Some(dir)) = (self.desktop.renaming.take(), self.desktop.dir.clone())
        else {
            return;
        };
        match files::rename(&dir, &old, value) {
            Ok(new_name) => {
                self.desktop.renamed(&old, &new_name);
                self.desktop.selected = [new_name].into();
            }
            Err(error) => self.desktop.notice = error.to_string(),
        }
        self.rescan_desktop();
    }

    fn open_desktop_item(&mut self, index: usize) {
        let Some(item) = self.desktop.item(index).cloned() else {
            return;
        };
        let result = match item.kind {
            Kind::Launcher { trusted: false } => {
                self.desktop.notice = format!(
                    "\"{}\" is not trusted yet: right-click it and choose Allow launching",
                    item.name
                );
                return;
            }
            Kind::Launcher { trusted: true } => {
                match files::load_launcher(&item.path, &Environment::from_env().locales) {
                    Some(entry) => self.starter.launch_entry(&entry),
                    None => self.starter.open_path(&item.path),
                }
            }
            Kind::Folder | Kind::File => self.starter.open_path(&item.path),
        };
        if let Err(error) = result {
            self.desktop.notice = format!("\"{}\" could not be opened: {error}", item.name);
        }
    }

    fn desktop_command(&mut self, command: Command) {
        let menu = self.desktop.menu.take();
        let at = menu.as_ref().map(|menu| menu.at);
        let target = menu.and_then(|menu| menu.target);
        let Some(dir) = self.desktop.dir.clone() else {
            return;
        };
        let result: Result<(), String> = match (command, target) {
            (Command::Open, Some(name)) => {
                if let Some(index) = self.desktop.items.iter().position(|i| i.file_name == name) {
                    self.open_desktop_item(index);
                }
                Ok(())
            }
            (Command::AllowLaunching, Some(name)) => {
                files::allow_launching(&dir, &name).map_err(|e| e.to_string())
            }
            (Command::Rename, Some(name)) => {
                self.desktop.selected = [name.clone()].into();
                self.desktop.renaming = Some(name);
                Ok(())
            }
            (Command::Trash, Some(name)) => self.trash_on_desktop(&dir, &name),
            (Command::NewFolder, _) => self.create_on_desktop(files::create_folder(&dir), at),
            (Command::NewFile, _) => self.create_on_desktop(files::create_file(&dir), at),
            (Command::OpenFolder, _) => self.starter.open_path(&dir).map_err(|e| e.to_string()),
            (Command::Settings, _) => self
                .spawn_settings(Some(SETTINGS_PAGE))
                .map_err(|error| format!("Settings could not open: {error}")),
            (_, None) => Ok(()),
        };
        if let Err(message) = result {
            self.desktop.notice = message;
        }
        self.rescan_desktop();
    }

    /// Moves `name` and, if it belongs to the selection, everything selected to the trash. The
    /// first problem is reported; the others are still tried.
    fn trash_on_desktop(&mut self, dir: &std::path::Path, name: &str) -> Result<(), String> {
        let mut first_error = None;
        for victim in self.desktop.affected(name) {
            if let Err(error) = files::trash(dir, &victim) {
                first_error.get_or_insert_with(|| error.to_string());
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// A new folder or file takes the cell under the menu and is renamed straight away, so the
    /// user can name it.
    fn create_on_desktop(
        &mut self,
        created: Result<String, files::DeskError>,
        at: Option<(i32, i32)>,
    ) -> Result<(), String> {
        let name = created.map_err(|error| error.to_string())?;
        if let (Some((x, y)), Some((_, grid))) = (at, self.desktop_grid()) {
            let wanted = grid.cell_at(f64::from(x), f64::from(y));
            self.desktop.reserve_near(&name, wanted, &grid);
        }
        self.desktop.rename_when_listed = Some(name);
        Ok(())
    }

    /// Copies an installed application's launcher onto the desktop.
    pub(super) fn add_to_desktop(&mut self, desktop_id: &str) {
        let Some(dir) = self.desktop.dir.clone() else {
            return;
        };
        let source: Option<PathBuf> = self.catalog.get(desktop_id).map(|entry| entry.path.clone());
        let result = match source {
            Some(source) => files::add_launcher(&dir, &source).map(|_| ()),
            None => Ok(()),
        };
        if let Err(error) = result {
            self.desktop.notice = error.to_string();
        }
        self.rescan_desktop();
    }

    /// The desktop folder changed on disk.
    pub(super) fn desktop_fd_ready(&mut self) -> bool {
        let Some(watch) = &mut self.desktop_watch else {
            return false;
        };
        match watch.drain() {
            Ok(names) => {
                if !names.is_empty() {
                    self.deadlines
                        .set(Due::Desktop, Instant::now() + DESKTOP_DEBOUNCE);
                }
                true
            }
            Err(error) => {
                tracing::warn!(%error, "the desktop watch stopped");
                self.desktop_watch = None;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn item(name: &str, kind: Kind, icon: Option<&str>) -> DeskItem {
        DeskItem {
            path: PathBuf::from(format!("/d/{name}")),
            file_name: name.to_owned(),
            name: name.to_owned(),
            kind,
            icon: icon.map(str::to_owned),
        }
    }

    #[test]
    fn folders_files_and_launchers_look_for_their_own_icons() {
        assert_eq!(icon_names(&item("Docs", Kind::Folder, None))[0], "folder");
        assert_eq!(
            icon_names(&item("cat.png", Kind::File, None)),
            ["image-x-generic", "text-x-generic", "unknown"]
        );
        let launcher = item(
            "notes.desktop",
            Kind::Launcher { trusted: true },
            Some("accessories-notes"),
        );
        assert_eq!(icon_names(&launcher)[0], "accessories-notes");
        assert!(icon_names(&launcher).contains(&"notes".to_owned()));
    }
}
