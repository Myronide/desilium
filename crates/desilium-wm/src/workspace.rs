use std::collections::HashMap;

use crate::geometry::{Point, Rect, Size};
use crate::ids::WindowId;
use crate::resize::{Edges, anchored_location};
use crate::state::{Shape, WindowState};

/// Resize state of a window; `Settling` lasts until the client's next commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResizePhase {
    Idle,
    Active { edges: Edges, initial: Rect },
    Settling { edges: Edges, initial: Rect },
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    rect: Rect,
    phase: ResizePhase,
    shape: Shape,
}

/// A window lifted out of one workspace, carrying its geometry and state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Window {
    pub(crate) id: WindowId,
    pub(crate) rect: Rect,
    pub(crate) shape: Shape,
}

/// Stacking order, focus and per-window geometry; knows nothing about ids or cascade.
#[derive(Debug, Default)]
pub(crate) struct Workspace {
    /// Bottom to top.
    stack: Vec<WindowId>,
    entries: HashMap<WindowId, Entry>,
    focused: Option<WindowId>,
}

impl Workspace {
    pub(crate) fn len(&self) -> usize {
        self.stack.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Bottom to top, including minimized windows.
    pub(crate) fn stack(&self) -> &[WindowId] {
        &self.stack
    }

    /// Adds a window on top of the stack and focuses it.
    pub(crate) fn open(&mut self, id: WindowId, rect: Rect) {
        self.insert(id, rect, Shape::default());
        self.focused = Some(id);
    }

    /// Adds a lifted window on top; focuses it only when nothing else holds focus.
    pub(crate) fn attach(&mut self, window: Window) {
        self.insert(window.id, window.rect, window.shape);
        if self.focused.is_none() {
            self.focused = Some(window.id);
        }
    }

    /// Lifts a window out of this workspace, keeping its rect and shape.
    pub(crate) fn detach(&mut self, id: WindowId) -> Option<Window> {
        let entry = self.entries.get(&id)?;
        let window = Window {
            id,
            rect: entry.rect,
            shape: entry.shape,
        };
        self.remove(id);
        Some(window)
    }

    fn insert(&mut self, id: WindowId, rect: Rect, shape: Shape) {
        let entry = Entry {
            rect,
            phase: ResizePhase::Idle,
            shape,
        };
        self.entries.insert(id, entry);
        self.stack.push(id);
    }

    /// Removes a window; focus moves to the topmost visible window if it was focused.
    pub(crate) fn remove(&mut self, id: WindowId) -> bool {
        if self.entries.remove(&id).is_none() {
            return false;
        }
        self.stack.retain(|&other| other != id);
        if self.focused == Some(id) {
            self.focused = self.topmost_visible();
        }
        true
    }

    pub(crate) fn raise(&mut self, id: WindowId) -> bool {
        if !self.entries.contains_key(&id) {
            return false;
        }
        self.stack.retain(|&other| other != id);
        self.stack.push(id);
        true
    }

    /// Minimized and unknown windows cannot take focus.
    pub(crate) fn focus(&mut self, id: WindowId) -> bool {
        if !self.is_visible(id) {
            return false;
        }
        self.raise(id);
        self.focused = Some(id);
        true
    }

    pub(crate) fn clear_focus(&mut self) {
        self.focused = None;
    }

    pub(crate) fn focused(&self) -> Option<WindowId> {
        self.focused
    }

    pub(crate) fn rect(&self, id: WindowId) -> Option<Rect> {
        self.entries.get(&id).map(|entry| entry.rect)
    }

    pub(crate) fn set_rect(&mut self, id: WindowId, rect: Rect) -> bool {
        let Some(entry) = self.entries.get_mut(&id) else {
            return false;
        };
        entry.rect = rect;
        true
    }

    pub(crate) fn shape(&self, id: WindowId) -> Option<Shape> {
        self.entries.get(&id).map(|entry| entry.shape)
    }

    pub(crate) fn shape_mut(&mut self, id: WindowId) -> Option<&mut Shape> {
        self.entries.get_mut(&id).map(|entry| &mut entry.shape)
    }

    /// A layout change invalidates any resize in flight.
    pub(crate) fn cancel_resize(&mut self, id: WindowId) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.phase = ResizePhase::Idle;
        }
    }

    pub(crate) fn move_to(&mut self, id: WindowId, loc: Point) -> bool {
        let Some(entry) = self.entries.get_mut(&id) else {
            return false;
        };
        entry.rect.loc = loc;
        true
    }

    /// Returns the rectangle the resize starts from; only normal windows can be resized.
    pub(crate) fn begin_resize(&mut self, id: WindowId, edges: Edges) -> Option<Rect> {
        let entry = self.entries.get_mut(&id)?;
        if entry.shape.state() != WindowState::Normal {
            return None;
        }
        let initial = entry.rect;
        entry.phase = ResizePhase::Active { edges, initial };
        Some(initial)
    }

    pub(crate) fn end_resize(&mut self, id: WindowId) {
        let Some(entry) = self.entries.get_mut(&id) else {
            return;
        };
        if let ResizePhase::Active { edges, initial } = entry.phase {
            entry.phase = ResizePhase::Settling { edges, initial };
        }
    }

    pub(crate) fn on_commit(&mut self, id: WindowId, size: Size) -> bool {
        let Some(entry) = self.entries.get_mut(&id) else {
            return false;
        };
        let before = entry.rect;
        entry.rect.size = size;
        // Maximized and fullscreen windows take the size the client gives them as-is.
        if entry.shape.state() == WindowState::Normal {
            apply_resize_phase(entry, size);
        }
        entry.rect != before
    }

    /// Bottom to top, visible windows only.
    pub(crate) fn placements(&self) -> Vec<(WindowId, Rect)> {
        self.stack
            .iter()
            .filter_map(|&id| {
                let entry = self.entries.get(&id)?;
                (!entry.shape.minimized).then_some((id, entry.rect))
            })
            .collect()
    }

    pub(crate) fn window_at(&self, point: Point) -> Option<WindowId> {
        self.stack
            .iter()
            .rev()
            .copied()
            .find(|&id| self.is_visible(id) && self.rect(id).is_some_and(|r| r.contains(point)))
    }

    /// Bottom to top, minimized windows only.
    pub(crate) fn minimized_ids(&self) -> Vec<WindowId> {
        self.stack
            .iter()
            .copied()
            .filter(|&id| self.shape(id).is_some_and(|shape| shape.minimized))
            .collect()
    }

    pub(crate) fn topmost_visible(&self) -> Option<WindowId> {
        self.stack
            .iter()
            .rev()
            .copied()
            .find(|&id| self.is_visible(id))
    }

    fn is_visible(&self, id: WindowId) -> bool {
        self.shape(id).is_some_and(|shape| !shape.minimized)
    }
}

/// Re-anchors during a resize; a settling window goes idle after its first commit.
fn apply_resize_phase(entry: &mut Entry, size: Size) {
    match entry.phase {
        ResizePhase::Idle => {}
        ResizePhase::Active { edges, initial } => {
            entry.rect.loc = anchored_location(initial, edges, size);
        }
        ResizePhase::Settling { edges, initial } => {
            entry.rect.loc = anchored_location(initial, edges, size);
            entry.phase = ResizePhase::Idle;
        }
    }
}

#[cfg(test)]
mod tests;
