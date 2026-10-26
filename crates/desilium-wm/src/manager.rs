use crate::geometry::{Point, Rect, Size};
use crate::ids::WindowId;
use crate::resize::Edges;
use crate::state::WindowState;
use crate::workspace::{Window, Workspace};

const CASCADE_INSET: i32 = 48;
const CASCADE_STEP: i32 = 32;
const CASCADE_SLOTS: u64 = 10;

/// A window's id, current rectangle and state, as reported in stacking order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub id: WindowId,
    pub rect: Rect,
    pub state: WindowState,
}

/// Floating window manager over a logical area and its usable work area; positions only, no rendering.
pub struct WindowManager {
    area: Rect,
    work_area: Rect,
    pub(crate) workspace: Workspace,
    last_id: u64,
    opened: u64,
}

impl WindowManager {
    pub fn new(area: Rect) -> Self {
        Self {
            area,
            work_area: area,
            workspace: Workspace::default(),
            last_id: 0,
            opened: 0,
        }
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    /// Usable area minus panels; maximized windows fill it, fullscreen ones ignore it.
    pub fn work_area(&self) -> Rect {
        self.work_area
    }

    pub fn set_work_area(&mut self, work_area: Rect) {
        self.work_area = work_area;
        self.refit_layouts();
    }

    /// Existing floating windows keep their positions; a work area that followed the area keeps following it.
    pub fn set_area(&mut self, area: Rect) {
        if self.work_area == self.area {
            self.work_area = area;
        }
        self.area = area;
        self.refit_layouts();
    }

    pub fn open_window(&mut self, size: Size) -> WindowId {
        let id = self.mint_id();
        self.open_window_with_id(size, id);
        id
    }

    /// Opens a window under an id minted elsewhere (used by `Desktops` for global uniqueness).
    pub(crate) fn open_window_with_id(&mut self, size: Size, id: WindowId) {
        let loc = cascade_location(self.area, size, self.opened);
        self.opened = self.opened.saturating_add(1);
        self.workspace.open(id, Rect { loc, size });
    }

    pub fn close_window(&mut self, id: WindowId) -> bool {
        self.workspace.remove(id)
    }

    pub fn focus(&mut self, id: WindowId) -> bool {
        self.workspace.focus(id)
    }

    pub fn clear_focus(&mut self) {
        self.workspace.clear_focus();
    }

    pub fn focused(&self) -> Option<WindowId> {
        self.workspace.focused()
    }

    pub fn rect(&self, id: WindowId) -> Option<Rect> {
        self.workspace.rect(id)
    }

    /// Moves a window. A maximized, fullscreen or snapped window first returns to its free-floating
    /// size. Minimized windows cannot be moved (returns false).
    pub fn move_to(&mut self, id: WindowId, loc: Point) -> bool {
        if self
            .workspace
            .shape(id)
            .is_some_and(|shape| shape.minimized)
        {
            return false;
        }
        self.leave_layout(id);
        self.workspace.move_to(id, loc)
    }

    /// Returns the rectangle the resize starts from; `None` unless the window is in the normal state.
    pub fn begin_resize(&mut self, id: WindowId, edges: Edges) -> Option<Rect> {
        self.workspace.begin_resize(id, edges)
    }

    pub fn end_resize(&mut self, id: WindowId) {
        self.workspace.end_resize(id);
    }

    /// Records the size the client actually committed; returns true if the rect changed.
    pub fn on_commit(&mut self, id: WindowId, size: Size) -> bool {
        self.workspace.on_commit(id, size)
    }

    /// Bottom to top, visible (non-minimized) windows only.
    pub fn placements(&self) -> Vec<Placement> {
        self.workspace
            .placements()
            .into_iter()
            .filter_map(|(id, rect)| {
                let state = self.workspace.shape(id)?.state();
                Some(Placement { id, rect, state })
            })
            .collect()
    }

    /// Topmost visible window under `point`; minimized windows are never hit.
    pub fn window_at(&self, point: Point) -> Option<WindowId> {
        self.workspace.window_at(point)
    }

    /// Number of windows, minimized ones included.
    pub fn len(&self) -> usize {
        self.workspace.len()
    }

    pub fn is_empty(&self) -> bool {
        self.workspace.is_empty()
    }

    /// Bottom to top, including minimized windows.
    pub(crate) fn window_ids(&self) -> Vec<WindowId> {
        self.workspace.stack().to_vec()
    }

    pub(crate) fn detach(&mut self, id: WindowId) -> Option<Window> {
        self.workspace.detach(id)
    }

    pub(crate) fn attach(&mut self, window: Window) {
        self.workspace.attach(window);
    }

    fn mint_id(&mut self) -> WindowId {
        self.last_id = self.last_id.saturating_add(1);
        WindowId::new(self.last_id)
    }
}

/// Staggered origin for the `index`-th opened window, pulled inside `area` when it fits.
fn cascade_location(area: Rect, size: Size, index: u64) -> Point {
    let slot = i32::try_from(index % CASCADE_SLOTS).unwrap_or(0);
    let offset = CASCADE_INSET.saturating_add(slot.saturating_mul(CASCADE_STEP));
    Point {
        x: fit_axis(
            area.loc.x.saturating_add(offset),
            area.loc.x,
            area.right(),
            size.w,
        ),
        y: fit_axis(
            area.loc.y.saturating_add(offset),
            area.loc.y,
            area.bottom(),
            size.h,
        ),
    }
}

/// Keeps a window of length `len` inside `[start, end)` where possible; `start` wins if it cannot fit.
fn fit_axis(candidate: i32, start: i32, end: i32, len: i32) -> i32 {
    candidate.min(end.saturating_sub(len)).max(start)
}

#[cfg(test)]
mod tests;
