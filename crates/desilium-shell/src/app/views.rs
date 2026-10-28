//! Builds what the bar and dock show from the models, and keeps minimize targets in sync.

use desilium_apps::{Catalog, IconResolver};
use desilium_config_types::KeyboardLayout;
use desilium_ui::{Rect, wayland::SurfaceId};

use super::Shell;
use crate::{
    bar::{ActiveApp, BarView},
    cli::{BarSpot, DemoHover},
    dock::{DockCell, DockView, Status},
    model::{
        battery::Battery,
        clock::ClockText,
        dock::{self, AppLookup},
        pins::DockConfig,
        windows::{WindowModel, Workspaces},
    },
    services::apps::CatalogLookup,
    view::icon::{IconSpec, candidate_names, resolve_icon},
};

/// Backdrop blur behind the bar.
pub(super) const BAR_BLUR: u32 = 20;
/// Backdrop blur behind the dock tile.
pub(super) const DOCK_BLUR: u32 = 24;
/// Backdrop blur behind the launcher.
pub(super) const LAUNCHER_BLUR: u32 = 32;
/// Edge of the dock's app icons in logical pixels.
const DOCK_ICON_PX: u32 = 48;
/// Edge of the application icon in the bar.
const BAR_ICON_PX: u32 = 16;
/// Bar text when no window has focus.
const DESKTOP_NAME: &str = "Desktop";
const DESKTOP_GLYPH: &str = "monitor";

/// First content of the bar and dock, built before their surfaces exist.
pub(super) struct InitialViews {
    pub bar: BarView,
    pub dock: DockView,
}

fn active_app(
    model: &WindowModel,
    workspace: u32,
    lookup: &dyn AppLookup,
    resolver: &mut IconResolver,
) -> ActiveApp {
    let Some(window) = model.active_on(workspace) else {
        return ActiveApp {
            name: DESKTOP_NAME.to_owned(),
            icon: IconSpec::Glyph(DESKTOP_GLYPH),
            title: String::new(),
        };
    };
    let (name, icon) = match lookup.by_app_id(&window.app_id) {
        Some(app) => (
            app.name.clone(),
            resolve_icon(
                resolver,
                &candidate_names(app.icon.as_deref(), &app.desktop_id),
                BAR_ICON_PX,
                1,
            ),
        ),
        None => (
            display_name(&window.app_id, &window.title),
            resolve_icon(resolver, &window_icon_names(window), BAR_ICON_PX, 1),
        ),
    };
    ActiveApp {
        name,
        icon,
        title: window.title.clone(),
    }
}

/// Icon names for a window whose application has no desktop file: KWin's, then its app id's.
pub(super) fn window_icon_names(window: &crate::model::windows::WindowInfo) -> Vec<String> {
    candidate_names(
        Some(&window.icon)
            .filter(|icon| !icon.is_empty())
            .map(String::as_str),
        &window.app_id,
    )
}

fn display_name(app_id: &str, title: &str) -> String {
    if app_id.is_empty() {
        title.to_owned()
    } else {
        app_id.to_owned()
    }
}

/// The bar content for the current state.
pub(super) fn bar_view(
    model: &WindowModel,
    workspaces: Workspaces,
    lookup: &dyn AppLookup,
    clock: &ClockText,
    battery: Option<Battery>,
    layout: KeyboardLayout,
    resolver: &mut IconResolver,
) -> BarView {
    BarView {
        workspaces,
        app: active_app(model, workspaces.active, lookup, resolver),
        clock: clock.clone(),
        battery,
        layout: layout.label().to_owned(),
    }
}

/// The dock content and the merged model items behind it.
pub(super) fn dock_view(
    pins: &DockConfig,
    model: &WindowModel,
    catalog: &Catalog,
    resolver: &mut IconResolver,
    scale: u32,
) -> (DockView, Vec<dock::DockItem>) {
    let items = dock::build(&pins.pinned, model, &CatalogLookup(catalog));
    let cells = items
        .iter()
        .map(|item| DockCell {
            key: item.key.clone(),
            name: item.name.clone(),
            icon: resolve_icon(
                resolver,
                &candidate_names(
                    item.icon.as_deref(),
                    item.desktop_id
                        .as_deref()
                        .unwrap_or_else(|| item.key.trim_start_matches("app:")),
                ),
                DOCK_ICON_PX,
                scale,
            ),
            dots: item.dots(model),
        })
        .collect();
    let view = DockView {
        cells,
        separator: dock::separator_index(&items),
    };
    (view, items)
}

/// Content for the surfaces' creation: no windows yet.
pub(super) fn initial_views(
    catalog: &Catalog,
    pins: &DockConfig,
    clock: &ClockText,
    battery: Option<Battery>,
    resolver: &mut IconResolver,
) -> InitialViews {
    let model = WindowModel::default();
    let lookup = CatalogLookup(catalog);
    InitialViews {
        bar: bar_view(
            &model,
            Workspaces::default(),
            &lookup,
            clock,
            battery,
            KeyboardLayout::default(),
            resolver,
        ),
        dock: dock_view(pins, &model, catalog, resolver, 1).0,
    }
}

impl Shell {
    /// Integer icon scale of the dock surface.
    fn current_icon_scale(&self) -> u32 {
        let scale = self
            .host
            .surface(self.dock.id())
            .map_or(1.0, |ui| ui.scale());
        scale.ceil().max(1.0) as u32
    }

    pub(super) fn refresh_bar(&mut self) {
        let lookup = CatalogLookup(&self.catalog);
        let view = bar_view(
            &self.windows,
            self.workspaces,
            &lookup,
            &self.clock,
            self.battery,
            self.store.get().compositor.keyboard_layout,
            &mut self.icons,
        );
        self.bar.show(&mut self.host, view);
        self.bar.configure(&mut self.host, &self.store.get().shell);
    }

    pub(super) fn refresh_dock(&mut self) {
        self.icon_scale = self.current_icon_scale();
        let (mut view, mut items) = dock_view(
            &self.pins,
            &self.windows,
            &self.catalog,
            &mut self.icons,
            self.icon_scale,
        );
        let settings = &self.store.get().shell;
        if !settings.show_running {
            let keep: Vec<_> = items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.pinned)
                .map(|(index, _)| index)
                .collect();
            view.cells = keep
                .iter()
                .filter_map(|index| view.cells.get(*index).cloned())
                .collect();
            items.retain(|item| item.pinned);
            view.separator = None;
        }
        let settings = settings.clone();
        let width = self
            .output_size()
            .map_or(1920, |(width, _)| width.max(120) as u32);
        let status = Status {
            time: self.clock.time.clone(),
            date: self.clock.date.clone(),
            battery: self.battery.map(|battery| battery.percent),
        };
        if let Err(error) = self
            .dock
            .configure(&mut self.host, &settings, width, status)
        {
            tracing::warn!(%error,"panel layout could not be applied");
        }
        self.dock_items = items;
        match self.dock.show(&mut self.host, view) {
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "dock update failed"),
        }
        self.update_panels();
        self.sync_minimize_rects();
        self.apply_effects();
    }

    /// Re-renders icons when the output scale changed after the first frame.
    pub(super) fn check_scale(&mut self) {
        if self.current_icon_scale() != self.icon_scale {
            self.refresh_dock();
        }
    }

    /// Tells the compositor where each window's dock icon is, only when it changed.
    pub(super) fn sync_minimize_rects(&mut self) {
        let Some(proto) = &self.proto else {
            return;
        };
        let Some(surface) = self.host.wl_surface(self.dock.id()) else {
            return;
        };
        for (index, item) in self.dock_items.iter().enumerate() {
            let Some(rect) = self.dock.visible_cell_rect(&mut self.host, index) else {
                for window in &item.windows {
                    if self.minimize_sent.remove(window).is_some() {
                        proto.set_minimize_rectangle(*window, None, (0, 0, 0, 0));
                    }
                }
                continue;
            };
            let target = (
                rect.x as i32,
                rect.y as i32,
                rect.width as i32,
                rect.height as i32,
            );
            for window in &item.windows {
                if self.minimize_sent.get(window) == Some(&target) {
                    continue;
                }
                proto.set_minimize_rectangle(*window, Some(&surface), target);
                self.minimize_sent.insert(*window, target);
            }
        }
        let live: Vec<_> = self.windows.windows().iter().map(|w| w.id).collect();
        self.minimize_sent.retain(|id, _| live.contains(id));
    }

    /// Moves the synthetic pointer once the target surface is mapped (`--demo-hover`).
    pub(super) fn run_demo_hover(&mut self) {
        let Some(spot) = self.args.demo_hover.filter(|_| self.demo_pending) else {
            return;
        };
        let (surface, point) = match spot {
            DemoHover::Dock(index) => (self.dock.id(), self.dock_point(index)),
            DemoHover::Bar(spot) => (self.bar.id(), self.bar_point(spot)),
        };
        if !self.host.is_configured(surface) || self.host.is_busy() {
            return;
        }
        self.demo_pending = false;
        if let Some((x, y)) = point {
            self.host.update(surface, |ui| ui.pointer_move(x, y));
        }
    }

    fn dock_point(&self, index: usize) -> Option<(f64, f64)> {
        let rect = match index {
            0 => self.dock.layout().launcher,
            n => self.dock.cell_rect(n - 1)?,
        };
        Some(centre(rect))
    }

    fn bar_point(&mut self, spot: BarSpot) -> Option<(f64, f64)> {
        let selector = match spot {
            BarSpot::Clock => "#clock",
            BarSpot::Status => ".tb-group",
        };
        let id: SurfaceId = self.bar.id();
        let rect = self.host.surface_mut(id)?.element_rect(selector).ok()?;
        Some(centre(rect))
    }
}

fn centre(rect: Rect) -> (f64, f64) {
    (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}
