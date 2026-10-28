//! Opening, filling and activating the launcher with the shell's data.

use std::time::{SystemTime, UNIX_EPOCH};

use desilium_apps::IconResolver;
use desilium_ui::KeyEvent;

use super::Shell;
use crate::{
    launcher::{Context, Parts, PinView, RowIcons, compute},
    model::{
        dock::AppLookup,
        launcher::{Activation, Outcome, PINNED_SLOTS, RowIcon},
    },
    services::apps::CatalogLookup,
    view::icon::{IconSpec, candidate_names, resolve_icon},
};

/// Edge of the pinned strip's icons.
const PIN_ICON_PX: u32 = 56;
/// Edge of a result row's icon.
const ROW_ICON_PX: u32 = 32;

struct ShellIcons<'a> {
    resolver: &'a mut IconResolver,
    scale: u32,
}

impl RowIcons for ShellIcons<'_> {
    fn app_icon(&mut self, icon: &RowIcon, px: u32) -> IconSpec {
        let RowIcon::App { desktop_id, icon } = icon else {
            return IconSpec::NEUTRAL;
        };
        resolve_icon(
            self.resolver,
            &candidate_names(icon.as_deref(), desktop_id),
            px,
            self.scale,
        )
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

impl Shell {
    /// Desktop ids behind `Alt+1..6`: the first pinned apps that are installed.
    fn pinned_ids(&self) -> Vec<String> {
        let lookup = CatalogLookup(&self.catalog);
        self.pins
            .pinned
            .iter()
            .filter(|id| lookup.by_desktop_id(id).is_some())
            .take(PINNED_SLOTS)
            .cloned()
            .collect()
    }

    fn pin_views(&mut self, ids: &[String], scale: u32) -> Vec<PinView> {
        let lookup = CatalogLookup(&self.catalog);
        let apps: Vec<_> = ids
            .iter()
            .filter_map(|id| lookup.by_desktop_id(id))
            .collect();
        apps.into_iter()
            .map(|app| PinView {
                icon: resolve_icon(
                    &mut self.icons,
                    &candidate_names(app.icon.as_deref(), &app.desktop_id),
                    PIN_ICON_PX,
                    scale,
                ),
                name: app.name,
            })
            .collect()
    }

    fn results_for(&self, query: &str) -> crate::model::launcher::Results {
        let ctx = Context {
            catalog: &self.catalog,
            usage: &self.usage,
            now_secs: now_secs(),
        };
        compute(&ctx, query, self.launcher.model.category())
    }

    /// Opens the launcher with `query` in the search field.
    pub(super) fn open_launcher(&mut self, query: &str) {
        self.hide_tooltip();
        let results = self.results_for(query);
        let scale = self.icon_scale;
        let ids = self.pinned_ids();
        let pins = self.pin_views(&ids, scale);
        let output_height = self.output_size().map(|(_, height)| height);
        let mut icons = ShellIcons {
            resolver: &mut self.icons,
            scale,
        };
        let parts = Parts {
            pinned: &ids,
            pins: &pins,
            icons: &mut icons,
        };
        let opened = self
            .launcher
            .open(&mut self.host, output_height, query, results, parts);
        match opened {
            Ok(()) => self.apply_effects(),
            Err(error) => tracing::warn!(%error, "cannot open the launcher"),
        }
    }

    pub(super) fn close_launcher(&mut self) {
        self.launcher_focused = false;
        self.launcher.close(&mut self.host);
        self.hide_tooltip();
    }

    /// Opens a closed launcher, closes an open one.
    pub(super) fn toggle_launcher(&mut self) {
        if self.launcher.is_open() {
            self.close_launcher();
        } else {
            self.open_launcher("");
        }
    }

    /// Redraws the launcher from its model (`recompute` re-runs the search first).
    fn redraw_launcher(&mut self, recompute: bool) {
        let ids = self.pinned_ids();
        let scale = self.icon_scale;
        let results = recompute.then(|| self.results_for(self.launcher.model.query()));
        let mut icons = ShellIcons {
            resolver: &mut self.icons,
            scale,
        };
        let parts = Parts {
            pinned: &ids,
            pins: &[],
            icons: &mut icons,
        };
        let outcome = match results {
            Some(results) => self.launcher.show_results(&mut self.host, results, parts),
            None => self.launcher.redraw(&mut self.host, parts),
        };
        if let Err(error) = outcome {
            tracing::warn!(%error, "launcher redraw failed");
        }
    }

    pub(super) fn launcher_text_changed(&mut self, value: &str) {
        if value == self.launcher.model.query() {
            return;
        }
        self.launcher.model.set_query(value);
        self.redraw_launcher(true);
    }

    pub(super) fn launcher_key(&mut self, event: &KeyEvent) {
        let ids = self.pinned_ids();
        match self.launcher.model.handle_key(event, &ids) {
            Outcome::Ignored => {}
            Outcome::SelectionChanged => self.redraw_launcher(false),
            Outcome::CategoryChanged => self.redraw_launcher(true),
            Outcome::Close => self.close_launcher(),
            Outcome::Activate(activation) => self.activate(&activation),
        }
    }

    pub(super) fn launcher_focus_changed(&mut self, focused: bool) {
        tracing::info!(focused, "launcher keyboard focus");
        if focused {
            self.launcher_focused = true;
            return;
        }
        // Losing focus before ever having it (map race) must not close the panel.
        if self.launcher_focused && self.launcher.is_open() {
            self.close_launcher();
        }
    }

    pub(super) fn launcher_action(&mut self, action: &str, arg: &str) {
        match action {
            "activate" => {
                let row = arg.parse::<usize>().ok();
                let activation = row
                    .and_then(|index| self.launcher.model.results().rows.get(index))
                    .map(|row| row.activation.clone());
                if let Some(activation) = activation {
                    self.activate(&activation);
                }
            }
            "pin" => self.activate_pin(arg),
            "category" => {
                let category = crate::model::launcher::Category::from_id(arg);
                if category.is_some_and(|category| self.launcher.model.set_category(category)) {
                    self.redraw_launcher(true);
                }
            }
            _ => {}
        }
    }

    fn activate_pin(&mut self, arg: &str) {
        let ids = self.pinned_ids();
        let id = arg.parse::<usize>().ok().and_then(|slot| ids.get(slot));
        if let Some(desktop_id) = id {
            self.activate(&Activation::LaunchApp {
                desktop_id: desktop_id.clone(),
            });
        }
    }

    /// Performs a result and closes the launcher.
    fn activate(&mut self, activation: &Activation) {
        let started = match activation {
            Activation::LaunchApp { desktop_id } => {
                self.start_app(desktop_id);
                Ok(())
            }
            Activation::RunShell(line) => self.starter.run_shell(line),
            Activation::RunInTerminal(text) => self.starter.run_in_terminal(text),
            Activation::WebSearch(text) => self.starter.web_search(text),
        };
        if let Err(error) = started {
            tracing::warn!(%error, "cannot run the launcher action");
        }
        self.close_launcher();
    }
}

#[allow(dead_code)]
const _ROW_ICON: u32 = ROW_ICON_PX;
