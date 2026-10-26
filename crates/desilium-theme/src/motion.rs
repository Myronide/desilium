//! Motion tokens: durations, easing curves and spring parameters.

use std::time::Duration;

use serde::{Deserialize, Serialize};

const EXPO_RATE: f32 = 10.0;
const CUBIC_MID: f32 = 0.5;

/// Named easing curve; maps progress `0..=1` to eased progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Easing {
    /// Constant speed.
    Linear,
    /// Fast start, gentle stop.
    EaseOutCubic,
    /// Gentle start, fast end.
    EaseInCubic,
    /// Gentle start and stop.
    EaseInOutCubic,
    /// Very fast start with a long, soft landing: the Desilium signature.
    EaseOutExpo,
    /// Light-weight symmetric curve for small movements.
    EaseInOutQuad,
}

impl Easing {
    /// Evaluates the curve; `t` is clamped to `0..=1`.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInCubic => t.powi(3),
            Easing::EaseInOutCubic if t < CUBIC_MID => 4.0 * t.powi(3),
            Easing::EaseInOutCubic => 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0,
            Easing::EaseOutExpo if t >= 1.0 => 1.0,
            Easing::EaseOutExpo => 1.0 - 2f32.powf(-EXPO_RATE * t),
            Easing::EaseInOutQuad if t < CUBIC_MID => 2.0 * t * t,
            Easing::EaseInOutQuad => 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0,
        }
    }
}

/// Damped spring parameters for interruptible motion (drags, snaps).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spring {
    /// Spring constant `k`.
    pub stiffness: f32,
    /// Damping coefficient `c`.
    pub damping: f32,
    /// Mass `m`.
    pub mass: f32,
}

impl Spring {
    /// Damping ratio; below 1 the spring overshoots slightly before settling.
    pub fn damping_ratio(&self) -> f32 {
        self.damping / (2.0 * (self.stiffness * self.mass).sqrt())
    }
}

impl Default for Spring {
    fn default() -> Self {
        Spring {
            stiffness: 320.0,
            damping: 28.0,
            mass: 1.0,
        }
    }
}

/// The animated events a theme assigns a duration to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionKind {
    /// Hover and press feedback.
    Fast,
    /// Default transitions.
    Normal,
    /// Large or emphasised transitions.
    Slow,
    /// A window appearing.
    WindowOpen,
    /// A window disappearing.
    WindowClose,
    /// Sliding between workspaces.
    WorkspaceSwitch,
}

/// Motion section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Motion {
    /// Hover and press feedback, in ms.
    pub fast_ms: u32,
    /// Default transition, in ms.
    pub normal_ms: u32,
    /// Emphasised transition, in ms.
    pub slow_ms: u32,
    /// Window open, in ms.
    pub window_open_ms: u32,
    /// Window close, in ms (shorter than open: leaving should not linger).
    pub window_close_ms: u32,
    /// Workspace switch, in ms.
    pub workspace_switch_ms: u32,
    /// Curve for things that move on screen and stay there.
    pub standard: Easing,
    /// Curve for things entering.
    pub enter: Easing,
    /// Curve for things leaving.
    pub exit: Easing,
    /// Spring for interactive motion.
    pub spring: Spring,
    /// Accessibility switch: all durations collapse to zero.
    pub reduce_motion: bool,
}

impl Motion {
    /// Duration for an event; zero when `reduce_motion` is set.
    pub fn duration(&self, kind: MotionKind) -> Duration {
        if self.reduce_motion {
            return Duration::ZERO;
        }
        let millis = match kind {
            MotionKind::Fast => self.fast_ms,
            MotionKind::Normal => self.normal_ms,
            MotionKind::Slow => self.slow_ms,
            MotionKind::WindowOpen => self.window_open_ms,
            MotionKind::WindowClose => self.window_close_ms,
            MotionKind::WorkspaceSwitch => self.workspace_switch_ms,
        };
        Duration::from_millis(u64::from(millis))
    }
}

impl Default for Motion {
    fn default() -> Self {
        Motion {
            fast_ms: 120,
            normal_ms: 200,
            slow_ms: 320,
            window_open_ms: 240,
            window_close_ms: 160,
            workspace_switch_ms: 360,
            standard: Easing::EaseInOutCubic,
            enter: Easing::EaseOutExpo,
            exit: Easing::EaseInCubic,
            spring: Spring::default(),
            reduce_motion: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Easing; 6] = [
        Easing::Linear,
        Easing::EaseOutCubic,
        Easing::EaseInCubic,
        Easing::EaseInOutCubic,
        Easing::EaseOutExpo,
        Easing::EaseInOutQuad,
    ];

    #[test]
    fn easings_hit_endpoints_and_stay_monotonic() {
        for easing in ALL {
            assert!(easing.apply(0.0).abs() < 1e-3, "{easing:?}");
            assert!((easing.apply(1.0) - 1.0).abs() < 1e-6, "{easing:?}");
            let mut previous = 0.0;
            for step in 0..=100 {
                let value = easing.apply(step as f32 / 100.0);
                assert!(value >= previous - 1e-6, "{easing:?} at {step}");
                previous = value;
            }
        }
    }

    #[test]
    fn easing_clamps_input() {
        assert_eq!(Easing::EaseOutCubic.apply(-4.0), 0.0);
        assert_eq!(Easing::EaseOutCubic.apply(9.0), 1.0);
    }

    #[test]
    fn expo_front_loads_progress() {
        assert!(Easing::EaseOutExpo.apply(0.2) > 0.7);
    }

    #[test]
    fn serde_names_are_kebab_case() {
        let text = toml::to_string(&Motion::default()).unwrap();
        assert!(text.contains("enter = \"ease-out-expo\""), "{text}");
        assert!(text.contains("standard = \"ease-in-out-cubic\""), "{text}");
    }

    #[test]
    fn reduce_motion_zeroes_every_duration() {
        let motion = Motion {
            reduce_motion: true,
            ..Motion::default()
        };
        assert_eq!(motion.duration(MotionKind::WindowOpen), Duration::ZERO);
        let normal = Motion::default().duration(MotionKind::Normal);
        assert_eq!(normal, Duration::from_millis(200));
    }

    #[test]
    fn default_spring_is_slightly_underdamped() {
        let ratio = Spring::default().damping_ratio();
        assert!(ratio > 0.6 && ratio < 1.0, "{ratio}");
    }
}
