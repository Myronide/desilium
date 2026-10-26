//! Connects the window manager to the pure animation engine in [`crate::animation`].
//!
//! The scene build observes every window once per frame (see [`DesiliumState::window_poses`]);
//! after the frame is presented [`DesiliumState::advance_animations`] decides whether another
//! frame is needed. An idle desktop therefore never redraws.

use super::{ClosingWindow, SceneWindow};
use crate::animation::{
    Clock, Demo, Freeze, MinimizeAim, MotionBook, Observation, Pace, Pose, Presence, Timing,
};
use crate::decoration::{Button, ButtonState};
use crate::state::DesiliumState;
use desilium_wm::{Placement, WindowId, WindowState};
use smithay::desktop::Window;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Constant "now" of a frozen clock; far from zero so shifted tween starts stay representable.
/// Frames a close request waits for its snapshot before it is sent anyway.
const SNAPSHOT_WAIT_FRAMES: u8 = 3;

/// How long a snapshot is kept for a close request the client has not answered.
const CLOSE_GRACE: Duration = Duration::from_secs(5);

const FROZEN_NOW: Duration = Duration::from_secs(3600);

/// Clock and per-window motion state owned by the compositor state.
pub struct MotionRuntime {
    clock: Clock,
    freeze: Option<Freeze>,
    book: RefCell<MotionBook>,
    /// Time of the most recent scene build; the next-frame decision uses it, not a fresh reading.
    sampled: Cell<Duration>,
    /// Scripted action still waiting for the first drawn window (development captures only).
    demo: Cell<Option<Demo>>,
    /// Windows asked to close once their snapshot exists, with the frames waited so far.
    pending_close: RefCell<Vec<(WindowId, u8)>>,
    /// Windows whose close request was sent, with the time it was sent; their snapshot must
    /// survive until the client destroys the window (or the grace period ends).
    close_sent: RefCell<HashMap<WindowId, Duration>>,
    /// Windows that currently have a fresh snapshot on the render side.
    snapshotted: RefCell<HashSet<WindowId>>,
}

impl MotionRuntime {
    /// Real-time runtime measuring from `epoch`.
    pub fn new(epoch: Instant) -> MotionRuntime {
        MotionRuntime {
            clock: Clock::monotonic(epoch),
            freeze: None,
            book: RefCell::new(MotionBook::default()),
            sampled: Cell::new(Duration::ZERO),
            demo: Cell::new(None),
            pending_close: RefCell::new(Vec::new()),
            close_sent: RefCell::new(HashMap::new()),
            snapshotted: RefCell::new(HashSet::new()),
        }
    }
}

impl DesiliumState {
    /// Applies `--freeze-time` (render animations of the selected kinds at a fixed offset after
    /// their start and never advance them) and `--demo` (one scripted window action).
    pub fn configure_animation_dev(&mut self, freeze: Option<Freeze>, demo: Option<Demo>) {
        if let Some(freeze) = freeze {
            self.motion.clock = Clock::Frozen(FROZEN_NOW);
            self.motion.freeze = Some(freeze);
        }
        self.motion.demo.set(demo);
    }

    /// Observes all windows at the current time and returns the pose of every drawn window.
    pub(crate) fn window_poses(&self, placements: &[Placement]) -> HashMap<WindowId, Pose> {
        let now = self.motion.clock.now();
        let timing = self.motion_timing(now);
        let grabbed = self
            .seat
            .get_pointer()
            .is_some_and(|pointer| pointer.is_grabbed());
        let seen: Vec<(WindowId, Presence)> = self
            .windows
            .iter()
            .map(|(id, window)| (*id, self.presence_of(*id, window, placements, grabbed)))
            .collect();
        let mut book = self.motion.book.borrow_mut();
        book.sync(&timing, &seen);
        self.motion.sampled.set(now);
        book.ids()
            .into_iter()
            .filter_map(|id| Some((id, book.pose(id, now, self.minimize_aim(id))?)))
            .collect()
    }

    /// After a presented frame: forgets finished animations and asks for one more frame while
    /// anything still moves. The frame after the last running tween renders its final value.
    pub fn advance_animations(&mut self) {
        self.run_demo();
        self.flush_pending_closes();
        self.expire_close_requests();
        let sampled = self.motion.sampled.get();
        let active = {
            let mut book = self.motion.book.borrow_mut();
            book.prune(sampled);
            book.is_active(sampled)
        };
        if active && !self.motion.clock.is_frozen() {
            self.request_redraw();
        }
    }

    /// Performs the `--demo` action once a window has been drawn.
    fn run_demo(&mut self) {
        let Some(demo) = self.motion.demo.get() else {
            return;
        };
        let Some(id) = self.wm.focused().filter(|id| self.is_drawn(*id)) else {
            return;
        };
        self.motion.demo.set(None);
        match demo {
            Demo::Maximize => self.toggle_maximize_window(id),
            Demo::Close => self.close_focused_window(),
        }
    }

    fn is_drawn(&self, id: WindowId) -> bool {
        let book = self.motion.book.borrow();
        book.pose(
            id,
            self.motion.sampled.get(),
            MinimizeAim::Point((0.0, 0.0)),
        )
        .is_some()
    }

    /// Starts the close animation of a destroyed window, or forgets it when there is no
    /// snapshot to fade (or animations are off).
    pub(crate) fn begin_close_motion(&self, id: WindowId) {
        self.motion.close_sent.borrow_mut().remove(&id);
        let snapshotted = self.motion.snapshotted.borrow_mut().remove(&id);
        let animate = !self.motion_timing(self.motion.clock.now()).is_instant();
        if snapshotted && animate {
            return;
        }
        self.motion.book.borrow_mut().forget(id);
    }

    /// Asks a client to close. With animations on, the request waits until the render side has
    /// snapshotted the window, so the close can fade out its last frame.
    pub(crate) fn request_close_animated(&self, id: WindowId) {
        let animate = !self.motion_timing(self.motion.clock.now()).is_instant();
        if !animate || self.motion.snapshotted.borrow().contains(&id) {
            self.send_close(id);
            return;
        }
        self.motion.pending_close.borrow_mut().push((id, 0));
        self.request_redraw();
    }

    fn flush_pending_closes(&mut self) {
        let waiting = std::mem::take(&mut *self.motion.pending_close.borrow_mut());
        let mut still_waiting = Vec::new();
        for (id, frames) in waiting {
            let ready = self.motion.snapshotted.borrow().contains(&id);
            if ready || frames >= SNAPSHOT_WAIT_FRAMES {
                self.send_close(id);
                continue;
            }
            still_waiting.push((id, frames + 1));
        }
        if !still_waiting.is_empty() {
            self.request_redraw();
        }
        self.motion.pending_close.borrow_mut().extend(still_waiting);
    }

    /// Forgets close requests the client never acted on, so their snapshots can go.
    fn expire_close_requests(&self) {
        let now = self.motion.clock.now();
        self.motion
            .close_sent
            .borrow_mut()
            .retain(|_, sent| now.saturating_sub(*sent) < CLOSE_GRACE);
    }

    fn send_close(&self, id: WindowId) {
        let now = self.motion.clock.now();
        self.motion.close_sent.borrow_mut().insert(id, now);
        let toplevel = self.windows.get(&id).and_then(|window| window.toplevel());
        if let Some(toplevel) = toplevel {
            toplevel.send_close();
        }
    }

    /// Whether the renderer should keep a fresh snapshot of `window`: its close button is
    /// under the pointer or a close is waiting for one.
    pub(crate) fn snapshot_wanted(&self, window: &SceneWindow) -> bool {
        if self.motion_timing(self.motion.clock.now()).is_instant() {
            return false;
        }
        let hovering = window.buttons[Button::Close.index()] != ButtonState::Rest;
        hovering || self.close_requested(window.id)
    }

    /// A close for `id` is waiting for its snapshot or was sent and not yet answered. The
    /// window may already be unmapped by then, so this must not depend on drawing it.
    pub(crate) fn close_requested(&self, id: WindowId) -> bool {
        self.motion.close_sent.borrow().contains_key(&id)
            || self
                .motion
                .pending_close
                .borrow()
                .iter()
                .any(|(pending, _)| *pending == id)
    }

    /// Records whether the renderer holds a snapshot of `id`.
    pub(crate) fn note_snapshot(&self, id: WindowId, taken: bool) {
        let mut snapshotted = self.motion.snapshotted.borrow_mut();
        if taken {
            snapshotted.insert(id);
            return;
        }
        snapshotted.remove(&id);
    }

    /// Destroyed windows still fading out, after the frame's `scene_windows` call.
    pub fn closing_windows(&self) -> Vec<ClosingWindow> {
        let now = self.motion.sampled.get();
        let book = self.motion.book.borrow();
        let mut ghosts: Vec<ClosingWindow> = book
            .ids()
            .into_iter()
            .filter(|id| book.is_closing(*id))
            .filter_map(|id| {
                let pose = book.pose(id, now, MinimizeAim::Point((0.0, 0.0)))?;
                Some(ClosingWindow {
                    id,
                    layout: pose.layout.to_rect(),
                    focus: pose.focus,
                    xform: pose.xform(),
                })
            })
            .collect();
        ghosts.sort_by_key(|ghost| ghost.id);
        ghosts
    }

    fn motion_timing(&self, now: Duration) -> Timing {
        let appearance = self.appearance();
        let pace = Pace::new(appearance.animations, appearance.animation_speed);
        Timing::new(now, &self.theme.motion, pace, self.motion.freeze)
    }

    fn presence_of(
        &self,
        id: WindowId,
        window: &Window,
        placements: &[Placement],
        grabbed: bool,
    ) -> Presence {
        let placement = placements.iter().find(|placement| placement.id == id);
        let Some(placement) = placement else {
            return match self.wm.window_state(id) {
                Some(WindowState::Minimized) => Presence::Minimized,
                _ => Presence::Elsewhere,
            };
        };
        // Until the client has committed a buffer there is nothing worth showing yet.
        let size = window.geometry().size;
        if size.w <= 0 || size.h <= 0 {
            return Presence::Elsewhere;
        }
        let interaction = self.decoration.interaction();
        Presence::Visible(Observation {
            rect: placement.rect,
            state: placement.state,
            focused: self.wm.focused() == Some(id),
            buttons: Button::ALL.map(|button| interaction.button_state(id, button)),
            grabbed,
        })
    }

    /// Where window `id` goes when minimized: its dock tile, or the bottom centre of the output.
    fn minimize_aim(&self, id: WindowId) -> MinimizeAim {
        MinimizeAim::for_tile(self.minimize_target(id), self.wm.area())
    }
}
