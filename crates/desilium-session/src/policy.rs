//! Pure restart policy. Decides what happens after a component exits.
//!
//! Every clock value is passed in explicitly, so the whole module is testable
//! without spawning processes or sleeping.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::component::Criticality;

const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(30);
const DEFAULT_BACKOFF_FACTOR: u32 = 2;
const DEFAULT_STABLE_AFTER: Duration = Duration::from_secs(60);
const DEFAULT_CRASH_LOOP_MAX: usize = 5;
const DEFAULT_CRASH_LOOP_WINDOW: Duration = Duration::from_secs(60);

/// Tunable restart behaviour for one component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestartPolicy {
    /// Delay before the first restart after a failure.
    pub initial_backoff: Duration,
    /// Upper bound for the delay between two restarts.
    pub max_backoff: Duration,
    /// Multiplier applied to the delay after each consecutive failure.
    pub factor: u32,
    /// A run lasting at least this long counts as healthy and resets backoff and crash history.
    pub stable_after: Duration,
    /// Failures inside `crash_loop_window` that make the tracker give up. Must be at least 1.
    pub crash_loop_max: usize,
    /// Sliding window over which failures are counted.
    pub crash_loop_window: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            factor: DEFAULT_BACKOFF_FACTOR,
            stable_after: DEFAULT_STABLE_AFTER,
            crash_loop_max: DEFAULT_CRASH_LOOP_MAX,
            crash_loop_window: DEFAULT_CRASH_LOOP_WINDOW,
        }
    }
}

/// How a run ended, as far as restart policy cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitKind {
    /// Exited with status 0.
    Success,
    /// Non-zero status, killed by a signal, or failed to start.
    Failure,
}

/// Why a component is no longer restarted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiveUpReason {
    /// Too many failures inside the crash-loop window.
    CrashLoop,
}

/// What the supervisor should do after a component exited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Start the component again once this much time has passed.
    RestartAfter(Duration),
    /// Stop restarting this component; the session continues without it.
    GiveUp(GiveUpReason),
    /// The session cannot continue; shut everything down.
    EndSession,
}

/// Per-component restart state: backoff position and recent failures.
#[derive(Debug)]
pub struct RestartTracker {
    policy: RestartPolicy,
    criticality: Criticality,
    next_backoff: Duration,
    failures: VecDeque<Instant>,
}

impl RestartTracker {
    /// Creates a tracker with a fresh backoff and an empty crash history.
    pub fn new(policy: RestartPolicy, criticality: Criticality) -> Self {
        Self {
            policy,
            criticality,
            next_backoff: policy.initial_backoff,
            failures: VecDeque::new(),
        }
    }

    /// Decides the fate of a component that just exited.
    ///
    /// `uptime` is how long the run lasted and `now` is the moment it exited.
    pub fn on_exit(&mut self, now: Instant, uptime: Duration, exit: ExitKind) -> Decision {
        if self.criticality == Criticality::Critical {
            return Decision::EndSession;
        }
        match exit {
            // A clean exit is a normal lifecycle event, not a crash: restart promptly
            // and leave the crash history alone.
            ExitKind::Success => Decision::RestartAfter(self.policy.initial_backoff),
            ExitKind::Failure => self.on_failure(now, uptime),
        }
    }

    fn on_failure(&mut self, now: Instant, uptime: Duration) -> Decision {
        if uptime >= self.policy.stable_after {
            self.reset();
        }
        self.record_failure(now);
        if self.failures.len() >= self.policy.crash_loop_max {
            return Decision::GiveUp(GiveUpReason::CrashLoop);
        }
        let delay = self.next_backoff;
        self.next_backoff = self.grown(delay);
        Decision::RestartAfter(delay)
    }

    fn reset(&mut self) {
        self.next_backoff = self.policy.initial_backoff;
        self.failures.clear();
    }

    fn record_failure(&mut self, now: Instant) {
        // If the window reaches before the clock's origin, no recorded failure can be outside it.
        if let Some(window_start) = now.checked_sub(self.policy.crash_loop_window) {
            while self
                .failures
                .front()
                .is_some_and(|&failed_at| failed_at < window_start)
            {
                self.failures.pop_front();
            }
        }
        self.failures.push_back(now);
    }

    fn grown(&self, delay: Duration) -> Duration {
        let max = self.policy.max_backoff;
        delay
            .checked_mul(self.policy.factor)
            .map_or(max, |grown| grown.min(max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY_100_MS: Duration = Duration::from_millis(100);
    const DELAY_200_MS: Duration = Duration::from_millis(200);
    const DELAY_400_MS: Duration = Duration::from_millis(400);
    const DELAY_500_MS: Duration = Duration::from_millis(500);

    fn policy() -> RestartPolicy {
        RestartPolicy {
            initial_backoff: DELAY_100_MS,
            max_backoff: DELAY_500_MS,
            factor: 2,
            stable_after: Duration::from_secs(60),
            crash_loop_max: 100,
            crash_loop_window: Duration::from_secs(10),
        }
    }

    fn at(base: Instant, seconds: u64) -> Instant {
        base + Duration::from_secs(seconds)
    }

    fn fail(tracker: &mut RestartTracker, now: Instant) -> Decision {
        tracker.on_exit(now, Duration::ZERO, ExitKind::Failure)
    }

    #[test]
    fn backoff_grows_geometrically_and_caps_at_max() {
        let base = Instant::now();
        let mut tracker = RestartTracker::new(policy(), Criticality::Restartable);
        let delays: Vec<Decision> = (0..5)
            .map(|step| fail(&mut tracker, at(base, step)))
            .collect();
        assert_eq!(
            delays,
            vec![
                Decision::RestartAfter(DELAY_100_MS),
                Decision::RestartAfter(DELAY_200_MS),
                Decision::RestartAfter(DELAY_400_MS),
                Decision::RestartAfter(DELAY_500_MS),
                Decision::RestartAfter(DELAY_500_MS),
            ]
        );
    }

    #[test]
    fn stable_uptime_resets_backoff() {
        let base = Instant::now();
        let mut tracker = RestartTracker::new(policy(), Criticality::Restartable);
        fail(&mut tracker, at(base, 0));
        fail(&mut tracker, at(base, 1));
        let after_stable = tracker.on_exit(at(base, 2), Duration::from_secs(60), ExitKind::Failure);
        assert_eq!(after_stable, Decision::RestartAfter(DELAY_100_MS));
    }

    #[test]
    fn stable_run_also_clears_crash_history() {
        let base = Instant::now();
        let mut limited = policy();
        limited.crash_loop_max = 3;
        let mut tracker = RestartTracker::new(limited, Criticality::Restartable);
        fail(&mut tracker, at(base, 0));
        fail(&mut tracker, at(base, 1));
        tracker.on_exit(at(base, 2), Duration::from_secs(60), ExitKind::Failure);
        // Only two failures are inside the window now, so the third must not give up yet.
        assert!(matches!(
            fail(&mut tracker, at(base, 3)),
            Decision::RestartAfter(_)
        ));
        assert_eq!(
            fail(&mut tracker, at(base, 4)),
            Decision::GiveUp(GiveUpReason::CrashLoop)
        );
    }

    #[test]
    fn three_failures_inside_window_are_a_crash_loop() {
        let base = Instant::now();
        let mut limited = policy();
        limited.crash_loop_max = 3;
        let mut tracker = RestartTracker::new(limited, Criticality::Restartable);
        assert!(matches!(
            fail(&mut tracker, at(base, 0)),
            Decision::RestartAfter(_)
        ));
        assert!(matches!(
            fail(&mut tracker, at(base, 1)),
            Decision::RestartAfter(_)
        ));
        assert_eq!(
            fail(&mut tracker, at(base, 2)),
            Decision::GiveUp(GiveUpReason::CrashLoop)
        );
    }

    #[test]
    fn failures_spread_outside_window_are_not_a_crash_loop() {
        let base = Instant::now();
        let mut limited = policy();
        limited.crash_loop_max = 3;
        let mut tracker = RestartTracker::new(limited, Criticality::Restartable);
        let decisions: Vec<Decision> = [0, 6, 12, 18, 24]
            .into_iter()
            .map(|seconds| fail(&mut tracker, at(base, seconds)))
            .collect();
        assert!(
            decisions
                .iter()
                .all(|d| matches!(d, Decision::RestartAfter(_)))
        );
    }

    #[test]
    fn clean_exits_restart_promptly_and_never_give_up() {
        let base = Instant::now();
        let mut limited = policy();
        limited.crash_loop_max = 2;
        let mut tracker = RestartTracker::new(limited, Criticality::Optional);
        for step in 0..5 {
            let decision = tracker.on_exit(at(base, step), Duration::ZERO, ExitKind::Success);
            assert_eq!(decision, Decision::RestartAfter(DELAY_100_MS));
        }
    }

    #[test]
    fn single_allowed_failure_gives_up_immediately_when_max_is_one() {
        let base = Instant::now();
        let mut strict = policy();
        strict.crash_loop_max = 1;
        let mut tracker = RestartTracker::new(strict, Criticality::Restartable);
        assert_eq!(
            fail(&mut tracker, base),
            Decision::GiveUp(GiveUpReason::CrashLoop)
        );
    }

    #[test]
    fn critical_component_always_ends_the_session() {
        let base = Instant::now();
        let mut tracker = RestartTracker::new(policy(), Criticality::Critical);
        assert_eq!(
            tracker.on_exit(base, Duration::ZERO, ExitKind::Success),
            Decision::EndSession
        );
        assert_eq!(
            tracker.on_exit(base, Duration::from_secs(600), ExitKind::Failure),
            Decision::EndSession
        );
    }

    #[test]
    fn default_policy_grows_from_a_nonzero_backoff() {
        let defaults = RestartPolicy::default();
        assert!(defaults.initial_backoff > Duration::ZERO);
        assert!(defaults.max_backoff >= defaults.initial_backoff);
        assert!(defaults.crash_loop_max >= 1);
    }
}
