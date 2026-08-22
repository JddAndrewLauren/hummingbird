//! The two read queries against a [`CalendarSnapshot`]: `events overlapping
//! interval` and `current/next event`. Both take an explicit `now`/interval
//! from the caller — the core never samples a clock itself (same discipline
//! as [`crate::storage::Envelope::as_of`]), which is what keeps these
//! queries deterministic and testable against hand-built snapshots.
//!
//! Since ADR-0015's 2026-08-10 amendment the caller supplies its civil
//! context too, not just its instants: [`EventWhen`] has two arms, and the
//! all-day arm can only be asked about in *dates*. The core owns no tzdb
//! and never resolves a civil date to an instant — the reader computes both
//! shapes of its own window in its own zone and hands them in together, the
//! same two-shapes-of-one-fact idiom [`crate::rank::Now`] already uses.

use super::event::{EventRecord, EventStatus, EventWhen};
use super::snapshot::CalendarSnapshot;

/// The reader's query window, in **both** shapes: a half-open UTC
/// millisecond interval `[start_ms, end_ms)` for timed events, and a
/// half-open civil date range `[start_date, end_date)` — `YYYY-MM-DD`,
/// exclusive end — for all-day ones.
///
/// Both halves are the caller's own, computed in the reader's zone. They
/// describe the same window and are never derived from one another here:
/// deriving either would need the zone this crate deliberately does not
/// have.
///
/// Half-open on both arms, so that an event ending exactly when another
/// starts (an abutment) does not count as overlapping either — the same
/// convention providers use for exclusive all-day end dates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interval {
    pub start_ms: i64,
    pub end_ms: i64,
    pub start_date: String,
    pub end_date: String,
}

impl Interval {
    pub fn new(
        start_ms: i64,
        end_ms: i64,
        start_date: impl Into<String>,
        end_date: impl Into<String>,
    ) -> Self {
        Self {
            start_ms,
            end_ms,
            start_date: start_date.into(),
            end_date: end_date.into(),
        }
    }
}

/// Whether an event is something to answer a query with at all.
///
/// The snapshot is a faithful mirror of what the provider reported and so
/// deliberately keeps cancelled events — the adapter asks Google for them
/// (`showDeleted=true`) so a consumer can tell "cancelled" apart from
/// "absent". A cancellation is nonetheless a record that something is *not*
/// happening, so no read query hands one back as context: a cancelled future
/// instance must never become "Next" or bias task ranking. Cancelled
/// instances are also stored as a zero-length span (only `originalStartTime`
/// exists to place them), and that placeholder is not an event anyone is
/// attending.
pub(crate) fn is_actionable(event: &EventRecord) -> bool {
    event.status != EventStatus::Cancelled
}

/// Whether `event` overlaps `interval`, each arm asked in its own terms:
/// instants against instants, dates against dates. An all-day event is
/// never resolved to instants to answer this, which is the whole amendment.
fn overlaps(event: &EventRecord, interval: &Interval) -> bool {
    match &event.when {
        EventWhen::Timed { start_ms, end_ms } => {
            *start_ms < interval.end_ms && *end_ms > interval.start_ms
        }
        EventWhen::AllDay {
            start_date,
            end_date,
        } => {
            start_date.as_str() < interval.end_date.as_str()
                && end_date.as_str() > interval.start_date.as_str()
        }
    }
}

/// The sort key one event orders by: all-day events first (a whole-day fact
/// precedes the day's times — and there is no instant to interleave one at
/// anyway), then each arm in its own order.
fn order_key(event: &EventRecord) -> (u8, &str, &str, i64, i64, &str) {
    match &event.when {
        EventWhen::AllDay {
            start_date,
            end_date,
        } => (
            0,
            start_date.as_str(),
            end_date.as_str(),
            0,
            0,
            event.provider_event_id.as_str(),
        ),
        EventWhen::Timed { start_ms, end_ms } => (
            1,
            "",
            "",
            *start_ms,
            *end_ms,
            event.provider_event_id.as_str(),
        ),
    }
}

/// Returns every non-cancelled event in `snapshot` overlapping `interval`,
/// in a deterministic order: all-day events first (ascending by start date,
/// then end date), then timed events ascending by start instant, ties in
/// each arm broken by end and then by `provider_event_id`.
///
/// A consumer that wants events interleaved by day — #122's weekend pane —
/// re-buckets per day itself, in its own zone, which is the only place the
/// two arms can be meaningfully put in one sequence.
pub fn events_overlapping_interval<'a>(
    snapshot: &'a CalendarSnapshot,
    interval: &Interval,
) -> Vec<&'a EventRecord> {
    let mut matches: Vec<&EventRecord> = snapshot
        .events
        .iter()
        .filter(|event| is_actionable(event))
        .filter(|event| overlaps(event, interval))
        .collect();

    matches.sort_by(|a, b| order_key(a).cmp(&order_key(b)));

    matches
}

/// The result of [`current_or_next_event`].
#[derive(Debug, PartialEq, Eq)]
pub enum CurrentOrNext<'a> {
    /// An event happening right now — an all-day event covering the
    /// reader's `today`, or a timed event whose `[start, end)` contains
    /// `now`.
    InProgress(&'a EventRecord),
    /// Nothing is in progress; this is the soonest event starting after
    /// `now`.
    Upcoming(&'a EventRecord),
    /// Nothing in progress and nothing upcoming.
    None,
}

/// Finds the in-progress event (if any), else the soonest upcoming event,
/// as of `now_ms` — and, for the all-day arm, as of the reader's own civil
/// date `today` (`YYYY-MM-DD`, resolved in the reader's zone; this crate
/// cannot derive it from `now_ms`). Cancelled events are never returned —
/// see [`is_actionable`].
///
/// **The two arms are ranked, not interleaved**, because they are not
/// comparable: a whole day and a half-hour have no common ordering.
/// In progress: an all-day event covering `today` wins over a timed event
/// in progress — a day off is the more consequential fact about right now,
/// and it preserves the behaviour of the flattened shape this replaced
/// (where an all-day event's midnight start beat every meeting).
/// Upcoming: an all-day event beginning after `today` wins over a timed
/// event. A civil date and an instant cannot be interleaved without a time
/// zone, which the core intentionally does not own; preferring the next
/// all-day fact means tomorrow's day off is not hidden by a meeting months
/// away. Within either arm, the normal deterministic ordering applies.
///
/// **Its host caller left with #245** — ADR-0015 replaced Now's context
/// tile with the ranked pane region, and `ffi-web`'s `currentOrNext` shim
/// went with it. This stays: [`crate::rank`] consumes [`CurrentOrNext`] for
/// its 30-minute calendar nudge, and the next host consumer is the "what's
/// on now / next" standing question (ADR-0015, under #117).
pub fn current_or_next_event<'a>(
    snapshot: &'a CalendarSnapshot,
    now_ms: i64,
    today: &str,
) -> CurrentOrNext<'a> {
    let actionable = || snapshot.events.iter().filter(|event| is_actionable(event));

    let all_day_now = actionable()
        .filter(|event| match &event.when {
            EventWhen::AllDay {
                start_date,
                end_date,
            } => start_date.as_str() <= today && end_date.as_str() > today,
            EventWhen::Timed { .. } => false,
        })
        .min_by(|a, b| order_key(a).cmp(&order_key(b)));

    if let Some(event) = all_day_now {
        return CurrentOrNext::InProgress(event);
    }

    let timed_now = actionable()
        .filter(|event| match &event.when {
            EventWhen::Timed { start_ms, end_ms } => *start_ms <= now_ms && *end_ms > now_ms,
            EventWhen::AllDay { .. } => false,
        })
        .min_by(|a, b| order_key(a).cmp(&order_key(b)));

    if let Some(event) = timed_now {
        return CurrentOrNext::InProgress(event);
    }

    let all_day_upcoming = actionable()
        .filter(|event| match &event.when {
            EventWhen::AllDay { start_date, .. } => start_date.as_str() > today,
            EventWhen::Timed { .. } => false,
        })
        .min_by(|a, b| order_key(a).cmp(&order_key(b)));

    if let Some(event) = all_day_upcoming {
        return CurrentOrNext::Upcoming(event);
    }

    let timed_upcoming = actionable()
        .filter(|event| match &event.when {
            EventWhen::Timed { start_ms, .. } => *start_ms > now_ms,
            EventWhen::AllDay { .. } => false,
        })
        .min_by(|a, b| order_key(a).cmp(&order_key(b)));

    match timed_upcoming {
        Some(event) => CurrentOrNext::Upcoming(event),
        None => CurrentOrNext::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::event::EventStatus;

    fn timed_event(id: &str, start_ms: i64, end_ms: i64) -> EventRecord {
        EventRecord {
            provider_event_id: id.to_string(),
            calendar_id: "cal-primary".to_string(),
            title: id.to_string(),
            when: EventWhen::timed(start_ms, end_ms),
            recurrence_id: None,
            location: None,
            organizer: None,
            status: EventStatus::Confirmed,
            provider_updated_at_ms: start_ms,
            html_link: None,
            description: None,
        }
    }

    fn all_day_event(id: &str, start_date: &str, end_date: &str) -> EventRecord {
        EventRecord {
            when: EventWhen::all_day(start_date, end_date),
            ..timed_event(id, 0, 0)
        }
    }

    /// A cancelled recurring instance as the Google adapter stores one: a
    /// zero-length span, because only `originalStartTime` places it.
    fn cancelled_event(id: &str, start_ms: i64) -> EventRecord {
        EventRecord {
            status: EventStatus::Cancelled,
            ..timed_event(id, start_ms, start_ms)
        }
    }

    /// A window carrying both shapes. The dates are the caller's own — the
    /// test states them rather than deriving them from the instants, which
    /// is exactly what a real reader does in its own zone.
    fn window(start_ms: i64, end_ms: i64, start_date: &str, end_date: &str) -> Interval {
        Interval::new(start_ms, end_ms, start_date, end_date)
    }

    /// A window whose civil arm matches nothing, for the timed-only tests.
    fn timed_window(start_ms: i64, end_ms: i64) -> Interval {
        Interval::new(start_ms, end_ms, "1970-01-01", "1970-01-01")
    }

    // -- events_overlapping_interval -----------------------------------

    #[test]
    fn interval_query_returns_events_in_local_time_order() {
        let snapshot = CalendarSnapshot::new(vec![
            timed_event("afternoon", 3_000, 4_000),
            timed_event("morning", 1_000, 2_000),
        ]);

        let results = events_overlapping_interval(&snapshot, &timed_window(0, 5_000));
        let ids: Vec<&str> = results
            .iter()
            .map(|event| event.provider_event_id.as_str())
            .collect();
        assert_eq!(ids, vec!["morning", "afternoon"]);
    }

    #[test]
    fn overlapping_events_both_match_but_abutting_events_do_not() {
        let overlapping = timed_event("overlapping", 1_000, 3_000);
        let abutting_before = timed_event("abuts-before-query", 0, 1_000);
        let abutting_after = timed_event("abuts-after-query", 5_000, 6_000);
        let snapshot =
            CalendarSnapshot::new(vec![overlapping.clone(), abutting_before, abutting_after]);

        let results = events_overlapping_interval(&snapshot, &timed_window(1_000, 5_000));

        assert_eq!(results, vec![&overlapping]);
    }

    #[test]
    fn two_events_that_abut_each_other_both_overlap_a_query_spanning_the_boundary() {
        let first = timed_event("first", 1_000, 2_000);
        let second = timed_event("second", 2_000, 3_000);
        let snapshot = CalendarSnapshot::new(vec![first.clone(), second.clone()]);

        let results = events_overlapping_interval(&snapshot, &timed_window(1_500, 2_500));

        assert_eq!(results, vec![&first, &second]);
    }

    #[test]
    fn empty_snapshot_returns_no_events() {
        let snapshot = CalendarSnapshot::new(vec![]);
        let results = events_overlapping_interval(&snapshot, &timed_window(0, 1_000));
        assert!(results.is_empty());
    }

    #[test]
    fn an_all_day_event_is_tested_against_the_civil_arm_never_the_instants() {
        // The instants here name a window nowhere near the dates: an
        // implementation that resolved the dates to instants (in any zone)
        // could not answer this correctly.
        let holiday = all_day_event("holiday", "2026-09-09", "2026-09-10");
        let snapshot = CalendarSnapshot::new(vec![holiday.clone()]);

        let results =
            events_overlapping_interval(&snapshot, &window(0, 1, "2026-09-09", "2026-09-10"));

        assert_eq!(results, vec![&holiday]);
    }

    #[test]
    fn an_all_day_events_exclusive_end_date_abutting_the_window_does_not_overlap() {
        // Sep 9 only (end date is Sep 10, exclusive). A window starting on
        // Sep 10 abuts it and must not match.
        let holiday = all_day_event("holiday", "2026-09-09", "2026-09-10");
        let snapshot = CalendarSnapshot::new(vec![holiday]);

        let results =
            events_overlapping_interval(&snapshot, &window(0, 1, "2026-09-10", "2026-09-11"));

        assert!(results.is_empty());
    }

    #[test]
    fn a_windows_own_exclusive_end_date_abutting_an_all_day_start_does_not_overlap() {
        let holiday = all_day_event("holiday", "2026-09-09", "2026-09-10");
        let snapshot = CalendarSnapshot::new(vec![holiday]);

        let results =
            events_overlapping_interval(&snapshot, &window(0, 1, "2026-09-08", "2026-09-09"));

        assert!(results.is_empty());
    }

    #[test]
    fn a_multi_day_all_day_event_overlaps_a_window_inside_it() {
        let trip = all_day_event("india", "2026-09-09", "2026-09-16");
        let snapshot = CalendarSnapshot::new(vec![trip.clone()]);

        let results =
            events_overlapping_interval(&snapshot, &window(0, 1, "2026-09-12", "2026-09-13"));

        assert_eq!(results, vec![&trip]);
    }

    #[test]
    fn all_day_events_sort_ahead_of_timed_ones() {
        let meeting = timed_event("meeting", 1_000, 2_000);
        let holiday = all_day_event("holiday", "2026-09-09", "2026-09-10");
        let snapshot = CalendarSnapshot::new(vec![meeting.clone(), holiday.clone()]);

        let results =
            events_overlapping_interval(&snapshot, &window(0, 5_000, "2026-09-09", "2026-09-10"));

        assert_eq!(results, vec![&holiday, &meeting]);
    }

    // -- current_or_next_event -------------------------------------------

    #[test]
    fn in_progress_event_wins_over_an_upcoming_one() {
        let current = timed_event("current", 1_000, 3_000);
        let upcoming = timed_event("upcoming", 5_000, 6_000);
        let snapshot = CalendarSnapshot::new(vec![current.clone(), upcoming]);

        assert_eq!(
            current_or_next_event(&snapshot, 2_000, "2026-08-11"),
            CurrentOrNext::InProgress(&current)
        );
    }

    #[test]
    fn upcoming_event_returned_when_nothing_is_in_progress() {
        let past = timed_event("past", 0, 1_000);
        let soonest = timed_event("soonest", 5_000, 6_000);
        let later = timed_event("later", 10_000, 11_000);
        let snapshot = CalendarSnapshot::new(vec![past, later, soonest.clone()]);

        assert_eq!(
            current_or_next_event(&snapshot, 2_000, "2026-08-11"),
            CurrentOrNext::Upcoming(&soonest)
        );
    }

    #[test]
    fn a_cancelled_instance_never_becomes_next() {
        // The snapshot keeps the cancellation (that is the adapter's job),
        // but it is not something to do: the confirmed event behind it is
        // what "Next" means here.
        let cancelled = cancelled_event("cancelled-standup", 5_000);
        let real = timed_event("real-meeting", 8_000, 9_000);
        let snapshot = CalendarSnapshot::new(vec![cancelled, real.clone()]);

        assert_eq!(
            current_or_next_event(&snapshot, 1_000, "2026-08-11"),
            CurrentOrNext::Upcoming(&real)
        );
    }

    #[test]
    fn a_cancelled_event_is_never_in_progress_either() {
        let cancelled = EventRecord {
            status: EventStatus::Cancelled,
            ..timed_event("cancelled-long-meeting", 1_000, 9_000)
        };
        let snapshot = CalendarSnapshot::new(vec![cancelled]);

        assert_eq!(
            current_or_next_event(&snapshot, 5_000, "2026-08-11"),
            CurrentOrNext::None
        );
    }

    #[test]
    fn interval_query_omits_cancelled_events_including_zero_length_placeholders() {
        // A cancelled instance's zero-length placeholder still falls
        // strictly inside a surrounding interval, so the overlap test alone
        // would return it.
        let cancelled = cancelled_event("cancelled-standup", 2_000);
        let real = timed_event("real-meeting", 1_500, 2_500);
        let snapshot = CalendarSnapshot::new(vec![cancelled, real.clone()]);

        let results = events_overlapping_interval(&snapshot, &timed_window(0, 5_000));

        assert_eq!(results, vec![&real]);
    }

    #[test]
    fn none_when_snapshot_is_empty() {
        let snapshot = CalendarSnapshot::new(vec![]);
        assert_eq!(
            current_or_next_event(&snapshot, 1_000, "2026-08-11"),
            CurrentOrNext::None
        );
    }

    #[test]
    fn none_when_every_event_is_in_the_past() {
        let past = timed_event("past", 0, 1_000);
        let snapshot = CalendarSnapshot::new(vec![past]);
        assert_eq!(
            current_or_next_event(&snapshot, 5_000, "2026-08-11"),
            CurrentOrNext::None
        );
    }

    #[test]
    fn an_all_day_event_containing_today_is_in_progress_whatever_the_instant() {
        let holiday = all_day_event("holiday", "2026-08-10", "2026-08-12");
        let snapshot = CalendarSnapshot::new(vec![holiday.clone()]);

        for now_ms in [0, 1_786_000_000_000, i64::MAX / 2] {
            assert_eq!(
                current_or_next_event(&snapshot, now_ms, "2026-08-11"),
                CurrentOrNext::InProgress(&holiday)
            );
        }
    }

    #[test]
    fn an_all_day_events_exclusive_end_date_is_not_still_in_progress() {
        // Aug 10 only. On Aug 11 it is over — and, having no start date
        // after today either, there is nothing upcoming.
        let holiday = all_day_event("holiday", "2026-08-10", "2026-08-11");
        let snapshot = CalendarSnapshot::new(vec![holiday]);

        assert_eq!(
            current_or_next_event(&snapshot, 1_000, "2026-08-11"),
            CurrentOrNext::None
        );
    }

    #[test]
    fn an_all_day_event_covering_today_wins_over_a_timed_event_in_progress() {
        // The ranking the flattened shape used to produce incidentally (its
        // local-midnight start beat every meeting), now stated on purpose.
        let holiday = all_day_event("holiday", "2026-08-11", "2026-08-12");
        let meeting = timed_event("meeting", 1_000, 9_000);
        let snapshot = CalendarSnapshot::new(vec![meeting, holiday.clone()]);

        assert_eq!(
            current_or_next_event(&snapshot, 5_000, "2026-08-11"),
            CurrentOrNext::InProgress(&holiday)
        );
    }

    #[test]
    fn an_upcoming_all_day_event_is_preferred_over_a_timed_one() {
        // The core cannot compare a civil date with an instant without a
        // time zone. Prefer the all-day fact so it remains visible instead
        // of being hidden by any future timed event.
        let tomorrow_off = all_day_event("day-off", "2026-08-12", "2026-08-13");
        let meeting = timed_event("meeting", 5_000, 6_000);
        let snapshot = CalendarSnapshot::new(vec![tomorrow_off.clone(), meeting]);

        assert_eq!(
            current_or_next_event(&snapshot, 1_000, "2026-08-11"),
            CurrentOrNext::Upcoming(&tomorrow_off)
        );
    }

    #[test]
    fn a_nearby_all_day_event_is_not_hidden_by_a_later_timed_event() {
        let tomorrow_off = all_day_event("day-off", "2026-08-12", "2026-08-13");
        let distant_meeting = timed_event("distant-meeting", 10_000_000, 10_001_000);
        let snapshot = CalendarSnapshot::new(vec![distant_meeting, tomorrow_off.clone()]);

        assert_eq!(
            current_or_next_event(&snapshot, 1_000, "2026-08-11"),
            CurrentOrNext::Upcoming(&tomorrow_off)
        );
    }

    #[test]
    fn an_all_day_event_is_upcoming_when_nothing_timed_is() {
        let tomorrow_off = all_day_event("day-off", "2026-08-12", "2026-08-13");
        let past_meeting = timed_event("past", 0, 1_000);
        let snapshot = CalendarSnapshot::new(vec![tomorrow_off.clone(), past_meeting]);

        assert_eq!(
            current_or_next_event(&snapshot, 5_000, "2026-08-11"),
            CurrentOrNext::Upcoming(&tomorrow_off)
        );
    }

    #[test]
    fn dst_spring_forward_day_current_next_uses_real_elapsed_instants_not_wall_clock_hours() {
        // 2024-03-10: US spring-forward day (America/Los_Angeles loses the
        // 2:00-3:00am wall-clock hour). A meeting scheduled 9:00-9:30am
        // Pacific that day is a normal-length, correctly-computed instant
        // span regardless of the earlier transition — the timed arm only
        // ever compares instants, so the DST transition itself needs no
        // special handling here (it is baked into the offset Google sent).
        let meeting_start = 1_710_082_800_000; // 2024-03-10T09:00:00-07:00
        let meeting_end = 1_710_084_600_000; // 2024-03-10T09:30:00-07:00 (30 real minutes)
        let meeting = timed_event("dst-meeting", meeting_start, meeting_end);
        let snapshot = CalendarSnapshot::new(vec![meeting.clone()]);

        // Just before: upcoming.
        assert_eq!(
            current_or_next_event(&snapshot, meeting_start - 1, "2024-03-10"),
            CurrentOrNext::Upcoming(&meeting)
        );
        // During: in progress.
        assert_eq!(
            current_or_next_event(&snapshot, meeting_start + 60_000, "2024-03-10"),
            CurrentOrNext::InProgress(&meeting)
        );
        // At the exclusive end: no longer in progress, nothing upcoming.
        assert_eq!(
            current_or_next_event(&snapshot, meeting_end, "2024-03-10"),
            CurrentOrNext::None
        );
    }

    #[test]
    fn dst_fall_back_day_interval_query_still_returns_events_in_start_order() {
        // 2024-11-03: US fall-back day (America/Los_Angeles repeats the
        // 1:00-2:00am wall-clock hour). Two events that would be ambiguous
        // under naive wall-clock-only handling are unambiguous here because
        // start/end are already resolved instants.
        let first_1am = 1_730_620_800_000; // 2024-11-03T01:00:00-07:00 (pre-transition)
        let second_1am = 1_730_624_400_000; // 2024-11-03T01:00:00-08:00 (post-transition, same wall clock)
        let early = timed_event("first-1am", first_1am, first_1am + 1_800_000);
        let repeated = timed_event("second-1am", second_1am, second_1am + 1_800_000);
        let snapshot = CalendarSnapshot::new(vec![repeated.clone(), early.clone()]);

        let results = events_overlapping_interval(
            &snapshot,
            &timed_window(first_1am, second_1am + 1_800_000),
        );
        let ids: Vec<&str> = results
            .iter()
            .map(|event| event.provider_event_id.as_str())
            .collect();
        assert_eq!(ids, vec!["first-1am", "second-1am"]);
    }
}
