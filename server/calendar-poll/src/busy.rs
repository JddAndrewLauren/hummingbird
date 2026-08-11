//! The `busy_now` snapshot — this poller's **second job** in one poll
//! (#136's own addition to the #135 scaffolding), separate from the
//! evaluated-stream cursor: the current busy window's *boundaries*, not a
//! boolean.
//!
//! That is deliberate, and it is the part of the brief most likely to be
//! got wrong: the engine reads this row and compares `now` against the
//! stored boundaries **at its own evaluation time**, not the poll's — so a
//! poll-old snapshot still answers correctly between polls, the same
//! reason `city-waste`'s snapshot carries a `scheduled` date rather than a
//! `is_collection_today` boolean. A boolean captured at poll time would go
//! stale the instant the meeting it described ended.
//!
//! **Busy means a timed event in progress: `start_ms <= now_ms < end_ms`.**
//! Three exclusions never mark busy, each preventing over-suppression of a
//! notification the brief cares about ringing anyway: a transparent/free
//! event (the operator marked themselves available), a declined event (the
//! operator said no), and an all-day event (it says nothing about being in
//! a meeting right now).

use crate::calendar_event::{parse_calendar_event, CalendarEvent, ParsedCalendarEvent};

/// One busy window: the boundaries of whichever event is making the
/// operator busy right now. No clock is read here and none is stored —
/// [`busy_window`] takes `now_ms` from its caller and returns only the
/// event's own `start_ms`/`end_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusyWindow {
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Whether `event` counts toward busy **at all** — the brief's three
/// exclusions, each read off a typed field ([`CalendarEvent`]'s own parse)
/// rather than a raw string, so a future Google field-name or value change
/// fails loudly at `calendar_event.rs`'s parse, never silently here.
fn is_busy_candidate(event: &CalendarEvent) -> bool {
    !event.is_all_day
        && !event.is_transparent
        && event.self_response_status.as_deref() != Some("declined")
}

/// The current busy window among `events`, evaluated at `now_ms` — `None`
/// when nothing timed, opaque, and un-declined is in progress right now.
///
/// When more than one busy event covers `now_ms` (an overlap), the one with
/// the **latest end** wins: reporting an earlier end would understate how
/// long the operator stays unavailable, which is the direction a
/// notification-suppression consumer must never be wrong in. Ties (an
/// identical end) break on the earliest start, so the answer is
/// deterministic rather than dependent on iteration order.
pub fn busy_window(events: &[CalendarEvent], now_ms: i64) -> Option<BusyWindow> {
    events
        .iter()
        .filter(|e| is_busy_candidate(e) && e.starts_at_ms <= now_ms && now_ms < e.ends_at_ms)
        .map(|e| BusyWindow { start_ms: e.starts_at_ms, end_ms: e.ends_at_ms })
        .reduce(|best, candidate| {
            if candidate.end_ms > best.end_ms || (candidate.end_ms == best.end_ms && candidate.start_ms < best.start_ms) {
                candidate
            } else {
                best
            }
        })
}

/// Every live (non-cancelled, well-formed) event in `raw_events` — the
/// busy job's own parse, sharing [`parse_calendar_event`] with the
/// evaluated stream but keeping the typed [`CalendarEvent`] rather than
/// mapping onto an `Event`, since [`busy_window`] needs
/// `is_transparent`/`self_response_status`/`is_all_day`, none of which the
/// rule-engine `Event` shape carries. A cancelled or unparseable item is
/// silently excluded here (not an alert-worthy or cursor-affecting fact —
/// this query rides no cursor at all) rather than named, since the busy
/// answer only needs "what's true right now," not an audit of every item
/// the API returned.
pub fn live_calendar_events(raw_events: &[String]) -> Vec<CalendarEvent> {
    raw_events
        .iter()
        .filter_map(|raw| match parse_calendar_event(raw) {
            Ok(ParsedCalendarEvent::Live(evt)) => Some(*evt),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(start_ms: i64, end_ms: i64) -> CalendarEvent {
        CalendarEvent {
            id: "evt".into(),
            recurring_event_id: None,
            original_start_time: "x".into(),
            summary: "s".into(),
            description: None,
            location: None,
            html_link: None,
            organizer_email: None,
            attendee_emails: vec![],
            is_all_day: false,
            starts_at_ms: start_ms,
            ends_at_ms: end_ms,
            is_transparent: false,
            self_response_status: None,
        }
    }

    /// AC: a timed event with `start <= now < end` marks busy.
    #[test]
    fn a_timed_event_in_progress_marks_busy() {
        let e = event(1000, 2000);
        assert_eq!(busy_window(&[e], 1500), Some(BusyWindow { start_ms: 1000, end_ms: 2000 }));
    }

    #[test]
    fn an_event_that_has_not_started_or_already_ended_is_not_busy() {
        let e = event(1000, 2000);
        assert_eq!(busy_window(std::slice::from_ref(&e), 999), None);
        assert_eq!(busy_window(&[e], 2000), None, "end is exclusive");
    }

    /// AC: exactly at `start_ms`, busy — the boundary is inclusive on the
    /// low end.
    #[test]
    fn the_start_boundary_is_inclusive() {
        let e = event(1000, 2000);
        assert_eq!(busy_window(&[e], 1000), Some(BusyWindow { start_ms: 1000, end_ms: 2000 }));
    }

    /// AC: a transparent/free event never marks busy.
    #[test]
    fn a_transparent_event_never_marks_busy() {
        let mut e = event(1000, 2000);
        e.is_transparent = true;
        assert_eq!(busy_window(&[e], 1500), None);
    }

    /// AC: a declined event never marks busy.
    #[test]
    fn a_declined_event_never_marks_busy() {
        let mut e = event(1000, 2000);
        e.self_response_status = Some("declined".to_string());
        assert_eq!(busy_window(&[e], 1500), None);
    }

    /// AC: an all-day event never marks busy.
    #[test]
    fn an_all_day_event_never_marks_busy() {
        let mut e = event(1000, 2000);
        e.is_all_day = true;
        assert_eq!(busy_window(&[e], 1500), None);
    }

    /// AC: a snapshot written before `now` (boundaries captured at poll
    /// time) still yields the correct busy answer when evaluated later,
    /// against a fresh `now_ms` — nothing about `busy_window` depends on
    /// when it is called relative to the poll.
    #[test]
    fn a_stored_window_answers_correctly_at_a_later_now() {
        let e = event(1000, 5000);
        let at_poll_time = busy_window(std::slice::from_ref(&e), 1200).unwrap();
        // The engine reads the window later and compares against ITS OWN
        // now, not the poll's — simulated here by re-checking the same
        // boundaries at a later instant still inside the window.
        assert!(at_poll_time.start_ms <= 3000 && 3000 < at_poll_time.end_ms);
        // ...and correctly answers "no longer busy" once past the end.
        assert!(!(at_poll_time.start_ms <= 6000 && 6000 < at_poll_time.end_ms));
    }

    #[test]
    fn overlapping_busy_events_report_the_latest_end() {
        let short = event(1000, 2000);
        let long = event(1200, 4000);
        assert_eq!(busy_window(&[short, long], 1500), Some(BusyWindow { start_ms: 1200, end_ms: 4000 }));
    }

    #[test]
    fn no_busy_event_covering_now_is_none() {
        assert_eq!(busy_window(&[], 1500), None);
    }

    #[test]
    fn live_calendar_events_drops_cancelled_and_unparseable_items() {
        let raw = vec![
            r#"{"id":"e1","status":"cancelled"}"#.to_string(),
            "not json".to_string(),
            r#"{"id":"e2","status":"confirmed","summary":"s","start":{"dateTime":"2026-08-15T09:00:00-07:00"},"end":{"dateTime":"2026-08-15T09:30:00-07:00"}}"#.to_string(),
        ];
        let events = live_calendar_events(&raw);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "e2");
    }
}
