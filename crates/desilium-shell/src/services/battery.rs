//! Reads batteries from `/sys/class/power_supply`.

use std::{fs, path::Path};

use crate::model::battery::{Battery, combine, parse_capacity, parse_status};

/// Where the kernel exposes power supplies.
pub const POWER_SUPPLY_DIR: &str = "/sys/class/power_supply";
const BATTERY_TYPE: &str = "Battery";

fn read(dir: &Path, name: &str) -> Option<String> {
    fs::read_to_string(dir.join(name)).ok()
}

fn read_one(dir: &Path) -> Option<Battery> {
    let kind = read(dir, "type")?;
    if kind.trim() != BATTERY_TYPE {
        return None;
    }
    // Peripheral batteries (mice, headsets) report `scope = Device`; only the system's count.
    if read(dir, "scope").is_some_and(|scope| scope.trim() == "Device") {
        return None;
    }
    let percent = parse_capacity(&read(dir, "capacity")?)?;
    let status = parse_status(&read(dir, "status").unwrap_or_default());
    Some(Battery { percent, status })
}

/// The machine's battery under `root`, `None` on desktops.
pub fn read_battery(root: &Path) -> Option<Battery> {
    let entries = fs::read_dir(root).ok()?;
    let mut dirs: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    dirs.sort();
    let batteries: Vec<Battery> = dirs.iter().filter_map(|dir| read_one(dir)).collect();
    combine(&batteries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::battery::BatteryStatus;

    fn supply(root: &Path, name: &str, files: &[(&str, &str)]) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        for (file, text) in files {
            fs::write(dir.join(file), text).unwrap();
        }
    }

    #[test]
    fn reads_the_system_battery_and_ignores_mains_and_peripherals() {
        let root = tempfile::tempdir().unwrap();
        supply(root.path(), "AC", &[("type", "Mains\n")]);
        supply(
            root.path(),
            "BAT0",
            &[
                ("type", "Battery\n"),
                ("capacity", "82\n"),
                ("status", "Discharging\n"),
            ],
        );
        supply(
            root.path(),
            "hidpp_battery_0",
            &[
                ("type", "Battery\n"),
                ("scope", "Device\n"),
                ("capacity", "10\n"),
            ],
        );
        assert_eq!(
            read_battery(root.path()),
            Some(Battery {
                percent: 82,
                status: BatteryStatus::Discharging
            })
        );
    }

    #[test]
    fn machines_without_a_battery_yield_none() {
        let root = tempfile::tempdir().unwrap();
        supply(root.path(), "AC", &[("type", "Mains\n")]);
        assert_eq!(read_battery(root.path()), None);
        assert_eq!(read_battery(Path::new("/nonexistent/power_supply")), None);
    }

    #[test]
    fn unreadable_capacity_skips_the_battery() {
        let root = tempfile::tempdir().unwrap();
        supply(
            root.path(),
            "BAT0",
            &[("type", "Battery\n"), ("capacity", "n/a\n")],
        );
        assert_eq!(read_battery(root.path()), None);
    }
}
