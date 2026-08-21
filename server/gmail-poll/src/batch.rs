//! Folding one poll's raw per-message fetches into events for evaluation
//! (#264 review item 4, as redrawn by #685). The three ways one message
//! can fail to contribute an `Event` are not the same failure mode, and
//! this module is what keeps them from sharing a branch:
//!
//! - **A transient fetch failure** (transport error, a 5xx, a timeout on
//!   `messages.get`) means the message likely still exists and may still
//!   match a rule. [`fold_messages`] returns `Err` on the first one,
//!   aborting the whole batch before `main.rs` ever calls `post_cursor`,
//!   so that message's id stays inside the next poll's `history.list`
//!   window rather than being lost the moment the cursor advances past
//!   it.
//! - **A message that is gone** (`messages.get` answers 404/410 for an id
//!   `history.list` still names — an ordinary mailbox deletion racing the
//!   poll window) is permanent. It is skipped, named in
//!   [`Batch::vanished`], and the batch proceeds.
//! - **An unparseable message** (a malformed body Gmail actually returned
//!   with a 200) is permanent for the same reason and handled the same
//!   way, named in [`Batch::unparseable`].
//!
//! **The first revision drew this split at fetch-vs-parse rather than at
//! transient-vs-permanent, and #685 is what that cost.** A deleted message
//! is a permanent failure that arrives on the *fetch* side; with no slot
//! for it, it fell to the aborting branch, the cursor could never advance
//! past it, and every subsequent poll replayed the identical window onto
//! the identical dead id — ~600 consecutive failed runs over six days,
//! doing precisely the thing this header had already named as the worse
//! outcome ("wedging the whole poller on one bad message forever would be
//! worse than losing that one event"). Permanence, not the layer the
//! failure surfaces at, is what decides whether a message may be skipped.
//!
//! Which statuses count as gone is `main.rs`'s call, not this module's:
//! only it sees an HTTP status, so it maps 404/410 onto `Ok(None)` and
//! leaves everything else an `Err`. That is the same division of labour
//! [`crate::resume`] already draws for the cursor-loss decision, kept for
//! the same reason — the decision stays natively testable here, and the
//! status-reading stays in the untestable edge.

use std::collections::BTreeSet;
use std::fmt;

use hummingbird_domain::Event;

use crate::{message_to_event, parse_message};

/// One poll's fold result: the events ready for [`crate::evaluate_events`],
/// plus each message id that contributed none, split by why — the ones
/// Gmail no longer has, and the ones it returned but that would not parse
/// (with the parse failure's own reason).
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    pub events: Vec<Event>,
    pub vanished: Vec<String>,
    pub unparseable: Vec<(String, String)>,
}

/// Folds `ids` into a [`Batch`] via `fetch`, which does the one real I/O
/// call this module cannot make itself (`messages.get`, injected so this
/// stays natively testable against stubbed outcomes). Duplicate ids
/// (`history.list`'s own pagination can repeat one) are fetched at most
/// once, the same de-duplication `main.rs`'s first revision already did.
///
/// `fetch` reports the three outcomes as `Ok(Some(json))`, `Ok(None)` —
/// the message is gone, and skipping it is the only way forward — and
/// `Err`, a transient failure. Only the last aborts, and it aborts on the
/// first one with never a partial [`Batch`], so the caller cannot post a
/// cursor advance past an id it might still have retrieved. Deciding
/// which HTTP statuses mean gone belongs to the caller (module doc).
pub fn fold_messages<E: fmt::Display>(
    ids: &[String],
    fetch: impl Fn(&str) -> Result<Option<String>, E>,
) -> Result<Batch, String> {
    let mut seen = BTreeSet::new();
    let mut events = Vec::new();
    let mut vanished = Vec::new();
    let mut unparseable = Vec::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(json) = fetch(id).map_err(|e| format!("fetch failed for {id}: {e}"))? else {
            vanished.push(id.clone());
            continue;
        };
        match parse_message(&json) {
            Ok(msg) => events.push(message_to_event(&msg)),
            Err(e) => unparseable.push((id.clone(), e.to_string())),
        }
    }
    Ok(Batch { events, vanished, unparseable })
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
        let batch = fold_messages(&ids, |id| Ok::<_, String>(Some(message_json(id, "hi")))).unwrap();
        assert_eq!(batch.events.len(), 2);
        assert!(batch.vanished.is_empty());
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
                Ok(Some(message_json(id, "hi")))
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
                Ok::<_, String>(Some("not json".to_string()))
            } else {
                Ok(Some(message_json(id, "hi")))
            }
        })
        .expect("an unparseable message must not abort the batch");
        assert_eq!(batch.events.len(), 1, "only the parseable message became an event");
        assert_eq!(batch.unparseable.len(), 1);
        assert_eq!(batch.unparseable[0].0, "m-1");
    }

    /// #685: the message `history.list` names but `messages.get` no
    /// longer has. It is permanent, so it must be skipped — named in
    /// `vanished` — while the rest of the batch still produces events and
    /// the call still returns `Ok`, which is what lets `main.rs` reach
    /// `post_cursor` and advance past it. Aborting here instead is what
    /// wedged the live poller for six days on one deleted message.
    #[test]
    fn a_message_gmail_no_longer_has_is_skipped_and_does_not_abort_the_batch() {
        let ids = vec!["m-1".to_string(), "gone".to_string(), "m-2".to_string()];
        let batch = fold_messages(&ids, |id| {
            if id == "gone" {
                Ok::<_, String>(None)
            } else {
                Ok(Some(message_json(id, "hi")))
            }
        })
        .expect("a vanished message must not abort the batch");
        assert_eq!(batch.events.len(), 2, "the surrounding messages still became events");
        assert_eq!(batch.vanished, vec!["gone".to_string()]);
        assert!(batch.unparseable.is_empty(), "gone is not the same as unparseable");
    }

    /// `history.list`'s own pagination can repeat an id across pages; it
    /// must be fetched (and counted) only once.
    #[test]
    fn a_duplicate_id_is_fetched_and_counted_only_once() {
        let ids = vec!["m-1".to_string(), "m-1".to_string()];
        let calls = std::cell::RefCell::new(0);
        let batch = fold_messages(&ids, |id| {
            *calls.borrow_mut() += 1;
            Ok::<_, String>(Some(message_json(id, "hi")))
        })
        .unwrap();
        assert_eq!(*calls.borrow(), 1, "fetched once despite appearing twice");
        assert_eq!(batch.events.len(), 1);
    }
}
