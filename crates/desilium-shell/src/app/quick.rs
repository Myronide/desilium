use super::Shell;
use crate::services::controls::{Action, State};
impl Shell {
    pub(super) fn toggle_quick_settings(&mut self, from_taskbar: bool) {
        if self.quick.is_open() {
            self.quick.close(&mut self.host);
            return;
        }
        if !self.store.get().shell.quick_settings {
            return;
        }
        self.launcher.close(&mut self.host);
        self.quick_focused = false;
        self.quick.menu = None;
        let battery = self.battery.map_or_else(
            || "Desktop controls".into(),
            |battery| format!("Battery {}%", battery.percent),
        );
        let output = self.output_size().unwrap_or((1280, 800));
        let origin = if from_taskbar
            && self.store.get().shell.edge == desilium_config_types::PanelEdge::Bottom
        {
            crate::quick::placement::Origin::Bottom
        } else {
            crate::quick::placement::Origin::Top
        };
        let insets = self.popup_insets();
        if let Err(error) = self.quick.open(
            &mut self.host,
            &self.store.get().shell,
            &battery,
            output,
            insets,
            origin,
        ) {
            tracing::warn!(%error,"quick settings could not open");
        }
        self.quick_send(Action::Refresh);
        if let (Some(proto), Some(surface)) = (
            &self.proto,
            self.quick.id().and_then(|id| self.host.wl_surface(id)),
        ) {
            proto.set_surface_effects(&surface, 24, [20; 4]);
        }
    }
    pub(super) fn quick_updated(&mut self, state: State) {
        self.quick.state = state;
        let battery = self.battery.map_or_else(
            || "Desktop controls".into(),
            |battery| format!("Battery {}%", battery.percent),
        );
        if let Err(error) = self
            .quick
            .redraw(&mut self.host, &self.store.get().shell, &battery)
        {
            tracing::warn!(%error,"quick settings could not update");
        }
    }
    fn quick_send(&mut self, action: Action) {
        let error = self
            .controls
            .as_ref()
            .ok_or_else(|| "System controls are unavailable.".to_owned())
            .and_then(|worker| worker.send(action));
        if let Err(error) = error {
            let mut state = self.quick.state.clone();
            state.notice = error;
            self.quick_updated(state);
        }
    }
    pub(super) fn quick_action(&mut self, action: &str, arg: &str) {
        if self.context_action(action, arg) {
            return;
        }

        let enabled = arg.parse::<bool>().unwrap_or(false);
        let percent = arg.parse::<u8>().unwrap_or(0).min(100);
        let command = match action {
            "close" => {
                self.quick.close(&mut self.host);
                return;
            }
            "wifi" => Action::Wifi(enabled),
            "wifi-connect" => Action::WifiConnect(arg.into()),
            "bluetooth" => Action::Bluetooth(enabled),
            "microphone" => Action::Microphone(enabled),
            "mute" => Action::Mute(enabled),
            "volume" => Action::Volume(percent),
            "brightness" => Action::Brightness(percent),
            "profile" => Action::Profile(arg.into()),
            "theme" => {
                self.edit_quick_config(|config| {
                    config.appearance.theme = if config.appearance.theme == "desilium-dark" {
                        "desilium-light"
                    } else {
                        "desilium-dark"
                    }
                    .into()
                });
                return;
            }
            "mode" => {
                self.edit_quick_config(|config| {
                    config.shell.mode = if arg == "taskbar" {
                        desilium_config_types::PanelMode::Taskbar
                    } else {
                        desilium_config_types::PanelMode::Dock
                    }
                });
                return;
            }
            "settings" | "panel-settings" => {
                self.open_settings(action == "panel-settings");
                return;
            }
            "refresh" => Action::Refresh,
            _ => return,
        };
        self.quick_send(command);
    }
    fn edit_quick_config(&mut self, edit: impl FnOnce(&mut desilium_config_types::DesiliumConfig)) {
        let mut transaction = self.store.transaction();
        transaction.edit(edit);
        match transaction.commit() {
            Ok(_) => {
                self.apply_theme();
                self.refresh_bar();
                self.refresh_dock();
                self.apply_effects();
                let state = self.quick.state.clone();
                self.quick_updated(state);
            }
            Err(error) => {
                let mut state = self.quick.state.clone();
                state.notice = format!("Settings were not saved: {error}");
                self.quick_updated(state);
            }
        }
    }
    fn open_settings(&mut self, panel: bool) {
        let path = std::env::current_exe()
            .ok()
            .map(|path| path.with_file_name("desilium-settings"))
            .filter(|path| path.is_file());
        let mut command =
            std::process::Command::new(path.unwrap_or_else(|| "desilium-settings".into()));
        command.arg("--config-dir").arg(&self.config_dir);
        if panel {
            command.args(["--page", "panel"]);
        }
        match command.spawn() {
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(error) => {
                let mut state = self.quick.state.clone();
                state.notice = format!("Settings could not open: {error}");
                self.quick_updated(state);
            }
        }
    }
}
