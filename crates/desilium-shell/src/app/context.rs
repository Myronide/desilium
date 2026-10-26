use super::{Shell, system};
use crate::{model::windows::WindowId, quick::AppMenu};
impl Shell {
    pub(super) fn open_context(&mut self, key: &str) {
        let Some(item) = self.dock_items.iter().find(|item| item.key == key) else {
            return;
        };
        let menu = AppMenu {
            key: key.into(),
            name: item.name.clone(),
            pinned: item.pinned,
            can_pin: item.desktop_id.is_some(),
            windows: item
                .windows
                .iter()
                .filter_map(|id| {
                    self.windows.get(*id).map(|window| {
                        (
                            id.0,
                            if window.title.is_empty() {
                                item.name.clone()
                            } else {
                                window.title.clone()
                            },
                        )
                    })
                })
                .collect(),
        };
        self.quick.close(&mut self.host);
        self.quick.menu = Some(menu);
        self.quick_focused = false;
        self.launcher.close(&mut self.host);
        let output = self.output_size().unwrap_or((1280, 800));
        let settings = &self.store.get().shell;
        let surface = crate::dock::layout::screen_rect(settings, self.dock.layout(), output);
        let index = self.dock_items.iter().position(|item| item.key == key);
        let local = index
            .and_then(|index| self.dock.visible_cell_rect(&mut self.host, index))
            .unwrap_or(self.dock.layout().launcher);
        let button = desilium_ui::Rect {
            x: surface.x + local.x,
            y: surface.y + local.y,
            ..local
        };
        let origin = crate::quick::placement::Origin::App {
            button,
            below: settings.edge == desilium_config_types::PanelEdge::Top,
        };
        let insets = self.popup_insets();
        let settings = &self.store.get().shell;
        if let Err(error) = self
            .quick
            .open(&mut self.host, settings, "", output, insets, origin)
        {
            tracing::warn!(%error,"app menu could not open");
        }
    }
    pub(super) fn context_action(&mut self, action: &str, arg: &str) -> bool {
        match action {
            "dock-pin" | "dock-left" | "dock-right" => self.edit_dock_pin(action, arg),
            "dock-new" => {
                self.dock_new_instance(arg);
                self.quick.close(&mut self.host);
            }
            "window-activate" | "window-close" => {
                if let (Ok(id), Some(proto)) = (arg.parse::<u32>(), &self.proto) {
                    if action == "window-close" {
                        proto.close_window(WindowId(id));
                    } else {
                        proto.activate(WindowId(id));
                    }
                }
                self.quick.close(&mut self.host);
            }
            _ => return false,
        }
        true
    }
    fn edit_dock_pin(&mut self, action: &str, key: &str) {
        let Some(item) = self.dock_items.iter().find(|item| item.key == key) else {
            return;
        };
        let Some(desktop_id) = &item.desktop_id else {
            return;
        };
        let mut pins = self.pins.clone();
        let index = pins.pinned.iter().position(|id| id == desktop_id);
        match (action, index) {
            ("dock-pin", Some(index)) => {
                pins.pinned.remove(index);
            }
            ("dock-pin", None) => pins.pinned.push(desktop_id.clone()),
            ("dock-left", Some(index)) if index > 0 => pins.pinned.swap(index, index - 1),
            ("dock-right", Some(index)) if index + 1 < pins.pinned.len() => {
                pins.pinned.swap(index, index + 1)
            }
            _ => return,
        }
        match system::persist_pins(&self.config_dir, &pins) {
            Ok(()) => {
                self.pins = pins;
                self.refresh_dock();
                self.quick.close(&mut self.host);
            }
            Err(error) => {
                let mut state = self.quick.state.clone();
                state.notice = format!("Pins were not saved: {error}");
                self.quick_updated(state);
            }
        }
    }
}
