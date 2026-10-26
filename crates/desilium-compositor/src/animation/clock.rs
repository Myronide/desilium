//! Time source for animations; injectable so animation logic never reads the wall clock.

use std::time::{Duration, Instant};

/// Where "now" comes from.
#[derive(Debug)]
pub enum Clock {
    /// Real time: monotonic, measured from the compositor start.
    Monotonic(Instant),
    /// Time that only moves when told to (tests).
    #[cfg(test)]
    Manual(std::cell::Cell<Duration>),
    /// Time that never advances; used with a freeze offset for deterministic captures.
    Frozen(Duration),
}

impl Clock {
    /// Real time measured from `epoch`.
    pub fn monotonic(epoch: Instant) -> Clock {
        Clock::Monotonic(epoch)
    }

    /// A hand-driven clock starting at `start`.
    #[cfg(test)]
    pub fn manual(start: Duration) -> Clock {
        Clock::Manual(std::cell::Cell::new(start))
    }

    /// Current time since the clock's epoch.
    pub fn now(&self) -> Duration {
        match self {
            Clock::Monotonic(epoch) => epoch.elapsed(),
            #[cfg(test)]
            Clock::Manual(time) => time.get(),
            Clock::Frozen(time) => *time,
        }
    }

    /// Moves a manual clock forward; other clocks ignore this.
    #[cfg(test)]
    pub fn advance(&self, by: Duration) {
        if let Clock::Manual(time) = self {
            time.set(time.get().saturating_add(by));
        }
    }

    /// Time never moves on its own, so nothing needs repeated redraws.
    pub fn is_frozen(&self) -> bool {
        matches!(self, Clock::Frozen(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_moves_only_when_advanced() {
        let clock = Clock::manual(Duration::from_millis(5));
        assert_eq!(clock.now(), Duration::from_millis(5));
        clock.advance(Duration::from_millis(10));
        assert_eq!(clock.now(), Duration::from_millis(15));
    }

    #[test]
    fn frozen_clock_ignores_advance_and_reports_frozen() {
        let clock = Clock::Frozen(Duration::from_secs(1));
        clock.advance(Duration::from_secs(5));
        assert_eq!(clock.now(), Duration::from_secs(1));
        assert!(clock.is_frozen());
        assert!(!Clock::manual(Duration::ZERO).is_frozen());
    }

    #[test]
    fn monotonic_clock_does_not_go_backwards() {
        let clock = Clock::monotonic(Instant::now());
        let first = clock.now();
        assert!(clock.now() >= first);
    }
}
