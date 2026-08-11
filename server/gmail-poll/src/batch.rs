//! Folding one poll's raw per-message fetches into events for evaluation
//! (#264 review item 4). The two ways one message can fail to contribute
//! an `Event` are not the same failure mode, and this module is what keeps
//! them from sharing a branch the way `main.rs`'s first revision did:
//!
//! - **A fetch failure** (transport error, a 5xx, a timeout on
//!   `messages.get`) is transient — the message likely still exists and
//!   may still match a rule. [`fold_messages`] returns `Err` on the first
//!   one, aborting the whole batch before `main.rs` ever calls
//!   `post_cursor`, so that message's id stays inside the next poll's
//!   `history.list` window rather than being lost the moment the cursor
//!   advances past it.
//! - **An unparseable message** (a malformed body Gmail actually returned
//!   with a 200) is permanent — retrying will not fix it. Skipping it
//!   lets the rest of the batch (and every future poll) proceed; wedging
//!   the whole poller on one bad message forever would be worse than
//!   losing that one event. It is still loud: every skip is named in
//!   [`Batch::unparseable`] for `main.rs` to log.

use std::collections::BTreeSet;
use std::fmt;

use hummingbird_domain::Event;

use crate::{message_to_event, parse_message};

/// One poll's fold result: the events ready for [`crate::evaluate_events`],
/// plus every message id that fetched fine but would not parse, alongside
/// why.
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    pub events: Vec<Event>,
    pub unparseable: Vec<(String, String)>,
}

/// Folds `ids` into a [`Batch`] via `fetch`, which does the one real I/O
/// call this module cannot make itself (`messages.get`, injected so this
/// stays natively testable against stubbed outcomes). Duplicate ids
/// (`history.list`'s own pagination can repeat one) are fetched at most
/// once, the same de-duplication `main.rs`'s first revision already did.
///
/// Returns `Err` on the first fetch failure — never a partial `Batch` — so
/// the caller cannot accidentally post a cursor advance past an id it
/// never actually retrieved.
pub fn fold_messages<E: fmt::Display>(
    ids: &[String],
    fetch: impl Fn(&str) -> Result<String, E>,
) -> Result<Batch, String> {
    let mut seen = BTreeSet::new();
    let mut events = Vec::new();
    let mut unparseable = Vec::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        let json = fetch(id).map_err(|e| format!("fetch failed for {id}: {e}"))?;
        match parse_message(&json) {
            Ok(msg) => events.push(message_to_event(&msg)),
            Err(e) => unparseable.push((id.clone(), e.to_string())),
        }
    }
    Ok(Batch { events, unparseable })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message_json(id: &str, subject: &str) -> String {
        format!(
            r#"{{
                "id": "{id}",
                "internalDate": "1691700000000",
                "payload": {{
                    "headers": [{{"name": "Subject", "value": "{subject}"}}],
                    "body": {{}},
                    "parts": []
                }},
                "labelIds": [],
                "snippet": ""
            }}"#
        )
    }

    /// Every id fetches and parses fine: every one becomes an event, and
    /// nothing is marked unparseable.
    #[test]
    fn every_id_fetching_and_parsing_cleanly_produces_one_event_each() {
        let ids = vec!["m-1".to_string(), "m-2".to_string()];
        let batch = fold_messages(&ids, |id| Ok::<_, String>(message_json(id, "hi"))).unwrap();
        assert_eq!(batch.events.len(), 2);
        assert!(batch.unparseable.is_empty());
    }

    /// AC4/review item 4: a transient fetch failure aborts the whole
    /// batch — `Err`, not a partial `Batch` missing that one event, which
    /// is what makes it impossible for `main.rs` to post a cursor advance
    /// past an id it never actually retrieved.
    #[test]
    fn a_fetch_failure_aborts_the_whole_batch_rather_than_dropping_that_one_message() {
        let ids = vec!["m-1".to_string(), "m-2".to_string()];
        let result = fold_messages(&ids, |id| {
            if id == "m-2" {
                Err("timeout".to_string())
            } else {
                Ok(message_json(id, "hi"))
            }
        });
        assert!(result.is_err(), "a fetch failure must not silently continue");
    }

    /// A message that fetches fine but will not parse is skipped, loudly
    /// (named in `unparseable`) but non-fatally — the rest of the batch
    /// still proceeds, so one permanently bad message can never wedge the
    /// poller.
    #[test]
    fn an_unparseable_message_is_skipped_loudly_and_does_not_abort_the_batch() {
        let ids = vec!["m-1".to_string(), "m-2".to_string()];
        let batch = fold_messages(&ids, |id| {
            if id == "m-1" {
                Ok::<_, String>("not json".to_string())
            } else {
                Ok(message_json(id, "hi"))
            }
        })
        .expect("an unparseable message must not abort the batch");
        assert_eq!(batch.events.len(), 1, "only the parseable message became an event");
        assert_eq!(batch.unparseable.len(), 1);
        assert_eq!(batch.unparseable[0].0, "m-1");
    }

    /// `history.list`'s own pagination can repeat an id across pages; it
    /// must be fetched (and counted) only once.
    #[test]
    fn a_duplicate_id_is_fetched_and_counted_only_once() {
        let ids = vec!["m-1".to_string(), "m-1".to_string()];
        let calls = std::cell::RefCell::new(0);
        let batch = fold_messages(&ids, |id| {
            *calls.borrow_mut() += 1;
            Ok::<_, String>(message_json(id, "hi"))
        })
        .unwrap();
        assert_eq!(*calls.borrow(), 1, "fetched once despite appearing twice");
        assert_eq!(batch.events.len(), 1);
    }
}
