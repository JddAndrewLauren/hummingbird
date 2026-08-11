//! Folding one poll's raw event bodies into evaluation candidates
//! (`gmail_poll::batch`'s own role). Unlike Gmail's `history.list` (ids
//! only, needing a separate `messages.get` per id), `events.list` already
//! returns each item's full body — so there is no per-item network call
//! here to fail transiently, and therefore no analogue of
//! `gmail_poll::batch::fold_messages`'s "abort the whole batch on a fetch
//! failure": the only network call in the evaluated-stream leg is the one
//! `events.list` page fetch itself, and `main.rs` already aborts the run
//! with `?` before ever calling `post_cursor` if that fails. What remains
//! decidable, and belongs here, is folding each already-fetched item into
//! either a [`crate::evaluate::Candidate`] or a named, non-fatal skip:
//!
//! - **Cancelled** (`status: "cancelled"`, Google's own deletion marker
//!   inside an incremental sync page) is expected and permanent — nothing
//!   to evaluate, nothing to retry.
//! - **Unparseable** (a malformed body Google actually returned with a
//!   200) is also permanent — retrying will not fix it. Skipping it lets
//!   the rest of the batch (and every future poll) proceed; wedging the
//!   whole poller on one bad item forever would be worse than losing that
//!   one event, `gmail_poll::batch`'s own reasoning.

use hummingbird_domain::Event;

use crate::calendar_event::{parse_calendar_event, ParsedCalendarEvent};
use crate::event::calendar_event_to_event;
use crate::evaluate::Candidate;

/// One poll's fold result: the candidates ready for
/// [`crate::evaluate::evaluate_events`], plus every item skipped and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    pub candidates: Vec<Candidate>,
    pub skipped: Vec<(String, String)>,
}

/// Folds `raw_events` (each item's own compact JSON text,
/// [`crate::sync::SyncPage::raw_events`]) into a [`Batch`]. Pure — no I/O,
/// unlike `gmail_poll::batch::fold_messages` — because `events.list`
/// already handed every item's full body to `main.rs`; there is nothing
/// left to fetch.
pub fn fold_events(raw_events: &[String]) -> Batch {
    let mut candidates = Vec::new();
    let mut skipped = Vec::new();
    for raw in raw_events {
        match parse_calendar_event(raw) {
            Ok(ParsedCalendarEvent::Live(evt)) => {
                let event: Event = calendar_event_to_event(&evt);
                candidates.push(Candidate { event, ends_at_ms: evt.ends_at_ms });
            }
            Ok(ParsedCalendarEvent::Cancelled(id)) => skipped.push((id, "cancelled".to_string())),
            Err(e) => skipped.push(("?".to_string(), e.to_string())),
        }
    }
    Batch { candidates, skipped }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event_json(id: &str) -> String {
        format!(
            r#"{{"id": "{id}", "status": "confirmed", "summary": "s",
                "start": {{"dateTime": "2026-08-15T09:00:00-07:00"}},
                "end": {{"dateTime": "2026-08-15T09:30:00-07:00"}}}}"#
        )
    }

    #[test]
    fn every_item_parsing_cleanly_produces_one_candidate_each() {
        let raw = vec![event_json("e1"), event_json("e2")];
        let batch = fold_events(&raw);
        assert_eq!(batch.candidates.len(), 2);
        assert!(batch.skipped.is_empty());
    }

    #[test]
    fn a_cancelled_item_is_skipped_loudly_and_does_not_abort_the_batch() {
        let raw = vec![r#"{"id": "e1", "status": "cancelled"}"#.to_string(), event_json("e2")];
        let batch = fold_events(&raw);
        assert_eq!(batch.candidates.len(), 1);
        assert_eq!(batch.skipped, vec![("e1".to_string(), "cancelled".to_string())]);
    }

    #[test]
    fn an_unparseable_item_is_skipped_loudly_and_does_not_abort_the_batch() {
        let raw = vec!["not json".to_string(), event_json("e2")];
        let batch = fold_events(&raw);
        assert_eq!(batch.candidates.len(), 1);
        assert_eq!(batch.skipped.len(), 1);
        assert_eq!(batch.skipped[0].0, "?");
    }

    #[test]
    fn a_candidate_carries_its_end_time_for_expires_at() {
        let raw = vec![event_json("e1")];
        let batch = fold_events(&raw);
        assert_eq!(batch.candidates[0].ends_at_ms, 1_786_811_400_000);
    }
}
