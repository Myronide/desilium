use crate::Smallvil;
use smithay::{
    desktop::Window,
    input::keyboard::Keysym,
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    utils::{IsAlive, Logical, Rectangle, SERIAL_COUNTER},
};
use std::{
    collections::HashMap,
    process::{Child, Command},
};
use wc_core::{Chord, Placement, Settings, Snapshot};

pub struct Policy {
    pub settings: Settings,
    pub restore: HashMap<Window, Rectangle<i32, Logical>>,
    pub placements: HashMap<Window, Placement>,
    pub children: Vec<Child>,
}
impl Policy {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            restore: HashMap::new(),
            placements: HashMap::new(),
            children: Vec::new(),
        }
    }
}
impl Smallvil {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            window_details: self
                .space
                .elements()
                .filter(|w| w.alive())
                .map(|w| {
                    let loc = self.space.element_location(w).unwrap_or_default();
                    let size = w.geometry().size;
                    wc_core::WindowInfo {
                        x: loc.x,
                        y: loc.y,
                        width: size.w,
                        height: size.h,
                        focused: self.focused().as_ref() == Some(w),
                    }
                })
                .collect(),
            settings: self.policy.settings.clone(),
            windows: self.space.elements().filter(|w| w.alive()).count(),
            wayland_display: self.socket_name.to_string_lossy().into(),
        }
    }
    pub fn apply_settings(&mut self, settings: Settings) -> Result<(), String> {
        settings.validate()?;
        wc_core::save(&settings)?;
        self.seat.get_keyboard().unwrap().change_repeat_info(
            settings.keyboard_repeat_rate.into(),
            settings.keyboard_repeat_delay.into(),
        );
        self.policy.settings = settings;
        self.reflow();
        Ok(())
    }
    pub fn focused(&self) -> Option<Window> {
        let focus = self.seat.get_keyboard()?.current_focus()?;
        self.space
            .elements()
            .find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == &focus))
            .cloned()
    }
    pub fn focus_window(&mut self, window: &Window, raise: bool) {
        if raise {
            self.space.raise_element(window, true);
        }
        for w in self.space.elements() {
            w.set_activated(w == window);
            w.toplevel().unwrap().send_pending_configure();
        }
        self.seat.get_keyboard().unwrap().set_focus(
            self,
            Some(window.toplevel().unwrap().wl_surface().clone()),
            SERIAL_COUNTER.next_serial(),
        );
    }
    pub fn cycle(&mut self) {
        // Bottom to top stacking order: bringing the bottom window forward rotates the stack.
        let next = self.space.elements().find(|w| w.alive()).cloned();
        if let Some(w) = next {
            self.focus_window(&w, true);
        }
    }
    pub fn close_focused(&mut self) {
        if let Some(w) = self.focused() {
            w.toplevel().unwrap().send_close();
        }
    }
    pub fn place_focused(&mut self, placement: Placement) -> Result<(), String> {
        let window = self.focused().ok_or("Kein Fenster fokussiert")?;
        self.place_window(&window, placement);
        Ok(())
    }
    pub fn place_window(&mut self, window: &Window, placement: Placement) {
        let Some(output) = self.space.outputs().next().cloned() else {
            return;
        };
        let Some(output_geo) = self.space.output_geometry(&output) else {
            return;
        };
        let rect = if placement == Placement::Restore {
            self.policy.placements.remove(window);
            let Some(rect) = self.policy.restore.remove(window) else {
                return;
            };
            rect
        } else {
            let location = self.space.element_location(window).unwrap_or_default();
            self.policy
                .restore
                .entry(window.clone())
                .or_insert(Rectangle::new(location, window.geometry().size));
            self.policy.placements.insert(window.clone(), placement);
            let r = wc_core::placement_rect(
                output_geo.size.w,
                output_geo.size.h,
                self.policy.settings.gap,
                placement,
            );
            Rectangle::new(
                (r.x + output_geo.loc.x, r.y + output_geo.loc.y).into(),
                (r.w, r.h).into(),
            )
        };
        let top = window.toplevel().unwrap();
        top.with_pending_state(|s| {
            s.size = Some(rect.size);
            if placement == Placement::Fullscreen {
                s.states.set(xdg_toplevel::State::Fullscreen);
            } else {
                s.states.unset(xdg_toplevel::State::Fullscreen);
            }
            if placement == Placement::Maximize {
                s.states.set(xdg_toplevel::State::Maximized);
            } else {
                s.states.unset(xdg_toplevel::State::Maximized);
            }
        });
        self.space.map_element(window.clone(), rect.loc, false);
        top.send_pending_configure();
    }
    pub fn reflow(&mut self) {
        let placed: Vec<_> = self
            .policy
            .placements
            .iter()
            .map(|(w, p)| (w.clone(), *p))
            .collect();
        for (w, p) in placed {
            if w.alive() {
                self.place_window(&w, p);
            }
        }
    }
    pub fn prepare_interactive_move(&mut self, window: &Window) {
        if self.policy.placements.contains_key(window) {
            let pos = self.seat.get_pointer().unwrap().current_location();
            let width = self
                .policy
                .restore
                .get(window)
                .map(|r| r.size.w)
                .unwrap_or(600);
            self.place_window(window, Placement::Restore);
            self.space.map_element(
                window.clone(),
                (
                    (pos.x - f64::from(width) / 2.0).max(0.0) as i32,
                    (pos.y - 20.0).max(0.0) as i32,
                ),
                true,
            );
        }
    }
    pub fn snap_at(&mut self, window: &Window, position: smithay::utils::Point<f64, Logical>) {
        if !self.policy.settings.snap_enabled {
            return;
        }
        let Some(output) = self.space.outputs().next() else {
            return;
        };
        let Some(g) = self.space.output_geometry(output) else {
            return;
        };
        let d = f64::from(self.policy.settings.snap_distance);
        let placement = if position.y < f64::from(g.loc.y) + d {
            Some(Placement::Maximize)
        } else if position.x < f64::from(g.loc.x) + d {
            Some(Placement::Left)
        } else if position.x > f64::from(g.loc.x + g.size.w) - d {
            Some(Placement::Right)
        } else {
            None
        };
        if let Some(p) = placement {
            self.place_window(window, p);
        }
    }
    pub fn shortcut(
        &self,
        mods: &smithay::input::keyboard::ModifiersState,
        key: Keysym,
    ) -> Option<String> {
        self.policy
            .settings
            .shortcuts
            .entries()
            .into_iter()
            .find_map(|(action, value)| {
                let chord = Chord::parse(value).ok()?;
                let name = key.name()?.to_ascii_lowercase();
                (chord.super_ == mods.logo
                    && chord.ctrl == mods.ctrl
                    && chord.alt == mods.alt
                    && chord.shift == mods.shift
                    && chord.key == name)
                    .then(|| action.to_string())
            })
    }
    pub fn action(&mut self, action: &str) {
        match action {
            "terminal" => self.spawn_terminal(),
            "settings" => self.spawn_settings(),
            "close" => self.close_focused(),
            "cycle" => self.cycle(),
            "quit" => self.loop_signal.stop(),
            action => {
                let p = match action {
                    "maximize" => Placement::Maximize,
                    "left" => Placement::Left,
                    "right" => Placement::Right,
                    _ => Placement::Restore,
                };
                let _ = self.place_focused(p);
            }
        }
    }
    pub fn spawn_terminal(&mut self) {
        let mut command = Command::new(&self.policy.settings.terminal);
        command
            .env("WAYLAND_DISPLAY", &self.socket_name)
            .env_remove("DISPLAY");
        self.spawn(command);
    }
    pub fn spawn_settings(&mut self) {
        if let Ok(exe) = std::env::current_exe() {
            let mut command = Command::new(exe.with_file_name("wc-settings"));
            command
                .env("WAYLAND_DISPLAY", &self.socket_name)
                .env_remove("DISPLAY");
            self.spawn(command);
        }
    }
    pub fn spawn(&mut self, mut command: Command) {
        match command.spawn() {
            Ok(child) => self.policy.children.push(child),
            Err(error) => tracing::warn!(%error, "Programm konnte nicht gestartet werden"),
        }
    }
    pub fn housekeeping(&mut self) {
        self.policy.restore.retain(|w, _| w.alive());
        self.policy.placements.retain(|w, _| w.alive());
        self.policy
            .children
            .retain_mut(|c| matches!(c.try_wait(), Ok(None)));
        if self.seat.get_keyboard().unwrap().current_focus().is_some() && self.focused().is_none() {
            let top = self.space.elements().rfind(|w| w.alive()).cloned();
            if let Some(w) = top {
                self.focus_window(&w, false);
            }
        }
    }
}
