//! The two read queries against a [`CalendarSnapshot`]: `events overlapping
//! interval` and `current/next event`. Both take an explicit `now`/interval
//! from the caller — the core never samples a clock itself (same discipline
//! as [`crate::storage::Envelope::as_of`]), which is what keeps these
//! queries deterministic and testable against hand-built snapshots.

use super::event::EventRecord;
use super::snapshot::CalendarSnapshot;

/// A half-open UTC millisecond interval `[start_ms, end_ms)`.
///
/// Half-open so that an event ending exactly when another starts (an
/// abutment) does not count as overlapping either — the same convention
/// providers use for exclusive all-day end dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start_ms: i64,
    pub end_ms: i64,
}

impl Interval {
    pub fn new(start_ms: i64, end_ms: i64) -> Self {
        Self { start_ms, end_ms }
    }
}

/// Returns every event in `snapshot` whose `[start, end)` overlaps
/// `interval`'s `[start, end)`, in local-time order (ascending by start
/// instant, ties broken by end instant then by `provider_event_id` for a
/// stable, deterministic order).
pub fn events_overlapping_interval(
    snapshot: &CalendarSnapshot,
    interval: Interval,
) -> Vec<&EventRecord> {
    let mut matches: Vec<&EventRecord> = snapshot
        .events
        .iter()
        .filter(|event| {
            event.start.instant_ms < interval.end_ms && event.end.instant_ms > interval.start_ms
        })
        .collect();

    matches.sort_by(|a, b| {
        a.start
            .instant_ms
            .cmp(&b.start.instant_ms)
            .then(a.end.instant_ms.cmp(&b.end.instant_ms))
            .then(a.provider_event_id.cmp(&b.provider_event_id))
    });

    matches
}

/// The result of [`current_or_next_event`].
#[derive(Debug, PartialEq, Eq)]
pub enum CurrentOrNext<'a> {
    /// An event whose `[start, end)` contains `now`. When more than one
    /// event is in progress, the one that started first wins (ties broken
    /// by the one ending soonest).
    InProgress(&'a EventRecord),
    /// No event is in progress; this is the soonest event starting after
    /// `now`.
    Upcoming(&'a EventRecord),
    /// Nothing in progress and nothing upcoming.
    None,
}

/// Finds the in-progress event (if any), else the soonest upcoming event,
/// as of `now_ms`.
pub fn current_or_next_event(snapshot: &CalendarSnapshot, now_ms: i64) -> CurrentOrNext<'_> {
    let in_progress = snapshot
        .events
        .iter()
        .filter(|event| event.start.instant_ms <= now_ms && event.end.instant_ms > now_ms)
        .min_by(|a, b| {
            a.start
                .instant_ms
                .cmp(&b.start.instant_ms)
                .then(a.end.instant_ms.cmp(&b.end.instant_ms))
                .then(a.provider_event_id.cmp(&b.provider_event_id))
        });

    if let Some(event) = in_progress {
        return CurrentOrNext::InProgress(event);
    }

    let upcoming = snapshot
        .events
        .iter()
        .filter(|event| event.start.instant_ms > now_ms)
        .min_by(|a, b| {
            a.start
                .instant_ms
                .cmp(&b.start.instant_ms)
                .then(a.provider_event_id.cmp(&b.provider_event_id))
        });

    match upcoming {
        Some(event) => CurrentOrNext::Upcoming(event),
        None => CurrentOrNext::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::event::{EventStatus, EventTime};

    fn timed_event(id: &str, start_ms: i64, end_ms: i64) -> EventRecord {
        EventRecord {
            provider_event_id: id.to_string(),
            calendar_id: "cal-primary".to_string(),
            title: id.to_string(),
            start: EventTime::timed(start_ms, "America/Los_Angeles"),
            end: EventTime::timed(end_ms, "America/Los_Angeles"),
            all_day: false,
            recurrence_id: None,
            location: None,
            organizer: None,
            status: EventStatus::Confirmed,
            provider_updated_at_ms: start_ms,
            html_link: None,
        }
    }

    fn all_day_event(id: &str, start_ms: i64, end_ms: i64) -> EventRecord {
        EventRecord {
            all_day: true,
            start: EventTime::all_day(start_ms),
            end: EventTime::all_day(end_ms),
            ..timed_event(id, start_ms, end_ms)
        }
    }

    // -- events_overlapping_interval -----------------------------------

    #[test]
    fn interval_query_returns_events_in_local_time_order() {
        let snapshot = CalendarSnapshot::new(vec![
            timed_event("afternoon", 3_000, 4_000),
            timed_event("morning", 1_000, 2_000),
        ]);

        let results = events_overlapping_interval(&snapshot, Interval::new(0, 5_000));
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

        let results = events_overlapping_interval(&snapshot, Interval::new(1_000, 5_000));

        assert_eq!(results, vec![&overlapping]);
    }

    #[test]
    fn two_events_that_abut_each_other_both_overlap_a_query_spanning_the_boundary() {
        let first = timed_event("first", 1_000, 2_000);
        let second = timed_event("second", 2_000, 3_000);
        let snapshot = CalendarSnapshot::new(vec![first.clone(), second.clone()]);

        let results = events_overlapping_interval(&snapshot, Interval::new(1_500, 2_500));

        assert_eq!(results, vec![&first, &second]);
    }

    #[test]
    fn empty_snapshot_returns_no_events() {
        let snapshot = CalendarSnapshot::new(vec![]);
        let results = events_overlapping_interval(&snapshot, Interval::new(0, 1_000));
        assert!(results.is_empty());
    }

    #[test]
    fn all_day_event_boundary_is_exclusive_like_a_timed_event() {
        // A one-day all-day event for Aug 10 (UTC midnight boundaries).
        let day_ms = 24 * 60 * 60 * 1000;
        let aug_10_start = 19_579 * day_ms; // arbitrary epoch-day anchor
        let event = all_day_event("aug-10", aug_10_start, aug_10_start + day_ms);
        let snapshot = CalendarSnapshot::new(vec![event.clone()]);

        // A query for the exact same day overlaps.
        let same_day = events_overlapping_interval(
            &snapshot,
            Interval::new(aug_10_start, aug_10_start + day_ms),
        );
        assert_eq!(same_day, vec![&event]);

        // A query for the day after (exclusive end abuts) does not.
        let next_day = events_overlapping_interval(
            &snapshot,
            Interval::new(aug_10_start + day_ms, aug_10_start + 2 * day_ms),
        );
        assert!(next_day.is_empty());
    }

    // -- current_or_next_event -------------------------------------------

    #[test]
    fn in_progress_event_wins_over_an_upcoming_one() {
        let current = timed_event("current", 1_000, 3_000);
        let upcoming = timed_event("upcoming", 5_000, 6_000);
        let snapshot = CalendarSnapshot::new(vec![current.clone(), upcoming]);

        assert_eq!(
            current_or_next_event(&snapshot, 2_000),
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
            current_or_next_event(&snapshot, 2_000),
            CurrentOrNext::Upcoming(&soonest)
        );
    }

    #[test]
    fn none_when_snapshot_is_empty() {
        let snapshot = CalendarSnapshot::new(vec![]);
        assert_eq!(current_or_next_event(&snapshot, 1_000), CurrentOrNext::None);
    }

    #[test]
    fn none_when_every_event_is_in_the_past() {
        let past = timed_event("past", 0, 1_000);
        let snapshot = CalendarSnapshot::new(vec![past]);
        assert_eq!(current_or_next_event(&snapshot, 5_000), CurrentOrNext::None);
    }

    #[test]
    fn all_day_event_is_in_progress_for_any_instant_inside_its_date_span() {
        let day_ms = 24 * 60 * 60 * 1000;
        let aug_10_start = 19_579 * day_ms;
        let holiday = all_day_event("holiday", aug_10_start, aug_10_start + day_ms);
        let snapshot = CalendarSnapshot::new(vec![holiday.clone()]);

        // Noon on Aug 10.
        let noon = aug_10_start + 12 * 60 * 60 * 1000;
        assert_eq!(
            current_or_next_event(&snapshot, noon),
            CurrentOrNext::InProgress(&holiday)
        );

        // Exactly at the exclusive end (start of Aug 11): no longer in progress.
        assert_eq!(
            current_or_next_event(&snapshot, aug_10_start + day_ms),
            CurrentOrNext::None
        );
    }

    #[test]
    fn dst_spring_forward_day_current_next_uses_real_elapsed_instants_not_wall_clock_hours() {
        // 2024-03-10: US spring-forward day (America/Los_Angeles loses the
        // 2:00-3:00am wall-clock hour). A meeting scheduled 9:00-9:30am
        // Pacific that day is a normal-length, correctly-computed instant
        // span regardless of the earlier transition — the query only ever
        // compares instants, so the DST transition itself needs no special
        // handling here (it is baked into the adapter-computed instant_ms
        // this hand-built snapshot stands in for).
        let meeting_start = 1_710_082_800_000; // 2024-03-10T09:00:00-07:00
        let meeting_end = 1_710_084_600_000; // 2024-03-10T09:30:00-07:00 (30 real minutes)
        let meeting = timed_event("dst-meeting", meeting_start, meeting_end);
        let snapshot = CalendarSnapshot::new(vec![meeting.clone()]);

        // Just before: upcoming.
        assert_eq!(
            current_or_next_event(&snapshot, meeting_start - 1),
            CurrentOrNext::Upcoming(&meeting)
        );
        // During: in progress.
        assert_eq!(
            current_or_next_event(&snapshot, meeting_start + 60_000),
            CurrentOrNext::InProgress(&meeting)
        );
        // At the exclusive end: no longer in progress, nothing upcoming.
        assert_eq!(
            current_or_next_event(&snapshot, meeting_end),
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
            Interval::new(first_1am, second_1am + 1_800_000),
        );
        let ids: Vec<&str> = results
            .iter()
            .map(|event| event.provider_event_id.as_str())
            .collect();
        assert_eq!(ids, vec!["first-1am", "second-1am"]);
    }
}
