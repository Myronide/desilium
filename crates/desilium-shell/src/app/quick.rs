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
        self.quick.dark = self.theme.mode == desilium_theme::Mode::Dark;
        let battery = self.battery_text();
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
        self.apply_popup_effects();
    }

    /// Backdrop blur and rounded clipping behind the popup, the same as the launcher's glass.
    pub(super) fn apply_popup_effects(&mut self) {
        if let Some(id) = self.quick.id() {
            let radius = self.popup_radius();
            self.set_effects(id, super::views::LAUNCHER_BLUR, [radius; 4]);
        }
    }
    /// `87%`, or nothing on a machine without a battery.
    fn battery_text(&self) -> String {
        self.battery
            .map_or_else(String::new, |battery| format!("{}%", battery.percent))
    }

    pub(super) fn quick_updated(&mut self, state: State) {
        self.quick.state = state;
        self.quick.dark = self.theme.mode == desilium_theme::Mode::Dark;
        self.fit_quick_settings();
        let battery = self.battery_text();
        if let Err(error) = self
            .quick
            .redraw(&mut self.host, &self.store.get().shell, &battery)
        {
            tracing::warn!(%error,"quick settings could not update");
        }
    }

    /// Gives the open panel the height its content needs now (services answer after it opened).
    fn fit_quick_settings(&mut self) {
        if !self.quick.is_open() || self.quick.menu.is_some() {
            return;
        }
        let output = self.output_size().unwrap_or((1280, 800));
        let insets = self.popup_insets();
        match self.quick.reposition(&mut self.host, output, insets) {
            Ok(()) => self.apply_popup_effects(),
            Err(error) => tracing::warn!(%error, "quick settings could not be resized"),
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
            "networks" => {
                self.quick.networks_open = !self.quick.networks_open;
                let state = self.quick.state.clone();
                self.quick_updated(state);
                return;
            }
            _ => return,
        };
        self.quick_send(command);
    }
    pub(super) fn edit_quick_config(
        &mut self,
        edit: impl FnOnce(&mut desilium_config_types::DesiliumConfig),
    ) {
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
    pub(super) fn open_settings(&mut self, panel: bool) {
        let page = panel.then_some("panel");
        if let Err(error) = self.spawn_settings(page) {
            let mut state = self.quick.state.clone();
            state.notice = format!("Settings could not open: {error}");
            self.quick_updated(state);
        }
    }

    /// Starts the settings application, on `page` when one is given.
    pub(super) fn spawn_settings(&self, page: Option<&str>) -> std::io::Result<()> {
        let path = std::env::current_exe()
            .ok()
            .map(|path| path.with_file_name("desilium-settings"))
            .filter(|path| path.is_file());
        let mut command =
            std::process::Command::new(path.unwrap_or_else(|| "desilium-settings".into()));
        command.arg("--config-dir").arg(&self.config_dir);
        if let Some(page) = page {
            command.args(["--page", page]);
        }
        let mut child = command.spawn()?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}
