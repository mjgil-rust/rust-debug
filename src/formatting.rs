use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Level;

pub fn format_colored(
    namespace: &str,
    level: Level,
    color: u8,
    args: &fmt::Arguments<'_>,
    diff: &str,
    show_time: bool,
    show_diff: bool,
    ts: &str,
) -> String {
    let (ns_color, level_tag) = match level {
        Level::Error => (1, " ERROR"),
        Level::Warn => (3, " WARN"),
        _ => (color, ""),
    };

    let time_prefix = if show_time {
        format!("{} ", ts)
    } else {
        String::new()
    };

    let diff_suffix = if show_diff {
        format!(" \x1b[9{}m+{}\x1b[0m", ns_color, diff)
    } else {
        String::new()
    };

    format!(
        "  {}\x1b[9{}m{}{}\x1b[0m \x1b[90m{}\x1b[0m{}\n",
        time_prefix, ns_color, namespace, level_tag, args, diff_suffix
    )
}

pub fn format_plain(
    namespace: &str,
    level: Level,
    args: &fmt::Arguments<'_>,
    diff: &str,
    show_diff: bool,
    ts: &str,
) -> String {
    let level_tag = match level {
        Level::Error => " ERROR",
        Level::Warn => " WARN",
        _ => "",
    };
    let diff_suffix = if show_diff {
        format!(" +{}", diff)
    } else {
        String::new()
    };

    format!(
        "{} {}{} {}{}\n",
        ts, namespace, level_tag, args, diff_suffix
    )
}

pub fn humanize(us: u128) -> String {
    const MS: u128 = 1_000;
    const SEC: u128 = 1_000_000;
    const MIN: u128 = 60 * SEC;
    const HOUR: u128 = 60 * MIN;

    if us >= HOUR {
        format!("{:.1}h", us as f64 / HOUR as f64)
    } else if us >= MIN {
        format!("{:.1}m", us as f64 / MIN as f64)
    } else if us >= SEC {
        format!("{:.1}s", us as f64 / SEC as f64)
    } else if us >= MS {
        format!("{:.1}ms", us as f64 / MS as f64)
    } else {
        format!("{}us", us)
    }
}

pub fn utc_timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();

    let days = (secs / 86400) as i64;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hours, minutes, seconds, millis
    )
}

pub(crate) fn utc_date_string() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (secs / 86400) as i64;
    let (year, month, day) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02}", year, month, day)
}

/// Convert days since Unix epoch to (year, month, day) in UTC.
/// Howard Hinnant's civil_from_days algorithm.
pub(crate) fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}
