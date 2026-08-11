//! Folding one poll's raw calendar-event bodies into evaluation candidates
//! — `mail_stream.rs`'s own pattern, for
//! [`crate::calendar_item::parse_calendar_item`]. No per-item network call
//! is needed here either (`mail_stream.rs`'s module doc applies verbatim):
//! Graph's `calendarView/delta` already returns each item's full body.
//!
//! - **Removed** (Graph's own `@removed` delta marker — a deletion or a
//!   cancellation) is expected and permanent.
//! - **Unparseable** is also permanent.

use crate::calendar_event::calendar_item_to_candidate;
use crate::calendar_item::{parse_calendar_item, ParsedCalendarItem};
use crate::evaluate::Candidate;

#[derive(Debug, Clone, PartialEq)]
pub struct CalendarBatch {
    pub candidates: Vec<Candidate>,
    pub skipped: Vec<(String, String)>,
}

/// Folds `raw_items` into a [`CalendarBatch`]. Pure — no I/O.
pub fn fold_calendar_items(raw_items: &[String]) -> CalendarBatch {
    let mut candidates = Vec::new();
    let mut skipped = Vec::new();
    for raw in raw_items {
        match parse_calendar_item(raw) {
            Ok(ParsedCalendarItem::Live(evt)) => candidates.push(calendar_item_to_candidate(&evt)),
            Ok(ParsedCalendarItem::Removed(id)) => skipped.push((id, "removed".to_string())),
            Err(e) => skipped.push(("?".to_string(), e.to_string())),
        }
    }
    CalendarBatch { candidates, skipped }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event_json(id: &str) -> String {
        format!(
            r#"{{"id": "{id}", "subject": "s",
                "start": {{"dateTime": "2026-08-15T09:00:00.0000000", "timeZone": "UTC"}},
                "end": {{"dateTime": "2026-08-15T09:30:00.0000000", "timeZone": "UTC"}}}}"#
        )
    }

    #[test]
    fn every_item_parsing_cleanly_produces_one_candidate_each() {
        let raw = vec![event_json("e1"), event_json("e2")];
        let batch = fold_calendar_items(&raw);
        assert_eq!(batch.candidates.len(), 2);
        assert!(batch.skipped.is_empty());
    }

    #[test]
    fn a_removed_item_is_skipped_loudly_and_does_not_abort_the_batch() {
        let raw = vec![r#"{"id": "e1", "@removed": {"reason": "deleted"}}"#.to_string(), event_json("e2")];
        let batch = fold_calendar_items(&raw);
        assert_eq!(batch.candidates.len(), 1);
        assert_eq!(batch.skipped, vec![("e1".to_string(), "removed".to_string())]);
    }

    #[test]
    fn an_unparseable_item_is_skipped_loudly_and_does_not_abort_the_batch() {
        let raw = vec!["not json".to_string(), event_json("e2")];
        let batch = fold_calendar_items(&raw);
        assert_eq!(batch.candidates.len(), 1);
        assert_eq!(batch.skipped.len(), 1);
    }

    #[test]
    fn a_candidate_carries_its_end_time_for_expires_at() {
        let raw = vec![event_json("e1")];
        let batch = fold_calendar_items(&raw);
        assert_eq!(batch.candidates[0].ends_at_ms, Some(1_786_786_200_000));
    }
}
