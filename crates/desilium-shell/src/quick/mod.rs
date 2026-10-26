pub(crate) mod placement;
mod view;
use crate::{services::controls::State, slot::Slot};
use desilium_theme::Theme;
use desilium_ui::{
    SurfaceOptions, UiError, UiSurface,
    wayland::{Anchor, KeyboardMode, Layer, LayerConfig, LayerHost, SurfaceId},
};
const CSS: &str = include_str!("style.css");
/// Height the quick settings want; the placement shrinks it to the space between the panels.
const DESIRED_HEIGHT: u32 = 640;
pub struct AppMenu {
    pub key: String,
    pub name: String,
    pub pinned: bool,
    pub can_pin: bool,
    pub windows: Vec<(u32, String)>,
}
pub struct QuickSettings {
    pub menu: Option<AppMenu>,
    slot: Slot,
    origin: placement::Origin,
    pub state: State,
}
impl QuickSettings {
    pub fn new(theme: &Theme) -> Result<Self, UiError> {
        let mut options = SurfaceOptions::new(410, 620);
        options.theme = theme.clone();
        let ui = UiSurface::new(
            "<div id=root></div>",
            &[crate::view::BASE_CSS, CSS],
            options,
        )?;
        let mut config = LayerConfig::new("desilium-quick-settings", (410, 620));
        config.layer = Layer::Overlay;
        config.anchor = Anchor {
            top: true,
            right: true,
            ..Anchor::CENTER
        };
        config.margins.top = 44;
        config.margins.right = 16;
        config.exclusive_zone = -1;
        config.keyboard = KeyboardMode::Exclusive;
        config.report_keys = true;
        Ok(Self {
            slot: Slot::parked(config, ui),
            origin: placement::Origin::Top,
            state: State::default(),
            menu: None,
        })
    }
    pub fn id(&self) -> Option<SurfaceId> {
        self.slot.id()
    }
    pub fn owns(&self, id: SurfaceId) -> bool {
        self.slot.owns(id)
    }
    pub fn is_open(&self) -> bool {
        self.slot.is_shown()
    }
    pub fn close(&mut self, host: &mut LayerHost) {
        self.slot.hide(host);
        self.menu = None;
    }
    pub fn open(
        &mut self,
        host: &mut LayerHost,
        settings: &desilium_config_types::ShellSettings,
        battery: &str,
        output: (i32, i32),
        insets: placement::Insets,
        origin: placement::Origin,
    ) -> Result<(), UiError> {
        self.origin = origin;
        self.reposition(host, output, insets)?;
        self.redraw(host, settings, battery)?;
        self.slot.show(host)?;
        Ok(())
    }
    pub fn reposition(
        &mut self,
        host: &mut LayerHost,
        output: (i32, i32),
        insets: placement::Insets,
    ) -> Result<(), UiError> {
        let desired = self
            .menu
            .as_ref()
            .map_or(DESIRED_HEIGHT, |menu| {
                (220 + menu.windows.len() as u32 * 48).min(DESIRED_HEIGHT)
            });
        let rect = placement::bounds(output, self.origin, desired, insets);
        if let Some(ui) = self.slot.ui_mut(host) {
            ui.resize(rect.width as u32, rect.height as u32)?;
        }
        self.slot.reconfigure(host, |config| {
            config.anchor = Anchor {
                top: true,
                left: true,
                bottom: false,
                right: false,
            };
            config.size = (rect.width as u32, rect.height as u32);
            config.margins.top = rect.y as i32;
            config.margins.left = rect.x as i32;
            config.margins.bottom = 0;
            config.margins.right = 0;
        })
    }
    pub fn redraw(
        &mut self,
        host: &mut LayerHost,
        settings: &desilium_config_types::ShellSettings,
        battery: &str,
    ) -> Result<(), UiError> {
        if let Some(ui) = self.slot.ui_mut(host) {
            let html = self.menu.as_ref().map_or_else(
                || view::html(&self.state, settings, battery),
                |menu| view::app_menu(menu, &self.state.notice),
            );
            ui.replace_children("#root", &html)?;
        }
        Ok(())
    }
    pub fn theme(&mut self, host: &mut LayerHost, theme: &Theme) {
        if let Some(ui) = self.slot.ui_mut(host) {
            ui.set_theme(theme);
        }
    }
}
#[cfg(test)]
pub fn html_for_tests(state: &State, settings: &desilium_config_types::ShellSettings) -> String {
    view::html(state, settings, "Battery")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_services_are_not_presented_as_enabled_toggles() {
        let html = html_for_tests(&State::default(), &Default::default());
        assert!(html.contains("Unavailable"));
        assert!(!html.contains("data-action=wifi"));
    }
    #[test]
    fn available_services_expose_real_actions_and_escape_errors() {
        let state = State {
            wifi: Some(true),
            bluetooth: Some(false),
            volume: Some(42),
            brightness: Some(70),
            notice: "<bad>".into(),
            ..Default::default()
        };
        let html = html_for_tests(&state, &Default::default());
        assert!(html.contains("data-action=wifi"));
        assert!(html.contains("42%"));
        assert!(html.contains("&lt;bad&gt;"));
        assert!(!html.contains("<bad>"));
    }
}

#[cfg(test)]
mod app_menu_tests {
    use super::*;
    #[test]
    fn app_menu_exposes_pins_and_live_window_actions_and_escapes_titles() {
        let menu = AppMenu {
            key: "kitty.desktop".into(),
            name: "Kitty".into(),
            pinned: true,
            can_pin: true,
            windows: vec![(4, "<shell>".into())],
        };
        let html = view::app_menu(&menu, "");
        assert!(html.contains("Unpin from panel"));
        assert!(html.contains("data-action=window-activate data-arg=4"));
        assert!(html.contains("&lt;shell&gt;"));
        assert!(!html.contains("<shell>"));
    }
}
