//! All windows' animation state, keyed by window id.

use super::motion::{MinimizeAim, Pose, Presence, WindowMotion};
use super::timing::Timing;
use desilium_wm::WindowId;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Animation state of every window the compositor has drawn.
#[derive(Debug, Default)]
pub struct MotionBook {
    windows: HashMap<WindowId, WindowMotion>,
}

impl MotionBook {
    /// Applies this frame's view of every live window.
    ///
    /// Windows seen for the first time while visible start their open animation; known windows
    /// missing from `seen` are closing.
    pub fn sync(&mut self, timing: &Timing, seen: &[(WindowId, Presence)]) {
        for (id, presence) in seen {
            self.observe_one(timing, *id, *presence);
        }
        let live: HashSet<WindowId> = seen.iter().map(|(id, _)| *id).collect();
        for (id, motion) in &mut self.windows {
            if !live.contains(id) {
                motion.close(timing);
            }
        }
    }

    fn observe_one(&mut self, timing: &Timing, id: WindowId, presence: Presence) {
        if let Some(motion) = self.windows.get_mut(&id) {
            motion.observe(timing, presence);
            return;
        }
        if let Presence::Visible(observation) = presence {
            self.windows
                .insert(id, WindowMotion::appear(timing, &observation));
        }
    }

    /// Drops windows whose close animation has finished.
    pub fn prune(&mut self, now: Duration) {
        self.windows
            .retain(|_, motion| !motion.is_finished_closing(now));
    }

    /// The pose of `id` at `now` when it is minimized toward `aim`; `None` when it is not drawn.
    pub fn pose(&self, id: WindowId, now: Duration, aim: MinimizeAim) -> Option<Pose> {
        self.windows.get(&id)?.pose(now, aim)
    }

    /// Forgets `id` without any animation.
    pub fn forget(&mut self, id: WindowId) {
        self.windows.remove(&id);
    }

    /// Ids of every tracked window.
    pub fn ids(&self) -> Vec<WindowId> {
        self.windows.keys().copied().collect()
    }

    /// True for a window the animation is closing.
    pub fn is_closing(&self, id: WindowId) -> bool {
        self.windows.get(&id).is_some_and(WindowMotion::is_closing)
    }

    /// Whether any window has a running tween at `now`.
    pub fn is_active(&self, now: Duration) -> bool {
        self.windows.values().any(|motion| motion.is_active(now))
    }

    /// Number of tracked windows.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.windows.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::Observation;
    use crate::animation::timing::Pace;
    use crate::decoration::ButtonState;
    use desilium_theme::Motion;
    use desilium_wm::{Rect, Size, WindowManager, WindowState};

    const AIM: MinimizeAim = MinimizeAim::Point((640.0, 800.0));

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn timing_at(millis: u64) -> Timing {
        Timing::new(ms(millis), &Motion::default(), Pace::new(true, 1.0), None)
    }

    fn id(value: u64) -> WindowId {
        let mut wm = WindowManager::new(Rect::new(0, 0, 100, 100));
        let mut last = wm.open_window(Size { w: 10, h: 10 });
        while last.get() < value {
            last = wm.open_window(Size { w: 10, h: 10 });
        }
        last
    }

    fn visible() -> Presence {
        Presence::Visible(Observation {
            rect: Rect::new(10, 10, 300, 200),
            state: WindowState::Normal,
            focused: true,
            buttons: [ButtonState::Rest; 3],
            grabbed: false,
        })
    }

    #[test]
    fn new_visible_windows_open_and_unseen_hidden_ones_are_ignored() {
        let mut book = MotionBook::default();
        book.sync(
            &timing_at(0),
            &[(id(1), visible()), (id(2), Presence::Minimized)],
        );
        assert_eq!(book.len(), 1);
        let start = book.pose(id(1), ms(0), AIM).map(|pose| pose.alpha);
        assert_eq!(start, Some(0.0));
        assert!(book.is_active(ms(10)));
        assert!(!book.is_active(ms(300)));
    }

    #[test]
    fn missing_windows_close_and_are_pruned_after_the_fade() {
        let mut book = MotionBook::default();
        book.sync(&timing_at(0), &[(id(1), visible())]);
        book.sync(&timing_at(1000), &[]);
        assert!(book.is_closing(id(1)));
        assert!(book.is_active(ms(1050)));
        book.prune(ms(1050));
        assert_eq!(book.len(), 1);
        book.prune(ms(1200));
        assert_eq!(book.len(), 0);
    }

    #[test]
    fn closing_instantly_without_animations() {
        let off = Timing::new(ms(1000), &Motion::default(), Pace::new(false, 1.0), None);
        let mut book = MotionBook::default();
        book.sync(&off, &[(id(1), visible())]);
        book.sync(&off, &[]);
        book.prune(ms(1000));
        assert_eq!(book.len(), 0);
        assert!(!book.is_active(ms(1000)));
    }

    #[test]
    fn idle_book_reports_nothing_to_do() {
        let book = MotionBook::default();
        assert!(!book.is_active(ms(0)));
        assert!(book.ids().is_empty());
    }
}
