//! Battery state parsed from the kernel's `power_supply` attributes.

/// Charging state of a battery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryStatus {
    /// Drawing from the battery.
    Discharging,
    /// Charging from mains.
    Charging,
    /// Full and plugged in.
    Full,
    /// Unknown or not charging.
    Unknown,
}

/// One battery's reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    /// Charge level, 0-100.
    pub percent: u8,
    /// Charging state.
    pub status: BatteryStatus,
}

/// Parses `capacity` (`"82\n"`); values above 100 are clamped, garbage is `None`.
pub fn parse_capacity(text: &str) -> Option<u8> {
    let value: u32 = text.trim().parse().ok()?;
    u8::try_from(value.min(100)).ok()
}

/// Parses `status` (`"Charging\n"`).
pub fn parse_status(text: &str) -> BatteryStatus {
    match text.trim().to_ascii_lowercase().as_str() {
        "discharging" => BatteryStatus::Discharging,
        "charging" => BatteryStatus::Charging,
        "full" => BatteryStatus::Full,
        _ => BatteryStatus::Unknown,
    }
}

/// Merges all batteries of the machine: the mean level; charging if any one charges.
pub fn combine(batteries: &[Battery]) -> Option<Battery> {
    if batteries.is_empty() {
        return None;
    }
    let total: u32 = batteries.iter().map(|b| u32::from(b.percent)).sum();
    let count = u32::try_from(batteries.len()).ok()?;
    let percent = u8::try_from(total.checked_div(count)?).ok()?;
    let any = |wanted: BatteryStatus| batteries.iter().any(|b| b.status == wanted);
    let status = if any(BatteryStatus::Charging) {
        BatteryStatus::Charging
    } else if any(BatteryStatus::Discharging) {
        BatteryStatus::Discharging
    } else if any(BatteryStatus::Full) {
        BatteryStatus::Full
    } else {
        BatteryStatus::Unknown
    };
    Some(Battery { percent, status })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_parsing() {
        assert_eq!(parse_capacity("82\n"), Some(82));
        assert_eq!(parse_capacity(" 100 "), Some(100));
        assert_eq!(parse_capacity("140"), Some(100));
        assert_eq!(parse_capacity("-3"), None);
        assert_eq!(parse_capacity("full"), None);
        assert_eq!(parse_capacity(""), None);
    }

    #[test]
    fn status_parsing_ignores_case_and_whitespace() {
        assert_eq!(parse_status("Charging\n"), BatteryStatus::Charging);
        assert_eq!(parse_status("DISCHARGING"), BatteryStatus::Discharging);
        assert_eq!(parse_status("Full"), BatteryStatus::Full);
        assert_eq!(parse_status("Not charging"), BatteryStatus::Unknown);
    }

    #[test]
    fn no_battery_combines_to_none() {
        assert_eq!(combine(&[]), None);
    }

    #[test]
    fn two_batteries_average_and_charging_wins() {
        let a = Battery {
            percent: 80,
            status: BatteryStatus::Discharging,
        };
        let b = Battery {
            percent: 41,
            status: BatteryStatus::Charging,
        };
        assert_eq!(
            combine(&[a, b]),
            Some(Battery {
                percent: 60,
                status: BatteryStatus::Charging
            })
        );
    }
}
