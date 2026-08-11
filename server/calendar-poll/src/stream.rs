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
            Err(e) => skipped.push((raw_event_id(raw), e.to_string())),
        }
    }
    Batch { candidates, skipped }
}

/// A best-effort id for a permanently-skipped, unparseable item — the
/// cursor advances past it either way, so this is the log's only trace.
/// `MissingField`/`BadTimestamp` both arrive from bodies that DO carry an
/// `"id"` (only some other field is missing or malformed), so reading it
/// off the raw JSON directly — bypassing the very parse that just failed —
/// recovers the real id in exactly those cases. `"?"` survives only when
/// even that read comes up empty (a body too malformed to hold an id at
/// all), `gmail_poll::batch`'s own "name the real id" discipline.
fn raw_event_id(raw: &str) -> String {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| v.get("id").and_then(|id| id.as_str()).map(str::to_string))
        .unwrap_or_else(|| "?".to_string())
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
        // Not even valid JSON, so there is no `id` to recover.
        assert_eq!(batch.skipped[0].0, "?");
    }

    #[test]
    fn an_unparseable_item_with_a_known_id_names_it_instead_of_a_placeholder() {
        // Valid JSON, missing `start`/`end` — `parse_calendar_event` fails
        // with `MissingField`, but the raw body still carries `"id"`.
        let raw = vec![r#"{"id": "e1", "status": "confirmed", "summary": "s"}"#.to_string()];
        let batch = fold_events(&raw);
        assert!(batch.candidates.is_empty());
        assert_eq!(batch.skipped.len(), 1);
        assert_eq!(batch.skipped[0].0, "e1");
    }

    #[test]
    fn a_candidate_carries_its_end_time_for_expires_at() {
        let raw = vec![event_json("e1")];
        let batch = fold_events(&raw);
        assert_eq!(batch.candidates[0].ends_at_ms, 1_786_811_400_000);
    }
}
