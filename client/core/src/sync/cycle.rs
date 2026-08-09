//! [`SyncCycle`]: ADR-0007's one sync cycle — drain the outbound queue
//! (S4/#102), then sweep (S2/#100) — wired together with the atomic
//! sweep-commit, backoff, and the active-issue count this issue (#103) owns.
//!
//! **Drain, then sweep, in that order, every time.** This is what makes a
//! still-queued edit never get flagged as a conflict against pre-write
//! server state: draining first means this device's own writes have already
//! landed by the time the sweep asks the authority for the truth, so the
//! truth it gets back already reflects them.
//!
//! **401 holds the whole cycle, not just the queue.** ADR-0007: "queue
//! holds, polling holds" — a dead credential means neither half of the
//! cycle can trust its next call to succeed, so [`SyncCycle::run`] never
//! reaches the sweep once `drain` reports
//! [`super::queue::DrainOutcome::CredentialNeeded`].
//!
//! **The sweep commits atomically.** [`super::mirror::SyncMirror::apply_sweep`]
//! already guarantees a mid-apply panic leaves the previous mirror
//! byte-identical (built on a scratch copy, swapped in as the last step);
//! this module's contribution is that the *fetch* itself is the same
//! all-or-nothing unit — [`super::adapter::fetch_sweep`] returns a complete
//! [`hummingbird_domain::ChangesResponse`] or an error, never a partial one,
//! so a transport failure mid-sweep never reaches `apply_sweep` at all and
//! the mirror this cycle started with is what a caller sees after.
//!
//! **Persisted immediately, not deferred.** The queue is durably saved right
//! after `drain` returns — regardless of what `drain` did — and the mirror
//! is durably saved right after a sweep applies, both before `run` returns.
//! This closes the #102-reviewer-forwarded gap: `enqueue`/`drain` alone only
//! mutate the in-memory value, and it was previously a caller convention,
//! not something the type system enforced, that a persist always followed.
//! [`SyncCycle::load`] closes the other half of that same finding: a
//! [`crate::storage::SnapshotError::Deserialize`] on boot propagates as
//! `Err`, never collapses into an empty [`OutboundQueue`] or [`SyncMirror`]
//! — an empty *mirror* would merely be refilled by the next sweep, but an
//! empty *queue* would be silent loss of every capture made while offline,
//! so this function does not special-case either table down to "start
//! fresh".
//!
//! **Time and jitter are both injected.** `now_ms` and `jitter_unit` are
//! caller-supplied on every call, never sampled internally — the same
//! "bare wasm32-unknown-unknown has no clock (or RNG) that does not panic"
//! reasoning [`super::mirror`] and `crate::task::mirror` already document.

use crate::storage::{load_snapshot, save_snapshot, SnapshotError, SnapshotStore};

use super::adapter::fetch_sweep;
use super::mirror::SyncMirror;
use super::queue::{DrainOutcome, OutboundQueue, QueueEntry, QUEUE_SCHEMA_VERSION};
use super::transport::ChangesTransport;
use super::write::transport::MutationTransport;

/// The schema version this cycle persists the mirror snapshot under. Kept
/// alongside [`QUEUE_SCHEMA_VERSION`] rather than reusing it — the queue and
/// the mirror are two independent persisted slots with independent shapes,
/// and coupling their version numbers would make a mirror-only shape change
/// look like a queue bump too.
pub const SYNC_MIRROR_SCHEMA_VERSION: u32 = 1;

/// Backoff is exponential with jitter, capped at five minutes (ADR-0007).
/// Base delay doubles per consecutive failure; "full jitter" (a random
/// point between zero and the capped delay, rather than a fixed multiplier)
/// so many devices retrying after the same outage don't reconverge on the
/// same instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Backoff {
    attempt: u32,
    next_attempt_at_ms: Option<i64>,
}

/// One second: the first failure's un-jittered delay.
const BASE_DELAY_MS: i64 = 1_000;
/// Five minutes (ADR-0007's cap).
const MAX_DELAY_MS: i64 = 5 * 60 * 1_000;

impl Backoff {
    pub fn new() -> Self {
        Self {
            attempt: 0,
            next_attempt_at_ms: None,
        }
    }

    /// Whether a timer-driven attempt is allowed at `now_ms`. Always `true`
    /// before any failure, or once `now_ms` reaches the delay a prior
    /// failure recorded.
    pub fn ready(&self, now_ms: i64) -> bool {
        self.next_attempt_at_ms.is_none_or(|at| now_ms >= at)
    }

    /// Records one failed attempt at `now_ms` and returns the delay (ms)
    /// before the next one is allowed. `jitter_unit` must be in `[0, 1)` —
    /// out-of-range values are clamped rather than panicking, since a bad
    /// RNG source should degrade, not crash sync.
    pub fn record_failure(&mut self, now_ms: i64, jitter_unit: f64) -> i64 {
        let shift = self.attempt.min(20); // well past MAX_DELAY_MS by then
        let capped = BASE_DELAY_MS.saturating_mul(1i64 << shift).min(MAX_DELAY_MS);
        let delay = (capped as f64 * jitter_unit.clamp(0.0, 1.0)) as i64;
        self.attempt = self.attempt.saturating_add(1);
        self.next_attempt_at_ms = Some(now_ms + delay);
        delay
    }

    /// Any user-facing trigger resets backoff (ADR-0007): a deliberate
    /// gesture always gets an immediate attempt, whatever the failure
    /// streak was.
    pub fn reset(&mut self) {
        self.attempt = 0;
        self.next_attempt_at_ms = None;
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

/// What triggered this cycle attempt (ADR-0007's cadence: "event-driven
/// first, timer second"). Every trigger runs the identical cycle — there is
/// no special-cased path — but a [`Trigger::User`] resets backoff first, and
/// only [`Trigger::Timer`] is ever gated by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// App open, reconnect, window focus, or a manual refresh — a
    /// deliberate gesture or moment that always gets an immediate attempt.
    User,
    /// The foreground 60-second cadence timer — gated by backoff so a
    /// standing failure doesn't retry every tick.
    Timer,
}

/// What one [`SyncCycle::run`] call did.
#[derive(Debug, Clone, PartialEq)]
pub enum CycleOutcome {
    /// A [`Trigger::Timer`] attempt arrived before backoff's delay elapsed;
    /// nothing was attempted at all.
    Skipped,
    /// The queue halted on a 401 (ADR-0007: "queue holds, polling holds") —
    /// the sweep was never attempted this cycle. Persisting the drained
    /// queue up to that point still happened.
    CredentialNeeded,
    /// Persisting the queue right after drain, or the mirror right after a
    /// successful sweep, failed. The in-memory state may be ahead of what
    /// is durable; the message is the underlying store/serde error's
    /// `Display`.
    PersistFailed(String),
    /// The sweep's fetch or parse failed (never a partial apply — the
    /// previous mirror is untouched). Backoff was recorded; `retry_after_ms`
    /// is the delay [`Backoff::record_failure`] returned.
    SweepFailed {
        drain: DrainOutcome,
        retry_after_ms: i64,
    },
    /// The full cycle completed: the queue drained (whatever its own
    /// per-entry outcome), the sweep applied, and the mirror was persisted.
    Completed {
        drain: DrainOutcome,
        active_item_count: usize,
    },
}

/// Either persisted table failed to load with something other than "never
/// written yet" — see [`SyncCycle::load`]'s module-doc note on why this
/// must never collapse into an empty table.
#[derive(Debug)]
pub enum LoadError<QE, ME> {
    Queue(SnapshotError<QE>),
    Mirror(SnapshotError<ME>),
}

/// Drains, then sweeps, then persists both — ADR-0007's one cycle, as one
/// type. Generic over the two independent snapshot stores the queue and the
/// mirror persist through; the read/write transports and the access token
/// are call-time arguments to [`SyncCycle::run`] instead, the same way
/// [`OutboundQueue::drain`] and [`fetch_sweep`] already take them.
pub struct SyncCycle<QS, MS> {
    queue: OutboundQueue,
    mirror: SyncMirror,
    backoff: Backoff,
    queue_store: QS,
    mirror_store: MS,
}

impl<QS, MS> SyncCycle<QS, MS>
where
    QS: SnapshotStore,
    MS: SnapshotStore,
{
    /// Fresh, empty state over the given stores — for a first-ever run with
    /// nothing persisted yet. Use [`SyncCycle::load`] to resume from
    /// whatever is already durable.
    pub fn new(queue_store: QS, mirror_store: MS) -> Self {
        Self {
            queue: OutboundQueue::new(),
            mirror: SyncMirror::new(),
            backoff: Backoff::new(),
            queue_store,
            mirror_store,
        }
    }

    /// Loads the queue and the mirror from their stores. "Nothing written
    /// yet" (`Ok(None)`) is the only case that defaults to an empty table —
    /// any other error propagates as `Err` rather than being silently
    /// mapped to the same default, per the module docs' forwarded-review
    /// note: a schema-shape `Deserialize` failure on the queue must never
    /// look like "the device has no pending offline captures".
    pub async fn load(queue_store: QS, mirror_store: MS) -> Result<Self, LoadError<QS::Error, MS::Error>> {
        let queue = match load_snapshot::<OutboundQueue, _>(&queue_store).await {
            Ok(Some(envelope)) => envelope.payload,
            Ok(None) => OutboundQueue::new(),
            Err(error) => return Err(LoadError::Queue(error)),
        };
        let mirror = match load_snapshot::<SyncMirror, _>(&mirror_store).await {
            Ok(Some(envelope)) => envelope.payload,
            Ok(None) => SyncMirror::new(),
            Err(error) => return Err(LoadError::Mirror(error)),
        };
        Ok(Self {
            queue,
            mirror,
            backoff: Backoff::new(),
            queue_store,
            mirror_store,
        })
    }

    pub fn mirror(&self) -> &SyncMirror {
        &self.mirror
    }

    pub fn queue(&self) -> &OutboundQueue {
        &self.queue
    }

    /// The population ADR-0001's 250-issue watchline measures — see
    /// [`SyncMirror::active_item_count`].
    pub fn active_item_count(&self) -> usize {
        self.mirror.active_item_count()
    }

    /// Enqueues `entry` and durably persists the queue before returning —
    /// closing the other half of the module docs' forwarded-review gap:
    /// [`OutboundQueue::enqueue`] alone only mutates memory, and a caller
    /// that composed enqueue-then-persist by hand could get the order
    /// wrong, or skip the persist, with no compiler or test catching it.
    /// Every capture should reach for this rather than call `enqueue`
    /// directly.
    pub async fn enqueue(&mut self, entry: QueueEntry, as_of_ms: i64) -> Result<(), SnapshotError<QS::Error>> {
        self.queue.enqueue(entry);
        save_snapshot(
            &self.queue_store,
            QUEUE_SCHEMA_VERSION,
            as_of_ms.max(0) as u64,
            self.queue.clone(),
        )
        .await
    }

    /// Runs one ADR-0007 cycle: drain, persist the queue, then (unless the
    /// drain needed a fresh credential) sweep and persist the mirror.
    ///
    /// `now_ms` and `jitter_unit` (`[0, 1)`) are caller-injected — see the
    /// module docs. `Trigger::User` resets backoff before attempting;
    /// `Trigger::Timer` is skipped outright if backoff's delay has not
    /// elapsed at `now_ms`.
    pub async fn run(
        &mut self,
        read_transport: &impl ChangesTransport,
        write_transport: &impl MutationTransport,
        access_token: &str,
        now_ms: i64,
        trigger: Trigger,
        jitter_unit: f64,
    ) -> CycleOutcome {
        match trigger {
            Trigger::User => self.backoff.reset(),
            Trigger::Timer if !self.backoff.ready(now_ms) => return CycleOutcome::Skipped,
            Trigger::Timer => {}
        }

        let drain_outcome = self.queue.drain(write_transport, access_token, now_ms).await;

        if let Err(error) = save_snapshot(
            &self.queue_store,
            QUEUE_SCHEMA_VERSION,
            now_ms.max(0) as u64,
            self.queue.clone(),
        )
        .await
        {
            return CycleOutcome::PersistFailed(error.to_string());
        }

        // ADR-0007: "401 is not a failure of the cycle: queue holds,
        // polling holds" — the sweep is never attempted this cycle.
        if matches!(drain_outcome, DrainOutcome::CredentialNeeded) {
            return CycleOutcome::CredentialNeeded;
        }

        match fetch_sweep(read_transport, access_token).await {
            Ok(response) => {
                self.mirror.apply_sweep(response, now_ms);
                if let Err(error) = save_snapshot(
                    &self.mirror_store,
                    SYNC_MIRROR_SCHEMA_VERSION,
                    now_ms.max(0) as u64,
                    self.mirror.clone(),
                )
                .await
                {
                    return CycleOutcome::PersistFailed(error.to_string());
                }
                self.backoff.reset();
                CycleOutcome::Completed {
                    drain: drain_outcome,
                    active_item_count: self.mirror.active_item_count(),
                }
            }
            Err(error) if error.is_unauthorized() => CycleOutcome::CredentialNeeded,
            Err(_) => {
                let retry_after_ms = self.backoff.record_failure(now_ms, jitter_unit);
                CycleOutcome::SweepFailed {
                    drain: drain_outcome,
                    retry_after_ms,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemorySnapshotStore;
    use crate::sync::queue::MutationIntent;
    use crate::sync::transport::TransportError;
    use crate::sync::write::transport::{HttpMethod, RawResponse};
    use hummingbird_domain::ChangesResponse;
    use std::sync::Mutex;

    /// Records the order two independently-scripted transports were called
    /// in, across both — this is what makes "drain strictly before sweep"
    /// directly observable rather than merely assumed.
    #[derive(Default)]
    struct CallLog(Mutex<Vec<&'static str>>);

    impl CallLog {
        fn record(&self, label: &'static str) {
            self.0.lock().unwrap().push(label);
        }

        fn calls(&self) -> Vec<&'static str> {
            self.0.lock().unwrap().clone()
        }
    }

    struct ScriptedRead<'a> {
        log: &'a CallLog,
        sweep: Mutex<Option<Result<String, TransportError>>>,
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl ChangesTransport for ScriptedRead<'_> {
        async fn fetch_changes(&self, _access_token: &str, _since: i64) -> Result<String, TransportError> {
            unreachable!("the cycle only ever calls fetch_sweep")
        }

        async fn fetch_sweep(&self, _access_token: &str) -> Result<String, TransportError> {
            self.log.record("sweep");
            self.sweep
                .lock()
                .unwrap()
                .take()
                .expect("fetch_sweep called but no script was set")
        }
    }

    struct ScriptedWrite<'a> {
        log: &'a CallLog,
        responses: Mutex<std::collections::VecDeque<Result<RawResponse, TransportError>>>,
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl MutationTransport for ScriptedWrite<'_> {
        async fn send(
            &self,
            _access_token: &str,
            _request: super::super::write::transport::MutationRequest,
        ) -> Result<RawResponse, TransportError> {
            self.log.record("drain");
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| panic!("no more scripted write responses"))
        }
    }

    fn ok(status: u16, body: impl Into<String>) -> Result<RawResponse, TransportError> {
        Ok(RawResponse {
            status,
            body: body.into(),
        })
    }

    fn create_entry(id: &str, item_id: &str) -> QueueEntry {
        QueueEntry {
            id: id.to_string(),
            intent: MutationIntent::Create {
                path: "/api/items".to_string(),
                body: serde_json::json!({"id": item_id}),
            },
        }
    }

    fn empty_sweep_body(version: i64) -> String {
        serde_json::to_string(&ChangesResponse::empty(version)).unwrap()
    }

    // --------------------------------------------------------- ADR-0007 rules

    /// ADR-0007 / #103 acceptance: "A queued edit is never flagged as a
    /// conflict against pre-write server state — the test that proves
    /// drain-before-sweep." Directly observes call order across both
    /// transports rather than inferring it: whatever the queue and the
    /// sweep each return, every `drain`-attributed call must appear before
    /// the (one) `sweep`-attributed call in the shared log.
    #[tokio::test]
    async fn a_queued_edit_is_never_flagged_as_a_conflict_against_pre_write_server_state_drain_before_sweep(
    ) {
        let log = CallLog::default();
        let read = ScriptedRead {
            log: &log,
            sweep: Mutex::new(Some(Ok(empty_sweep_body(1)))),
        };
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(201, r#"{"id":"a-1","version":1}"#)].into()),
        };
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.queue.enqueue(create_entry("m-1", "a-1"));

        let outcome = cycle.run(&read, &write, "token", 1_000, Trigger::User, 0.0).await;

        assert!(matches!(outcome, CycleOutcome::Completed { .. }));
        assert_eq!(
            log.calls(),
            vec!["drain", "sweep"],
            "the queue's own write must be sent before the sweep ever asks for truth"
        );
    }

    /// #103 acceptance: "A mid-pagination failure leaves the previous
    /// mirror byte-identical." `fetch_sweep` is complete-or-nothing (#100),
    /// so a transport failure never reaches `apply_sweep` at all — this
    /// pins that the cycle actually preserves that guarantee end to end.
    #[tokio::test]
    async fn a_mid_pagination_failure_leaves_the_previous_mirror_byte_identical() {
        let log = CallLog::default();
        let queue_store = MemorySnapshotStore::default();
        let mirror_store = MemorySnapshotStore::default();
        let mut cycle = SyncCycle::new(queue_store, mirror_store);

        // Seed a mirror with a first, successful cycle.
        let seed_read = ScriptedRead {
            log: &log,
            sweep: Mutex::new(Some(Ok(serde_json::to_string(&ChangesResponse {
                version: 1,
                items: vec![],
                ..ChangesResponse::empty(1)
            })
            .unwrap()))),
        };
        let seed_write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        cycle.run(&seed_read, &seed_write, "token", 1_000, Trigger::User, 0.0).await;
        let before = cycle.mirror().clone();

        // A second cycle whose sweep fails outright — never a partial body.
        let failing_read = ScriptedRead {
            log: &log,
            sweep: Mutex::new(Some(Err(TransportError::new("connection reset")))),
        };
        let failing_write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        let outcome = cycle
            .run(&failing_read, &failing_write, "token", 2_000, Trigger::User, 0.0)
            .await;

        assert!(matches!(outcome, CycleOutcome::SweepFailed { .. }));
        assert_eq!(
            cycle.mirror(),
            &before,
            "a failed sweep must leave the mirror exactly as it was"
        );
    }

    /// #103 acceptance: "An item missing from a complete sweep becomes
    /// absent, drops from every working view, and remains in the snapshot."
    #[tokio::test]
    async fn an_item_missing_from_a_complete_sweep_becomes_absent_drops_from_views_and_stays_in_the_snapshot(
    ) {
        let log = CallLog::default();
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };

        let first_body = serde_json::to_string(&ChangesResponse {
            version: 1,
            items: vec![hummingbird_domain::Item {
                id: "a-1".to_string(),
                seq: Some(1),
                title: "item a-1".to_string(),
                description: None,
                stage: hummingbird_domain::Stage::Triage,
                size: None,
                energy: None,
                context: None,
                priority: 0,
                project_id: None,
                project_pos: None,
                due_date: None,
                scheduled_date: None,
                source: None,
                source_key: None,
                source_url: None,
                archived_at: None,
                created_at: 1,
                updated_at: 1,
                version: 1,
            }],
            ..ChangesResponse::empty(1)
        })
        .unwrap();
        let read1 = ScriptedRead {
            log: &log,
            sweep: Mutex::new(Some(Ok(first_body))),
        };
        cycle.run(&read1, &write, "token", 1_000, Trigger::User, 0.0).await;
        assert!(cycle.mirror().item("a-1").is_some());

        let second_body = empty_sweep_body(2);
        let read2 = ScriptedRead {
            log: &log,
            sweep: Mutex::new(Some(Ok(second_body))),
        };
        cycle.run(&read2, &write, "token", 2_000, Trigger::User, 0.0).await;

        assert!(
            cycle.mirror().item("a-1").is_none(),
            "absence from a complete sweep must drop the item from working views"
        );
        assert!(
            cycle.mirror().item_including_absent("a-1").is_some(),
            "the record must remain in the retained snapshot"
        );
    }

    /// #103 acceptance: "A same-field conflict never reaches the transport
    /// [again,] and does appear in the journal." Reuses #101's own rebase
    /// guarantee (a same-field collision never retries — one send only) at
    /// the cycle level, and pins that the loser lands in the dead-letter
    /// journal named.
    #[tokio::test]
    async fn a_same_field_conflict_never_reaches_the_transport_again_and_lands_in_the_journal() {
        let log = CallLog::default();
        let conflict_body = r#"{"error":"version_conflict","current":{"id":"a-1","title":"someone else's","version":2}}"#;
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(409, conflict_body)].into()),
        };
        let read = ScriptedRead {
            log: &log,
            sweep: Mutex::new(Some(Ok(empty_sweep_body(1)))),
        };
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.queue.enqueue(QueueEntry {
            id: "m-1".to_string(),
            intent: MutationIntent::Patch {
                path: "/api/items/a-1".to_string(),
                method: HttpMethod::Patch,
                base: serde_json::json!({"id": "a-1", "title": "buy milk", "version": 1}),
                base_updated_at: 1_000,
                patch_fields: serde_json::json!({"title": "buy oat milk"}),
            },
        });

        cycle.run(&read, &write, "token", 5_000, Trigger::User, 0.0).await;

        assert_eq!(
            log.calls().iter().filter(|c| **c == "drain").count(),
            1,
            "a same-field collision must never be retried against the transport"
        );
        assert_eq!(cycle.queue().dead_letters().len(), 1);
        match &cycle.queue().dead_letters()[0].reason {
            super::super::queue::DeadLetterReason::Conflict { fields, .. } => {
                assert_eq!(fields, &vec!["title".to_string()])
            }
            other => panic!("expected a named collision, got {other:?}"),
        }
    }

    /// #103 acceptance: "...a disjoint-field change does reach the
    /// transport." The rebased retry is a second send, and it succeeds —
    /// nothing is dead-lettered.
    #[tokio::test]
    async fn a_disjoint_field_change_does_reach_the_transport() {
        let log = CallLog::default();
        let conflict_body = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy milk","context":"@computer","version":2}}"#;
        let retry_success = r#"{"id":"a-1","title":"buy oat milk","context":"@computer","version":3}"#;
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(409, conflict_body), ok(200, retry_success)].into()),
        };
        let read = ScriptedRead {
            log: &log,
            sweep: Mutex::new(Some(Ok(empty_sweep_body(1)))),
        };
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.queue.enqueue(QueueEntry {
            id: "m-1".to_string(),
            intent: MutationIntent::Patch {
                path: "/api/items/a-1".to_string(),
                method: HttpMethod::Patch,
                base: serde_json::json!({"id": "a-1", "title": "buy milk", "context": "@calls", "version": 1}),
                base_updated_at: 1_000,
                patch_fields: serde_json::json!({"title": "buy oat milk"}),
            },
        });

        let outcome = cycle.run(&read, &write, "token", 5_000, Trigger::User, 0.0).await;

        assert!(matches!(outcome, CycleOutcome::Completed { .. }));
        assert_eq!(
            log.calls().iter().filter(|c| **c == "drain").count(),
            2,
            "the rebased retry must reach the transport a second time"
        );
        assert!(cycle.queue().dead_letters().is_empty());
        assert!(cycle.queue().is_empty());
    }

    /// #103 acceptance: "Backoff caps at five minutes and resets on a user
    /// trigger."
    #[test]
    fn backoff_caps_at_five_minutes_and_resets_on_a_user_trigger() {
        let mut backoff = Backoff::new();
        for _ in 0..20 {
            backoff.record_failure(0, 1.0); // full jitter, max draw every time
        }
        let delay = backoff.record_failure(0, 1.0);
        assert_eq!(delay, MAX_DELAY_MS, "delay must never exceed the five-minute cap");

        backoff.reset();
        assert!(
            backoff.ready(0),
            "a user trigger's reset must make the very next moment ready again"
        );
    }

    /// #103 acceptance: "The cycle exposes the active-issue count, so the
    /// 250-issue watchline is observed rather than remembered."
    #[tokio::test]
    async fn the_cycle_exposes_the_active_issue_count() {
        let log = CallLog::default();
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        let active_item = |id: &str| hummingbird_domain::Item {
            id: id.to_string(),
            seq: Some(1),
            title: format!("item {id}"),
            description: None,
            stage: hummingbird_domain::Stage::Ready,
            size: None,
            energy: None,
            context: None,
            priority: 0,
            project_id: None,
            project_pos: None,
            due_date: None,
            scheduled_date: None,
            source: None,
            source_key: None,
            source_url: None,
            archived_at: None,
            created_at: 1,
            updated_at: 1,
            version: 1,
        };
        let body = serde_json::to_string(&ChangesResponse {
            version: 1,
            items: vec![active_item("a-1"), active_item("a-2")],
            ..ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead {
            log: &log,
            sweep: Mutex::new(Some(Ok(body))),
        };
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());

        let outcome = cycle.run(&read, &write, "token", 1_000, Trigger::User, 0.0).await;

        assert_eq!(cycle.active_item_count(), 2);
        match outcome {
            CycleOutcome::Completed {
                active_item_count, ..
            } => assert_eq!(active_item_count, 2),
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    /// A 401 on the queue holds the whole cycle — the sweep transport must
    /// never even be called (ADR-0007: "queue holds, polling holds").
    #[tokio::test]
    async fn a_401_on_the_queue_holds_the_whole_cycle_the_sweep_is_never_attempted() {
        let log = CallLog::default();
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(401, "")].into()),
        };
        let read = ScriptedRead {
            log: &log,
            sweep: Mutex::new(None),
        };
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.queue.enqueue(create_entry("m-1", "a-1"));

        let outcome = cycle.run(&read, &write, "token", 1_000, Trigger::User, 0.0).await;

        assert_eq!(outcome, CycleOutcome::CredentialNeeded);
        assert!(
            !log.calls().contains(&"sweep"),
            "the read side must hold too, not just the queue"
        );
    }

    /// A `Trigger::Timer` attempt before backoff's delay elapsed is skipped
    /// outright — neither transport is touched.
    #[tokio::test]
    async fn a_timer_trigger_before_backoff_elapses_is_skipped() {
        let log = CallLog::default();
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        let read = ScriptedRead {
            log: &log,
            sweep: Mutex::new(None),
        };
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.backoff.record_failure(1_000, 1.0); // now_attempt_at = 1_000 + 300_000

        let outcome = cycle.run(&read, &write, "token", 1_500, Trigger::Timer, 0.0).await;

        assert_eq!(outcome, CycleOutcome::Skipped);
        assert!(log.calls().is_empty());
    }

    /// [`SyncCycle::load`] must never collapse a genuinely broken queue
    /// snapshot into an empty queue — the forwarded #102 review finding.
    /// Simulated by durably saving a differently-shaped `Persistable`
    /// payload (the S1 `task::Mirror`, which shares no field shape with
    /// `OutboundQueue`) under the queue's own store and then loading it
    /// back as an `OutboundQueue` — a genuine shape mismatch, not a
    /// hand-written malformed literal.
    #[tokio::test]
    async fn load_propagates_a_deserialize_failure_rather_than_defaulting_to_an_empty_queue() {
        let queue_store = MemorySnapshotStore::default();
        save_snapshot(&queue_store, 1, 0, crate::task::Mirror::new())
            .await
            .unwrap();

        let result = SyncCycle::load(queue_store, MemorySnapshotStore::default()).await;

        assert!(
            matches!(result, Err(LoadError::Queue(SnapshotError::Deserialize(_)))),
            "a shape mismatch must surface as an error, never as an empty queue"
        );
    }
}
