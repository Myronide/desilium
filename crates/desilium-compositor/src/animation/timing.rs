//! Turns theme motion tokens and user settings into concrete tween schedules.

use desilium_anim::{Easing, Tween};
use desilium_theme::{Easing as TokenEasing, Motion, MotionKind};
use std::time::Duration;

/// Fastest speed multiplier honoured; keeps `duration / speed` from collapsing to nothing by accident.
const MAX_SPEED: f32 = 16.0;
/// Control points of the ease-in curve (CSS `ease-in` cubic).
const EASE_IN: (f32, f32, f32, f32) = (0.32, 0.0, 0.67, 0.0);
/// Control points of the symmetric quadratic curve.
const EASE_IN_OUT_QUAD: (f32, f32, f32, f32) = (0.45, 0.0, 0.55, 1.0);

/// The families of animation the compositor runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A window appearing (scale and fade).
    Open,
    /// A window disappearing.
    Close,
    /// Frame rectangle changes: maximize, snap, minimize, restore.
    Geometry,
    /// Active/inactive cross-fade.
    Focus,
    /// Title bar button hover fills.
    Hover,
}

/// User-level switches: `appearance.animations` and `appearance.animation_speed`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pace {
    enabled: bool,
    speed: f32,
}

impl Pace {
    /// Builds the pace from the settings values.
    pub fn new(enabled: bool, speed: f32) -> Pace {
        Pace { enabled, speed }
    }

    /// Scales a base duration; zero when animations are off or the speed is unusable.
    pub fn scale(self, base: Duration) -> Duration {
        if !self.enabled || !self.speed.is_finite() || self.speed <= 0.0 {
            return Duration::ZERO;
        }
        base.div_f32(self.speed.min(MAX_SPEED))
    }
}

/// Renders every animation of the selected kinds exactly `at` after it started, and never advances.
///
/// A development aid for reproducible captures; kinds not selected finish instantly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Freeze {
    pub at: Duration,
    pub kinds: KindSet,
}

/// A set of [`Kind`]s as bit flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindSet(u8);

impl KindSet {
    /// Every kind.
    pub const ALL: KindSet = KindSet(0b1_1111);

    /// The set containing only `kind`.
    pub fn only(kind: Kind) -> KindSet {
        KindSet(1 << kind as u8)
    }

    /// Union of two sets.
    pub fn with(self, other: KindSet) -> KindSet {
        KindSet(self.0 | other.0)
    }

    /// Membership test.
    pub fn contains(self, kind: Kind) -> bool {
        self.0 & (1 << kind as u8) != 0
    }

    /// Parses a kind name as used on the command line.
    pub fn parse_kind(name: &str) -> Option<Kind> {
        match name {
            "open" => Some(Kind::Open),
            "close" => Some(Kind::Close),
            "geometry" => Some(Kind::Geometry),
            "focus" => Some(Kind::Focus),
            "hover" => Some(Kind::Hover),
            _ => None,
        }
    }
}

/// Everything needed to start tweens at one instant.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    now: Duration,
    pace: Pace,
    freeze: Option<Freeze>,
    open: Duration,
    close: Duration,
    fast: Duration,
    geometry: Duration,
    minimize: Duration,
    standard: Easing,
}

impl Timing {
    /// Reads the durations and curves from the theme's motion tokens.
    pub fn new(now: Duration, motion: &Motion, pace: Pace, freeze: Option<Freeze>) -> Timing {
        Timing {
            now,
            pace,
            freeze,
            open: motion.duration(MotionKind::WindowOpen),
            close: motion.duration(MotionKind::WindowClose),
            fast: motion.duration(MotionKind::Fast),
            geometry: motion.duration(MotionKind::Slow),
            minimize: motion.duration(MotionKind::Normal),
            standard: easing_from_token(motion.standard),
        }
    }

    /// The instant tweens started now begin at.
    pub fn now(&self) -> Duration {
        self.now
    }

    /// Curve for things that move and stay.
    pub fn standard(&self) -> Easing {
        self.standard
    }

    /// Base duration (before pace) of `kind`.
    fn base(&self, kind: Kind) -> Duration {
        match kind {
            Kind::Open => self.open,
            Kind::Close => self.close,
            Kind::Geometry => self.geometry,
            Kind::Focus | Kind::Hover => self.fast,
        }
    }

    /// Start time and duration for a tween of `kind` begun now.
    pub fn schedule(&self, kind: Kind) -> (Duration, Duration) {
        self.schedule_with(kind, self.base(kind))
    }

    /// Like [`Timing::schedule`] for minimize and restore, which use their own base duration.
    pub fn schedule_minimize(&self) -> (Duration, Duration) {
        self.schedule_with(Kind::Geometry, self.minimize)
    }

    fn schedule_with(&self, kind: Kind, base: Duration) -> (Duration, Duration) {
        let duration = self.pace.scale(base);
        let Some(freeze) = self.freeze else {
            return (self.now, duration);
        };
        if duration.is_zero() || !freeze.kinds.contains(kind) {
            return (self.now, Duration::ZERO);
        }
        (self.now.saturating_sub(freeze.at), duration)
    }

    /// True when animations are effectively off (disabled, zero speed or reduced motion).
    pub fn is_instant(&self) -> bool {
        self.pace.scale(self.open).is_zero()
    }

    /// A tween that is already at `value` and stays there.
    pub fn settled<T: desilium_anim::Lerp>(&self, value: T) -> Tween<T> {
        Tween::new(value, value, self.now, Duration::ZERO, Easing::Linear)
    }

    /// A new tween of `kind` from `from` to `to`.
    pub fn tween<T: desilium_anim::Lerp>(
        &self,
        kind: Kind,
        from: T,
        to: T,
        easing: Easing,
    ) -> Tween<T> {
        let (start, duration) = self.schedule(kind);
        Tween::new(from, to, start, duration, easing)
    }

    /// Redirects `tween` toward `to` from wherever it currently is, without a jump.
    pub fn retarget<T: desilium_anim::Lerp>(
        &self,
        kind: Kind,
        tween: &Tween<T>,
        to: T,
        easing: Easing,
    ) -> Tween<T> {
        let from = tween.value(self.now);
        self.tween(kind, from, to, easing)
    }
}

/// Maps a theme easing token onto the animation engine's curves.
pub fn easing_from_token(token: TokenEasing) -> Easing {
    match token {
        TokenEasing::Linear => Easing::Linear,
        TokenEasing::EaseOutCubic => Easing::EaseOutCubic,
        TokenEasing::EaseInCubic => bezier(EASE_IN),
        TokenEasing::EaseInOutCubic => Easing::EaseInOutCubic,
        TokenEasing::EaseOutExpo => Easing::EaseOutExpo,
        TokenEasing::EaseInOutQuad => bezier(EASE_IN_OUT_QUAD),
    }
}

fn bezier((x1, y1, x2, y2): (f32, f32, f32, f32)) -> Easing {
    Easing::CubicBezier(x1, y1, x2, y2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn timing(pace: Pace, freeze: Option<Freeze>) -> Timing {
        Timing::new(ms(1000), &Motion::default(), pace, freeze)
    }

    #[test]
    fn default_pace_keeps_theme_durations() {
        let (start, duration) = timing(Pace::new(true, 1.0), None).schedule(Kind::Open);
        assert_eq!((start, duration), (ms(1000), ms(240)));
    }

    #[test]
    fn speed_divides_duration() {
        let (_, duration) = timing(Pace::new(true, 2.0), None).schedule(Kind::Open);
        assert_eq!(duration, ms(120));
    }

    #[test]
    fn disabled_or_unusable_speed_means_instant() {
        for pace in [
            Pace::new(false, 1.0),
            Pace::new(true, 0.0),
            Pace::new(true, -1.0),
            Pace::new(true, f32::NAN),
        ] {
            assert_eq!(
                timing(pace, None).schedule(Kind::Geometry).1,
                Duration::ZERO
            );
        }
    }

    #[test]
    fn reduce_motion_means_instant() {
        let motion = Motion {
            reduce_motion: true,
            ..Motion::default()
        };
        let timing = Timing::new(ms(5), &motion, Pace::new(true, 1.0), None);
        assert_eq!(timing.schedule(Kind::Open).1, Duration::ZERO);
        assert_eq!(timing.schedule_minimize().1, Duration::ZERO);
    }

    #[test]
    fn huge_speed_is_capped() {
        let (_, duration) = timing(Pace::new(true, 1e9), None).schedule(Kind::Open);
        assert_eq!(duration, ms(240).div_f32(MAX_SPEED));
    }

    #[test]
    fn freeze_shifts_selected_kinds_and_settles_the_rest() {
        let freeze = Freeze {
            at: ms(60),
            kinds: KindSet::only(Kind::Open),
        };
        let timing = timing(Pace::new(true, 1.0), Some(freeze));
        assert_eq!(timing.schedule(Kind::Open), (ms(940), ms(240)));
        assert_eq!(timing.schedule(Kind::Focus), (ms(1000), Duration::ZERO));
    }

    #[test]
    fn kind_sets_combine() {
        let set = KindSet::only(Kind::Open).with(KindSet::only(Kind::Focus));
        assert!(set.contains(Kind::Open) && set.contains(Kind::Focus));
        assert!(!set.contains(Kind::Close));
        assert!(KindSet::ALL.contains(Kind::Hover));
        assert_eq!(KindSet::parse_kind("geometry"), Some(Kind::Geometry));
        assert_eq!(KindSet::parse_kind("nope"), None);
    }

    #[test]
    fn retarget_continues_from_the_current_value() {
        let timing = timing(Pace::new(true, 1.0), None);
        let tween = Tween::new(0.0f32, 10.0, ms(900), ms(200), Easing::Linear);
        let next = timing.retarget(Kind::Geometry, &tween, 0.0, Easing::Linear);
        assert!((next.value(ms(1000)) - 5.0).abs() < 1e-4);
    }

    #[test]
    fn token_curves_start_at_zero_and_end_at_one() {
        for token in [
            TokenEasing::Linear,
            TokenEasing::EaseInCubic,
            TokenEasing::EaseInOutQuad,
            TokenEasing::EaseOutExpo,
        ] {
            let curve = easing_from_token(token);
            assert_eq!(curve.apply(0.0), 0.0);
            assert_eq!(curve.apply(1.0), 1.0);
        }
    }
}
