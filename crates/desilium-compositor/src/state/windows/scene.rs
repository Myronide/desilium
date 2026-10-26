use crate::animation::{ButtonFade, Pose, Xform};
use crate::decoration::{Button, ButtonState, Frame};
use crate::state::DesiliumState;
use desilium_theme::Theme;
use desilium_wm::{Placement, Rect, WindowId, WindowState};
use smithay::desktop::Window;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;

/// One visible window as the renderer needs it.
pub struct SceneWindow {
    pub id: WindowId,
    pub window: Window,
    /// The window's frame: managed rectangle plus the title bar, button and client rectangles.
    pub frame: Frame,
    pub state: WindowState,
    /// The title the client set (empty if none).
    pub title: String,
    /// Hover and pressed state of the title bar buttons.
    pub buttons: [ButtonState; 3],
    /// Animated look: `0` is the inactive and `1` the active colour set.
    pub focus: f32,
    /// Fading hover backgrounds of the title bar buttons.
    pub fades: [ButtonFade; 3],
    /// Maps the window's final layout to what is on screen right now.
    pub xform: Xform,
}

/// A destroyed window fading out from its snapshot.
pub struct ClosingWindow {
    pub id: WindowId,
    /// The frame rectangle the snapshot shows.
    pub layout: Rect,
    pub focus: f32,
    pub xform: Xform,
}

impl DesiliumState {
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        // Border and title bar sizes move the client surface inside its frame.
        self.sync_space();
    }

    /// Windows to draw, bottom to top, each with its current animation pose.
    pub fn scene_windows(&self) -> Vec<SceneWindow> {
        let placements = self.wm.placements();
        let poses = self.window_poses(&placements);
        let mut windows: Vec<SceneWindow> = placements
            .iter()
            .filter_map(|placement| self.scene_window(placement, poses.get(&placement.id)?))
            .collect();
        // Windows still shrinking into the minimized pose stay on top until they are gone.
        let mut leaving: Vec<_> = poses
            .iter()
            .filter(|(id, _)| !placements.iter().any(|placement| placement.id == **id))
            .collect();
        leaving.sort_by_key(|(id, _)| **id);
        for (id, pose) in leaving {
            let placement = Placement {
                id: *id,
                rect: pose.layout.to_rect(),
                state: WindowState::Normal,
            };
            windows.extend(self.scene_window(&placement, pose));
        }
        windows
    }

    fn scene_window(&self, placement: &Placement, pose: &Pose) -> Option<SceneWindow> {
        let window = self.windows.get(&placement.id)?.clone();
        let interaction = self.decoration.interaction();
        Some(SceneWindow {
            id: placement.id,
            frame: self.frame_of(placement.id, placement.rect),
            state: placement.state,
            title: window_title(&window),
            buttons: Button::ALL.map(|button| interaction.button_state(placement.id, button)),
            focus: pose.focus,
            fades: pose.buttons,
            xform: pose.xform(),
            window,
        })
    }
}

fn window_title(window: &Window) -> String {
    let Some(toplevel) = window.toplevel() else {
        return String::new();
    };
    with_states(toplevel.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok().and_then(|data| data.title.clone()))
            .unwrap_or_default()
    })
}
