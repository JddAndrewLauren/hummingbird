//! Validation for the item `deadline` field (ADR-0013, #153): a naive
//! calendar date or a naive local date-time to minute precision, and
//! nothing else. No seconds, no timezone offset or `Z` suffix, no bare
//! time. One implementation here so both the authority and the client core
//! inherit it, rather than each guessing at the shape independently.
//!
//! The two accepted forms are fixed-width (`YYYY-MM-DD` is 10 bytes,
//! `YYYY-MM-DDTHH:MM` is 16), which is what keeps the raw-string ordering
//! the client's `by_priority_then_due` depends on intact: a date-only value
//! is a strict textual prefix of every same-day date-time, so it always
//! sorts first.

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
    fn a_date_only_value_sorts_before_any_time_on_the_same_day() {
        assert!("2026-08-15" < "2026-08-15T00:00");
        assert!("2026-08-15" < "2026-08-15T23:59");
    }

    #[test]
    fn lexicographic_order_matches_chronological_order_across_mixed_forms() {
        let mut values = [
            "2026-08-16",
            "2026-08-15T23:59",
            "2026-08-15",
            "2026-08-15T00:01",
            "2026-08-14T12:00",
        ];
        values.sort();
        assert_eq!(
            values,
            [
                "2026-08-14T12:00",
                "2026-08-15",
                "2026-08-15T00:01",
                "2026-08-15T23:59",
                "2026-08-16",
            ]
        );
    }
}
