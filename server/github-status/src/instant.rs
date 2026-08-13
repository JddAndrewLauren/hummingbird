//! Turning one GitHub Actions run timestamp (`run_started_at`/`created_at`,
//! always UTC, always `"YYYY-MM-DDTHH:MM:SSZ"`) into epoch milliseconds.
//!
//! **No tzdb, dependency-free integer arithmetic** — `race-poll`'s
//! `schedule.rs` gives the same reasoning for the same shape of stamp, and
//! carries the same `days_from_civil` algorithm; it is copied here rather
//! than shared, on that module's own reasoning: this is a poller, not a
//! library, and neither poller may depend on the other.

/// Days since 1970-01-01 for a proleptic Gregorian civil date (Howard
/// Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Parses `"YYYY-MM-DDTHH:MM:SSZ"` (fractional seconds tolerated and
/// truncated, on `race-poll`'s own reasoning for its own stamp) into epoch
/// ms. `None` for anything else — most importantly a stamp with no
/// trailing `Z`, since reading a local-looking time as UTC would move every
/// age computed from it by hours, silently.
pub fn parse_iso8601_utc(stamp: &str) -> Option<i64> {
    let stamp = stamp.strip_suffix('Z')?;
    let (date, time) = stamp.split_once('T')?;

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second: i64 = match time_parts.next() {
        None => 0,
        Some(s) => s.split('.').next()?.parse().ok()?,
    };
    if time_parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    Some((days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_itself() {
        assert_eq!(parse_iso8601_utc("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn an_ordinary_run_timestamp() {
        assert_eq!(parse_iso8601_utc("2026-03-08T04:00:00Z"), Some(1_772_942_400_000));
    }

    #[test]
    fn fractional_seconds_are_truncated() {
        assert_eq!(
            parse_iso8601_utc("2026-03-08T04:00:00.123Z"),
            Some(1_772_942_400_000)
        );
    }

    #[test]
    fn a_leap_day() {
        assert_eq!(parse_iso8601_utc("2024-02-29T12:30:00Z"), Some(1_709_209_800_000));
    }

    #[test]
    fn a_stamp_without_a_utc_marker_is_refused() {
        for stamp in [
            "2026-03-08T04:00:00",
            "2026-03-08T04:00:00+01:00",
            "2026-13-08T04:00:00Z",
            "not-a-timestamp",
            "",
        ] {
            assert_eq!(parse_iso8601_utc(stamp), None, "{stamp}");
        }
    }
}
