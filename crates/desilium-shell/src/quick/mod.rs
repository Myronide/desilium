pub(crate) mod placement;
mod view;
use crate::{services::controls::State, slot::Slot};
use desilium_theme::Theme;
use desilium_ui::{
    SurfaceOptions, UiError, UiSurface,
    wayland::{Anchor, KeyboardMode, Layer, LayerConfig, LayerHost, SurfaceId},
};
const CSS: &str = include_str!("style.css");
/// Height cap of the app menu; the placement shrinks any height to the space between the panels.
const DESIRED_HEIGHT: u32 = 640;
/// Panel padding, top row, tile rows and their gaps, in logical pixels (see `style.css`).
const PADDING: u32 = 36;
const TOP_ROW: u32 = 40;
const SLIDER_ROW: u32 = 40;
const TILE_ROW: u32 = 52;
const TILE_ROWS: u32 = 3;
const GAP: u32 = 14;
const TILE_GAP: u32 = 10;
const NETWORK_ROW: u32 = 40;
const NOTICE: u32 = 44;
/// Wi-Fi networks listed at most, as many as the controls report.
const MAX_NETWORKS: usize = crate::services::controls::MAX_WIFI_NETWORKS;

/// Height the panel needs for what `state` currently shows, so nothing scrolls.
fn panel_height(state: &State, networks_open: bool) -> u32 {
    let sliders = u32::from(state.volume.is_some()) + u32::from(state.brightness.is_some());
    let mut height = PADDING + TOP_ROW;
    height += sliders * (GAP + SLIDER_ROW);
    height += GAP + TILE_ROWS * TILE_ROW + (TILE_ROWS - 1) * TILE_GAP;
    let networks = state.networks.len().min(MAX_NETWORKS) as u32;
    if networks_open && networks > 0 {
        height += GAP + networks * NETWORK_ROW;
    }
    if !state.notice.is_empty() {
        height += GAP + NOTICE;
    }
    height
}
pub(crate) struct AppMenu {
    pub key: String,
    pub name: String,
    pub pinned: bool,
    pub can_pin: bool,
    pub windows: Vec<(u32, String)>,
}
pub(crate) struct QuickSettings {
    pub menu: Option<AppMenu>,
    slot: Slot,
    origin: placement::Origin,
    pub state: State,
    /// The dark theme is active (lights the "Dark" tile).
    pub dark: bool,
    /// The Wi-Fi network list is unfolded.
    pub networks_open: bool,
    /// Position and size last requested, to skip identical requests.
    placed: Option<(i32, i32, u32, u32)>,
}
impl QuickSettings {
    pub(crate) fn new(theme: &Theme) -> Result<Self, UiError> {
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
            dark: true,
            networks_open: false,
            placed: None,
            menu: None,
        })
    }
    pub(crate) fn id(&self) -> Option<SurfaceId> {
        self.slot.id()
    }
    pub(crate) fn owns(&self, id: SurfaceId) -> bool {
        self.slot.owns(id)
    }
    pub(crate) fn is_open(&self) -> bool {
        self.slot.is_shown()
    }
    pub(crate) fn close(&mut self, host: &mut LayerHost) {
        self.slot.hide(host);
        self.menu = None;
        self.networks_open = false;
        self.placed = None;
    }
    pub(crate) fn open(
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
    pub(crate) fn reposition(
        &mut self,
        host: &mut LayerHost,
        output: (i32, i32),
        insets: placement::Insets,
    ) -> Result<(), UiError> {
        let desired = self.menu.as_ref().map_or_else(
            || panel_height(&self.state, self.networks_open),
            |menu| (220 + menu.windows.len() as u32 * 48).min(DESIRED_HEIGHT),
        );
        let rect = placement::bounds(output, self.origin, desired, insets);
        let placed = (
            rect.x as i32,
            rect.y as i32,
            rect.width as u32,
            rect.height as u32,
        );
        if self.placed == Some(placed) {
            return Ok(());
        }
        self.placed = Some(placed);
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
    pub(crate) fn redraw(
        &mut self,
        host: &mut LayerHost,
        settings: &desilium_config_types::ShellSettings,
        battery: &str,
    ) -> Result<(), UiError> {
        if let Some(ui) = self.slot.ui_mut(host) {
            let look = view::Look {
                dark: self.dark,
                battery,
                networks_open: self.networks_open,
            };
            let html = self.menu.as_ref().map_or_else(
                || view::html(&self.state, settings, look),
                |menu| view::app_menu(menu, &self.state.notice),
            );
            ui.replace_children("#root", &html)?;
        }
        Ok(())
    }
    pub(crate) fn theme(&mut self, host: &mut LayerHost, theme: &Theme) {
        if let Some(ui) = self.slot.ui_mut(host) {
            ui.set_theme(theme);
        }
    }
}
#[cfg(test)]
pub(crate) fn html_for_tests(
    state: &State,
    settings: &desilium_config_types::ShellSettings,
) -> String {
    let look = view::Look {
        dark: true,
        battery: "87%",
        networks_open: true,
    };
    view::html(state, settings, look)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(state: &State) -> String {
        html_for_tests(state, &Default::default())
    }

    #[test]
    fn missing_services_are_dimmed_and_do_nothing() {
        let html = plain(&State::default());
        assert!(html.contains("tile unavailable"));
        assert!(!html.contains("data-action='wifi'"));
        assert!(!html.contains("data-action='bluetooth'"));
        assert!(
            !html.contains("class=slider"),
            "no slider without audio or backlight"
        );
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
        let html = plain(&state);
        assert!(
            html.contains("data-action='wifi' data-arg='false'"),
            "wifi is on, a click turns it off"
        );
        assert!(html.contains("data-action='bluetooth' data-arg='true'"));
        assert!(html.contains("data-action='volume' data-arg='100'"));
        assert!(html.contains("flex:42 1 0") && html.contains("flex:58 1 0"));
        assert!(html.contains("&lt;bad&gt;"));
        assert!(!html.contains("<bad>"));
    }

    #[test]
    fn tiles_are_lit_by_their_state_and_labels_stay_short() {
        let state = State {
            bluetooth: Some(true),
            microphone: Some(true),
            profile: Some("performance".into()),
            ..Default::default()
        };
        let html = plain(&state);
        assert!(html.contains("tile on' data-action='bluetooth'"));
        assert!(
            !html.contains("tile on' data-action='microphone'"),
            "a muted microphone is not lit"
        );
        assert!(html.contains("data-action='microphone' data-arg='false'"));
        assert!(html.contains("<span>Performance</span>"));
        assert!(html.contains("data-action='profile' data-arg='power-saver'"));
        assert!(
            html.contains("tile on' data-action='theme'"),
            "the test look is dark"
        );
        assert!(html.contains("<span>87%</span>"));
    }

    #[test]
    fn muting_shows_an_empty_slider_and_offers_to_unmute() {
        let state = State {
            volume: Some(60),
            muted: true,
            ..Default::default()
        };
        let html = plain(&state);
        assert!(html.contains("data-action='mute' data-arg='false'"));
        assert!(html.contains("flex:0 1 0"));
    }

    #[test]
    fn the_panel_mode_tile_offers_the_other_mode() {
        let html = plain(&State::default());
        assert!(html.contains("data-action='mode' data-arg='taskbar'"));
        let taskbar = desilium_config_types::ShellSettings {
            mode: desilium_config_types::PanelMode::Taskbar,
            ..Default::default()
        };
        let html = html_for_tests(&State::default(), &taskbar);
        assert!(html.contains("tile on' data-action='mode' data-arg='dock'"));
    }

    #[test]
    fn network_list_shows_only_when_unfolded() {
        use crate::services::controls::WifiNetwork;
        let state = State {
            wifi: Some(true),
            networks: vec![WifiNetwork {
                ssid: "Office".into(),
                signal: 80,
                secured: true,
                active: true,
            }],
            ..Default::default()
        };
        let open = view::Look {
            dark: true,
            battery: "",
            networks_open: true,
        };
        let closed = view::Look {
            networks_open: false,
            ..open
        };
        assert!(view::html(&state, &Default::default(), open).contains("data-action=wifi-connect"));
        assert!(!view::html(&state, &Default::default(), closed).contains("wifi-connect"));
    }

    #[test]
    fn height_follows_what_is_shown() {
        let bare = panel_height(&State::default(), false);
        let sliders = State {
            volume: Some(10),
            brightness: Some(10),
            ..Default::default()
        };
        assert_eq!(panel_height(&sliders, false), bare + 2 * (GAP + SLIDER_ROW));
        let networks = State {
            networks: vec![Default::default(); 3],
            ..Default::default()
        };
        assert_eq!(
            panel_height(&networks, false),
            bare,
            "folded list takes no room"
        );
        assert_eq!(panel_height(&networks, true), bare + GAP + 3 * NETWORK_ROW);
        let notice = State {
            notice: "x".into(),
            ..Default::default()
        };
        assert_eq!(panel_height(&notice, false), bare + GAP + NOTICE);
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
