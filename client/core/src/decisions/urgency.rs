//! Urgency banding and the deadline-field grammar, sunk here from the web's
//! `urgency.ts`/`deadline-parts.ts` by ADR-0025 (#141/M1-2).
//!
//! CONTEXT.md: "Urgency … is computed by consumers at read time over the
//! mirror. Never a stored class and never a routing decision at ingestion."
//! [`compute_urgency`] is that read-time computation, shared by every
//! client instead of a TS copy on the web and a Kotlin copy on Android that
//! could silently disagree about which band a deadline falls in.
//!
//! **Validity and ordering are never re-derived here.** `is_valid_deadline`,
//! `deadline_sort_key` and the new [`hummingbird_domain::minutes_until`]
//! all live in `hummingbird_domain::deadline` — this module calls them and
//! adds nothing of its own about what a deadline string means, only what a
//! *duration until one* means for the band a reader sees.
//!
//! **No timezone, on purpose.** Per the ADR-0015 amendment this crate
//! resolves no civil date to an instant — the reader does, in its own
//! zone. [`compute_urgency`] therefore takes `now` as a deadline-shaped
//! naive string (`YYYY-MM-DDTHH:MM`), the caller's own local wall clock
//! already rendered into that shape, exactly the same kind of value a
//! `deadline` field already is. The web seam is what turns a real
//! `Date.now()` reading into that string; nothing in `hummingbird-core`
//! ever touches a timezone offset.

use hummingbird_domain::{deadline_sort_key, is_valid_deadline, minutes_until};

/// How pressing a deadline reads right now. `hummingbird-core` stays
/// binding-agnostic (ADR-0003, pinned by
/// `cargo_toml_has_no_binding_macro_dependencies` in `lib.rs`), so this
/// stays a plain enum; M1-6 (#504) exposes it to Kotlin as
/// `ffi-mobile::MobileUrgencyBand`, a mirroring type carrying its own
/// `#[derive(uniffi::Enum)]` (see that type's doc), never a second
/// definition of the four bands crossing this module's own boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrgencyBand {
    Calm,
    Soon,
    Now,
    Overdue,
}

impl UrgencyBand {
    /// The wire/JS spelling each web caller already reads
    /// (`urgency.ts`'s retired `Urgency` union: `"calm" | "soon" | "now" |
    /// "overdue"`).
    pub fn as_str(self) -> &'static str {
        match self {
            UrgencyBand::Calm => "calm",
            UrgencyBand::Soon => "soon",
            UrgencyBand::Now => "now",
            UrgencyBand::Overdue => "overdue",
        }
    }
}

/// Past due within this window reads as "now" rather than merely "soon" —
/// generous enough that a same-day deadline is never mistaken for something
/// days off, tight enough that "soon" still means "not today".
const NOW_WINDOW_MINUTES: i64 = 24 * 60;

/// Beyond [`NOW_WINDOW_MINUTES`] but inside this window reads as "soon";
/// beyond it, "calm". Three days: long enough to surface a coming deadline
/// without making most of a normal backlog read as urgent.
const SOON_WINDOW_MINUTES: i64 = 3 * 24 * 60;

/// The one urgency computation this app has: no deadline, or one that
/// cannot be resolved against `now`, is "calm" — never an error and never
/// treated as "no consequence" the other direction (overdue).
///
/// `now` is deadline-shaped (`is_valid_deadline`-accepted), the reader's own
/// local wall clock — see the module header for why this takes a string
/// rather than an epoch millisecond count.
pub fn compute_urgency(deadline: Option<&str>, now: &str) -> UrgencyBand {
    let Some(deadline) = deadline else {
        return UrgencyBand::Calm;
    };
    let Some(remaining) = minutes_until(deadline, now) else {
        return UrgencyBand::Calm;
    };
    if remaining < 0 {
        return UrgencyBand::Overdue;
    }
    if remaining <= NOW_WINDOW_MINUTES {
        return UrgencyBand::Now;
    }
    if remaining <= SOON_WINDOW_MINUTES {
        return UrgencyBand::Soon;
    }
    UrgencyBand::Calm
}

/// `hummingbird_domain::is_valid_deadline` verbatim — re-exported at this
/// module's boundary so a caller reaching decisions through
/// `client/core/src/decisions` never needs a second import path for the
/// same rule.
pub fn is_valid_deadline_field(deadline: &str) -> bool {
    is_valid_deadline(deadline)
}

/// A scheduled date is a whole civil day — a do-date has no minute — so the
/// date-time form [`is_valid_deadline_field`] also accepts is refused here.
pub fn is_valid_scheduled_date(scheduled_date: &str) -> bool {
    scheduled_date.len() == 10 && is_valid_deadline(scheduled_date)
}

/// ADR-0013's comparison key, re-exported the same way as
/// [`is_valid_deadline_field`].
pub fn deadline_sort_key_field(deadline: &str) -> String {
    deadline_sort_key(deadline).into_owned()
}

/// Splitting a deadline into the two controls that edit it (a date picker
/// and an optional time picker) — the sunk half of `deadline-parts.ts`.
/// `date` is `YYYY-MM-DD`, or the whole raw value when `value` is neither
/// shape (which is what keeps a legacy free-text deadline visible and
/// intact rather than emptied on load); `time` is `HH:MM`, or `None` when
/// the deadline names a whole day.
pub struct DeadlineParts {
    pub date: String,
    pub time: Option<String>,
}

pub fn split_deadline(value: &str) -> DeadlineParts {
    if is_date_time_shaped(value) {
        let (date, rest) = value.split_at(10);
        return DeadlineParts { date: date.to_string(), time: Some(rest[1..].to_string()) };
    }
    DeadlineParts { date: value.to_string(), time: None }
}

/// Shape only (`\d{4}-\d{2}-\d{2}T\d{2}:\d{2}`), deliberately not validity —
/// this mirrors the retired TS regex exactly, so an impossible calendar
/// date (`2026-02-30T09:30`) still splits into its two controls rather
/// than falling through to the free-text branch. Whether it can be *sent*
/// is [`is_valid_deadline_field`]'s question, asked separately by the form.
fn is_date_time_shaped(value: &str) -> bool {
    if value.len() != 16 {
        return false;
    }
    let b = value.as_bytes();
    let digit = |i: usize| b[i].is_ascii_digit();
    (0..4).all(digit)
        && b[4] == b'-'
        && (5..7).all(digit)
        && b[7] == b'-'
        && (8..10).all(digit)
        && b[10] == b'T'
        && (11..13).all(digit)
        && b[13] == b':'
        && (14..16).all(digit)
}

/// The inverse of [`split_deadline`], with two rules the form leans on: an
/// empty date is an empty deadline (clearing the date clears the whole
/// value, time included — a time with no day is not a deadline), and a
/// `date` this module does not recognise as `YYYY-MM-DD` is returned as it
/// came, `time` discarded.
pub fn join_deadline(date: &str, time: Option<&str>) -> String {
    if date.is_empty() {
        return String::new();
    }
    let Some(time) = time.filter(|t| !t.is_empty()) else {
        return date.to_string();
    };
    if is_date_only(date) {
        format!("{date}T{time}")
    } else {
        date.to_string()
    }
}

fn is_date_only(s: &str) -> bool {
    s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-'
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------- compute_urgency
    // Ported verbatim from `urgency.test.ts`, `now` recast as an explicit
    // deadline-shaped string rather than an epoch millisecond count — the
    // two are the same value once the (necessarily client-side) local
    // resolution has already happened.

    const NOW: &str = "2026-08-15T12:00";

    #[test]
    fn is_calm_with_no_deadline_at_all() {
        assert_eq!(compute_urgency(None, NOW), UrgencyBand::Calm);
    }

    #[test]
    fn is_overdue_once_the_deadline_has_passed() {
        assert_eq!(compute_urgency(Some("2026-08-14"), NOW), UrgencyBand::Overdue);
        assert_eq!(compute_urgency(Some("2026-08-15T11:00"), NOW), UrgencyBand::Overdue);
    }

    #[test]
    fn is_now_within_the_near_term_window() {
        assert_eq!(compute_urgency(Some("2026-08-15T18:00"), NOW), UrgencyBand::Now);
    }

    #[test]
    fn is_soon_further_out_but_still_within_days() {
        assert_eq!(compute_urgency(Some("2026-08-17"), NOW), UrgencyBand::Soon);
    }

    #[test]
    fn is_calm_for_a_deadline_far_in_the_future() {
        assert_eq!(compute_urgency(Some("2026-12-01"), NOW), UrgencyBand::Calm);
    }

    #[test]
    fn a_day_only_deadline_on_today_is_still_now_at_noon_not_overdue() {
        // Resolves to 2026-08-15T23:59 — end of day, still ahead of NOW.
        assert_eq!(compute_urgency(Some("2026-08-15"), NOW), UrgencyBand::Now);
    }

    #[test]
    fn treats_an_unparseable_deadline_as_calm_rather_than_erroring() {
        assert_eq!(compute_urgency(Some("not-a-date"), NOW), UrgencyBand::Calm);
    }

    // --------------------------------------------------------- deadline_sort_key
    // Ported from `urgency.test.ts`'s `deadlineSortKey` describe block —
    // `hummingbird_domain::deadline_sort_key` already carries its own
    // exhaustive suite; these two pin only that this module's boundary
    // function is a pass-through.

    #[test]
    fn deadline_sort_key_field_resolves_a_day_only_deadline_to_end_of_day() {
        assert_eq!(deadline_sort_key_field("2026-08-15"), "2026-08-15T23:59");
    }

    #[test]
    fn deadline_sort_key_field_returns_a_minute_precision_deadline_unchanged() {
        assert_eq!(deadline_sort_key_field("2026-08-15T09:30"), "2026-08-15T09:30");
    }

    // ---------------------------------------------------- is_valid_deadline

    #[test]
    fn is_valid_deadline_field_accepts_both_shapes() {
        assert!(is_valid_deadline_field("2026-08-15"));
        assert!(is_valid_deadline_field("2026-08-15T09:30"));
    }

    #[test]
    fn is_valid_deadline_field_rejects_junk() {
        assert!(!is_valid_deadline_field("2026-02-30"));
        assert!(!is_valid_deadline_field("not-a-date"));
    }

    #[test]
    fn is_valid_scheduled_date_refuses_a_time_of_day() {
        assert!(is_valid_scheduled_date("2026-08-30"));
        assert!(!is_valid_scheduled_date("2026-08-30T09:30"));
    }

    // ------------------------------------------------------- split_deadline
    // Ported verbatim from `deadline-parts.test.ts`.

    #[test]
    fn split_deadline_reads_a_whole_day_deadline_as_a_date_with_no_time() {
        let parts = split_deadline("2026-09-01");
        assert_eq!(parts.date, "2026-09-01");
        assert_eq!(parts.time, None);
    }

    #[test]
    fn split_deadline_splits_a_date_time_into_its_two_controls() {
        let parts = split_deadline("2026-09-01T09:30");
        assert_eq!(parts.date, "2026-09-01");
        assert_eq!(parts.time, Some("09:30".to_string()));
    }

    #[test]
    fn split_deadline_reads_an_empty_deadline_as_an_empty_date() {
        let parts = split_deadline("");
        assert_eq!(parts.date, "");
        assert_eq!(parts.time, None);
    }

    #[test]
    fn split_deadline_passes_a_shape_it_does_not_recognise_straight_through() {
        for raw in ["next tuesday", "2026-09-01T09:30:00", "2026-9-1", "09:30"] {
            let parts = split_deadline(raw);
            assert_eq!(parts.date, raw, "{raw:?}");
            assert_eq!(parts.time, None, "{raw:?}");
        }
    }

    // -------------------------------------------------------- join_deadline

    #[test]
    fn join_deadline_round_trips_both_shapes() {
        for value in ["2026-09-01", "2026-09-01T09:30", ""] {
            let parts = split_deadline(value);
            assert_eq!(join_deadline(&parts.date, parts.time.as_deref()), value);
        }
    }

    #[test]
    fn join_deadline_drops_the_time_when_the_date_is_cleared() {
        assert_eq!(join_deadline("", Some("09:30")), "");
    }

    #[test]
    fn join_deadline_treats_an_empty_time_the_same_as_no_time() {
        assert_eq!(join_deadline("2026-09-01", Some("")), "2026-09-01");
        assert_eq!(join_deadline("2026-09-01", None), "2026-09-01");
    }

    #[test]
    fn join_deadline_refuses_to_build_a_date_time_on_a_date_it_does_not_recognise() {
        assert_eq!(join_deadline("next tuesday", Some("09:30")), "next tuesday");
    }
}
