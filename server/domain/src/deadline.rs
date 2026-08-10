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
}
