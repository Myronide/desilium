//! Damped spring motion, integrated analytically so any time step is stable.

use std::time::Duration;

/// Smallest mass accepted; keeps the natural frequency finite for degenerate input.
const MIN_MASS: f64 = 1e-6;
/// Mass used when the caller supplies a non-finite mass.
const DEFAULT_MASS: f32 = 1.0;
/// Stiffness and damping of the default spring (near critically damped, quick settle).
const DEFAULT_STIFFNESS: f32 = 170.0;
const DEFAULT_DAMPING: f32 = 26.0;
/// Relative band around critical damping where the closed-form solution is used.
const CRITICAL_TOLERANCE: f64 = 1e-6;

/// Physical parameters of a spring: `mass * x'' = -stiffness * (x - target) - damping * x'`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// Restoring force per unit displacement.
    pub stiffness: f32,
    /// Force per unit velocity that drains energy.
    pub damping: f32,
    /// Inertia of the moving value.
    pub mass: f32,
}

impl Default for Spring {
    fn default() -> Self {
        Self {
            stiffness: DEFAULT_STIFFNESS,
            damping: DEFAULT_DAMPING,
            mass: DEFAULT_MASS,
        }
    }
}

/// Position and velocity of a spring, plus the target it was last stepped toward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringState {
    position: f32,
    velocity: f32,
    target: f32,
}

impl SpringState {
    /// Creates a resting state at `position`.
    pub fn new(position: f32) -> Self {
        Self {
            position,
            velocity: 0.0,
            target: position,
        }
    }

    /// Current position.
    pub fn position(&self) -> f32 {
        self.position
    }

    /// Current velocity in units per second.
    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    /// Advances the spring by `dt` toward `target`.
    ///
    /// The solution is exact for the given `dt`, so very large steps simply let the
    /// spring settle instead of diverging. A non-finite `target` leaves the state unchanged.
    pub fn step(&mut self, spring: &Spring, target: f32, dt: Duration) {
        if !target.is_finite() {
            return;
        }
        self.repair_non_finite_state(target);
        let (stiffness, damping, mass) = spring.sanitized();
        let offset = f64::from(self.position) - f64::from(target);
        let (next_offset, next_velocity) = analytic_step(
            offset,
            f64::from(self.velocity),
            stiffness,
            damping,
            mass,
            dt.as_secs_f64(),
        );
        self.position = (f64::from(target) + next_offset) as f32;
        self.velocity = next_velocity as f32;
        self.target = target;
    }

    /// Returns `true` when both the velocity and the distance to the last target are
    /// within `epsilon`.
    pub fn is_at_rest(&self, epsilon: f32) -> bool {
        (self.position - self.target).abs() <= epsilon && self.velocity.abs() <= epsilon
    }

    fn repair_non_finite_state(&mut self, target: f32) {
        if !self.position.is_finite() {
            self.position = target;
        }
        if !self.velocity.is_finite() {
            self.velocity = 0.0;
        }
    }
}

impl Spring {
    /// Returns `(stiffness, damping, mass)` as `f64`, replacing invalid values with safe ones.
    fn sanitized(&self) -> (f64, f64, f64) {
        let stiffness = non_negative_or_zero(self.stiffness);
        let damping = non_negative_or_zero(self.damping);
        let mass = if self.mass.is_finite() {
            f64::from(self.mass).max(MIN_MASS)
        } else {
            f64::from(DEFAULT_MASS)
        };
        (stiffness, damping, mass)
    }
}

fn non_negative_or_zero(value: f32) -> f64 {
    if value.is_finite() && value > 0.0 {
        f64::from(value)
    } else {
        0.0
    }
}

/// Closed-form solution of the damped oscillator for displacement `offset` and velocity
/// `velocity` after `t` seconds. Returns `(offset, velocity)`.
fn analytic_step(
    offset: f64,
    velocity: f64,
    stiffness: f64,
    damping: f64,
    mass: f64,
    t: f64,
) -> (f64, f64) {
    let decay_rate = damping / (2.0 * mass);
    let natural_frequency = (stiffness / mass).sqrt();
    let scale = decay_rate.max(natural_frequency).max(f64::MIN_POSITIVE);
    if (decay_rate - natural_frequency).abs() <= CRITICAL_TOLERANCE * scale {
        return critical(offset, velocity, decay_rate, t);
    }
    if decay_rate < natural_frequency {
        return underdamped(offset, velocity, decay_rate, natural_frequency, t);
    }
    overdamped(offset, velocity, decay_rate, natural_frequency, t)
}

fn critical(offset: f64, velocity: f64, rate: f64, t: f64) -> (f64, f64) {
    let carried = velocity + rate * offset;
    let decay = (-rate * t).exp();
    (
        decay * (offset + carried * t),
        decay * (velocity - rate * carried * t),
    )
}

fn underdamped(offset: f64, velocity: f64, rate: f64, frequency: f64, t: f64) -> (f64, f64) {
    let damped = (frequency * frequency - rate * rate).sqrt();
    let sin_coefficient = (velocity + rate * offset) / damped;
    let phase = damped * t;
    let (sin, cos) = phase.sin_cos();
    let decay = (-rate * t).exp();
    let position = offset * cos + sin_coefficient * sin;
    let speed = (-rate * offset + damped * sin_coefficient) * cos
        + (-rate * sin_coefficient - damped * offset) * sin;
    (decay * position, decay * speed)
}

fn overdamped(offset: f64, velocity: f64, rate: f64, frequency: f64, t: f64) -> (f64, f64) {
    let spread = (rate * rate - frequency * frequency).sqrt();
    let fast = -rate + spread;
    let slow = -rate - spread;
    let fast_weight = (velocity - slow * offset) / (2.0 * spread);
    let slow_weight = offset - fast_weight;
    let fast_term = fast_weight * (fast * t).exp();
    let slow_term = slow_weight * (slow * t).exp();
    (fast_term + slow_term, fast * fast_term + slow * slow_term)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTLE_EPSILON: f32 = 1e-3;

    fn frame() -> Duration {
        Duration::from_nanos(16_666_667)
    }

    fn run(spring: &Spring, state: &mut SpringState, target: f32, step: Duration, steps: u32) {
        for _ in 0..steps {
            state.step(spring, target, step);
        }
    }

    #[test]
    fn converges_to_target_and_comes_to_rest() {
        let spring = Spring::default();
        let mut state = SpringState::new(0.0);
        run(&spring, &mut state, 1.0, frame(), 600);
        assert!((state.position() - 1.0).abs() < SETTLE_EPSILON);
        assert!(state.is_at_rest(SETTLE_EPSILON));
    }

    #[test]
    fn underdamped_spring_overshoots_target() {
        let spring = Spring {
            stiffness: 300.0,
            damping: 6.0,
            mass: 1.0,
        };
        let mut state = SpringState::new(0.0);
        let mut peak = f32::MIN;
        for _ in 0..600 {
            state.step(&spring, 1.0, frame());
            peak = peak.max(state.position());
        }
        assert!(peak > 1.05, "expected overshoot, peak {peak}");
        assert!((state.position() - 1.0).abs() < SETTLE_EPSILON);
    }

    #[test]
    fn overdamped_spring_approaches_without_overshoot() {
        let spring = Spring {
            stiffness: 100.0,
            damping: 80.0,
            mass: 1.0,
        };
        let mut state = SpringState::new(0.0);
        for _ in 0..1200 {
            state.step(&spring, 1.0, frame());
            assert!(
                state.position() <= 1.0 + 1e-5,
                "overshot to {}",
                state.position()
            );
        }
        assert!((state.position() - 1.0).abs() < SETTLE_EPSILON);
    }

    #[test]
    fn analytic_result_matches_fine_numerical_integration() {
        let spring = Spring {
            stiffness: 120.0,
            damping: 10.0,
            mass: 2.0,
        };
        let mut analytic = SpringState::new(0.0);
        analytic.step(&spring, 1.0, Duration::from_millis(500));

        let (mut x, mut v) = (0.0_f64, 0.0_f64);
        let h = 1e-5_f64;
        let k = f64::from(spring.stiffness);
        let c = f64::from(spring.damping);
        let m = f64::from(spring.mass);
        for _ in 0..50_000 {
            let a = (-k * (x - 1.0) - c * v) / m;
            v += a * h;
            x += v * h;
        }
        assert!((f64::from(analytic.position()) - x).abs() < 1e-3);
        assert!((f64::from(analytic.velocity()) - v).abs() < 1e-3);
    }

    #[test]
    fn zero_dt_leaves_state_unchanged() {
        let spring = Spring::default();
        let mut state = SpringState::new(0.3);
        state.step(&spring, 1.0, frame());
        let before = state;
        state.step(&spring, 1.0, Duration::ZERO);
        assert_eq!(state, before);
    }

    #[test]
    fn huge_time_steps_never_explode() {
        let spring = Spring {
            stiffness: 1.0e6,
            damping: 1.0,
            mass: 0.001,
        };
        let mut state = SpringState::new(0.0);
        for seconds in [1.0_f64, 1.0e3, 1.0e6, 1.0e12] {
            state.step(&spring, 1.0, Duration::from_secs_f64(seconds));
            assert!(state.position().is_finite());
            assert!(state.velocity().is_finite());
            assert!(
                state.position().abs() < 10.0,
                "exploded: {}",
                state.position()
            );
        }
        assert!((state.position() - 1.0).abs() < SETTLE_EPSILON);
    }

    #[test]
    fn huge_dt_settles_in_one_step() {
        let spring = Spring::default();
        let mut state = SpringState::new(0.0);
        state.step(&spring, 1.0, Duration::from_secs(3600));
        assert!((state.position() - 1.0).abs() < SETTLE_EPSILON);
        assert!(state.is_at_rest(SETTLE_EPSILON));
    }

    #[test]
    fn degenerate_parameters_stay_finite() {
        let spring = Spring {
            stiffness: f32::NAN,
            damping: f32::INFINITY,
            mass: 0.0,
        };
        let mut state = SpringState::new(0.0);
        run(&spring, &mut state, 1.0, frame(), 300);
        assert!(state.position().is_finite());
        assert!(state.velocity().is_finite());
    }

    #[test]
    fn non_finite_target_is_ignored() {
        let spring = Spring::default();
        let mut state = SpringState::new(0.5);
        let before = state;
        state.step(&spring, f32::NAN, frame());
        assert_eq!(state, before);
    }

    #[test]
    fn non_finite_state_is_repaired() {
        let spring = Spring::default();
        let mut state = SpringState::new(f32::NAN);
        state.step(&spring, 2.0, frame());
        assert!(state.position().is_finite());
        assert!(state.velocity().is_finite());
    }

    #[test]
    fn free_spring_without_forces_moves_linearly() {
        let spring = Spring {
            stiffness: 0.0,
            damping: 0.0,
            mass: 1.0,
        };
        let mut state = SpringState::new(0.0);
        state.step(&spring, 0.0, Duration::from_secs(1));
        assert_eq!(state.position(), 0.0);
        assert_eq!(state.velocity(), 0.0);
    }

    #[test]
    fn at_rest_respects_epsilon() {
        let state = SpringState::new(1.0);
        assert!(state.is_at_rest(0.0));
        let moved = SpringState {
            position: 1.5,
            velocity: 0.0,
            target: 1.0,
        };
        assert!(!moved.is_at_rest(0.1));
        assert!(moved.is_at_rest(0.6));
    }
}
