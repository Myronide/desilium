//! Routing of UI host events: clicks, hover (tooltips), keys and surface lifecycle.

use std::time::Instant;

use desilium_ui::{
    HoverTarget, Rect, UiAction, UiEvent,
    wayland::{HostEvent, SurfaceId},
};

use super::{PendingTip, Shell, TOOLTIP_DELAY, timers::Due};
use crate::{launcher::WIDTH as LAUNCHER_WIDTH, model::dock::DockClick, tooltip::Placement};

impl Shell {
    pub(super) fn handle_host_event(&mut self, event: HostEvent) {
        match event {
            HostEvent::Ui {
                surface,
                event: UiEvent::Action(action),
            } => self.handle_action(surface, &action),
            HostEvent::Ui {
                surface,
                event:
                    UiEvent::TextChanged {
                        element_id, value, ..
                    },
            } if self.launcher.owns(surface) && element_id.as_deref() == Some("q") => {
                self.launcher_text_changed(&value);
            }
            HostEvent::Hover { surface, target } => self.handle_hover(surface, target),
            HostEvent::Pointer { surface, inside } => self.panel_pointer(surface, inside),
            HostEvent::Key { surface, event }
                if self.quick.owns(surface) && event.key == desilium_ui::Key::Escape =>
            {
                self.quick.close(&mut self.host);
            }
            HostEvent::KeyboardFocus { surface, focused } if self.quick.owns(surface) => {
                if focused {
                    self.quick_focused = true;
                } else if self.quick_focused {
                    self.quick.close(&mut self.host);
                }
            }
            HostEvent::Key { surface, event } if self.launcher.owns(surface) => {
                self.launcher_key(&event);
            }
            HostEvent::KeyboardFocus { surface, focused } if self.launcher.owns(surface) => {
                self.launcher_focus_changed(focused);
            }
            HostEvent::Closed(surface) if surface == self.bar.id() => {
                tracing::info!("the compositor closed the bar, exiting");
                self.quit = true;
            }
            _ => {}
        }
    }

    fn handle_action(&mut self, surface: SurfaceId, action: &UiAction) {
        self.hide_tooltip();
        let arg = action.arg.as_deref().unwrap_or_default();
        if surface == self.bar.id() {
            self.bar_action(&action.action, arg);
        } else if surface == self.dock.id() {
            self.dock_action(&action.action, arg);
        } else if self.quick.owns(surface) {
            self.quick_action(&action.action, arg);
        } else if self.launcher.owns(surface) {
            self.launcher_action(&action.action, arg);
        }
    }

    fn bar_action(&mut self, action: &str, arg: &str) {
        match action {
            "workspace" => {
                let index = arg.parse::<u32>().ok();
                if let (Some(index), Some(proto)) = (index, &self.proto) {
                    proto.switch_workspace(index);
                }
            }
            "quick-settings" | "power" | "calendar" => self.toggle_quick_settings(false),
            "layout" => self.edit_quick_config(|config| {
                config.compositor.keyboard_layout = config.compositor.keyboard_layout.next();
            }),
            _ => {}
        }
    }

    fn dock_action(&mut self, action: &str, key: &str) {
        match action {
            "launcher" => self.toggle_launcher(),
            "quick-settings" => self.toggle_quick_settings(true),
            "settings" => self.open_settings(false),
            "show-desktop" => self.show_desktop(),
            "workspace-next" => {
                if let Some(proto) = &self.proto {
                    let count = self.workspaces.count.max(1);
                    proto.switch_workspace((self.workspaces.active + 1) % count);
                }
            }
            "app" => self.dock_click(key),
            "app-menu" => self.open_context(key),
            "app-new" => self.dock_new_instance(key),
            _ => {}
        }
    }

    /// Minimizes every window of the current workspace; when all are minimized already, restores them.
    fn show_desktop(&mut self) {
        let Some(proto) = &self.proto else {
            return;
        };
        let workspace = self.workspaces.active;
        let here: Vec<_> = self
            .windows
            .windows()
            .iter()
            .filter(|window| window.workspace == workspace)
            .collect();
        let hide = here.iter().any(|window| !window.minimized);
        for window in here {
            proto.set_minimized(window.id, hide);
        }
    }

    fn dock_click(&mut self, key: &str) {
        let Some(item) = self.dock_items.iter().find(|item| item.key == key) else {
            return;
        };
        let desktop_id = item.desktop_id.clone();
        let click = item.click(&self.windows);
        match (click, &self.proto) {
            (DockClick::Launch, _) => {
                if let Some(id) = desktop_id {
                    self.start_app(&id);
                }
            }
            (DockClick::Activate(window), Some(proto)) => proto.activate(window),
            (DockClick::Minimize(window), Some(proto)) => proto.set_minimized(window, true),
            _ => {}
        }
    }

    pub(super) fn dock_new_instance(&mut self, key: &str) {
        let desktop_id = self
            .dock_items
            .iter()
            .find(|item| item.key == key)
            .and_then(|item| item.desktop_id.clone());
        if let Some(id) = desktop_id {
            self.start_app(&id);
        }
    }

    /// Starts an application and records the launch for ranking.
    pub(super) fn start_app(&mut self, desktop_id: &str) {
        if let Err(error) = self.starter.launch_app(&self.catalog, desktop_id) {
            tracing::warn!(%error, desktop_id, "cannot start the application");
            return;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        self.usage.record(desktop_id, now);
        if let Err(error) = self.usage.save() {
            tracing::warn!(%error, "cannot save launch history");
        }
    }

    fn handle_hover(&mut self, surface: SurfaceId, target: Option<HoverTarget>) {
        if self.tooltip.owns(surface) {
            return;
        }
        let tip = target.and_then(|t| {
            t.tooltip
                .filter(|text| !text.is_empty())
                .map(|text| (text, t.rect))
        });
        let Some((text, rect)) = tip else {
            self.hide_tooltip();
            return;
        };
        self.pending_tip = Some(PendingTip {
            surface,
            text,
            rect,
        });
        if self.tooltip.is_visible() {
            self.show_pending_tooltip();
        } else {
            self.deadlines
                .set(Due::Tooltip, Instant::now() + TOOLTIP_DELAY);
        }
    }

    pub(super) fn hide_tooltip(&mut self) {
        self.pending_tip = None;
        self.deadlines.clear(Due::Tooltip);
        self.tooltip.hide(&mut self.host);
    }

    pub(super) fn show_pending_tooltip(&mut self) {
        let Some(tip) = self.pending_tip.take() else {
            return;
        };
        let Some((target, placement)) = self.screen_target(tip.surface, tip.rect) else {
            return;
        };
        let width = self.output_size().map(|(w, _)| w);
        let shown = self
            .tooltip
            .show(&mut self.host, &tip.text, target, placement, width);
        if let Err(error) = shown {
            tracing::warn!(%error, "cannot show the tooltip");
        }
        self.pending_tip = Some(tip);
    }

    /// Converts a surface-local rect to screen coordinates and picks the bubble side.
    fn screen_target(&self, surface: SurfaceId, rect: Rect) -> Option<(Rect, Placement)> {
        let output = self.output_size();
        if surface == self.bar.id() {
            return Some((rect, Placement::Below));
        }
        let (width, height) = output?;
        let (origin_x, origin_y, placement) = if surface == self.dock.id() {
            let settings = &self.store.get().shell;
            let bounds =
                crate::dock::layout::screen_rect(settings, self.dock.layout(), (width, height));
            (
                bounds.x as i32,
                bounds.y as i32,
                if settings.edge == desilium_config_types::PanelEdge::Top {
                    Placement::Below
                } else {
                    Placement::Above
                },
            )
        } else if self.launcher.owns(surface) {
            let (top, _) = self.launcher.top_and_height();
            let x = (width - i32::try_from(LAUNCHER_WIDTH).ok()?) / 2;
            (x, top, Placement::Below)
        } else {
            return None;
        };
        let moved = Rect {
            x: rect.x + f64::from(origin_x),
            y: rect.y + f64::from(origin_y),
            ..rect
        };
        Some((moved, placement))
    }
}
