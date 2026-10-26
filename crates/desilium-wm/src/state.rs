use crate::geometry::{Point, Rect, Size};
use crate::ids::WindowId;
use crate::manager::WindowManager;

/// What a client should be told about its window; a snapped window reports `Normal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Maximized,
    Fullscreen,
    Minimized,
}

/// Where a visible window is placed; `Floating` is the only layout that is user-positioned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Layout {
    #[default]
    Floating,
    Snapped,
    Maximized,
    Fullscreen,
}

/// Layout, minimized flag and the free-floating rect to return to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Shape {
    pub(crate) layout: Layout,
    pub(crate) minimized: bool,
    restore: Option<Rect>,
}

impl Shape {
    pub(crate) fn state(&self) -> WindowState {
        if self.minimized {
            return WindowState::Minimized;
        }
        match self.layout {
            Layout::Floating | Layout::Snapped => WindowState::Normal,
            Layout::Maximized => WindowState::Maximized,
            Layout::Fullscreen => WindowState::Fullscreen,
        }
    }

    pub(crate) fn restore_rect(&self) -> Option<Rect> {
        self.restore
    }

    /// Switches to a non-floating layout; `current` becomes the restore rect when leaving free floating.
    pub(crate) fn enter(&mut self, target: Layout, current: Rect) {
        if self.layout == Layout::Floating {
            self.restore = Some(current);
        }
        self.layout = target;
    }

    /// Returns to free floating and yields the rect to show; `current` if already floating.
    pub(crate) fn leave(&mut self, current: Rect) -> Rect {
        if self.layout == Layout::Floating {
            return current;
        }
        self.layout = Layout::Floating;
        self.restore.take().unwrap_or(current)
    }
}

impl WindowManager {
    /// The state reported to clients, or `None` for an unknown window.
    pub fn window_state(&self, id: WindowId) -> Option<WindowState> {
        self.workspace.shape(id).map(|shape| shape.state())
    }

    /// The free-floating rect a non-floating window returns to; `None` while floating.
    pub fn restore_rect(&self, id: WindowId) -> Option<Rect> {
        self.workspace.shape(id)?.restore_rect()
    }

    /// Bottom to top.
    pub fn minimized_windows(&self) -> Vec<WindowId> {
        self.workspace.minimized_ids()
    }

    /// Fills the work area. Returns true if the window exists.
    pub fn maximize(&mut self, id: WindowId) -> bool {
        let work_area = self.work_area();
        self.enter_layout(id, Layout::Maximized, work_area)
    }

    /// Returns a maximized window to its restore rect. Returns true if the window exists.
    pub fn unmaximize(&mut self, id: WindowId) -> bool {
        self.leave_if(id, Layout::Maximized)
    }

    /// Maximizes or unmaximizes. Returns true if the window exists.
    pub fn toggle_maximize(&mut self, id: WindowId) -> bool {
        if self.window_state(id) == Some(WindowState::Maximized) {
            return self.unmaximize(id);
        }
        self.maximize(id)
    }

    /// Covers the whole area (ignoring panels). Returns true if the window exists.
    pub fn set_fullscreen(&mut self, id: WindowId, on: bool) -> bool {
        if on {
            let area = self.area();
            return self.enter_layout(id, Layout::Fullscreen, area);
        }
        self.leave_if(id, Layout::Fullscreen)
    }

    /// Hides the window and moves focus to the topmost visible one. Returns true if the window exists.
    pub fn minimize(&mut self, id: WindowId) -> bool {
        let (Some(mut shape), Some(rect)) = (self.workspace.shape(id), self.workspace.rect(id))
        else {
            return false;
        };
        shape.minimized = true;
        self.apply_shape(id, shape, rect);
        if self.workspace.focused() == Some(id) {
            self.focus_topmost_visible();
        }
        true
    }

    /// Unminimizes, raises and focuses. Returns false for an unknown window.
    pub fn restore(&mut self, id: WindowId) -> bool {
        let (Some(mut shape), Some(rect)) = (self.workspace.shape(id), self.workspace.rect(id))
        else {
            return false;
        };
        shape.minimized = false;
        self.apply_shape(id, shape, rect);
        self.workspace.focus(id)
    }

    /// Starts a drag. A non-floating window is first restored to its normal size, then placed so
    /// the pointer keeps its relative x position inside it (and its offset from the top edge,
    /// clamped to the restored height). Returns the new top-left, which the caller then passes to
    /// `move_to` while the drag continues. Floating windows return their current location
    /// unchanged. Returns `None` for minimized or unknown windows.
    pub fn begin_move(&mut self, id: WindowId, pointer: Point) -> Option<Point> {
        let mut shape = self.workspace.shape(id)?;
        let current = self.workspace.rect(id)?;
        if shape.minimized {
            return None;
        }
        if shape.layout == Layout::Floating {
            return Some(current.loc);
        }
        let restored = shape.leave(current);
        let loc = grab_location(current, restored.size, pointer);
        self.apply_shape(
            id,
            shape,
            Rect {
                loc,
                size: restored.size,
            },
        );
        Some(loc)
    }

    /// Restores a non-floating window to its free-floating rect (keeping the position it had).
    pub(crate) fn leave_layout(&mut self, id: WindowId) -> bool {
        let (Some(mut shape), Some(current)) = (self.workspace.shape(id), self.workspace.rect(id))
        else {
            return false;
        };
        if shape.layout == Layout::Floating {
            return true;
        }
        let rect = shape.leave(current);
        self.apply_shape(id, shape, rect)
    }

    pub(crate) fn enter_layout(&mut self, id: WindowId, target: Layout, rect: Rect) -> bool {
        let (Some(mut shape), Some(current)) = (self.workspace.shape(id), self.workspace.rect(id))
        else {
            return false;
        };
        shape.enter(target, current);
        self.apply_shape(id, shape, rect)
    }

    /// Re-applies the target rect of every maximized and fullscreen window after an area change.
    pub(crate) fn refit_layouts(&mut self) {
        for id in self.workspace.stack().to_vec() {
            let Some(shape) = self.workspace.shape(id) else {
                continue;
            };
            let target = match shape.layout {
                Layout::Maximized => self.work_area(),
                Layout::Fullscreen => self.area(),
                Layout::Floating | Layout::Snapped => continue,
            };
            self.workspace.set_rect(id, target);
        }
    }

    fn leave_if(&mut self, id: WindowId, layout: Layout) -> bool {
        let Some(shape) = self.workspace.shape(id) else {
            return false;
        };
        if shape.layout == layout {
            self.leave_layout(id);
        }
        true
    }

    /// A layout or minimize change invalidates a resize in flight, hence the phase reset.
    fn apply_shape(&mut self, id: WindowId, shape: Shape, rect: Rect) -> bool {
        let Some(slot) = self.workspace.shape_mut(id) else {
            return false;
        };
        *slot = shape;
        self.workspace.set_rect(id, rect);
        self.workspace.cancel_resize(id);
        true
    }

    fn focus_topmost_visible(&mut self) {
        match self.workspace.topmost_visible() {
            Some(next) => {
                self.workspace.focus(next);
            }
            None => self.workspace.clear_focus(),
        }
    }
}

/// Top-left that keeps the pointer's relative x inside the window, scaled to the restored width.
fn grab_location(current: Rect, restored: Size, pointer: Point) -> Point {
    let span = i64::from(current.size.w.max(1));
    let along_x = offset_within(pointer.x, current.loc.x, current.size.w);
    let scaled_x = i64::from(along_x) * i64::from(restored.w.max(0)) / span;
    let offset_x = i32::try_from(scaled_x).unwrap_or(i32::MAX);
    let along_y = offset_within(pointer.y, current.loc.y, restored.h.saturating_sub(1));
    Point {
        x: pointer.x.saturating_sub(offset_x),
        y: pointer.y.saturating_sub(along_y),
    }
}

fn offset_within(pointer: i32, origin: i32, max: i32) -> i32 {
    pointer.saturating_sub(origin).clamp(0, max.max(0))
}

#[cfg(test)]
mod tests;
