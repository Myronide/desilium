//! Process statistics printed by `--exit-after`, to check the idle cost.

/// CPU seconds (user + system) used by this process so far.
pub(crate) fn cpu_seconds() -> f64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    // Fields after the parenthesised command name; utime and stime are the 12th and 13th.
    let fields: Vec<&str> = stat
        .rsplit(')')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    let ticks = |at: usize| {
        fields
            .get(at)
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    (ticks(11) + ticks(12)) / CLOCK_TICKS_PER_SECOND
}

/// Peak resident memory in MiB.
pub(crate) fn peak_rss_mib() -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    kib / KIB_PER_MIB
}

/// `sysconf(_SC_CLK_TCK)` is 100 on every Linux the shell targets.
const CLOCK_TICKS_PER_SECOND: f64 = 100.0;
const KIB_PER_MIB: f64 = 1024.0;
