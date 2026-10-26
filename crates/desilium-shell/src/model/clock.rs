//! Clock text and the instant at which it next changes. Pure: the caller supplies the time.

use std::time::Duration;

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const SECONDS_PER_MINUTE: u64 = 60;
const HOURS_PER_HALF_DAY: u32 = 12;
const NANOS_PER_SECOND: u32 = 1_000_000_000;

/// Broken-down local wall-clock time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    /// Day of the week, Monday = 0.
    pub weekday: u8,
    /// Day of the month, 1-based.
    pub day: u8,
    /// Month, 1-based.
    pub month: u8,
    /// Hour, 0-23.
    pub hour: u8,
    /// Minute, 0-59.
    pub minute: u8,
    /// Second, 0-59.
    pub second: u8,
    /// Nanoseconds into the current second.
    pub nanos: u32,
}

/// How the time of day is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HourCycle {
    /// `14:32`.
    #[default]
    H24,
    /// `2:32 PM`.
    H12,
}

/// The two strings the bar shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockText {
    /// `Mon 26 Oct`.
    pub date: String,
    /// `14:32`.
    pub time: String,
}

/// Formats `time`; seconds are appended when `with_seconds` is set.
pub fn format_clock(time: &LocalTime, cycle: HourCycle, with_seconds: bool) -> ClockText {
    let weekday = WEEKDAYS
        .get(usize::from(time.weekday))
        .copied()
        .unwrap_or("---");
    let month = usize::from(time.month)
        .checked_sub(1)
        .and_then(|index| MONTHS.get(index))
        .copied()
        .unwrap_or("---");
    let date = format!("{weekday} {} {month}", time.day);
    let mut clock = match cycle {
        HourCycle::H24 => format!("{:02}:{:02}", time.hour, time.minute),
        HourCycle::H12 => {
            let hour = u32::from(time.hour) % HOURS_PER_HALF_DAY;
            let hour = if hour == 0 { HOURS_PER_HALF_DAY } else { hour };
            format!("{hour}:{:02}", time.minute)
        }
    };
    if with_seconds {
        clock.push_str(&format!(":{:02}", time.second));
    }
    if cycle == HourCycle::H12 {
        clock.push_str(if time.hour < HOURS_PER_HALF_DAY as u8 {
            " AM"
        } else {
            " PM"
        });
    }
    ClockText { date, time: clock }
}

/// Time until the displayed text changes next (the next minute, or second boundary).
///
/// Never zero, so a timer armed with it cannot spin when it fires exactly on the boundary.
pub fn until_next_change(time: &LocalTime, with_seconds: bool) -> Duration {
    let into_second = Duration::new(0, time.nanos.min(NANOS_PER_SECOND - 1));
    let remaining_seconds = if with_seconds {
        1
    } else {
        SECONDS_PER_MINUTE
            .saturating_sub(u64::from(time.second))
            .max(1)
    };
    Duration::from_secs(remaining_seconds).saturating_sub(into_second)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u8, minute: u8, second: u8, nanos: u32) -> LocalTime {
        LocalTime {
            weekday: 0,
            day: 26,
            month: 10,
            hour,
            minute,
            second,
            nanos,
        }
    }

    #[test]
    fn formats_like_the_mockup() {
        let text = format_clock(&at(14, 32, 5, 0), HourCycle::H24, false);
        assert_eq!(text.date, "Mon 26 Oct");
        assert_eq!(text.time, "14:32");
    }

    #[test]
    fn seconds_and_twelve_hour_cycle() {
        assert_eq!(
            format_clock(&at(9, 5, 7, 0), HourCycle::H24, true).time,
            "09:05:07"
        );
        assert_eq!(
            format_clock(&at(0, 5, 0, 0), HourCycle::H12, false).time,
            "12:05 AM"
        );
        assert_eq!(
            format_clock(&at(15, 40, 0, 0), HourCycle::H12, false).time,
            "3:40 PM"
        );
        assert_eq!(
            format_clock(&at(12, 0, 0, 0), HourCycle::H12, false).time,
            "12:00 PM"
        );
    }

    #[test]
    fn out_of_range_fields_do_not_panic() {
        let mut time = at(1, 1, 1, 0);
        time.weekday = 9;
        time.month = 0;
        assert_eq!(
            format_clock(&time, HourCycle::H24, false).date,
            "--- 26 ---"
        );
    }

    #[test]
    fn minute_timer_aligns_to_the_next_minute() {
        let wait = until_next_change(&at(14, 32, 45, 250_000_000), false);
        assert_eq!(wait, Duration::from_millis(14_750));
    }

    #[test]
    fn second_timer_aligns_to_the_next_second() {
        let wait = until_next_change(&at(14, 32, 45, 250_000_000), true);
        assert_eq!(wait, Duration::from_millis(750));
    }

    #[test]
    fn timer_on_the_boundary_waits_a_full_period() {
        assert_eq!(
            until_next_change(&at(0, 0, 0, 0), false),
            Duration::from_secs(60)
        );
        assert_eq!(
            until_next_change(&at(0, 0, 59, 999_999_999), false),
            Duration::from_nanos(1)
        );
    }
}
