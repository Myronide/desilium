//! Deadlines of everything that wakes the shell without socket data.
//!
//! The loop sleeps until the earliest deadline (or fd activity), so an idle shell
//! wakes once a minute for the clock and once a minute for housekeeping.

use std::time::{Duration, Instant};

/// Why the loop woke up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// The clock text changes now.
    Clock,
    /// The tooltip delay elapsed.
    Tooltip,
    /// Catalog, icon theme and battery re-check.
    Housekeeping,
    /// Debounced configuration reload.
    Config,
    /// `--exit-after` elapsed.
    Exit,
    /// Next step of a demo script.
    Demo,
    /// The pointer left an auto-hide panel long enough ago to hide it.
    Panels,
    /// The window layout is written after a quiet moment.
    Session,
    /// Installed applications changed on disk; the catalog is reloaded after a quiet moment.
    Catalog,
    /// KDE's `kdeglobals` changed (colour scheme or icon theme); re-read after a quiet moment.
    Kde,
    /// The desktop folder changed; it is listed again after a quiet moment.
    Desktop,
}

/// One optional deadline per [`Due`] kind.
#[derive(Debug, Default)]
pub struct Deadlines {
    slots: [Option<Instant>; 11],
}

const ALL: [Due; 11] = [
    Due::Clock,
    Due::Tooltip,
    Due::Housekeeping,
    Due::Config,
    Due::Exit,
    Due::Demo,
    Due::Panels,
    Due::Session,
    Due::Catalog,
    Due::Kde,
    Due::Desktop,
];

/// Slot of `due`; an exhaustive match, so a new kind cannot be forgotten silently.
fn index(due: Due) -> usize {
    match due {
        Due::Clock => 0,
        Due::Tooltip => 1,
        Due::Housekeeping => 2,
        Due::Config => 3,
        Due::Exit => 4,
        Due::Demo => 5,
        Due::Panels => 6,
        Due::Session => 7,
        Due::Catalog => 8,
        Due::Kde => 9,
        Due::Desktop => 10,
    }
}

impl Deadlines {
    /// Arms `due` to fire at `at`, replacing an earlier arming.
    pub fn set(&mut self, due: Due, at: Instant) {
        if let Some(slot) = self.slots.get_mut(index(due)) {
            *slot = Some(at);
        }
    }

    /// Disarms `due`.
    pub fn clear(&mut self, due: Due) {
        if let Some(slot) = self.slots.get_mut(index(due)) {
            *slot = None;
        }
    }

    /// Time to sleep: until the earliest deadline, `None` when nothing is armed.
    pub fn timeout(&self, now: Instant) -> Option<Duration> {
        self.slots
            .iter()
            .flatten()
            .min()
            .map(|at| at.saturating_duration_since(now))
    }

    /// Removes and returns every deadline that has passed.
    pub fn take_due(&mut self, now: Instant) -> Vec<Due> {
        let mut fired = Vec::new();
        for (slot, due) in self.slots.iter_mut().zip(ALL) {
            if slot.is_some_and(|at| at <= now) {
                *slot = None;
                fired.push(due);
            }
        }
        fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_its_own_slot_in_all() {
        for (position, due) in ALL.iter().enumerate() {
            assert_eq!(index(*due), position);
        }
        let mut deadlines = Deadlines::default();
        let t0 = Instant::now();
        for due in ALL {
            deadlines.set(due, t0);
        }
        assert_eq!(deadlines.take_due(t0), ALL);
    }

    #[test]
    fn nothing_armed_sleeps_forever() {
        assert_eq!(Deadlines::default().timeout(Instant::now()), None);
    }

    #[test]
    fn earliest_deadline_wins_and_fires_once() {
        let t0 = Instant::now();
        let mut deadlines = Deadlines::default();
        deadlines.set(Due::Clock, t0 + Duration::from_secs(30));
        deadlines.set(Due::Tooltip, t0 + Duration::from_millis(400));
        assert_eq!(deadlines.timeout(t0), Some(Duration::from_millis(400)));
        assert!(
            deadlines
                .take_due(t0 + Duration::from_millis(399))
                .is_empty()
        );
        assert_eq!(
            deadlines.take_due(t0 + Duration::from_millis(400)),
            [Due::Tooltip]
        );
        assert!(
            deadlines
                .take_due(t0 + Duration::from_millis(401))
                .is_empty()
        );
        assert_eq!(deadlines.timeout(t0), Some(Duration::from_secs(30)));
    }

    #[test]
    fn clear_and_rearm() {
        let t0 = Instant::now();
        let mut deadlines = Deadlines::default();
        deadlines.set(Due::Config, t0);
        deadlines.clear(Due::Config);
        assert!(deadlines.take_due(t0 + Duration::from_secs(1)).is_empty());
    }

    #[test]
    fn overdue_deadlines_report_zero_timeout() {
        let t0 = Instant::now();
        let mut deadlines = Deadlines::default();
        deadlines.set(Due::Exit, t0);
        assert_eq!(
            deadlines.timeout(t0 + Duration::from_secs(5)),
            Some(Duration::ZERO)
        );
    }
}
