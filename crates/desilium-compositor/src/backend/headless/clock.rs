use std::time::{Duration, Instant};

/// Hard limit so a screenshot run can never hang.
pub const HARD_TIMEOUT: Duration = Duration::from_secs(20);

/// What the screenshot timer should do now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Wait,
    /// Clients were quiet for the full delay.
    Capture,
    /// The hard limit hit before the scene settled.
    TimedOut,
}

/// Decides when to take the screenshot; time is passed in so it is testable.
#[derive(Debug, Clone, Copy)]
pub struct ShotClock {
    started: Instant,
    delay: Duration,
    hard_timeout: Duration,
}

impl ShotClock {
    pub fn new(started: Instant, delay: Duration) -> Self {
        Self {
            started,
            delay,
            hard_timeout: HARD_TIMEOUT,
        }
    }

    /// `last_activity` is the last time a mapped window changed, if ever.
    pub fn verdict(&self, now: Instant, last_activity: Option<Instant>) -> Verdict {
        if let Some(activity) = last_activity
            && now.saturating_duration_since(activity) >= self.delay
        {
            return Verdict::Capture;
        }
        if now.saturating_duration_since(self.started) >= self.hard_timeout {
            return Verdict::TimedOut;
        }
        Verdict::Wait
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY: Duration = Duration::from_millis(2000);

    fn at(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn waits_until_delay_after_activity() {
        let start = Instant::now();
        let clock = ShotClock::new(start, DELAY);
        let activity = Some(at(start, 100));
        assert_eq!(clock.verdict(at(start, 2000), activity), Verdict::Wait);
        assert_eq!(clock.verdict(at(start, 2100), activity), Verdict::Capture);
    }

    #[test]
    fn never_captures_without_activity_but_times_out() {
        let start = Instant::now();
        let clock = ShotClock::new(start, DELAY);
        assert_eq!(clock.verdict(at(start, 19_999), None), Verdict::Wait);
        assert_eq!(clock.verdict(at(start, 20_000), None), Verdict::TimedOut);
    }

    #[test]
    fn constant_activity_times_out() {
        let start = Instant::now();
        let clock = ShotClock::new(start, DELAY);
        let now = at(start, 20_500);
        assert_eq!(
            clock.verdict(now, Some(at(start, 20_400))),
            Verdict::TimedOut
        );
    }
}
