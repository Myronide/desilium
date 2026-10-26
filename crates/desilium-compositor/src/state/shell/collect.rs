//! Builds the [`ShellSnapshot`] from the compositor state.

use super::snapshot::{
    OutputInfo, ShellSnapshot, WindowFlags, WindowInfo, WorkspaceInfo, clamp_text, scale_to_120ths,
};
use crate::state::DesiliumState;
use desilium_wm::WindowId;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;

impl DesiliumState {
    /// The state the shell can observe right now.
    pub(super) fn shell_snapshot(&self) -> ShellSnapshot {
        let focused = self.wm.focused();
        let windows = self
            .wm
            .all_windows()
            .into_iter()
            .filter_map(|(id, desktop)| self.shell_window_info(id, desktop, focused))
            .collect();
        let workspaces = WorkspaceInfo {
            count: self.wm.count(),
            active: self.wm.active(),
        };
        ShellSnapshot::new(windows, workspaces, self.shell_output_info())
    }

    fn shell_window_info(
        &self,
        id: WindowId,
        desktop: usize,
        focused: Option<WindowId>,
    ) -> Option<WindowInfo> {
        let toplevel = self.windows.get(&id)?.toplevel()?;
        let state = self.wm.manager(desktop)?.window_state(id)?;
        let (title, app_id) = with_states(toplevel.wl_surface(), |states| {
            let Some(data) = states.data_map.get::<XdgToplevelSurfaceData>() else {
                return (String::new(), String::new());
            };
            let Ok(attributes) = data.lock() else {
                return (String::new(), String::new());
            };
            (
                attributes.title.clone().unwrap_or_default(),
                attributes.app_id.clone().unwrap_or_default(),
            )
        });
        // Focus only exists on the desktop in view.
        let has_focus = desktop == self.wm.active() && focused == Some(id);
        Some(WindowInfo {
            id,
            title: clamp_text(&title),
            app_id: clamp_text(&app_id),
            flags: WindowFlags::new(state, has_focus),
            workspace: desktop,
            rect: self.wm.manager(desktop)?.rect(id)?,
        })
    }

    fn shell_output_info(&self) -> Option<OutputInfo> {
        let output = self.layer_output()?;
        Some(OutputInfo {
            area: self.wm.area(),
            work_area: self.wm.work_area(),
            scale_120ths: scale_to_120ths(output.current_scale().fractional_scale()),
        })
    }
}
