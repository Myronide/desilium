//! Per-window animation state machine: open, geometry, minimize, focus and hover.
//!
//! The machine is pure. It is told what the window manager says about a window each frame
//! ([`Presence`]) and answers with the pose to draw ([`Pose`]). Every change retargets running
//! tweens from their current value, so interrupting an animation never makes anything jump.

use super::timing::{Kind, Timing};
use super::xform::{RectF, Xform};
use crate::decoration::ButtonState;
use desilium_anim::{Easing, Lerp, Tween, close_scale_alpha, open_scale_alpha};
use desilium_wm::{Rect, WindowState};
use std::time::Duration;

/// Size of a minimized window relative to its normal size, at the end of the minimize motion.
const MINIMIZED_SCALE: f32 = 0.1;
/// Smallest size a window shrinks to when it flies into a dock tile.
const MIN_TILE_SCALE: f32 = 0.02;
const SHOWN: f32 = 1.0;
const HIDDEN: f32 = 0.0;
const FOCUSED: f32 = 1.0;
const UNFOCUSED: f32 = 0.0;

/// What the window manager reports about a visible window this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observation {
    pub rect: Rect,
    pub state: WindowState,
    pub focused: bool,
    pub buttons: [ButtonState; 3],
    /// An interactive move or resize is in progress: follow the pointer 1:1.
    pub grabbed: bool,
}

/// Where a known window currently is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Presence {
    Visible(Observation),
    Minimized,
    /// Alive but not on this desktop.
    Elsewhere,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Visible,
    Minimized,
    Elsewhere,
    Closing,
}

/// Fill state of one title bar button while its hover background fades.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonFade {
    /// The hover or pressed look being faded in or out.
    pub state: ButtonState,
    /// Opacity of that look, `0..=1`.
    pub level: f32,
}

/// Everything the renderer needs to draw one window at one instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// Final layout of the window frame.
    pub layout: RectF,
    /// Where that frame is drawn now.
    pub displayed: RectF,
    /// Extra scale around the displayed centre (open and close).
    pub scale: f32,
    pub alpha: f32,
    /// `0` inactive look, `1` active look.
    pub focus: f32,
    pub buttons: [ButtonFade; 3],
}

impl Pose {
    /// The transform from the final layout to the screen.
    pub fn xform(&self) -> Xform {
        Xform::mapping(self.layout, self.displayed)
            .then(Xform::scale_about(self.displayed.centre(), self.scale))
            .with_alpha(self.alpha)
    }
}

/// Where minimized windows go and how small they get.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MinimizeAim {
    /// Shrink toward a point at the default scale (no dock tile is known).
    Point((f32, f32)),
    /// Fly into the dock tile reported by the shell, shrunk until it fits.
    Tile(Rect),
}

impl MinimizeAim {
    /// The aim for a window: its dock tile when the shell reported one, else the bottom centre
    /// of the work `area`, where minimized windows used to go.
    pub fn for_tile(tile: Option<Rect>, area: Rect) -> MinimizeAim {
        match tile {
            Some(tile) => MinimizeAim::Tile(tile),
            None => {
                let x = area.loc.x as f32 + area.size.w as f32 / 2.0;
                let y = area.loc.y as f32 + area.size.h as f32;
                MinimizeAim::Point((x, y))
            }
        }
    }

    /// The frame a fully minimized window of layout `layout` ends up in.
    fn goal(self, layout: Rect) -> RectF {
        let (centre, scale) = match self {
            MinimizeAim::Point(point) => (point, MINIMIZED_SCALE),
            MinimizeAim::Tile(tile) => (RectF::from_rect(tile).centre(), tile_scale(tile, layout)),
        };
        let width = layout.size.w as f32 * scale;
        let height = layout.size.h as f32 * scale;
        RectF {
            x: centre.0 - width / 2.0,
            y: centre.1 - height / 2.0,
            w: width,
            h: height,
        }
    }
}

/// Scale that makes a window of `layout` fit into `tile`, kept between the minimum and the
/// default minimized scale.
fn tile_scale(tile: Rect, layout: Rect) -> f32 {
    fit_ratio(tile.size.w, layout.size.w)
        .min(fit_ratio(tile.size.h, layout.size.h))
        .clamp(MIN_TILE_SCALE, MINIMIZED_SCALE)
}

/// How many times `available` fits into `needed`; a window without size never constrains it.
fn fit_ratio(available: i32, needed: i32) -> f32 {
    if needed <= 0 {
        return f32::INFINITY;
    }
    available as f32 / needed as f32
}

/// Animation state of one window.
#[derive(Debug, Clone)]
pub struct WindowMotion {
    phase: Phase,
    layout: Rect,
    state: WindowState,
    rect: Tween<[f32; 4]>,
    /// Where the running `rect` tween started from; lets a changed target keep its timing.
    rect_from: [f32; 4],
    open: Tween<f32>,
    shown: Tween<f32>,
    focus: Tween<f32>,
    hover: [Tween<f32>; 3],
    hover_state: [ButtonState; 3],
    closing: Tween<f32>,
}

impl WindowMotion {
    /// Starts the open animation for a window that just became drawable.
    pub fn appear(timing: &Timing, observation: &Observation) -> WindowMotion {
        let rect = RectF::from_rect(observation.rect).to_array();
        let focus = focus_value(observation.focused);
        WindowMotion {
            phase: Phase::Visible,
            layout: observation.rect,
            state: observation.state,
            rect: timing.settled(rect),
            rect_from: rect,
            open: timing.tween(Kind::Open, 0.0, 1.0, Easing::Linear),
            shown: timing.settled(SHOWN),
            focus: timing.settled(focus),
            hover: [timing.settled(0.0); 3],
            hover_state: [ButtonState::Rest; 3],
            closing: timing.settled(0.0),
        }
    }

    /// Feeds the window manager's current view of the window.
    pub fn observe(&mut self, timing: &Timing, presence: Presence) {
        // A closing window is already gone from the client's point of view.
        if self.phase == Phase::Closing {
            return;
        }
        match presence {
            Presence::Visible(observation) => self.observe_visible(timing, &observation),
            Presence::Minimized => self.observe_minimized(timing),
            Presence::Elsewhere => self.observe_elsewhere(timing),
        }
    }

    /// Starts the close fade; the window keeps its last layout.
    pub fn close(&mut self, timing: &Timing) {
        if self.phase == Phase::Closing {
            return;
        }
        self.phase = Phase::Closing;
        // Leaving should feel brisk: the curve front-loads the change, unlike the open curve it mirrors.
        self.closing = timing.tween(Kind::Close, 0.0, 1.0, Easing::EaseOutCubic);
    }

    /// True once a closing window has faded out completely.
    pub fn is_finished_closing(&self, now: Duration) -> bool {
        self.phase == Phase::Closing && self.closing.is_finished(now)
    }

    /// True while the window is being closed by the animation.
    pub fn is_closing(&self) -> bool {
        self.phase == Phase::Closing
    }

    /// Whether any tween is changing a value at `now`.
    pub fn is_active(&self, now: Duration) -> bool {
        self.rect.is_running(now)
            || self.open.is_running(now)
            || self.shown.is_running(now)
            || self.focus.is_running(now)
            || self.closing.is_running(now)
            || self.hover.iter().any(|tween| tween.is_running(now))
    }

    /// The pose at `now`, or `None` when nothing of the window is visible.
    ///
    /// `aim` is where the window goes when it is minimized; it may change between frames.
    pub fn pose(&self, now: Duration, aim: MinimizeAim) -> Option<Pose> {
        let shown = self.shown.value(now);
        let hidden = match self.phase {
            Phase::Elsewhere => true,
            Phase::Minimized => shown <= 0.0,
            Phase::Visible | Phase::Closing => false,
        };
        if hidden {
            return None;
        }
        let (open_scale, open_alpha) = open_scale_alpha(self.open.value(now));
        let (close_scale, close_alpha) = close_scale_alpha(self.closing.value(now));
        let framed = RectF::from_array(self.rect.value(now));
        let displayed = toward_aim(framed, self.layout, aim, 1.0 - shown);
        Some(Pose {
            layout: RectF::from_rect(self.layout),
            displayed,
            scale: open_scale * close_scale,
            alpha: open_alpha * close_alpha * shown,
            focus: self.focus.value(now),
            buttons: self.button_fades(now),
        })
    }

    fn button_fades(&self, now: Duration) -> [ButtonFade; 3] {
        let mut fades = [ButtonFade {
            state: ButtonState::Rest,
            level: 0.0,
        }; 3];
        for (fade, (tween, state)) in fades
            .iter_mut()
            .zip(self.hover.iter().zip(self.hover_state))
        {
            fade.level = tween.value(now);
            fade.state = state;
        }
        fades
    }

    fn observe_visible(&mut self, timing: &Timing, observation: &Observation) {
        let previous = self.phase;
        self.phase = Phase::Visible;
        let target = RectF::from_rect(observation.rect).to_array();
        match previous {
            Phase::Visible => self.follow_geometry(timing, observation, target),
            Phase::Minimized => {
                self.rect = timing.settled(target);
                self.shown = minimize_tween(timing, self.shown.value(timing.now()), SHOWN);
            }
            Phase::Elsewhere | Phase::Closing => {
                self.rect = timing.settled(target);
                self.shown = timing.settled(SHOWN);
            }
        }
        self.layout = observation.rect;
        self.state = observation.state;
        self.follow_focus(timing, observation.focused);
        self.follow_hover(timing, &observation.buttons);
    }

    fn follow_geometry(&mut self, timing: &Timing, observation: &Observation, target: [f32; 4]) {
        let now = timing.now();
        let restyled = self.state != observation.state || self.layout.loc != observation.rect.loc;
        let animate = !observation.grabbed && !self.open.is_running(now);
        if animate && restyled {
            let from = self.rect.value(now);
            self.start_rect(timing, from, target);
            return;
        }
        if animate && self.rect.is_running(now) {
            // The client settled on a slightly different size: aim at it without restarting the
            // curve, so the motion stays continuous.
            if self.rect.target() != target {
                let span = self.rect.end_time().saturating_sub(self.rect.start_time());
                self.rect = Tween::new(
                    self.rect_from,
                    target,
                    self.rect.start_time(),
                    span,
                    timing.standard(),
                );
            }
            return;
        }
        if self.rect.target() != target || observation.grabbed {
            self.rect = timing.settled(target);
        }
    }

    fn start_rect(&mut self, timing: &Timing, from: [f32; 4], target: [f32; 4]) {
        self.rect_from = from;
        self.rect = timing.tween(Kind::Geometry, from, target, timing.standard());
    }

    fn follow_focus(&mut self, timing: &Timing, focused: bool) {
        let wanted = focus_value(focused);
        if self.focus.target() == wanted {
            return;
        }
        self.focus = timing.retarget(Kind::Focus, &self.focus, wanted, Easing::EaseOutCubic);
    }

    fn follow_hover(&mut self, timing: &Timing, buttons: &[ButtonState; 3]) {
        for (index, state) in buttons.iter().enumerate() {
            let wanted = if *state == ButtonState::Rest {
                0.0
            } else {
                1.0
            };
            if *state != ButtonState::Rest {
                self.hover_state[index] = *state;
            }
            if self.hover[index].target() == wanted {
                continue;
            }
            self.hover[index] =
                timing.retarget(Kind::Hover, &self.hover[index], wanted, Easing::Linear);
        }
    }

    fn observe_minimized(&mut self, timing: &Timing) {
        match self.phase {
            Phase::Visible => {
                self.shown = minimize_tween(timing, self.shown.value(timing.now()), HIDDEN);
            }
            Phase::Elsewhere => self.shown = timing.settled(HIDDEN),
            Phase::Minimized | Phase::Closing => {}
        }
        self.phase = Phase::Minimized;
    }

    fn observe_elsewhere(&mut self, timing: &Timing) {
        self.phase = Phase::Elsewhere;
        self.shown = timing.settled(SHOWN);
    }
}

fn focus_value(focused: bool) -> f32 {
    if focused { FOCUSED } else { UNFOCUSED }
}

fn minimize_tween(timing: &Timing, from: f32, to: f32) -> Tween<f32> {
    let (start, duration) = timing.schedule_minimize();
    Tween::new(from, to, start, duration, timing.standard())
}

/// Moves `rect` toward the minimized frame of `aim`; `amount` of 0 leaves it, 1 is the minimized pose.
fn toward_aim(rect: RectF, layout: Rect, aim: MinimizeAim, amount: f32) -> RectF {
    if amount <= 0.0 {
        return rect;
    }
    let goal = aim.goal(layout);
    RectF::from_array(rect.to_array().lerp(goal.to_array(), amount))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::timing::{Freeze, KindSet, Pace};
    use desilium_theme::Motion;

    const ANCHOR: (f32, f32) = (640.0, 800.0);
    const AIM: MinimizeAim = MinimizeAim::Point(ANCHOR);

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn timing_at(millis: u64) -> Timing {
        Timing::new(ms(millis), &Motion::default(), Pace::new(true, 1.0), None)
    }

    fn observation(rect: Rect, state: WindowState) -> Observation {
        Observation {
            rect,
            state,
            focused: true,
            buttons: [ButtonState::Rest; 3],
            grabbed: false,
        }
    }

    fn normal() -> Observation {
        observation(Rect::new(100, 100, 400, 300), WindowState::Normal)
    }

    fn normal_centre() -> (f32, f32) {
        RectF::from_rect(normal().rect).centre()
    }

    fn pose(motion: &WindowMotion, millis: u64) -> Pose {
        motion.pose(ms(millis), AIM).expect("window is drawn")
    }

    #[test]
    fn open_starts_small_and_transparent_and_ends_whole() {
        let motion = WindowMotion::appear(&timing_at(1000), &normal());
        let start = pose(&motion, 1000);
        assert_eq!(start.alpha, 0.0);
        assert!((start.scale - 0.94).abs() < 1e-5);
        let end = pose(&motion, 1240);
        assert_eq!((end.alpha, end.scale), (1.0, 1.0));
        assert!(end.xform().is_identity());
        assert!(!motion.is_active(ms(1240)));
        assert!(motion.is_active(ms(1100)));
    }

    #[test]
    fn open_progress_follows_ease_out_expo() {
        let motion = WindowMotion::appear(&timing_at(0), &normal());
        let quarter = pose(&motion, 60);
        assert!(quarter.alpha > 0.6, "expo front-loads the fade");
        assert!(quarter.alpha < 1.0);
    }

    #[test]
    fn instant_when_animations_are_off() {
        let timing = Timing::new(ms(5), &Motion::default(), Pace::new(false, 1.0), None);
        let motion = WindowMotion::appear(&timing, &normal());
        assert!(pose(&motion, 5).xform().is_identity());
        assert!(!motion.is_active(ms(5)));
    }

    #[test]
    fn maximize_interpolates_the_frame_and_retargets_smoothly() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        let full = observation(Rect::new(0, 0, 1280, 760), WindowState::Maximized);
        motion.observe(&timing_at(1000), Presence::Visible(full));
        let halfway = pose(&motion, 1160);
        assert!(halfway.displayed.w > 400.0 && halfway.displayed.w < 1280.0);
        assert_eq!(halfway.layout, RectF::from_rect(full.rect));

        // Interrupt: unmaximize mid-flight; the displayed frame must not jump.
        let before = pose(&motion, 1160).displayed;
        motion.observe(&timing_at(1160), Presence::Visible(normal()));
        let after = pose(&motion, 1160).displayed;
        assert_eq!(before, after);
        let settled = pose(&motion, 1160 + 400).displayed;
        assert_eq!(settled, RectF::from_rect(normal().rect));
    }

    #[test]
    fn a_size_adjustment_mid_flight_keeps_the_curve_running() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        let full = observation(Rect::new(0, 0, 1280, 760), WindowState::Maximized);
        motion.observe(&timing_at(1000), Presence::Visible(full));
        let mut adjusted = full;
        adjusted.rect = Rect::new(0, 0, 1274, 752);
        // Re-observing every frame must not stretch the animation past its original end.
        for frame in 1..=10 {
            motion.observe(&timing_at(1000 + frame * 16), Presence::Visible(adjusted));
        }
        assert_eq!(
            pose(&motion, 1320).displayed,
            RectF::from_rect(adjusted.rect)
        );
        assert!(!motion.is_active(ms(1320)));
    }

    #[test]
    fn grabbed_windows_follow_the_pointer_without_easing() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        let mut moved = observation(Rect::new(180, 140, 400, 300), WindowState::Normal);
        moved.grabbed = true;
        motion.observe(&timing_at(1000), Presence::Visible(moved));
        assert_eq!(pose(&motion, 1000).displayed, RectF::from_rect(moved.rect));
        assert!(!motion.is_active(ms(1000)));
    }

    #[test]
    fn client_size_commits_do_not_animate() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        let resized = observation(Rect::new(100, 100, 420, 310), WindowState::Normal);
        motion.observe(&timing_at(1000), Presence::Visible(resized));
        assert_eq!(
            pose(&motion, 1000).displayed,
            RectF::from_rect(resized.rect)
        );
        assert!(!motion.is_active(ms(1000)));
    }

    #[test]
    fn moves_during_the_open_animation_snap() {
        let mut motion = WindowMotion::appear(&timing_at(1000), &normal());
        let shifted = observation(Rect::new(120, 100, 400, 300), WindowState::Normal);
        motion.observe(&timing_at(1050), Presence::Visible(shifted));
        assert_eq!(
            pose(&motion, 1050).displayed,
            RectF::from_rect(shifted.rect)
        );
    }

    #[test]
    fn minimize_shrinks_toward_the_anchor_and_hides() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        motion.observe(&timing_at(1000), Presence::Minimized);
        let mid = pose(&motion, 1100);
        assert!(mid.alpha > 0.0 && mid.alpha < 1.0);
        assert!(mid.displayed.w < 400.0);
        assert!(motion.pose(ms(1200), AIM).is_none());
        assert!(!motion.is_active(ms(1200)));
    }

    #[test]
    fn without_a_dock_tile_the_aim_is_the_bottom_centre() {
        let area = Rect::new(0, 0, 1280, 800);
        assert_eq!(
            MinimizeAim::for_tile(None, area),
            MinimizeAim::Point((640.0, 800.0))
        );
        let offset = Rect::new(100, 50, 800, 600);
        assert_eq!(
            MinimizeAim::for_tile(None, offset),
            MinimizeAim::Point((500.0, 650.0))
        );
    }

    #[test]
    fn a_dock_tile_becomes_the_aim() {
        let tile = Rect::new(900, 760, 48, 40);
        let area = Rect::new(0, 0, 1280, 800);
        assert_eq!(
            MinimizeAim::for_tile(Some(tile), area),
            MinimizeAim::Tile(tile)
        );
    }

    #[test]
    fn a_minimized_window_ends_centred_on_the_dock_tile() {
        let tile = Rect::new(900, 760, 48, 40);
        let layout = Rect::new(100, 100, 400, 300);
        let goal = MinimizeAim::Tile(tile).goal(layout);
        assert_eq!(goal.centre(), RectF::from_rect(tile).centre());
    }

    #[test]
    fn the_tile_shrink_factor_fits_the_window_into_the_tile() {
        let tile = Rect::new(0, 0, 40, 40);
        let large = Rect::new(0, 0, 800, 600);
        assert!((tile_scale(tile, large) - 0.05).abs() < 1e-6);
        let small = Rect::new(0, 0, 200, 100);
        assert_eq!(tile_scale(tile, small), MINIMIZED_SCALE);
    }

    #[test]
    fn the_tile_shrink_factor_is_bounded_and_survives_empty_windows() {
        let tile = Rect::new(0, 0, 10, 10);
        let huge = Rect::new(0, 0, 10_000, 10_000);
        assert_eq!(tile_scale(tile, huge), MIN_TILE_SCALE);
        let empty = Rect::new(0, 0, 0, 0);
        assert_eq!(tile_scale(tile, empty), MINIMIZED_SCALE);
        let broken_tile = Rect::new(0, 0, -5, 40);
        assert_eq!(tile_scale(broken_tile, huge), MIN_TILE_SCALE);
    }

    #[test]
    fn minimize_flies_into_the_dock_tile() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        motion.observe(&timing_at(1000), Presence::Minimized);
        let tile = Rect::new(900, 760, 48, 40);
        let aim = MinimizeAim::Tile(tile);
        let mid = motion.pose(ms(1100), aim).expect("still visible");
        let centre = mid.displayed.centre();
        let tile_centre = RectF::from_rect(tile).centre();
        let start_centre = normal_centre();
        // Between the window and the tile, closer to the tile than the window was.
        assert!(centre.0 > start_centre.0 && centre.0 < tile_centre.0);
        assert!(mid.displayed.w < 400.0);
    }

    #[test]
    fn a_tile_that_moves_mid_flight_retargets_without_a_jump() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        motion.observe(&timing_at(1000), Presence::Minimized);
        let first = MinimizeAim::Tile(Rect::new(900, 760, 48, 40));
        let moved = MinimizeAim::Tile(Rect::new(20, 760, 48, 40));
        let before = motion.pose(ms(1100), first).expect("visible");
        let after = motion.pose(ms(1100), moved).expect("visible");
        // Same instant, same progress: only the destination changed, so the alpha is unchanged.
        assert_eq!(before.alpha, after.alpha);
        assert_ne!(before.displayed, after.displayed);
    }

    #[test]
    fn the_aim_does_not_change_the_motion_before_it_starts() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        motion.observe(&timing_at(1000), Presence::Minimized);
        let tile = MinimizeAim::Tile(Rect::new(900, 760, 48, 40));
        let early = motion.pose(ms(1000), tile).expect("visible");
        assert_eq!(early.displayed, RectF::from_rect(normal().rect));
    }

    #[test]
    fn restore_reverses_minimize_from_the_current_value() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        motion.observe(&timing_at(1000), Presence::Minimized);
        let mid = pose(&motion, 1100).alpha;
        motion.observe(&timing_at(1100), Presence::Visible(normal()));
        assert_eq!(pose(&motion, 1100).alpha, mid);
        assert_eq!(pose(&motion, 1500).alpha, 1.0);
    }

    #[test]
    fn restoring_a_long_minimized_window_starts_from_nothing() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        motion.observe(&timing_at(1000), Presence::Minimized);
        motion.observe(&timing_at(5000), Presence::Visible(normal()));
        assert_eq!(pose(&motion, 5000).alpha, 0.0);
        assert_eq!(pose(&motion, 5400).alpha, 1.0);
    }

    #[test]
    fn other_desktops_neither_draw_nor_animate_on_return() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        motion.observe(&timing_at(1000), Presence::Elsewhere);
        assert!(motion.pose(ms(1000), AIM).is_none());
        motion.observe(&timing_at(2000), Presence::Visible(normal()));
        assert!(pose(&motion, 2000).xform().is_identity());
    }

    #[test]
    fn focus_cross_fades_over_the_fast_duration() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        let mut blurred = normal();
        blurred.focused = false;
        motion.observe(&timing_at(1000), Presence::Visible(blurred));
        assert_eq!(pose(&motion, 1000).focus, 1.0);
        let mid = pose(&motion, 1060).focus;
        assert!(mid > 0.0 && mid < 1.0);
        assert_eq!(pose(&motion, 1120).focus, 0.0);
    }

    #[test]
    fn hover_fades_in_and_remembers_the_look_while_fading_out() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        let mut hovered = normal();
        hovered.buttons[2] = ButtonState::Hover;
        motion.observe(&timing_at(1000), Presence::Visible(hovered));
        let mid = pose(&motion, 1060).buttons[2];
        assert_eq!(mid.state, ButtonState::Hover);
        assert!(mid.level > 0.0 && mid.level < 1.0);

        motion.observe(&timing_at(1200), Presence::Visible(normal()));
        let fading = pose(&motion, 1260).buttons[2];
        assert_eq!(fading.state, ButtonState::Hover);
        assert!(fading.level > 0.0 && fading.level < 1.0);
        assert_eq!(pose(&motion, 1400).buttons[2].level, 0.0);
    }

    #[test]
    fn close_fades_and_finishes() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        motion.close(&timing_at(1000));
        assert!(motion.is_closing());
        let mid = pose(&motion, 1080);
        assert!(mid.alpha > 0.0 && mid.alpha < 1.0);
        assert!(motion.is_finished_closing(ms(1160)));
        assert!(!motion.is_finished_closing(ms(1100)));
    }

    #[test]
    fn closing_windows_ignore_later_observations_of_other_kinds() {
        let mut motion = WindowMotion::appear(&timing_at(0), &normal());
        motion.close(&timing_at(1000));
        motion.observe(&timing_at(1010), Presence::Minimized);
        motion.observe(&timing_at(1010), Presence::Elsewhere);
        assert!(motion.is_closing());
    }

    #[test]
    fn frozen_capture_shows_the_requested_instant() {
        let freeze = Freeze {
            at: ms(120),
            kinds: KindSet::ALL,
        };
        let timing = Timing::new(
            ms(5000),
            &Motion::default(),
            Pace::new(true, 1.0),
            Some(freeze),
        );
        let motion = WindowMotion::appear(&timing, &normal());
        let frozen = pose(&motion, 5000);
        let reference = pose(&WindowMotion::appear(&timing_at(0), &normal()), 120);
        assert_eq!(frozen.alpha, reference.alpha);
        assert_eq!(frozen.scale, reference.scale);
    }
}
