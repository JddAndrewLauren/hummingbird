//! Validation and ordering for the item `deadline` field (ADR-0013, #153): a
//! naive calendar date or a naive local date-time to minute precision, and
//! nothing else. No seconds, no timezone offset or `Z` suffix, no bare
//! time. One implementation here so both the authority and the client core
//! inherit it, rather than each guessing at the shape independently.
//!
//! ISO-8601's lexicographic sort keeps chronological order **across** days
//! for free — a date-only value and a date-time value on different days
//! compare correctly as raw strings. **Within** one day the raw sort is
//! wrong: a day-grained deadline means end of day (23:59), but as text
//! `"2026-08-15"` sorts *before* `"2026-08-15T14:30"`, which would rank
//! "sometime the 15th" ahead of "the 15th at 14:30" — backwards.
//! [`deadline_sort_key`] is the one comparison key ADR-0013 calls for to fix
//! this: it resolves a day-only value to that day's `T23:59` before
//! comparing, so the client's `by_priority_then_due` sort and #133's rule
//! evaluator share the exact same notion of "when this deadline is" and can
//! never disagree on the same pair of rows.

/// `true` for `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`; `false` for anything
/// else, including a value with seconds, a `Z`/offset suffix, a bare time,
/// or a calendar date that does not exist (e.g. `2026-02-30`).
pub fn is_valid_deadline(s: &str) -> bool {
    if !s.is_ascii() {
        return false;
    }
    match s.len() {
        10 => is_valid_date(s),
        16 => s.as_bytes()[10] == b'T' && is_valid_date(&s[..10]) && is_valid_time(&s[11..]),
        _ => false,
    }
}

fn is_valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let (Some(year), Some(month), Some(day)) =
        (digits(&s[0..4]), digits(&s[5..7]), digits(&s[8..10]))
    else {
        return false;
    };
    (1..=12).contains(&month) && (1..=days_in_month(year, month)).contains(&day)
}

fn is_valid_time(s: &str) -> bool {
    let b = s.as_bytes();
    if b[2] != b':' {
        return false;
    }
    let (Some(hour), Some(minute)) = (digits(&s[0..2]), digits(&s[3..5])) else {
        return false;
    };
    hour <= 23 && minute <= 59
}

/// `None` unless every byte is an ASCII digit — `str::parse` alone would
/// happily accept a leading `+`/`-`, which a date component must not.
fn digits(s: &str) -> Option<u32> {
    s.bytes().all(|b| b.is_ascii_digit()).then(|| s.parse().ok()).flatten()
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// The comparison key ADR-0013 defines for ordering and rule evaluation: a
/// day-only deadline resolves to the end of that day (`T23:59`) before
/// comparing, so `"2026-08-15"` sorts alongside `"2026-08-15T23:59"` and
/// after every earlier time that same day — never before it.
///
/// Callers must already hold an [`is_valid_deadline`]-accepted string; this
/// does no validation of its own, and its output is a comparison key, not
/// itself a valid deadline value.
pub fn deadline_sort_key(deadline: &str) -> std::borrow::Cow<'_, str> {
    if deadline.len() == 10 {
        std::borrow::Cow::Owned(format!("{deadline}T23:59"))
    } else {
        std::borrow::Cow::Borrowed(deadline)
    }
}

/// Relative-time duration units (ADR-0013): `m`inutes, `h`ours, `d`ays.
/// `date`-typed fields accept `Days` only — the rule engine (#133) is the
/// one that enforces that restriction, since it is a function of the
/// *field's* declared type, not of the duration literal alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationUnit {
    Minutes,
    Hours,
    Days,
}

/// Parses one ADR-0013 duration literal: an unsigned integer immediately
/// followed by one unit letter (`m`/`h`/`d`), no compounds, no sign. Signed
/// durations and inferred direction are the rejected alternatives — the
/// direction is always the operator (`within_next` / `within_last`), never
/// the literal.
pub fn parse_duration(s: &str) -> Option<(i64, DurationUnit)> {
    if s.len() < 2 || !s.is_ascii() {
        return None;
    }
    let (digits, unit) = s.split_at(s.len() - 1);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let amount: i64 = digits.parse().ok()?;
    let unit = match unit {
        "m" => DurationUnit::Minutes,
        "h" => DurationUnit::Hours,
        "d" => DurationUnit::Days,
        _ => return None,
    };
    Some((amount, unit))
}

/// Shifts a valid deadline-shaped string (`is_valid_deadline`) by a signed
/// number of minutes/hours/days, returning a string in the same precision
/// as `base` (date-only stays date-only; a date-time stays a date-time).
/// Used by the rule engine (#133) to compute `now ± D` as a string directly
/// comparable, lexicographically, against [`deadline_sort_key`] — so
/// relative-time evaluation shares the identical string-comparison
/// discipline the sort and the resolved key already establish, rather than
/// a second, numeric notion of "when this is."
///
/// `amount` may be negative (used internally for `within_last`); the wire
/// vocabulary itself never accepts a signed literal (see
/// [`parse_duration`]).
pub fn shift(base: &str, amount: i64, unit: DurationUnit) -> Option<String> {
    if !is_valid_deadline(base) {
        return None;
    }
    let date_only = base.len() == 10;
    let (year, month, day) = (
        digits(&base[0..4])?,
        digits(&base[5..7])?,
        digits(&base[8..10])?,
    );
    let (hour, minute) = if date_only {
        (0, 0)
    } else {
        (digits(&base[11..13])?, digits(&base[14..16])?)
    };

    let delta_minutes = match unit {
        DurationUnit::Minutes => amount,
        DurationUnit::Hours => amount.checked_mul(60)?,
        DurationUnit::Days => amount.checked_mul(1440)?,
    };

    let base_days = days_from_civil(year as i64, month as i64, day as i64);
    let base_minutes = base_days.checked_mul(1440)?.checked_add(hour as i64 * 60 + minute as i64)?;
    let shifted_minutes = base_minutes.checked_add(delta_minutes)?;

    let shifted_days = shifted_minutes.div_euclid(1440);
    let minute_of_day = shifted_minutes.rem_euclid(1440);
    let (y, mo, d) = civil_from_days(shifted_days);
    let (h, mi) = (minute_of_day / 60, minute_of_day % 60);

    Some(if date_only {
        format!("{y:04}-{mo:02}-{d:02}")
    } else {
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}")
    })
}

/// Howard Hinnant's `days_from_civil`: days since the epoch (1970-01-01) for
/// a proleptic-Gregorian calendar date. Chosen over pulling in a date/time
/// crate for one small, well-known, exhaustively-tested integer algorithm —
/// consistent with this module's existing hand-rolled validation.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// The inverse of [`days_from_civil`]: the proleptic-Gregorian calendar
/// date for a day count since the epoch.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_calendar_date() {
        assert!(is_valid_deadline("2026-08-15"));
    }

    #[test]
    fn accepts_a_minute_precision_date_time() {
        assert!(is_valid_deadline("2026-08-15T09:30"));
    }

    #[test]
    fn accepts_a_leap_day() {
        assert!(is_valid_deadline("2024-02-29"));
    }

    #[test]
    fn rejects_a_non_leap_years_february_29() {
        assert!(!is_valid_deadline("2026-02-29"));
    }

    #[test]
    fn rejects_seconds() {
        assert!(!is_valid_deadline("2026-08-15T09:30:00"));
    }

    #[test]
    fn rejects_a_z_suffix() {
        assert!(!is_valid_deadline("2026-08-15T09:30Z"));
    }

    #[test]
    fn rejects_an_offset_suffix() {
        assert!(!is_valid_deadline("2026-08-15T09:30+00:00"));
    }

    #[test]
    fn rejects_a_bare_time() {
        assert!(!is_valid_deadline("09:30"));
    }

    #[test]
    fn rejects_a_malformed_date() {
        for bad in ["2026-13-01", "2026-00-10", "2026-04-31", "not-a-date", "2026/08/15", ""] {
            assert!(!is_valid_deadline(bad), "{bad} should be rejected");
        }
    }

    #[test]
    fn a_date_only_key_resolves_to_end_of_day() {
        assert_eq!(deadline_sort_key("2026-08-15"), "2026-08-15T23:59");
    }

    #[test]
    fn a_date_time_key_is_returned_unchanged() {
        assert_eq!(deadline_sort_key("2026-08-15T09:30"), "2026-08-15T09:30");
    }

    #[test]
    fn a_date_only_value_sorts_after_an_earlier_time_the_same_day() {
        // The raw strings alone say otherwise ("2026-08-15" < "...T00:00"),
        // which is exactly the bug ADR-0013 requires the resolved key fix:
        // a day-grained deadline means end of day, so it must rank *after*
        // an explicit morning time on the same day.
        assert!(deadline_sort_key("2026-08-15") > deadline_sort_key("2026-08-15T00:00"));
    }

    #[test]
    fn a_date_only_value_ties_with_an_explicit_23_59_the_same_day() {
        assert_eq!(deadline_sort_key("2026-08-15"), deadline_sort_key("2026-08-15T23:59"));
    }

    #[test]
    fn resolved_keys_sort_chronologically_across_mixed_forms_and_days() {
        let mut values = [
            "2026-08-16",
            "2026-08-15T23:59",
            "2026-08-15",
            "2026-08-15T00:01",
            "2026-08-14T12:00",
        ];
        values.sort_by(|a, b| deadline_sort_key(a).cmp(&deadline_sort_key(b)));
        assert_eq!(
            values,
            [
                "2026-08-14T12:00",
                "2026-08-15T00:01",
                // "2026-08-15" and "2026-08-15T23:59" resolve to the same
                // key and tie; a stable sort keeps their original relative
                // order, which is why the explicit time appears first here.
                "2026-08-15T23:59",
                "2026-08-15",
                "2026-08-16",
            ]
        );
    }

    #[test]
    fn parse_duration_accepts_each_unit() {
        assert_eq!(parse_duration("2h"), Some((2, DurationUnit::Hours)));
        assert_eq!(parse_duration("10m"), Some((10, DurationUnit::Minutes)));
        assert_eq!(parse_duration("3d"), Some((3, DurationUnit::Days)));
    }

    #[test]
    fn parse_duration_rejects_compounds_signs_and_junk() {
        for bad in ["1d2h", "-10m", "2", "h", "", "2x", " 2h"] {
            assert_eq!(parse_duration(bad), None, "{bad} should be rejected");
        }
    }

    #[test]
    fn shift_adds_hours_across_a_day_boundary() {
        assert_eq!(
            shift("2026-08-15T23:00", 2, DurationUnit::Hours),
            Some("2026-08-16T01:00".to_string())
        );
    }

    #[test]
    fn shift_subtracts_minutes_across_a_month_boundary() {
        assert_eq!(
            shift("2026-09-01T00:10", -20, DurationUnit::Minutes),
            Some("2026-08-31T23:50".to_string())
        );
    }

    #[test]
    fn shift_adds_days_and_stays_date_only() {
        assert_eq!(shift("2026-08-15", 20, DurationUnit::Days), Some("2026-09-04".to_string()));
    }

    #[test]
    fn shift_across_a_leap_day() {
        assert_eq!(shift("2024-02-28", 2, DurationUnit::Days), Some("2024-03-01".to_string()));
    }

    #[test]
    fn shift_rejects_an_invalid_base() {
        assert_eq!(shift("not-a-date", 1, DurationUnit::Days), None);
    }
}
