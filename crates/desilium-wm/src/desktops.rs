use crate::geometry::{Rect, Size};
use crate::ids::WindowId;
use crate::manager::{Placement, WindowManager};
use crate::state::WindowState;
use std::ops::{Deref, DerefMut};

/// Desktop count used when the caller has no preference.
pub const DEFAULT_DESKTOPS: usize = 4;
/// Smallest and largest desktop counts `Desktops` accepts.
pub const MIN_DESKTOPS: usize = 1;
pub const MAX_DESKTOPS: usize = 32;

/// Virtual desktops, each a `WindowManager`, with window ids unique across all of them.
///
/// The first desktop is stored apart from the rest so that the active one is always reachable
/// without an `Option` or an index panic.
pub struct Desktops {
    area: Rect,
    work_area: Rect,
    first: WindowManager,
    rest: Vec<WindowManager>,
    active: usize,
    last_id: u64,
}

impl Desktops {
    /// Creates `count` desktops (clamped to 1..=32) sharing `area`.
    pub fn new(area: Rect, count: usize) -> Self {
        let count = count.clamp(MIN_DESKTOPS, MAX_DESKTOPS);
        Self {
            area,
            work_area: area,
            first: build_manager(area, area),
            rest: (1..count).map(|_| build_manager(area, area)).collect(),
            active: 0,
            last_id: 0,
        }
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    pub fn work_area(&self) -> Rect {
        self.work_area
    }

    /// Applies to every desktop; a work area that followed the area keeps following it.
    pub fn set_area(&mut self, area: Rect) {
        if self.work_area == self.area {
            self.work_area = area;
        }
        self.area = area;
        self.sync_managers();
    }

    pub fn set_work_area(&mut self, work_area: Rect) {
        self.work_area = work_area;
        self.sync_managers();
    }

    pub fn count(&self) -> usize {
        self.rest.len().saturating_add(1)
    }

    pub fn active(&self) -> usize {
        self.active
    }

    /// Returns false if `index` does not name a desktop.
    pub fn switch_to(&mut self, index: usize) -> bool {
        if index >= self.count() {
            return false;
        }
        self.active = index;
        true
    }

    /// Switches to the next desktop, wrapping; returns the new index.
    // The name is part of the API contract; this is not an iterator.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> usize {
        let count = self.count();
        self.active = self.active.saturating_add(1) % count;
        self.active
    }

    /// Switches to the previous desktop, wrapping; returns the new index.
    pub fn prev(&mut self) -> usize {
        let count = self.count();
        self.active = self.active.saturating_add(count).saturating_sub(1) % count;
        self.active
    }

    /// Resizes the desktop list (clamped to 1..=32). Windows on removed desktops move to the
    /// last remaining one, keeping their rects and states.
    pub fn set_count(&mut self, count: usize) {
        let target = count.clamp(MIN_DESKTOPS, MAX_DESKTOPS);
        while self.count() > target {
            self.remove_last();
        }
        while self.count() < target {
            self.rest.push(build_manager(self.area, self.work_area));
        }
        self.active = self.active.min(self.count().saturating_sub(1));
    }

    /// Opens a window on the active desktop and returns its globally unique id.
    pub fn open_window(&mut self, size: Size) -> WindowId {
        let id = self.mint_id();
        self.active_manager_mut().open_window_with_id(size, id);
        id
    }

    pub fn close_window(&mut self, id: WindowId) -> bool {
        self.manager_of_mut(id)
            .is_some_and(|manager| manager.close_window(id))
    }

    /// Moves a window to another desktop, keeping its rect. Fixes focus on both desktops: the
    /// source picks a new focus, the target focuses the window only if it had none.
    pub fn move_window_to(&mut self, id: WindowId, desktop: usize) -> bool {
        let Some(from) = self.desktop_of(id) else {
            return false;
        };
        if desktop >= self.count() {
            return false;
        }
        if from == desktop {
            return true;
        }
        let Some(window) = self
            .manager_at_mut(from)
            .and_then(|manager| manager.detach(id))
        else {
            return false;
        };
        if let Some(target) = self.manager_at_mut(desktop) {
            target.attach(window);
        }
        true
    }

    pub fn desktop_of(&self, id: WindowId) -> Option<usize> {
        self.managers()
            .position(|manager| manager.rect(id).is_some())
    }

    /// Rectangle of a window on any desktop, visible or not.
    pub fn rect(&self, id: WindowId) -> Option<Rect> {
        self.managers().find_map(|manager| manager.rect(id))
    }

    /// Records a client's committed size on whichever desktop holds the window.
    pub fn on_commit(&mut self, id: WindowId, size: Size) -> bool {
        self.manager_of_mut(id)
            .is_some_and(|manager| manager.on_commit(id, size))
    }

    /// Runs `act` on the desktop that holds `id` without switching to it; `None` for unknown ids.
    ///
    /// Client requests and chrome actions use this, so a window on a hidden desktop changes
    /// (and is configured) where it lives instead of being silently ignored.
    pub fn with_window_manager<R>(
        &mut self,
        id: WindowId,
        act: impl FnOnce(&mut WindowManager) -> R,
    ) -> Option<R> {
        self.manager_of_mut(id).map(act)
    }

    /// State of a window on any desktop, visible or not.
    pub fn window_state(&self, id: WindowId) -> Option<WindowState> {
        self.managers().find_map(|manager| manager.window_state(id))
    }

    /// Free-floating rect a non-floating window returns to, on whichever desktop holds it.
    pub fn restore_rect(&self, id: WindowId) -> Option<Rect> {
        self.managers().find_map(|manager| manager.restore_rect(id))
    }

    /// Every window on every desktop with its rect and state, minimized ones included.
    pub fn all_placements(&self) -> Vec<Placement> {
        self.managers()
            .flat_map(|manager| manager.all_placements())
            .collect()
    }

    pub fn active_manager(&self) -> &WindowManager {
        self.manager(self.active).unwrap_or(&self.first)
    }

    pub fn active_manager_mut(&mut self) -> &mut WindowManager {
        match self.active.checked_sub(1) {
            None => &mut self.first,
            Some(slot) => match self.rest.get_mut(slot) {
                Some(manager) => manager,
                None => &mut self.first,
            },
        }
    }

    pub fn manager_of_mut(&mut self, id: WindowId) -> Option<&mut WindowManager> {
        let index = self.desktop_of(id)?;
        self.manager_at_mut(index)
    }

    pub fn manager(&self, index: usize) -> Option<&WindowManager> {
        match index.checked_sub(1) {
            None => Some(&self.first),
            Some(slot) => self.rest.get(slot),
        }
    }

    /// Every window with the index of the desktop it lives on, desktop by desktop.
    pub fn all_windows(&self) -> Vec<(WindowId, usize)> {
        self.managers()
            .enumerate()
            .flat_map(|(index, manager)| {
                manager.window_ids().into_iter().map(move |id| (id, index))
            })
            .collect()
    }

    fn managers(&self) -> impl Iterator<Item = &WindowManager> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }

    fn manager_at_mut(&mut self, index: usize) -> Option<&mut WindowManager> {
        match index.checked_sub(1) {
            None => Some(&mut self.first),
            Some(slot) => self.rest.get_mut(slot),
        }
    }

    fn last_manager_mut(&mut self) -> &mut WindowManager {
        match self.rest.last_mut() {
            Some(manager) => manager,
            None => &mut self.first,
        }
    }

    /// Drops the last desktop (never the only one), handing its windows to the new last one.
    fn remove_last(&mut self) {
        let Some(mut removed) = self.rest.pop() else {
            return;
        };
        let target = self.last_manager_mut();
        for id in removed.window_ids() {
            if let Some(window) = removed.detach(id) {
                target.attach(window);
            }
        }
    }

    fn sync_managers(&mut self) {
        for manager in std::iter::once(&mut self.first).chain(self.rest.iter_mut()) {
            manager.set_area(self.area);
            manager.set_work_area(self.work_area);
        }
    }

    fn mint_id(&mut self) -> WindowId {
        self.last_id = self.last_id.saturating_add(1);
        WindowId::new(self.last_id)
    }
}

/// Window operations not keyed by id act on the active desktop, so existing callers keep
/// working unchanged. Same-named inherent methods (`open_window`, `close_window`, `rect`,
/// `on_commit`, `move_window_to`, `window_state`, `restore_rect`) take precedence and reach
/// every desktop. Id-keyed state changes go through `with_window_manager`.
impl Deref for Desktops {
    type Target = WindowManager;

    fn deref(&self) -> &WindowManager {
        self.active_manager()
    }
}

impl DerefMut for Desktops {
    fn deref_mut(&mut self) -> &mut WindowManager {
        self.active_manager_mut()
    }
}

fn build_manager(area: Rect, work_area: Rect) -> WindowManager {
    let mut manager = WindowManager::new(area);
    manager.set_work_area(work_area);
    manager
}

#[cfg(test)]
mod tests;
