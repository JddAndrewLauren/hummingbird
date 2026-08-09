//! [`OutboundQueue`]: the durable FIFO outbound queue (ADR-0003, #102) that
//! drains what the write adapter (S3/#101) encodes.
//!
//! **Durable before sent.** [`OutboundQueue::enqueue`] only ever mutates the
//! in-memory value; it is the caller's job to run it through
//! [`crate::storage::save_snapshot`] *before* ever calling
//! [`OutboundQueue::drain`] — that ordering, not anything inside this type,
//! is what ADR-0003 means by "a change made offline survives a crash". This
//! module never touches storage itself, the same separation
//! [`super::mirror::SyncMirror`] already keeps between the read model and
//! its persistence.
//!
//! **Strict FIFO, one writer.** [`OutboundQueue::drain`] attempts entries
//! front-to-back and stops the instant one can't be resolved right now: a
//! [`WriteError::Retryable`] blocks the queue (this entry and everything
//! after it stay untouched, for the next drain to retry from the same
//! place) and a [`WriteError::Unauthorized`] halts it entirely, emitting
//! exactly one [`DrainOutcome::CredentialNeeded`] — never one per remaining
//! entry, and never an attempt at anything past position 1. Only a
//! [`WriteError::Permanent`], an unresolved [`WriteError::Conflict`], or a
//! malformed response ([`WriteError::InvalidResponse`]) — nothing further
//! can fix any of the three by retrying unchanged — moves an entry to the
//! dead-letter journal and lets the drain continue. This is what makes
//! create-before-update ordering hold without a separate proof: a queue
//! that skipped ahead on a transient failure could send an update before
//! the create it depends on.
//!
//! Per ADR-0010 (#97's blocker cleared by PR #125), the queue is
//! single-writer by construction — one `SharedWorker` per origin, one
//! queue, no rival drain to race against. There is no compare-and-swap or
//! lock in this type; none is needed.
//!
//! **Crash-replay safety is inherited, not re-derived.** A crash between a
//! mutation landing at the authority and this queue durably recording that
//! it no longer needs resending is a gap this type does not try to close —
//! closing it would need a distributed transaction this app doesn't have.
//! Instead, replaying the *same* durably-stored entry against the authority
//! a second time must be safe, and that is exactly what #101 already
//! guarantees: a create is idempotent by its deterministic id, and a patch
//! that already landed rebases as [`super::write::RebaseDecision::Safe`]
//! ("already achieved") and reapplies as a no-op. `drain` never rebuilds an
//! entry's body from live state on retry — it resends the identical
//! `patch_fields`/`base` this entry was enqueued with — which is what makes
//! that guarantee apply here.
//!
//! **Conflict metadata for S5.** Each [`MutationIntent::Patch`] carries
//! `base_updated_at` and the touched fields ([`MutationIntent::touched_fields`])
//! — nothing in this slice reads them, but ADR-0007's field-level conflict
//! resolution (S5) needs both to decide whose edit wins, and they are
//! cheapest to capture at enqueue time, before anything about the item's
//! state can drift.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::storage::{Persistable, PersistableSealed};

use super::write::adapter::{create, patch_with_rebase};
use super::write::taxonomy::WriteError;
use super::write::transport::{HttpMethod, MutationTransport};

/// The schema version written into this queue's snapshot envelope.
///
/// Bump it when the payload shape changes. The failure mode it guards
/// against is *not* silent: unlike [`super::mirror::SyncMirror`] (an empty
/// result there is harmless — the next sweep refills it), an outbound queue
/// that "loaded empty" after a schema bump is silent loss of captures the
/// user made while offline. New fields on [`OutboundQueue`] or
/// [`MutationIntent`] must carry `#[serde(default)]` (or an equivalent
/// backward-compatible default) so that old, already-durable bytes keep
/// deserialising into the same entries rather than failing, or worse,
/// degrading into an empty queue.
pub const QUEUE_SCHEMA_VERSION: u32 = 1;

/// One durable outbound mutation, before the transport is ever called.
///
/// Deliberately holds `serde_json::Value` rather than any
/// `hummingbird_domain` DTO: the queue is generic over every entity in
/// ADR-0009's write vocabulary the same way [`super::write::adapter`] is,
/// and a `Value` is what both [`create`] and [`patch_with_rebase`] already
/// accept once instantiated at `Value` for their type parameters — there is
/// no per-entity queue variant to keep in sync as the write vocabulary
/// grows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MutationIntent {
    /// A `POST` create: idempotent by the DTO's own client-supplied id
    /// (#101) — replaying it is always safe.
    Create {
        /// e.g. `/api/items` — see [`super::write::paths`].
        path: String,
        /// The full create DTO, as sent on the wire.
        body: Value,
    },
    /// A CAS `PATCH`/`PUT`.
    Patch {
        /// e.g. `/api/items/<id>` — see [`super::write::paths`].
        path: String,
        method: HttpMethod,
        /// The entity as this client last knew it, for
        /// [`super::write::rebase::decide`] to diff a 409 against. Captured
        /// once, at enqueue time — a replay reuses this same value rather
        /// than re-reading live state, which is what keeps a replay
        /// identical to the original attempt.
        base: Value,
        /// The item's base update time — ADR-0007's field-level conflict
        /// resolution (S5) material, alongside [`MutationIntent::touched_fields`].
        base_updated_at: i64,
        /// The touched fields and their intended values, `expected_version`
        /// deliberately excluded — `drain` fills that in from whatever
        /// version is live at send time (the original attempt's, or a 409's
        /// retry version).
        patch_fields: Value,
    },
}

impl MutationIntent {
    /// The fields this mutation touches — ADR-0007's field-level conflict
    /// metadata for S5. Derived from `patch_fields`'s own keys rather than
    /// stored separately, so it can never drift from what is actually sent
    /// on the wire. A create touches no existing entity's fields.
    pub fn touched_fields(&self) -> Vec<String> {
        match self {
            MutationIntent::Create { .. } => Vec::new(),
            MutationIntent::Patch { patch_fields, .. } => patch_fields
                .as_object()
                .map(|fields| fields.keys().cloned().collect())
                .unwrap_or_default(),
        }
    }

    /// The path this intent targets, regardless of which variant it is.
    pub fn path(&self) -> &str {
        match self {
            MutationIntent::Create { path, .. } => path,
            MutationIntent::Patch { path, .. } => path,
        }
    }
}

/// One queued entry: an intent, plus an id for tracking and logging (not
/// used for FIFO ordering — position in [`OutboundQueue`]'s own deque is).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueEntry {
    pub id: String,
    pub intent: MutationIntent,
}

/// Why an entry was moved to the dead-letter journal rather than retried —
/// the client-side twin of the sweeper's terminal-rejection quarantine
/// (`sweep.py::_is_terminal`), generalized from [`crate::task::deadletter`]'s
/// "losing != vanishing" to the owned-schema write vocabulary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeadLetterReason {
    /// No retry can fix it unchanged: any 4xx other than 401, or a response
    /// body that was not the JSON shape its status promised
    /// ([`WriteError::InvalidResponse`]) — a malformed body is a server-side
    /// defect no client retry resolves either.
    Permanent(String),
    /// A 409 that did not auto-resolve: the fields where the intervening
    /// write and this entry's own intent disagree.
    Conflict { fields: Vec<String> },
}

/// One dead-lettered entry: the abandoned intent plus why, so it can be
/// re-applied by hand or inspected — never silently dropped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeadLetterEntry {
    pub entry: QueueEntry,
    pub reason: DeadLetterReason,
}

/// What one [`OutboundQueue::drain`] call did.
#[derive(Debug, Clone, PartialEq)]
pub enum DrainOutcome {
    /// Every entry queued at the start of this call was attempted.
    /// Permanent failures and unresolved conflicts along the way landed in
    /// the dead-letter journal, but nothing blocked the queue itself.
    Completed { dead_lettered: usize },
    /// A retryable failure blocked the queue: the entry it failed on, and
    /// everything after it, are untouched and unattempted — still at the
    /// front of the queue for the next `drain` to retry from the same
    /// place.
    Blocked { dead_lettered: usize },
    /// A 401 halted the queue entirely: nothing at or after the failing
    /// entry was attempted. Exactly one of these is ever produced per
    /// `drain` call, however many entries remained — a credential event is
    /// one fact ("this queue needs a fresh token"), not one per entry it
    /// would have touched.
    CredentialNeeded,
}

/// The durable FIFO outbound queue plus its dead-letter journal — one
/// persisted value, the same "atomically or not at all" reasoning
/// [`crate::task::mirror::Mirror`] already applies to keeping a journal
/// alongside the state it journals about: an entry that dead-letters and
/// the queue it leaves are one fact, and must commit or roll back together.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OutboundQueue {
    entries: VecDeque<QueueEntry>,
    /// `#[serde(default)]`: see [`QUEUE_SCHEMA_VERSION`] — bytes captured
    /// before this field existed must still deserialise, with an empty
    /// journal, not fail.
    #[serde(default)]
    dead_letters: Vec<DeadLetterEntry>,
}

impl PersistableSealed for OutboundQueue {}
impl Persistable for OutboundQueue {}

impl OutboundQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one entry to the back of the queue. The caller must persist
    /// the queue (`save_snapshot`) before this entry is ever handed to
    /// `drain` — see the module docs.
    pub fn enqueue(&mut self, entry: QueueEntry) {
        self.entries.push_back(entry);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The queue's current FIFO order, oldest (next to attempt) first.
    pub fn entries(&self) -> impl Iterator<Item = &QueueEntry> {
        self.entries.iter()
    }

    /// The dead-letter journal, oldest first. Acknowledging (dropping) an
    /// entry is intentionally not offered here yet — this slice only builds
    /// the durable structure and the drain semantics; the "1 edit didn't
    /// apply" affordance that lets a user act on this journal is a later
    /// UI-facing concern.
    pub fn dead_letters(&self) -> &[DeadLetterEntry] {
        &self.dead_letters
    }

    /// Strict FIFO drain: see the module docs for exactly what stops it and
    /// what merely dead-letters and continues.
    pub async fn drain(
        &mut self,
        transport: &impl MutationTransport,
        access_token: &str,
    ) -> DrainOutcome {
        let mut dead_lettered = 0;

        loop {
            let Some(entry) = self.entries.front() else {
                return DrainOutcome::Completed { dead_lettered };
            };

            match attempt(transport, access_token, &entry.intent).await {
                Ok(()) => {
                    self.entries.pop_front();
                }
                Err(WriteError::Retryable(_)) => {
                    return DrainOutcome::Blocked { dead_lettered };
                }
                Err(WriteError::Unauthorized) => {
                    return DrainOutcome::CredentialNeeded;
                }
                Err(WriteError::Permanent(message)) => {
                    self.dead_letter(DeadLetterReason::Permanent(message));
                    dead_lettered += 1;
                }
                Err(WriteError::InvalidResponse(message)) => {
                    self.dead_letter(DeadLetterReason::Permanent(message));
                    dead_lettered += 1;
                }
                Err(WriteError::Conflict { fields, .. }) => {
                    self.dead_letter(DeadLetterReason::Conflict { fields });
                    dead_lettered += 1;
                }
            }
        }
    }

    /// Moves the front entry to the dead-letter journal. Only ever called
    /// from `drain`, immediately after that same entry's `attempt` resolved
    /// to a non-retryable failure — the front entry is always the one just
    /// attempted.
    fn dead_letter(&mut self, reason: DeadLetterReason) {
        if let Some(entry) = self.entries.pop_front() {
            self.dead_letters.push(DeadLetterEntry { entry, reason });
        }
    }
}

/// Drives one entry's intent through the write adapter, discarding the
/// parsed response entity — `drain` only needs success-or-`WriteError`, and
/// `Value` (which every entity DTO serialises to and deserialises from) is
/// what lets this stay generic without a per-entity match arm.
async fn attempt(
    transport: &impl MutationTransport,
    access_token: &str,
    intent: &MutationIntent,
) -> Result<(), WriteError> {
    match intent {
        MutationIntent::Create { path, body } => {
            create::<Value, Value>(transport, access_token, path, body)
                .await
                .map(|_| ())
        }
        MutationIntent::Patch {
            path,
            method,
            base,
            patch_fields,
            ..
        } => {
            let patch_fields = patch_fields.clone();
            let build_patch = move |version: i64| {
                let mut patch = patch_fields.clone();
                if let Value::Object(fields) = &mut patch {
                    fields.insert("expected_version".to_string(), json!(version));
                }
                patch
            };

            patch_with_rebase::<Value, Value>(transport, access_token, *method, path, base, build_patch)
                .await
                .map(|_| ())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{load_snapshot, save_snapshot, MemorySnapshotStore};
    use crate::sync::write::transport::{RawResponse, TransportError};
    use serde_json::json;
    use std::sync::Mutex;

    /// A transport scripted to return one response per call, in enqueue
    /// order — so a fixture test never touches the network or a live
    /// credential (this issue's "every test runs with no live credentials
    /// and no network" acceptance).
    struct ScriptedTransport {
        responses: Mutex<VecDeque<Result<RawResponse, TransportError>>>,
        calls: Mutex<Vec<super::super::write::transport::MutationRequest>>,
    }

    impl ScriptedTransport {
        fn new(responses: Vec<Result<RawResponse, TransportError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().collect()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    fn ok(status: u16, body: impl Into<String>) -> Result<RawResponse, TransportError> {
        Ok(RawResponse {
            status,
            body: body.into(),
        })
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl MutationTransport for ScriptedTransport {
        async fn send(
            &self,
            _access_token: &str,
            request: super::super::write::transport::MutationRequest,
        ) -> Result<RawResponse, TransportError> {
            self.calls.lock().unwrap().push(request);
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| panic!("no more scripted responses"))
        }
    }

    fn create_entry(id: &str, item_id: &str) -> QueueEntry {
        QueueEntry {
            id: id.to_string(),
            intent: MutationIntent::Create {
                path: "/api/items".to_string(),
                body: json!({"id": item_id, "title": format!("item {item_id}")}),
            },
        }
    }

    fn patch_entry(id: &str, item_id: &str, base_version: i64) -> QueueEntry {
        QueueEntry {
            id: id.to_string(),
            intent: MutationIntent::Patch {
                path: format!("/api/items/{item_id}"),
                method: HttpMethod::Patch,
                base: json!({"id": item_id, "title": "buy milk", "version": base_version}),
                base_updated_at: 1_000,
                patch_fields: json!({"title": "buy oat milk"}),
            },
        }
    }

    // --------------------------------------------------- durability first

    /// #102 acceptance: "The queue is written and readable back before the
    /// transport is ever called." `enqueue` plus `save_snapshot` never
    /// touch a transport at all — proving durability does not depend on a
    /// network call having happened.
    #[tokio::test]
    async fn the_queue_is_durable_before_any_transport_call() {
        let store = MemorySnapshotStore::default();
        let mut queue = OutboundQueue::new();
        queue.enqueue(create_entry("m-1", "a-1"));

        save_snapshot(&store, QUEUE_SCHEMA_VERSION, 1, queue.clone())
            .await
            .unwrap();

        let loaded: OutboundQueue = load_snapshot(&store).await.unwrap().unwrap().payload;
        assert_eq!(loaded.len(), 1, "the entry must be readable back untouched");
        assert_eq!(loaded.entries().next().unwrap().id, "m-1");
    }

    // --------------------------------------------------------- strict FIFO

    /// #102 acceptance: "A retryable failure at position 2 leaves positions
    /// 2..n untouched and unattempted."
    #[tokio::test]
    async fn a_retryable_failure_at_position_two_blocks_everything_after_it() {
        let transport = ScriptedTransport::new(vec![
            ok(201, r#"{"id":"a-1","version":1}"#), // position 1: succeeds
            ok(503, ""),                            // position 2: retryable
        ]);
        let mut queue = OutboundQueue::new();
        queue.enqueue(create_entry("m-1", "a-1"));
        queue.enqueue(create_entry("m-2", "a-2"));
        queue.enqueue(create_entry("m-3", "a-3"));

        let outcome = queue.drain(&transport, "token").await;

        assert_eq!(outcome, DrainOutcome::Blocked { dead_lettered: 0 });
        assert_eq!(transport.call_count(), 2, "position 3 must never be attempted");
        assert_eq!(queue.len(), 2, "positions 2 and 3 remain queued");
        assert_eq!(
            queue.entries().next().unwrap().id,
            "m-2",
            "the blocked entry stays at the front for the next drain"
        );
    }

    // ------------------------------------------------------ dead-lettering

    /// #102 acceptance: "A permanent failure at position 2 dead-letters it
    /// and goes on to attempt position 3."
    #[tokio::test]
    async fn a_permanent_failure_at_position_two_dead_letters_and_continues() {
        let transport = ScriptedTransport::new(vec![
            ok(201, r#"{"id":"a-1","version":1}"#), // position 1: succeeds
            ok(400, r#"{"error":"validation"}"#),   // position 2: permanent
            ok(201, r#"{"id":"a-3","version":1}"#), // position 3: succeeds
        ]);
        let mut queue = OutboundQueue::new();
        queue.enqueue(create_entry("m-1", "a-1"));
        queue.enqueue(create_entry("m-2", "a-2"));
        queue.enqueue(create_entry("m-3", "a-3"));

        let outcome = queue.drain(&transport, "token").await;

        assert_eq!(outcome, DrainOutcome::Completed { dead_lettered: 1 });
        assert_eq!(transport.call_count(), 3, "position 3 must still be attempted");
        assert!(queue.is_empty());
        assert_eq!(queue.dead_letters().len(), 1);
        assert_eq!(queue.dead_letters()[0].entry.id, "m-2");
        assert!(matches!(
            queue.dead_letters()[0].reason,
            DeadLetterReason::Permanent(_)
        ));
    }

    /// A malformed response body dead-letters just like any other
    /// non-retryable failure — the forwarded #101 review note that
    /// `WriteError::InvalidResponse` had no defined queue semantics.
    /// Nothing about a body that fails to parse gets fixed by retrying it
    /// unchanged, so it is bucketed with `Permanent` rather than left to
    /// block the queue forever.
    #[tokio::test]
    async fn an_invalid_response_body_dead_letters_rather_than_blocking_forever() {
        let transport = ScriptedTransport::new(vec![ok(201, "not json")]);
        let mut queue = OutboundQueue::new();
        queue.enqueue(create_entry("m-1", "a-1"));

        let outcome = queue.drain(&transport, "token").await;

        assert_eq!(outcome, DrainOutcome::Completed { dead_lettered: 1 });
        assert!(queue.is_empty());
        assert_eq!(queue.dead_letters().len(), 1);
    }

    /// An unresolved conflict (a same-field 409, or a second 409 on the
    /// rebased retry) dead-letters too, naming the colliding fields —
    /// ADR-0007's field-level conflict resolution (S5) acts on this
    /// journal; this slice only needs it to not block the queue.
    #[tokio::test]
    async fn an_unresolved_conflict_dead_letters_naming_the_fields() {
        let conflict_body =
            r#"{"error":"version_conflict","current":{"id":"a-1","title":"someone else's","version":2}}"#;
        let transport = ScriptedTransport::new(vec![ok(409, conflict_body)]);
        let mut queue = OutboundQueue::new();
        queue.enqueue(patch_entry("m-1", "a-1", 1));

        let outcome = queue.drain(&transport, "token").await;

        assert_eq!(outcome, DrainOutcome::Completed { dead_lettered: 1 });
        match &queue.dead_letters()[0].reason {
            DeadLetterReason::Conflict { fields } => assert_eq!(fields, &vec!["title".to_string()]),
            other => panic!("expected a named collision, got {other:?}"),
        }
    }

    // -------------------------------------------------------------- 401

    /// #102 acceptance: "A 401 at position 1 attempts nothing further and
    /// records exactly one credential event."
    #[tokio::test]
    async fn a_401_at_position_one_halts_the_queue_with_exactly_one_credential_event() {
        let transport = ScriptedTransport::new(vec![ok(401, "")]);
        let mut queue = OutboundQueue::new();
        queue.enqueue(create_entry("m-1", "a-1"));
        queue.enqueue(create_entry("m-2", "a-2"));
        queue.enqueue(create_entry("m-3", "a-3"));

        let outcome = queue.drain(&transport, "token").await;

        assert_eq!(outcome, DrainOutcome::CredentialNeeded);
        assert_eq!(
            transport.call_count(),
            1,
            "nothing past position 1 is ever attempted"
        );
        assert_eq!(queue.len(), 3, "the entire queue is left untouched");
    }

    // --------------------------------------------------------- crash-replay

    /// #102 acceptance: "A simulated crash between send and acknowledgement
    /// replays with no duplicate side effect."
    ///
    /// The gap this simulates: `drain` sends a patch, the authority applies
    /// it and answers, but the process crashes before the queue's
    /// post-drain state (the entry popped) is durably saved — so the next
    /// boot reloads the *last durably persisted* queue, which still holds
    /// the original entry, and replays the identical `base`/`patch_fields`
    /// against it. That replay must converge, not duplicate or error:
    /// exactly the guarantee #101's `patch_with_rebase` already proves
    /// (`replaying_a_patch_after_a_swallowed_ack_converges`) — this test
    /// pins that the queue's replay path actually reaches it.
    #[tokio::test]
    async fn a_crash_between_send_and_acknowledgement_replays_without_duplicating() {
        let store = MemorySnapshotStore::default();
        let mut queue = OutboundQueue::new();
        queue.enqueue(patch_entry("m-1", "a-1", 1));
        save_snapshot(&store, QUEUE_SCHEMA_VERSION, 1, queue.clone())
            .await
            .unwrap();

        // First attempt: the send succeeds at the authority...
        let first_attempt_transport = ScriptedTransport::new(vec![ok(
            200,
            r#"{"id":"a-1","title":"buy oat milk","version":2}"#,
        )]);
        let mut working_copy: OutboundQueue =
            load_snapshot(&store).await.unwrap().unwrap().payload;
        let outcome = working_copy.drain(&first_attempt_transport, "token").await;
        assert_eq!(outcome, DrainOutcome::Completed { dead_lettered: 0 });
        // ...but the crash happens right here, before `working_copy`'s
        // popped state is ever saved back to `store` — so `store` still
        // holds the original, unpopped queue.

        // The "reboot": reload from the last durably persisted snapshot,
        // which is still the pre-drain queue with the entry present.
        let mut rebooted_queue: OutboundQueue =
            load_snapshot(&store).await.unwrap().unwrap().payload;
        assert_eq!(
            rebooted_queue.len(),
            1,
            "the crash means the entry was never durably removed"
        );

        // The replay: the authority's `current` already holds exactly what
        // this patch intends (its own prior success) — the 409 rebases as
        // `Safe` and the retry is a no-op that still reports success.
        let replay_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy oat milk","version":2}}"#;
        let replay_retry_success = r#"{"id":"a-1","title":"buy oat milk","version":2}"#;
        let replay_transport =
            ScriptedTransport::new(vec![ok(409, replay_conflict), ok(200, replay_retry_success)]);

        let replay_outcome = rebooted_queue.drain(&replay_transport, "token").await;

        assert_eq!(
            replay_outcome,
            DrainOutcome::Completed { dead_lettered: 0 },
            "the replay must converge, not dead-letter or block"
        );
        assert!(rebooted_queue.is_empty());
        assert!(
            rebooted_queue.dead_letters().is_empty(),
            "a converging replay is not a conflict — no duplicate side effect"
        );
    }

    /// The create half of the same guarantee: a replayed create lands on
    /// the idempotent "already exists" 200, not a duplicate row — #101's
    /// `replaying_a_create_twice_leaves_the_same_row`, reached through the
    /// queue's own replay path.
    #[tokio::test]
    async fn a_crash_after_a_create_replays_onto_the_idempotent_already_exists_path() {
        let store = MemorySnapshotStore::default();
        let mut queue = OutboundQueue::new();
        queue.enqueue(create_entry("m-1", "a-1"));
        save_snapshot(&store, QUEUE_SCHEMA_VERSION, 1, queue.clone())
            .await
            .unwrap();

        let first_attempt_transport =
            ScriptedTransport::new(vec![ok(201, r#"{"id":"a-1","version":1}"#)]);
        let mut working_copy: OutboundQueue =
            load_snapshot(&store).await.unwrap().unwrap().payload;
        working_copy.drain(&first_attempt_transport, "token").await;
        // Crash here, before `store` ever learns the entry was sent.

        let mut rebooted_queue: OutboundQueue =
            load_snapshot(&store).await.unwrap().unwrap().payload;
        let replay_transport =
            ScriptedTransport::new(vec![ok(200, r#"{"id":"a-1","version":1}"#)]);

        let replay_outcome = rebooted_queue.drain(&replay_transport, "token").await;

        assert_eq!(replay_outcome, DrainOutcome::Completed { dead_lettered: 0 });
        assert!(rebooted_queue.is_empty());
    }

    // ---------------------------------------------------- schema-version bump

    /// #102 acceptance: "A schema-version bump loads the old queue rather
    /// than silently emptying it — for the mirror an empty result is
    /// harmless ... but for the queue it is silent loss of captures the
    /// user made." Simulates the bytes a version before `dead_letters`
    /// existed would have on disk (no such key at all) and proves they
    /// still deserialise into the current shape with the entry intact,
    /// never an empty queue.
    #[tokio::test]
    async fn a_schema_version_bump_loads_the_old_queue_rather_than_emptying_it() {
        let mut queue = OutboundQueue::new();
        queue.enqueue(create_entry("m-1", "a-1"));

        // The shape a build predating the dead-letter journal field would
        // have written to disk: no `dead_letters` key at all.
        let mut payload = serde_json::to_value(&queue).unwrap();
        payload.as_object_mut().unwrap().remove("dead_letters");
        let old_bytes = serde_json::to_vec(&payload).unwrap();

        let restored: OutboundQueue = serde_json::from_slice(&old_bytes)
            .expect("old bytes without `dead_letters` must still deserialise");

        assert_eq!(
            restored.len(),
            1,
            "a schema-version bump must load the old entry, never silently empty the queue"
        );
        assert!(restored.dead_letters().is_empty());
    }
}
