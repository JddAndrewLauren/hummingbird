//! [`SyncCycle`]: ADR-0007's one sync cycle — drain the outbound queue
//! (S4/#102), then pull (S2/#100) — wired together with the atomic
//! sweep-commit, backoff, and the active-issue count this issue (#103) owns.
//!
//! **Drain, then pull, in that order, every time.** This is what makes a
//! still-queued edit never get flagged as a conflict against pre-write
//! server state: draining first means this device's own writes have already
//! landed by the time the pull asks the authority for the truth, so the
//! truth it gets back already reflects them.
//!
//! **The normal pull is delta; the full sweep is the backstop.** ADR-0008
//! amended ADR-0007's "the sweep is full-mirror, and it is the mechanism":
//! "the normal pull is changes since version N — provably complete because
//! rows are monotonic — [...] the full sweep remains the correctness
//! backstop (on app open + daily)". [`SyncCycle::run`] follows that split —
//! [`super::adapter::fetch_delta`] on every ordinary trigger,
//! [`super::adapter::fetch_sweep`] only when `force_full_sweep` is set (the
//! caller's "this is app open" signal) or a full day has passed since the
//! last one landed (including never). Sweeping on every 60-second timer
//! tick, rather than just this backstop cadence, is exactly the
//! write-amplification parent #95 named this slice to measure, against a
//! dataset a delta pull already answers for free.
//!
//! **401 holds the whole cycle, not just the queue.** ADR-0007: "queue
//! holds, polling holds" — a dead credential means neither half of the
//! cycle can trust its next call to succeed, so [`SyncCycle::run`] never
//! reaches the pull once `drain` reports
//! [`super::queue::DrainOutcome::CredentialNeeded`].
//!
//! **A retryable drain failure ends the cycle early, too.** ADR-0007: a
//! retryable failure "blocks the queue and ends the cycle early". `run`
//! records a backoff failure and returns
//! [`CycleOutcome::Blocked`] without ever attempting the pull —
//! [`super::queue::DrainOutcome::Blocked`] is not merely "the queue's own
//! problem", it is this cycle's too.
//!
//! **The sweep commits atomically; so does a delta.** [`super::mirror::SyncMirror::apply_sweep`]
//! and [`super::mirror::SyncMirror::apply_delta`] both build on a scratch
//! copy and swap it in as the last step, so a mid-apply panic leaves the
//! previous mirror byte-identical either way; this module's contribution is
//! that the *fetch* itself is the same all-or-nothing unit —
//! [`super::adapter::fetch_sweep`]/[`super::adapter::fetch_delta`] return a
//! complete [`hummingbird_domain::ChangesResponse`] or an error, never a
//! partial one, so a transport failure mid-pull never reaches `apply_*` at
//! all and the mirror this cycle started with is what a caller sees after.
//!
//! **Persisted immediately, not deferred — and, where it can be, persisted
//! first.** The queue is durably saved right after `drain` returns —
//! regardless of what `drain` did — and the mirror is durably saved right
//! after a pull applies, both before `run` returns. Where nothing has yet
//! been sent, the persist goes *before* the in-memory swap rather than
//! after it: [`SyncCycle::enqueue`] writes a candidate queue and installs
//! it only on success, and a pull applies to a candidate mirror that is
//! written before it (and the sweep clock, and the delta cursor it carries)
//! is installed. A failed write therefore leaves memory exactly where disk
//! is, instead of one step ahead of it.
//! This closes the #102-reviewer-forwarded gap: `enqueue`/`drain` alone only
//! mutate the in-memory value, and it was previously a caller convention,
//! not something the type system enforced, that a persist always followed.
//! [`SyncCycle::load`] closes the other half of that same finding: a
//! [`crate::storage::SnapshotError::Deserialize`] on boot propagates as
//! `Err`, never collapses into an empty [`OutboundQueue`] or [`SyncMirror`]
//! — an empty *mirror* would merely be refilled by the next pull, but an
//! empty *queue* would be silent loss of every capture made while offline,
//! so this function does not special-case either table down to "start
//! fresh" on an error. A *schema-version* mismatch is the one place those
//! two tables diverge — the mirror is discarded and rebuilt, the queue is
//! loaded anyway; see [`SyncCycle::load`].
//!
//! **A slot is persisted only when its value actually changed** (#165). The
//! dominant cost of ADR-0007's 60-second cadence was *how often* a whole
//! slot is rewritten, not how big it is: a steady-state tick — an empty
//! queue and a delta carrying zero rows — used to rewrite the entire read
//! model (~89 KB at ADR-0001's 250-item watchline, ~122 MiB/day) against a
//! mirror nothing had touched. `run` now compares before writing: the
//! mirror against the applied candidate, the queue against a clone taken
//! before `drain`. **Skipping is correct precisely because disk already
//! equals memory** — it does not weaken the persist-then-swap ordering
//! above; it is the one case where the ordering has nothing to order, and
//! the compare is structural (`PartialEq` over the whole value) so it
//! *proves* that equality rather than inferring it from what an `apply_*`
//! or a [`DrainOutcome`] reported. A cycle that skipped both writes is
//! still [`CycleOutcome::Completed`] — the cycle succeeded, only the
//! persist had nothing to do.
//!
//! **What that argument rests on is that memory always equals disk**, and
//! the queue is where it needs help. The mirror maintains it by
//! construction — `self.mirror` is only ever assigned a value that was
//! just written, or one read back at load — but [`OutboundQueue::drain`]
//! mutates in place, so a *failed* queue persist leaves memory one drain
//! ahead of disk, and every later compare would find the queue equal to
//! itself and skip the write that heals it. The `queue_needs_write` flag
//! carries exactly the divergences the payload compare cannot see (that
//! one, and a payload loaded at a stale `schema_version`); a guard like
//! this needs such a flag wherever a value can move without a successful
//! write behind it.
//!
//! The one consequence to preserve: the stored envelope's `as_of` now means
//! "when this content last changed", not "when we last checked". Nothing
//! reads the mirror's or the queue's `as_of` today — the only non-test
//! reader of any `as_of` is the calendar slot's, in
//! `ffi-web/src/calendar_host.rs` — but a future reader wanting "when did
//! we last poll" cannot get it from here.
//!
//! **Time and jitter are both injected.** `now_ms` and `jitter_unit` are
//! caller-supplied on every call, never sampled internally — the same
//! "bare wasm32-unknown-unknown has no clock (or RNG) that does not panic"
//! reasoning [`super::mirror`] and `crate::task::mirror` already document.
//! The "has a day passed since the last full sweep" clock this module keeps
//! ([`SyncCycle::last_full_sweep_at_ms`]) is in-memory only, the same as
//! [`Backoff`] — a restart re-enters with no record of the last sweep, which
//! this module treats as "due", the safe direction to fall on.

use crate::storage::{
    load_snapshot, load_snapshot_at_version, save_snapshot, SnapshotError, SnapshotStore,
};

use super::adapter::{fetch_delta, fetch_sweep};
use super::mirror::{SyncMirror, SYNC_MIRROR_SCHEMA_VERSION};
use super::queue::{DrainOutcome, OutboundQueue, QueueEntry, QUEUE_SCHEMA_VERSION};
use super::transport::ChangesTransport;
use super::write::transport::MutationTransport;

/// ADR-0008's full-sweep backstop cadence: "on app open + daily". A day,
/// exactly, tracked from the last full sweep that actually landed (or since
/// boot, if none has yet — see the module docs on why that is in-memory).
const FULL_SWEEP_BACKSTOP_INTERVAL_MS: i64 = 24 * 60 * 60 * 1_000;

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
    /// A retryable failure at the front of the queue (ADR-0007: "blocks the
    /// queue and ends the cycle early") — the pull was never attempted this
    /// cycle. The drained-so-far queue was still persisted, and backoff was
    /// recorded (`retry_after_ms` is the delay [`Backoff::record_failure`]
    /// returned).
    ///
    /// `drain` is the whole [`DrainOutcome`], not just its dead-letter
    /// count: the entries before the blockage did real work, and a host
    /// badging "1 edit didn't apply" needs it whether the cycle finished or
    /// stopped short.
    Blocked {
        drain: DrainOutcome,
        retry_after_ms: i64,
    },
    /// A 401 — from the queue (ADR-0007: "queue holds, polling holds", so
    /// the pull was never attempted) or from the pull itself. Persisting
    /// the drained queue up to that point still happened, and `drain`
    /// carries what it did; a pull-side 401 is distinguishable from a
    /// queue-side one precisely by that field being a completed drain.
    CredentialNeeded { drain: DrainOutcome },
    /// Persisting the queue right after drain, or the mirror right after a
    /// successful pull, failed. `message` is the underlying store/serde
    /// error's `Display`. Backoff was recorded too — a store that just
    /// failed is exactly the kind of thing worth backing off from before
    /// retrying, the same as a transport failure.
    ///
    /// Deliberately the one variant with no `drain`: its whole meaning is
    /// that the durability of this cycle's state is in doubt, so badging
    /// "1 edit didn't apply" off it would be badging state that may not
    /// have survived. A caller that needs the count should wait for the
    /// retry that persists.
    PersistFailed {
        message: String,
        retry_after_ms: i64,
    },
    /// The pull's fetch or parse failed (never a partial apply — the
    /// previous mirror is untouched). Backoff was recorded; `retry_after_ms`
    /// is the delay [`Backoff::record_failure`] returned.
    PullFailed {
        drain: DrainOutcome,
        retry_after_ms: i64,
    },
    /// The full cycle completed: the queue drained (whatever its own
    /// per-entry outcome), the pull applied (delta or, per the backstop
    /// cadence, a full sweep — `was_full_sweep` says which), and the mirror
    /// was persisted.
    Completed {
        drain: DrainOutcome,
        active_item_count: usize,
        was_full_sweep: bool,
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

impl<QE: std::fmt::Debug, ME: std::fmt::Debug> std::fmt::Display for LoadError<QE, ME> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Queue(error) => write!(f, "loading the queue snapshot: {error}"),
            LoadError::Mirror(error) => write!(f, "loading the mirror snapshot: {error}"),
        }
    }
}

impl<QE: std::fmt::Debug, ME: std::fmt::Debug> std::error::Error for LoadError<QE, ME> {}

/// Drains, then sweeps, then persists both — ADR-0007's one cycle, as one
/// type. Generic over the two independent snapshot stores the queue and the
/// mirror persist through; the read/write transports and the access token
/// are call-time arguments to [`SyncCycle::run`] instead, the same way
/// [`OutboundQueue::drain`] and [`fetch_sweep`] already take them.
pub struct SyncCycle<QS, MS> {
    queue: OutboundQueue,
    mirror: SyncMirror,
    backoff: Backoff,
    /// When the last full sweep actually landed — `None` means "never (this
    /// process)", which [`SyncCycle::run`] treats as due. In-memory only;
    /// see the module docs.
    last_full_sweep_at_ms: Option<i64>,
    /// **Disk does not hold what memory holds, for a reason the payload
    /// compare cannot see** — the escape hatch #165's write-skip guard
    /// needs, and the queue's alone. Two facts set it, and they are the
    /// same fact:
    ///
    /// 1. *The stamp moved, not the value.* The guard compares payloads,
    ///    but `save_snapshot` also writes `schema_version`, and the queue's
    ///    load is deliberately version-blind (#102's never-lose-captures
    ///    decision — see [`SyncCycle::load`]), so an unchanged payload can
    ///    be sitting on disk at an old version and would otherwise skip its
    ///    own migration forever. The mirror cannot reach this state:
    ///    [`load_snapshot_at_version`] discards a mismatch before the
    ///    payload is parsed.
    /// 2. *A write failed.* [`OutboundQueue::drain`] mutates `self.queue`
    ///    **in place** — it cannot be persist-then-swap, having already
    ///    sent what it drained — so a failed persist leaves memory ahead of
    ///    disk. The next cycle drains nothing, compares equal to itself,
    ///    and would skip the write that heals it, stranding the divergence
    ///    permanently. This is the one place the guard's own safety
    ///    argument ("disk already equals memory") does not hold on its own,
    ///    so the flag carries it instead. [`SyncCycle::enqueue`] needs no
    ///    such flag: it *is* persist-then-swap, so its failure discards the
    ///    candidate and leaves memory exactly where disk is.
    ///
    /// Any successful queue write clears it.
    queue_needs_write: bool,
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
            last_full_sweep_at_ms: None,
            queue_needs_write: false,
            queue_store,
            mirror_store,
        }
    }

    /// Loads the queue and the mirror from their stores. A `Deserialize`
    /// failure on either propagates as `Err` rather than being silently
    /// mapped to an empty table, per the module docs' forwarded-review
    /// note: a schema-shape failure on the queue must never look like "the
    /// device has no pending offline captures".
    ///
    /// **The envelope's schema version is checked, and the two tables
    /// answer it differently.** A snapshot written at any version other
    /// than the current one is discarded for the *mirror*
    /// (rebuilt by the next pull, which — with `last_full_sweep_at_ms`
    /// starting `None` — is the full-sweep backstop) and kept for the
    /// *queue*. That asymmetry is #102's recorded decision, pinned by
    /// `queue::tests::a_schema_version_bump_loads_the_old_queue_rather_than_emptying_it`:
    /// "for the mirror an empty result is harmless ... but for the queue it
    /// is silent loss of captures the user made". The check is what makes
    /// the version constants load-bearing rather than decorative, and it is
    /// needed because the dangerous shape changes are exactly the ones
    /// serde cannot see: #153's `due_date` → `deadline` rename leaves old
    /// bytes deserialising cleanly, with every deadline quietly `None`.
    ///
    /// **The mirror's version is read before its payload is parsed**
    /// ([`crate::storage::load_snapshot_at_version`]), and that ordering is
    /// what makes the check work at all. The other half of a schema bump —
    /// #140's added `rules` field — is the shape change serde *does* see: a
    /// pre-#140 snapshot has no `rules` key, so parsing it as the current
    /// [`SyncMirror`] is a hard `Deserialize` error. Checking the version on
    /// an already-parsed envelope therefore never got the chance to discard
    /// it; the load failed first, `Core::init` failed with it, and the device
    /// reported "the task core did not start" on every reload until its
    /// storage was cleared by hand. A discard-on-mismatch rule has to decide
    /// on the version *first* or it only covers the changes serde tolerates.
    pub async fn load(queue_store: QS, mirror_store: MS) -> Result<Self, LoadError<QS::Error, MS::Error>> {
        let (queue, queue_needs_write) = match load_snapshot::<OutboundQueue, _>(&queue_store).await
        {
            // Deliberately version-blind: old bytes are loaded anyway. The
            // version is still *read* off the envelope rather than
            // discarded, because a queue loaded at an old version has to
            // write once even if its payload never changes — see
            // `queue_needs_write`.
            Ok(Some(envelope)) => (
                envelope.payload,
                envelope.schema_version != QUEUE_SCHEMA_VERSION,
            ),
            Ok(None) => (OutboundQueue::new(), false),
            Err(error) => return Err(LoadError::Queue(error)),
        };
        let mirror =
            match load_snapshot_at_version::<SyncMirror, _>(&mirror_store, SYNC_MIRROR_SCHEMA_VERSION)
                .await
            {
                // `Ok(None)` is both "nothing stored" and "stored at another
                // schema version" — the same answer either way, and the
                // version is decided before the payload is parsed at all.
                Ok(Some(envelope)) => envelope.payload,
                Ok(None) => SyncMirror::new(),
                Err(error) => return Err(LoadError::Mirror(error)),
            };
        Ok(Self {
            queue,
            mirror,
            backoff: Backoff::new(),
            last_full_sweep_at_ms: None,
            queue_needs_write,
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

    /// The store the queue persists through — for a caller (or a test) that
    /// wants to confirm what is actually durable, independent of the
    /// in-memory value [`SyncCycle::queue`] returns.
    pub fn queue_store(&self) -> &QS {
        &self.queue_store
    }

    /// The store the mirror persists through — see [`SyncCycle::queue_store`].
    pub fn mirror_store(&self) -> &MS {
        &self.mirror_store
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
    ///
    /// Persist-then-swap, not swap-then-persist: the entry is added to a
    /// *candidate* queue, that candidate is written, and only a successful
    /// write installs it as `self.queue`. #102's criterion is literal —
    /// "the queue is written and readable back **before** the transport is
    /// ever called" — and mutating memory first would leave a later `run`
    /// draining an entry that was never durable, which is the one ordering
    /// this method exists to make unrepresentable.
    pub async fn enqueue(&mut self, entry: QueueEntry, as_of_ms: i64) -> Result<(), SnapshotError<QS::Error>> {
        let mut candidate = self.queue.clone();
        candidate.enqueue(entry);
        save_snapshot(
            &self.queue_store,
            QUEUE_SCHEMA_VERSION,
            as_of_ms.max(0) as u64,
            &candidate,
        )
        .await?;
        self.queue = candidate;
        // An enqueue always changes the queue, so it always writes — and
        // that write is at the current version, which settles any staleness
        // the load carried in.
        self.queue_needs_write = false;
        Ok(())
    }

    /// Runs one ADR-0007 cycle: drain, persist the queue, then (unless the
    /// drain needed a fresh credential or was itself blocked) pull — a full
    /// sweep if `force_full_sweep` is set or the daily backstop is due,
    /// otherwise the normal delta — and persist the mirror.
    ///
    /// `now_ms` and `jitter_unit` (`[0, 1)`) are caller-injected — see the
    /// module docs. `Trigger::User` resets backoff before attempting;
    /// `Trigger::Timer` is skipped outright if backoff's delay has not
    /// elapsed at `now_ms`. `force_full_sweep` is the caller's "this is app
    /// open" signal (ADR-0008's other backstop trigger, alongside "daily")
    /// — independent of `trigger`, since a full sweep on app open is called
    /// for whether or not that open also happens to reset backoff.
    // Eight arguments, one over clippy's default seven, and deliberately so:
    // five of them are the caller-injected dependencies this module exists to
    // keep injectable (both transports, the token, the clock, the jitter —
    // bare `wasm32-unknown-unknown` has neither a clock nor an RNG that does
    // not panic, so none of them can be sampled here). Bundling them into a
    // parameter struct would only move the same eight values one line up at
    // every call site, and S6/#104 is about to consume this signature as it
    // stands.
    #[allow(clippy::too_many_arguments)]
    pub async fn run(
        &mut self,
        read_transport: &impl ChangesTransport,
        write_transport: &impl MutationTransport,
        access_token: &str,
        now_ms: i64,
        trigger: Trigger,
        force_full_sweep: bool,
        jitter_unit: f64,
    ) -> CycleOutcome {
        match trigger {
            Trigger::User => self.backoff.reset(),
            Trigger::Timer if !self.backoff.ready(now_ms) => return CycleOutcome::Skipped,
            Trigger::Timer => {}
        }

        // The witness. `drain` mutates `self.queue` in place, so once it has
        // run there is no "before" left to compare against — and
        // `DrainOutcome` is not a substitute for one (it reports what the
        // drain *did*, which is a different claim from "the persisted value
        // moved"). The asymmetry with the mirror below is deliberate: the
        // mirror clones to build a candidate it may discard, the queue
        // clones to keep a witness of what it had.
        let queue_before = self.queue.clone();
        let drain_outcome = self.queue.drain(write_transport, access_token, now_ms).await;

        if self.queue != queue_before || self.queue_needs_write {
            if let Err(error) = save_snapshot(
                &self.queue_store,
                QUEUE_SCHEMA_VERSION,
                now_ms.max(0) as u64,
                &self.queue,
            )
            .await
            {
                // Memory is now ahead of disk and `drain` has no undo, so
                // the next cycle's compare — which will find the queue
                // equal to itself — must not be what decides whether to
                // write. See `queue_needs_write`.
                self.queue_needs_write = true;
                let retry_after_ms = self.backoff.record_failure(now_ms, jitter_unit);
                return CycleOutcome::PersistFailed {
                    message: error.to_string(),
                    retry_after_ms,
                };
            }
            self.queue_needs_write = false;
        }

        // ADR-0007: "401 is not a failure of the cycle: queue holds,
        // polling holds" — the pull is never attempted this cycle.
        if matches!(drain_outcome, DrainOutcome::CredentialNeeded { .. }) {
            return CycleOutcome::CredentialNeeded {
                drain: drain_outcome,
            };
        }

        // ADR-0007: a retryable failure "blocks the queue and ends the
        // cycle early" — the pull is never attempted this cycle either,
        // and the streak counts as a cycle-level failure for backoff.
        if matches!(drain_outcome, DrainOutcome::Blocked { .. }) {
            let retry_after_ms = self.backoff.record_failure(now_ms, jitter_unit);
            return CycleOutcome::Blocked {
                drain: drain_outcome,
                retry_after_ms,
            };
        }

        let due_for_full_sweep = force_full_sweep
            || self
                .last_full_sweep_at_ms
                .is_none_or(|at| now_ms.saturating_sub(at) >= FULL_SWEEP_BACKSTOP_INTERVAL_MS);

        let pull_result = if due_for_full_sweep {
            fetch_sweep(read_transport, access_token)
                .await
                .map(PullResponse::Sweep)
        } else {
            fetch_delta(read_transport, access_token, self.mirror.version())
                .await
                .map(PullResponse::Delta)
        };

        match pull_result {
            Ok(pull) => {
                // One commit, in ADR-0007's sense: the applied mirror, its
                // snapshot, and the sweep clock move together or not at
                // all. The apply lands on a *candidate* mirror that is
                // persisted first; only a successful write installs it and
                // advances `last_full_sweep_at_ms`. Mutating in place
                // first would leave memory and the delta cursor ahead of
                // what is durable after a persist failure.
                let was_full_sweep = matches!(pull, PullResponse::Sweep(_));
                let mut candidate = self.mirror.clone();
                match pull {
                    PullResponse::Sweep(response) => candidate.apply_sweep(response, now_ms),
                    PullResponse::Delta(response) => candidate.apply_delta(response),
                }
                // ...unless the applied candidate is what the mirror already
                // holds, in which case there is nothing to persist and
                // nothing to swap: the structural compare *proves* disk
                // equals memory rather than trusting an `apply_*` report,
                // and skipping is correct precisely because of that
                // equality. The persist-then-swap ordering above is
                // untouched — this is the one case where the ordering has
                // nothing to order.
                if candidate != self.mirror {
                    if let Err(error) = save_snapshot(
                        &self.mirror_store,
                        SYNC_MIRROR_SCHEMA_VERSION,
                        now_ms.max(0) as u64,
                        &candidate,
                    )
                    .await
                    {
                        let retry_after_ms = self.backoff.record_failure(now_ms, jitter_unit);
                        return CycleOutcome::PersistFailed {
                            message: error.to_string(),
                            retry_after_ms,
                        };
                    }
                    self.mirror = candidate;
                }
                if was_full_sweep {
                    self.last_full_sweep_at_ms = Some(now_ms);
                }
                self.backoff.reset();
                CycleOutcome::Completed {
                    drain: drain_outcome,
                    active_item_count: self.mirror.active_item_count(),
                    was_full_sweep,
                }
            }
            Err(error) if error.is_unauthorized() => CycleOutcome::CredentialNeeded {
                drain: drain_outcome,
            },
            Err(_) => {
                let retry_after_ms = self.backoff.record_failure(now_ms, jitter_unit);
                CycleOutcome::PullFailed {
                    drain: drain_outcome,
                    retry_after_ms,
                }
            }
        }
    }
}

/// Which pull this cycle actually made — both branches carry an
/// `AdapterError` on failure (checked before this type is even built, since
/// [`fetch_sweep`]/[`fetch_delta`] share the error type), so this only ever
/// wraps the success case.
enum PullResponse {
    Sweep(hummingbird_domain::ChangesResponse),
    Delta(hummingbird_domain::ChangesResponse),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{load_snapshot, InstrumentedSnapshotStore, MemorySnapshotStore};
    use crate::sync::queue::MutationIntent;
    use crate::sync::transport::TransportError;
    use crate::sync::write::transport::{HttpMethod, RawResponse};
    use hummingbird_domain::ChangesResponse;
    use std::sync::Mutex;

    /// Records the order two independently-scripted transports were called
    /// in, across both — this is what makes "drain strictly before pull"
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
        changes: Mutex<Option<Result<String, TransportError>>>,
        sweep: Mutex<Option<Result<String, TransportError>>>,
        /// Every `since` cursor `fetch_changes` was called with, in order.
        /// A `Vec` rather than an `Option` so it pins the call count as
        /// well as the value — and because [`CallLog`] can only carry
        /// `&'static str`, so the cursor cannot ride along in there.
        since_calls: Mutex<Vec<i64>>,
    }

    impl<'a> ScriptedRead<'a> {
        /// Scripted to answer a full sweep only — `fetch_changes` panics if
        /// ever called, so a test using this fails loudly if the cycle
        /// takes the delta path instead of the sweep path it expects.
        fn sweep_only(log: &'a CallLog, result: Result<String, TransportError>) -> Self {
            Self {
                log,
                changes: Mutex::new(None),
                sweep: Mutex::new(Some(result)),
                since_calls: Mutex::new(Vec::new()),
            }
        }

        /// Scripted to answer a delta pull only — the mirror image of
        /// `sweep_only`, for a test asserting the normal (non-backstop) path.
        fn changes_only(log: &'a CallLog, result: Result<String, TransportError>) -> Self {
            Self {
                log,
                changes: Mutex::new(Some(result)),
                sweep: Mutex::new(None),
                since_calls: Mutex::new(Vec::new()),
            }
        }

        /// Neither script is ever expected to be consumed — for a test
        /// proving the pull is never attempted at all.
        fn unreachable(log: &'a CallLog) -> Self {
            Self {
                log,
                changes: Mutex::new(None),
                sweep: Mutex::new(None),
                since_calls: Mutex::new(Vec::new()),
            }
        }

        /// The `since` cursors this fixture's delta pulls were asked for.
        fn since_calls(&self) -> Vec<i64> {
            self.since_calls.lock().unwrap().clone()
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl ChangesTransport for ScriptedRead<'_> {
        async fn fetch_changes(&self, _access_token: &str, since: i64) -> Result<String, TransportError> {
            self.log.record("delta");
            self.since_calls.lock().unwrap().push(since);
            self.changes
                .lock()
                .unwrap()
                .take()
                .expect("fetch_changes called but no script was set — the cycle took the delta path unexpectedly")
        }

        async fn fetch_sweep(&self, _access_token: &str) -> Result<String, TransportError> {
            self.log.record("sweep");
            self.sweep
                .lock()
                .unwrap()
                .take()
                .expect("fetch_sweep called but no script was set — the cycle took the sweep path unexpectedly")
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

    fn empty_body(version: i64) -> String {
        serde_json::to_string(&ChangesResponse::empty(version)).unwrap()
    }

    fn item_fixture(id: &str, stage: hummingbird_domain::Stage) -> hummingbird_domain::Item {
        hummingbird_domain::Item {
            id: id.to_string(),
            seq: Some(1),
            title: format!("item {id}"),
            description: None,
            stage,
            size: None,
            energy: None,
            context: None,
            priority: 0,
            project_id: None,
            project_pos: None,
            deadline: None,
            scheduled_date: None,
            source: None,
            source_key: None,
            source_url: None,
            archived_at: None,
            agent: false,
            created_at: 1,
            updated_at: 1,
            version: 1,
        }
    }

    // --------------------------------------------------------- ADR-0007 rules

    /// ADR-0007 / #103 acceptance: "A queued edit is never flagged as a
    /// conflict against pre-write server state — the test that proves
    /// drain-before-sweep." Directly observes call order across both
    /// transports rather than inferring it: whatever the queue and the
    /// pull each return, every `drain`-attributed call must appear before
    /// the (one) `sweep`-attributed call in the shared log. Forces the full
    /// sweep (`force_full_sweep: true`) since ADR-0007's own wording is
    /// about the sweep specifically.
    #[tokio::test]
    async fn a_queued_edit_is_never_flagged_as_a_conflict_against_pre_write_server_state_drain_before_sweep(
    ) {
        let log = CallLog::default();
        let read = ScriptedRead::sweep_only(&log, Ok(empty_body(1)));
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(201, r#"{"id":"a-1","version":1}"#)].into()),
        };
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.queue.enqueue(create_entry("m-1", "a-1"));

        let outcome = cycle
            .run(&read, &write, "token", 1_000, Trigger::User, true, 0.0)
            .await;

        assert!(matches!(outcome, CycleOutcome::Completed { .. }));
        assert_eq!(
            log.calls(),
            vec!["drain", "sweep"],
            "the queue's own write must be sent before the pull ever asks for truth"
        );
    }

    /// #103 acceptance: "A mid-pagination failure leaves the previous
    /// mirror byte-identical." `fetch_sweep` is complete-or-nothing (#100),
    /// so a transport failure never reaches `apply_sweep` at all — this
    /// pins that the cycle actually preserves that guarantee end to end.
    /// Seeds a *non-empty* mirror (an item, a project) so the byte-identical
    /// assertion actually has content to disturb — an empty-to-empty
    /// comparison would pass even if the guarantee were broken.
    #[tokio::test]
    async fn a_mid_pagination_failure_leaves_the_previous_mirror_byte_identical() {
        let log = CallLog::default();
        let queue_store = MemorySnapshotStore::default();
        let mirror_store = MemorySnapshotStore::default();
        let mut cycle = SyncCycle::new(queue_store, mirror_store);

        // Seed a non-empty mirror with a first, successful full sweep.
        let seed_read = ScriptedRead::sweep_only(
            &log,
            Ok(serde_json::to_string(&ChangesResponse {
                version: 1,
                projects: vec![hummingbird_domain::Project {
                    id: "p-1".to_string(),
                    name: "project p-1".to_string(),
                    archived_at: None,
                    created_at: 1,
                    updated_at: 1,
                    version: 1,
                }],
                items: vec![item_fixture("a-1", hummingbird_domain::Stage::Ready)],
                ..ChangesResponse::empty(1)
            })
            .unwrap()),
        );
        let seed_write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        cycle
            .run(&seed_read, &seed_write, "token", 1_000, Trigger::User, true, 0.0)
            .await;
        let before = cycle.mirror().clone();
        assert!(
            before.item("a-1").is_some(),
            "the seed must actually have landed before the failure case is exercised"
        );

        // A second cycle whose sweep fails outright — never a partial body.
        // `force_full_sweep: true` again, so this exercises the sweep path
        // specifically; the delta path has its own test, named so this
        // reference breaks under `grep` if it is ever renamed or deleted:
        // `a_failed_delta_pull_leaves_the_previous_mirror_byte_identical_and_records_backoff`.
        let failing_read = ScriptedRead::sweep_only(&log, Err(TransportError::new("connection reset")));
        let failing_write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        let outcome = cycle
            .run(&failing_read, &failing_write, "token", 2_000, Trigger::User, true, 0.0)
            .await;

        assert!(matches!(outcome, CycleOutcome::PullFailed { .. }));
        assert_eq!(
            cycle.mirror(),
            &before,
            "a failed sweep must leave the mirror exactly as it was, item and project content included"
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
            items: vec![item_fixture("a-1", hummingbird_domain::Stage::Triage)],
            ..ChangesResponse::empty(1)
        })
        .unwrap();
        let read1 = ScriptedRead::sweep_only(&log, Ok(first_body));
        cycle.run(&read1, &write, "token", 1_000, Trigger::User, true, 0.0).await;
        assert!(cycle.mirror().item("a-1").is_some());

        let second_body = empty_body(2);
        let read2 = ScriptedRead::sweep_only(&log, Ok(second_body));
        cycle.run(&read2, &write, "token", 2_000, Trigger::User, true, 0.0).await;

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
        let read = ScriptedRead::sweep_only(&log, Ok(empty_body(1)));
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.queue.enqueue(QueueEntry {
            id: "m-1".to_string(),
            intent: MutationIntent::Patch {
                path: "/api/items/a-1".to_string(),
                method: HttpMethod::Patch,
                base: serde_json::json!({"id": "a-1", "title": "buy milk", "version": 1}),
                base_updated_at: 1_000,
                patch_fields: serde_json::json!({"title": "buy oat milk"}),
                rebase_fields: None,
            },
        });

        cycle.run(&read, &write, "token", 5_000, Trigger::User, true, 0.0).await;

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
        let read = ScriptedRead::sweep_only(&log, Ok(empty_body(1)));
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.queue.enqueue(QueueEntry {
            id: "m-1".to_string(),
            intent: MutationIntent::Patch {
                path: "/api/items/a-1".to_string(),
                method: HttpMethod::Patch,
                base: serde_json::json!({"id": "a-1", "title": "buy milk", "context": "@calls", "version": 1}),
                base_updated_at: 1_000,
                patch_fields: serde_json::json!({"title": "buy oat milk"}),
                rebase_fields: None,
            },
        });

        let outcome = cycle
            .run(&read, &write, "token", 5_000, Trigger::User, true, 0.0)
            .await;

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

    /// A zero jitter draw must produce a zero delay — pins the "full
    /// jitter" formula at its floor, not just its cap.
    #[test]
    fn a_zero_jitter_unit_produces_a_zero_delay() {
        let mut backoff = Backoff::new();
        assert_eq!(backoff.record_failure(1_000, 0.0), 0);
        assert!(
            backoff.ready(1_000),
            "a zero delay must make the very next moment ready again"
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
        let body = serde_json::to_string(&ChangesResponse {
            version: 1,
            items: vec![
                item_fixture("a-1", hummingbird_domain::Stage::Ready),
                item_fixture("a-2", hummingbird_domain::Stage::Ready),
            ],
            ..ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(&log, Ok(body));
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());

        let outcome = cycle
            .run(&read, &write, "token", 1_000, Trigger::User, true, 0.0)
            .await;

        assert_eq!(cycle.active_item_count(), 2);
        match outcome {
            CycleOutcome::Completed {
                active_item_count, ..
            } => assert_eq!(active_item_count, 2),
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    /// A 401 on the queue holds the whole cycle — the pull transport must
    /// never even be called (ADR-0007: "queue holds, polling holds").
    #[tokio::test]
    async fn a_401_on_the_queue_holds_the_whole_cycle_the_pull_is_never_attempted() {
        let log = CallLog::default();
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(401, "")].into()),
        };
        let read = ScriptedRead::unreachable(&log);
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.queue.enqueue(create_entry("m-1", "a-1"));

        let outcome = cycle
            .run(&read, &write, "token", 1_000, Trigger::User, true, 0.0)
            .await;

        assert_eq!(
            outcome,
            CycleOutcome::CredentialNeeded {
                drain: DrainOutcome::CredentialNeeded { dead_lettered: 0 }
            },
            "a queue-side 401 is the one whose own drain reports CredentialNeeded"
        );
        assert!(
            log.calls().iter().all(|c| *c == "drain"),
            "the read side must hold too, not just the queue"
        );
    }

    /// The read side's own 401 (`run`'s `error.is_unauthorized()` arm) had
    /// no test at all. The queue has already drained by the time the pull
    /// makes its call, so what distinguishes this outcome from the
    /// queue-side 401 above is exactly its `drain` field: a *completed*
    /// drain, not a halted one.
    ///
    /// Known gap this deliberately pins rather than fixes: unlike every
    /// other pull failure, a 401 on the pull records no backoff, so a
    /// 60-second timer re-attempts a doomed cycle every tick until a fresh
    /// token arrives. What that wants is a credential hold (nothing is
    /// attempted until the host supplies a new token), not an exponential
    /// backoff — a behaviour change, and S6/#104's to make, since it is
    /// #104 that gives the host somewhere to surface the prompt.
    #[tokio::test]
    async fn a_401_on_the_pull_holds_the_cycle_too_after_the_queue_already_drained() {
        let log = CallLog::default();
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(400, r#"{"error":"validation"}"#)].into()),
        };
        let read = ScriptedRead::sweep_only(&log, Err(TransportError::http(401, "token revoked")));
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.queue.enqueue(create_entry("m-1", "a-1"));

        let outcome = cycle
            .run(&read, &write, "token", 1_000, Trigger::User, true, 1.0)
            .await;

        assert_eq!(
            outcome,
            CycleOutcome::CredentialNeeded {
                drain: DrainOutcome::Completed { dead_lettered: 1 }
            },
            "a pull-side 401 must still report what the drain that preceded it did"
        );
        assert_eq!(
            log.calls(),
            vec!["drain", "sweep"],
            "the pull really was attempted — this is not the queue-side halt"
        );
        assert!(
            cycle.backoff.ready(1_000),
            "pinning the current asymmetry: a pull-side 401 records no backoff (see this test's doc comment)"
        );
    }

    /// ADR-0007: a retryable failure "blocks the queue and ends the cycle
    /// early" — the pull must never be attempted, and the failure must
    /// record a backoff delay (previously it fell through to the pull and
    /// recorded nothing).
    #[tokio::test]
    async fn a_retryable_drain_failure_blocks_the_queue_and_ends_the_cycle_early_recording_backoff(
    ) {
        let log = CallLog::default();
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(503, "")].into()),
        };
        let read = ScriptedRead::unreachable(&log);
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.queue.enqueue(create_entry("m-1", "a-1"));

        let outcome = cycle
            .run(&read, &write, "token", 1_000, Trigger::User, true, 1.0)
            .await;

        match outcome {
            CycleOutcome::Blocked {
                drain,
                retry_after_ms,
            } => {
                assert!(retry_after_ms > 0, "a recorded failure must produce a positive delay");
                assert_eq!(drain, DrainOutcome::Blocked { dead_lettered: 0 });
            }
            other => panic!("expected Blocked, got {other:?}"),
        }
        assert!(
            log.calls().iter().all(|c| *c == "drain"),
            "the pull must never be attempted once the queue is blocked"
        );
        assert_eq!(cycle.queue().len(), 1, "the blocked entry stays queued");

        // A following successful cycle must not see a stale reset — pin
        // that a real failure was actually recorded, not silently ignored,
        // by checking a `Trigger::Timer` attempt right afterwards is gated.
        assert!(
            !cycle.backoff.ready(1_000),
            "the Blocked outcome must have left backoff un-ready, not reset"
        );
    }

    // --------------------------------------------------- delta vs. sweep

    /// ADR-0008: "the normal pull is changes since version N" — an ordinary
    /// cycle (no `force_full_sweep`, well within the daily backstop) must
    /// pull the delta, not run a full sweep. Seeds `last_full_sweep_at_ms`
    /// via one forced sweep first, since a cold cycle (never swept) is
    /// unconditionally due for one — this test is about steady state.
    #[tokio::test]
    async fn the_normal_pull_is_the_delta_since_the_mirrors_own_version_not_a_full_sweep() {
        let log = CallLog::default();
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());

        let seed_read = ScriptedRead::sweep_only(&log, Ok(empty_body(1)));
        cycle
            .run(&seed_read, &write, "token", 1_000, Trigger::User, true, 0.0)
            .await;

        let delta_read = ScriptedRead::changes_only(&log, Ok(empty_body(2)));
        let outcome = cycle
            .run(&delta_read, &write, "token", 2_000, Trigger::User, false, 0.0)
            .await;

        assert!(matches!(
            outcome,
            CycleOutcome::Completed {
                was_full_sweep: false,
                ..
            }
        ));
        assert_eq!(
            log.calls().iter().filter(|c| **c == "delta").count(),
            1,
            "the steady-state pull must be the delta, not the sweep"
        );
        assert_eq!(log.calls().iter().filter(|c| **c == "sweep").count(), 1);
        assert_eq!(
            delta_read.since_calls(),
            vec![1],
            "the delta must be asked for since the mirror's own version — the seed left it at 1"
        );
    }

    /// The delta half of "a failed pull leaves the previous mirror
    /// byte-identical" — the sweep half is
    /// `a_mid_pagination_failure_leaves_the_previous_mirror_byte_identical`,
    /// whose comment used to point at a test that did not exist.
    ///
    /// Beyond the mirror itself, this pins the *cursor*: `mirror.version()`
    /// is what the next delta is asked for as `since`, so an advance past a
    /// delta that never applied would silently skip rows forever. The
    /// drain is made non-trivial (one entry that 400s) so the outcome's
    /// `drain` field is carrying a real fact, and `jitter_unit: 1.0` is
    /// deliberate — with `0.0` the backoff assertion would be vacuous.
    #[tokio::test]
    async fn a_failed_delta_pull_leaves_the_previous_mirror_byte_identical_and_records_backoff() {
        let log = CallLog::default();
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());

        // Seed a non-empty mirror at version 1 with one forced sweep — an
        // empty-to-empty comparison would pass even if the guarantee broke.
        let seed_read = ScriptedRead::sweep_only(
            &log,
            Ok(serde_json::to_string(&ChangesResponse {
                version: 1,
                projects: vec![hummingbird_domain::Project {
                    id: "p-1".to_string(),
                    name: "project p-1".to_string(),
                    archived_at: None,
                    created_at: 1,
                    updated_at: 1,
                    version: 1,
                }],
                items: vec![item_fixture("a-1", hummingbird_domain::Stage::Ready)],
                ..ChangesResponse::empty(1)
            })
            .unwrap()),
        );
        let seed_write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        cycle
            .run(&seed_read, &seed_write, "token", 1_000, Trigger::User, true, 0.0)
            .await;
        let before = cycle.mirror().clone();
        assert!(before.item("a-1").is_some(), "sanity: the seed landed");
        assert_eq!(before.version(), 1);

        // The steady-state cycle: a delta pull that fails, over a drain
        // that dead-letters one entry on its way through.
        let failing_read = ScriptedRead::changes_only(&log, Err(TransportError::new("connection reset")));
        let failing_write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(400, r#"{"error":"validation"}"#)].into()),
        };
        cycle.queue.enqueue(create_entry("m-1", "a-1"));

        let outcome = cycle
            .run(&failing_read, &failing_write, "token", 2_000, Trigger::User, false, 1.0)
            .await;

        match outcome {
            CycleOutcome::PullFailed {
                drain,
                retry_after_ms,
            } => {
                assert_eq!(drain, DrainOutcome::Completed { dead_lettered: 1 });
                assert!(retry_after_ms > 0, "a failed pull must record a backoff delay");
            }
            other => panic!("expected PullFailed, got {other:?}"),
        }
        assert_eq!(
            log.calls().iter().filter(|c| **c == "delta").count(),
            1,
            "the steady-state failure must be on the delta path"
        );
        assert_eq!(
            log.calls().iter().filter(|c| **c == "sweep").count(),
            1,
            "only the seed swept — a failed delta must not fall back to a full sweep"
        );
        assert_eq!(
            cycle.mirror(),
            &before,
            "a failed delta must leave the mirror exactly as it was, item and project content included"
        );
        assert_eq!(
            cycle.mirror().version(),
            1,
            "the cursor must not advance past a delta that never applied"
        );
    }

    /// ADR-0008: the full sweep is the backstop "on app open" — `run`'s
    /// `force_full_sweep` is that signal, and it must win even when the
    /// daily gate would not otherwise be due.
    #[tokio::test]
    async fn force_full_sweep_wins_even_when_the_daily_backstop_is_not_yet_due() {
        let log = CallLog::default();
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());

        let seed_read = ScriptedRead::sweep_only(&log, Ok(empty_body(1)));
        cycle
            .run(&seed_read, &write, "token", 1_000, Trigger::User, true, 0.0)
            .await;

        // Only a second later — nowhere near the daily backstop — but this
        // trigger is an "app open" signal.
        let app_open_read = ScriptedRead::sweep_only(&log, Ok(empty_body(2)));
        let outcome = cycle
            .run(&app_open_read, &write, "token", 2_000, Trigger::User, true, 0.0)
            .await;

        assert!(matches!(
            outcome,
            CycleOutcome::Completed {
                was_full_sweep: true,
                ..
            }
        ));
    }

    /// ADR-0008: the full sweep is the backstop "... + daily" — once a full
    /// day has passed since the last one landed, the next pull must be a
    /// sweep even without `force_full_sweep`.
    #[tokio::test]
    async fn a_full_sweep_is_due_once_a_day_has_passed_since_the_last_one() {
        let log = CallLog::default();
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());

        let seed_read = ScriptedRead::sweep_only(&log, Ok(empty_body(1)));
        cycle
            .run(&seed_read, &write, "token", 0, Trigger::User, true, 0.0)
            .await;

        let due_read = ScriptedRead::sweep_only(&log, Ok(empty_body(2)));
        let outcome = cycle
            .run(
                &due_read,
                &write,
                "token",
                FULL_SWEEP_BACKSTOP_INTERVAL_MS,
                Trigger::User,
                false,
                0.0,
            )
            .await;

        assert!(matches!(
            outcome,
            CycleOutcome::Completed {
                was_full_sweep: true,
                ..
            }
        ));
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
        let read = ScriptedRead::unreachable(&log);
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.backoff.record_failure(1_000, 1.0); // next_attempt_at = 1_000 + 300_000

        let outcome = cycle
            .run(&read, &write, "token", 1_500, Trigger::Timer, false, 0.0)
            .await;

        assert_eq!(outcome, CycleOutcome::Skipped);
        assert!(log.calls().is_empty());
    }

    // --------------------------------------------------- durable-before-...

    /// Forwarded #101/#102 review item 1: `enqueue` must not merely mutate
    /// memory — it must be readable back from the store before returning.
    /// Fails if `save_snapshot` were removed from `SyncCycle::enqueue`.
    #[tokio::test]
    async fn enqueue_persists_the_queue_durably_before_returning() {
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());

        cycle.enqueue(create_entry("m-1", "a-1"), 1_000).await.unwrap();

        let loaded: OutboundQueue = load_snapshot(cycle.queue_store())
            .await
            .unwrap()
            .unwrap()
            .payload;
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.entries().next().unwrap().id, "m-1");
    }

    /// Forwarded #101/#102 review item 1: `run` must persist the queue
    /// right after `drain`, independent of the in-memory value. Fails if
    /// that `save_snapshot` call were removed — the store would still hold
    /// the pre-drain (non-empty) queue while `cycle.queue()` shows it empty.
    #[tokio::test]
    async fn run_persists_the_drained_queue_state_not_just_the_in_memory_one() {
        let log = CallLog::default();
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(201, r#"{"id":"a-1","version":1}"#)].into()),
        };
        let read = ScriptedRead::sweep_only(&log, Ok(empty_body(1)));
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());
        cycle.enqueue(create_entry("m-1", "a-1"), 500).await.unwrap();

        cycle
            .run(&read, &write, "token", 1_000, Trigger::User, true, 0.0)
            .await;

        assert!(cycle.queue().is_empty(), "sanity: the in-memory queue drained");
        let loaded: OutboundQueue = load_snapshot(cycle.queue_store())
            .await
            .unwrap()
            .unwrap()
            .payload;
        assert!(
            loaded.is_empty(),
            "the persisted queue must reflect the post-drain state, not the pre-drain one"
        );
    }

    /// Forwarded #101/#102 review item 1: `run` must persist the mirror
    /// right after a successful pull. Fails if that `save_snapshot` call
    /// were removed — the store would hold nothing while `cycle.mirror()`
    /// already shows the applied sweep.
    #[tokio::test]
    async fn run_persists_the_mirror_after_a_successful_pull() {
        let log = CallLog::default();
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        let body = serde_json::to_string(&ChangesResponse {
            version: 1,
            items: vec![item_fixture("a-1", hummingbird_domain::Stage::Ready)],
            ..ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(&log, Ok(body));
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default());

        cycle
            .run(&read, &write, "token", 1_000, Trigger::User, true, 0.0)
            .await;

        let loaded: SyncMirror = load_snapshot(cycle.mirror_store())
            .await
            .unwrap()
            .unwrap()
            .payload;
        assert!(
            loaded.item("a-1").is_some(),
            "the persisted mirror must actually hold the applied sweep"
        );
    }

    /// #102 acceptance, taken literally: "the queue is written and readable
    /// back *before* the transport is ever called." If the write fails, the
    /// entry was never durable — so it must not be sitting in the in-memory
    /// queue for the next `run` to drain. Only reachable through the
    /// fallible store fake: with `MemorySnapshotStore`'s `Infallible` error
    /// this branch could not be entered at all, which is how the
    /// swap-then-persist ordering cleared four reviews.
    #[tokio::test]
    async fn a_failed_enqueue_persist_leaves_nothing_drainable_behind() {
        let mut cycle = SyncCycle::new(InstrumentedSnapshotStore::failing(), MemorySnapshotStore::default());

        let result = cycle.enqueue(create_entry("m-1", "a-1"), 1_000).await;

        assert!(result.is_err(), "a failing store must surface as an error");
        assert!(
            cycle.queue().is_empty(),
            "an entry that never reached the store must never become drainable"
        );
        assert_eq!(
            cycle.queue_store().write_count(),
            1,
            "sanity: the write really was attempted"
        );
    }

    /// The mirror half of the same rule (ADR-0007: "one commit"; #103:
    /// "commits mirror, snapshot and publish in one step"). A persist
    /// failure after a successful pull must leave the in-memory mirror —
    /// and with it the delta cursor `run` sends as `since`, and the
    /// full-sweep clock — exactly where the store still is, not one pull
    /// ahead of it. Seeds durably first, then breaks the store, so the
    /// "unchanged" being asserted is real content rather than emptiness.
    #[tokio::test]
    async fn a_failed_mirror_persist_leaves_the_mirror_and_its_cursor_where_the_store_is() {
        let log = CallLog::default();
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        let mut cycle = SyncCycle::new(MemorySnapshotStore::default(), InstrumentedSnapshotStore::new());

        let seed_body = serde_json::to_string(&ChangesResponse {
            version: 1,
            items: vec![item_fixture("a-1", hummingbird_domain::Stage::Ready)],
            ..ChangesResponse::empty(1)
        })
        .unwrap();
        let seed_read = ScriptedRead::sweep_only(&log, Ok(seed_body));
        cycle
            .run(&seed_read, &write, "token", 1_000, Trigger::User, true, 0.0)
            .await;
        let before = cycle.mirror().clone();
        assert!(before.item("a-1").is_some(), "sanity: the seed landed");

        // The store breaks, and the next pull is a full sweep that would
        // otherwise wipe `a-1` and advance the cursor to version 2.
        cycle.mirror_store().set_failing(true);
        let failing_read = ScriptedRead::sweep_only(&log, Ok(empty_body(2)));
        let outcome = cycle
            .run(&failing_read, &write, "token", 2_000, Trigger::User, true, 1.0)
            .await;

        match outcome {
            CycleOutcome::PersistFailed { retry_after_ms, .. } => {
                assert!(retry_after_ms > 0, "a persist failure records a backoff delay")
            }
            other => panic!("expected PersistFailed, got {other:?}"),
        }
        assert_eq!(
            cycle.mirror(),
            &before,
            "an unpersisted apply must not be published to memory — disk and memory move together"
        );
        assert_eq!(
            cycle.mirror().version(),
            1,
            "the cursor must not advance past a pull that never became durable"
        );
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
        save_snapshot(&queue_store, 1, 0, &crate::task::Mirror::new())
            .await
            .unwrap();

        let result = SyncCycle::load(queue_store, MemorySnapshotStore::default()).await;

        assert!(
            matches!(result, Err(LoadError::Queue(SnapshotError::Deserialize(_)))),
            "a shape mismatch must surface as an error, never as an empty queue"
        );
    }

    /// The mirror half of the schema-version asymmetry (#153): a stored
    /// mirror whose envelope version is not the current one is *discarded*,
    /// not trusted, because a shape change like #153's `due_date` →
    /// `deadline` rename deserialises perfectly cleanly into the current
    /// shape while silently meaning something else. The envelope version is
    /// the only thing that can tell those bytes apart, so this test writes
    /// a mirror that would load fine and proves the version alone rejects
    /// it.
    ///
    /// The queue in the same load takes the opposite branch, on purpose:
    /// see `queue::tests::a_schema_version_bump_loads_the_old_queue_rather_than_emptying_it`
    /// — an empty mirror costs one full sweep, an empty queue is silent
    /// loss of captures the user made offline.
    #[tokio::test]
    async fn load_discards_a_mirror_at_a_stale_schema_version_but_keeps_the_queue() {
        let mirror_store = MemorySnapshotStore::default();
        let mut stale_mirror = SyncMirror::new();
        stale_mirror.apply_sweep(
            ChangesResponse {
                version: 7,
                items: vec![item_fixture("a-1", hummingbird_domain::Stage::Ready)],
                ..ChangesResponse::empty(7)
            },
            1_000,
        );
        save_snapshot(&mirror_store, SYNC_MIRROR_SCHEMA_VERSION - 1, 0, &stale_mirror)
            .await
            .unwrap();
        // Sanity: these bytes are not corrupt — they load into the current
        // shape without complaint. Only the version says otherwise.
        assert!(
            load_snapshot::<SyncMirror, _>(&mirror_store)
                .await
                .unwrap()
                .is_some(),
            "the stale payload must itself be perfectly deserialisable — that is the danger"
        );

        let queue_store = MemorySnapshotStore::default();
        let mut queue = OutboundQueue::new();
        queue.enqueue(create_entry("m-1", "a-1"));
        save_snapshot(&queue_store, QUEUE_SCHEMA_VERSION - 1, 0, &queue)
            .await
            .unwrap();

        let cycle = SyncCycle::load(queue_store, mirror_store).await.unwrap();

        assert_eq!(
            cycle.mirror(),
            &SyncMirror::new(),
            "a mirror at another schema version must be discarded and rebuilt, not trusted"
        );
        assert_eq!(
            cycle.mirror().version(),
            0,
            "the discarded mirror's delta cursor goes with it, so the next pull rebuilds from scratch"
        );
        assert_eq!(
            cycle.queue().len(),
            1,
            "the queue keeps its entries across a version mismatch — the deliberate asymmetry"
        );
    }

    /// The regression the live site hit: a mirror at a stale schema version
    /// whose payload *cannot* be parsed as the current [`SyncMirror`] at all
    /// must still be discarded, not surface as a boot failure.
    ///
    /// #140 added the `rules` field, so every pre-#140 snapshot is missing a
    /// key `serde` requires — `Deserialize(missing field \`rules\`)`. While
    /// [`SyncCycle::load`] parsed the payload before consulting the version,
    /// that error escaped as `LoadError::Mirror` and `Core::init` failed with
    /// it, so the app reported "the task core did not start" on every reload
    /// until the device's storage was cleared by hand — the exact upgrade the
    /// version bump was supposed to make silent. The sibling test above
    /// covers the changes serde tolerates; this one covers the changes it
    /// does not, and only the version-first ordering satisfies both.
    ///
    /// Simulated the same way the queue's shape-mismatch test is: a
    /// differently-shaped `Persistable` saved under the mirror's own store.
    #[tokio::test]
    async fn load_discards_a_stale_mirror_whose_payload_no_longer_parses() {
        let mirror_store = MemorySnapshotStore::default();
        save_snapshot(
            &mirror_store,
            SYNC_MIRROR_SCHEMA_VERSION - 1,
            0,
            &crate::task::Mirror::new(),
        )
        .await
        .unwrap();
        // Sanity: unlike the sibling test's bytes, these really are
        // unparseable as the current mirror — the version is the only thing
        // that can be read off them safely.
        assert!(
            matches!(
                load_snapshot::<SyncMirror, _>(&mirror_store).await,
                Err(SnapshotError::Deserialize(_))
            ),
            "the stale payload must genuinely fail to parse — that is what this test is about"
        );

        let queue_store = MemorySnapshotStore::default();
        let mut queue = OutboundQueue::new();
        queue.enqueue(create_entry("m-1", "a-1"));
        save_snapshot(&queue_store, QUEUE_SCHEMA_VERSION, 0, &queue)
            .await
            .unwrap();

        let cycle = SyncCycle::load(queue_store, mirror_store)
            .await
            .expect("a stale mirror is discarded, however its shape moved — never a boot failure");

        assert_eq!(
            cycle.mirror(),
            &SyncMirror::new(),
            "the unparseable stale mirror is rebuilt by the next pull's full sweep"
        );
        assert_eq!(
            cycle.queue().len(),
            1,
            "and the queue's offline captures survive it, as always"
        );
    }

    // ------------------------------------------------- write amplification

    /// ADR-0001's watchline population — the size #95 asks about.
    const WATCHLINE_ITEM_COUNT: usize = 250;

    /// What a [`WATCHLINE_ITEM_COUNT`]-item mirror serialised to when this
    /// was measured: **89,178 bytes on 2026-08-09**, at PR #161's head
    /// (88,848 for the slightly leaner item shape the same day's live
    /// measurement used — the two agree to within half a percent). At
    /// ADR-0007's 60-second timer that *was* ~5.1 MiB/hour, ~122 MiB/day
    /// rewritten against a mirror nothing changed — the cost #165's
    /// write-skip removed, and the reason it was worth removing.
    ///
    /// The assertions below hold a *band* around this figure rather than
    /// the figure itself: adding a field to [`hummingbird_domain::Item`]
    /// moves it, and that is churn, not a regression. What the figure is
    /// *for*, since #165, is the size of the write that no longer happens
    /// on a steady-state tick — a write this big is still what one real
    /// change costs, so the read model's size stays measured even though
    /// the per-tick rewrite is gone.
    const MEASURED_WATCHLINE_MIRROR_BYTES: usize = 89_178;

    /// Half to double the measured figure — see
    /// [`MEASURED_WATCHLINE_MIRROR_BYTES`].
    fn assert_within_measured_band(bytes: usize, what: &str) {
        let low = MEASURED_WATCHLINE_MIRROR_BYTES / 2;
        let high = MEASURED_WATCHLINE_MIRROR_BYTES * 2;
        assert!(
            (low..=high).contains(&bytes),
            "{what} was {bytes} bytes, outside the band {low}..={high} around the \
             {MEASURED_WATCHLINE_MIRROR_BYTES} measured on 2026-08-09 — if a domain field was \
             added this is churn and the constant wants re-measuring, but an order-of-magnitude \
             move is the regression this test exists to catch"
        );
    }

    /// A full sweep carrying a watchline-sized item set.
    fn watchline_sweep_body(version: i64) -> String {
        serde_json::to_string(&ChangesResponse {
            version,
            items: (0..WATCHLINE_ITEM_COUNT)
                .map(|i| item_fixture(&format!("a-{i:03}"), hummingbird_domain::Stage::Ready))
                .collect(),
            ..ChangesResponse::empty(version)
        })
        .unwrap()
    }

    /// Seeds a cycle whose mirror holds the watchline population, through
    /// one real forced sweep, on instrumented stores. The full-sweep clock
    /// is set on the way out, so the caller's next `run` takes the delta
    /// path rather than the daily backstop.
    async fn cycle_seeded_with_a_watchline_mirror(
        log: &CallLog,
    ) -> SyncCycle<InstrumentedSnapshotStore, InstrumentedSnapshotStore> {
        let mut cycle = SyncCycle::new(InstrumentedSnapshotStore::new(), InstrumentedSnapshotStore::new());
        let read = ScriptedRead::sweep_only(log, Ok(watchline_sweep_body(1)));
        let write = ScriptedWrite {
            log,
            responses: Mutex::new(vec![].into()),
        };
        let outcome = cycle
            .run(&read, &write, "token", 1_000, Trigger::User, true, 0.0)
            .await;
        assert!(
            matches!(outcome, CycleOutcome::Completed { .. }),
            "sanity: the seed sweep must land, or every measurement below is of an empty mirror"
        );
        assert_eq!(cycle.active_item_count(), WATCHLINE_ITEM_COUNT);
        cycle
    }

    /// #95's write-amplification question, half one: **how many** writes —
    /// and #165's answer to it. A steady-state tick (an empty queue, a
    /// delta carrying no rows at the version the mirror already holds)
    /// writes **neither** slot: the queue is byte-for-byte what it was
    /// before `drain`, the applied candidate is byte-for-byte the mirror,
    /// and persisting either would be writing a value disk already holds.
    /// The cycle still completes — only the persist had nothing to do.
    ///
    /// This is the inversion of the test that measured the problem: it used
    /// to assert exactly one write per slot per cycle "however little
    /// changed", which at ADR-0007's 60-second timer was the multiplier on
    /// [`MEASURED_WATCHLINE_MIRROR_BYTES`].
    #[tokio::test]
    async fn a_steady_state_tick_writes_neither_snapshot_slot() {
        let log = CallLog::default();
        let mut cycle = cycle_seeded_with_a_watchline_mirror(&log).await;
        let queue_writes_before = cycle.queue_store().write_count();
        let mirror_writes_before = cycle.mirror_store().write_count();

        // An empty queue and a delta that carries no rows: the emptiest
        // steady-state tick the timer can produce.
        let read = ScriptedRead::changes_only(&log, Ok(empty_body(1)));
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        let outcome = cycle
            .run(&read, &write, "token", 2_000, Trigger::Timer, false, 0.0)
            .await;

        assert!(
            matches!(
                outcome,
                CycleOutcome::Completed {
                    was_full_sweep: false,
                    ..
                }
            ),
            "the cycle still succeeded — skipping a persist is not a failure — got {outcome:?}"
        );
        assert_eq!(
            cycle.queue_store().write_count() - queue_writes_before,
            0,
            "an empty queue that drained nothing is what disk already holds; re-serialising it \
             is the cost #165 removed"
        );
        assert_eq!(
            cycle.mirror_store().write_count() - mirror_writes_before,
            0,
            "a delta that applied no rows leaves a candidate equal to the mirror — the whole \
             read model must not be rewritten to say so"
        );
    }

    /// #95's write-amplification question, half two: **how much** — the
    /// figure that made the skip worth building. A zero-row delta produces
    /// no mirror write at all now, so the only write in the store is the
    /// seed sweep's (empty mirror → 250 items genuinely differs), and it is
    /// that write the band is anchored to: one real change still costs the
    /// whole read model, which is why the per-tick repetition mattered.
    #[tokio::test]
    async fn a_zero_row_delta_writes_nothing_though_one_real_change_costs_the_whole_read_model() {
        let log = CallLog::default();
        let mut cycle = cycle_seeded_with_a_watchline_mirror(&log).await;

        let delta_body = empty_body(1);
        let read = ScriptedRead::changes_only(&log, Ok(delta_body.clone()));
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        cycle
            .run(&read, &write, "token", 2_000, Trigger::Timer, false, 0.0)
            .await;

        let sizes = cycle.mirror_store().write_sizes();
        assert_eq!(
            sizes.len(),
            1,
            "the seed sweep wrote; the steady-state tick after it did not"
        );
        let seeded = sizes[0];

        assert!(
            seeded > delta_body.len() * 100,
            "the persisted payload ({seeded} bytes) should dwarf a delta of this size \
             ({} bytes) — that gap is the amplification #95 asked about, and it is what a \
             per-tick rewrite used to pay",
            delta_body.len()
        );
        assert_within_measured_band(seeded, "a mirror write carrying the watchline population");
    }

    /// #95 names only the mirror, but the queue's persisted payload has the
    /// same unbounded-growth shape: it bundles the never-pruned dead-letter
    /// journal with the live FIFO. #165's skip takes that cost off *idle*
    /// cycles — nothing changed, nothing written — but the journal still
    /// rides along on **every [`SyncCycle::enqueue`]**, since an enqueue
    /// always changes the queue. Nothing prunes it today; only the unbuilt
    /// "1 edit didn't apply" affordance would, so the remaining cost stays
    /// pinned here.
    #[tokio::test]
    async fn the_queue_snapshot_carries_the_dead_letter_journal_on_every_enqueue() {
        let log = CallLog::default();
        let mut cycle = SyncCycle::new(InstrumentedSnapshotStore::new(), InstrumentedSnapshotStore::new());

        // Tick 1: an empty queue that drains nothing — under the guard this
        // writes nothing at all, so the queue store is still untouched.
        let read = ScriptedRead::sweep_only(&log, Ok(empty_body(1)));
        let idle_write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        cycle
            .run(&read, &idle_write, "token", 1_000, Trigger::User, true, 0.0)
            .await;
        assert!(
            cycle.queue_store().write_sizes().is_empty(),
            "an empty queue that drained nothing has nothing to persist"
        );

        // One capture that the authority rejects permanently: it leaves the
        // FIFO empty again, but the journal keeps it forever.
        cycle.enqueue(create_entry("m-1", "a-1"), 1_500).await.unwrap();
        let read = ScriptedRead::changes_only(&log, Ok(empty_body(1)));
        let rejecting_write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(400, r#"{"error":"validation"}"#)].into()),
        };
        let outcome = cycle
            .run(&read, &rejecting_write, "token", 2_000, Trigger::Timer, false, 0.0)
            .await;
        assert!(
            matches!(
                outcome,
                CycleOutcome::Completed {
                    drain: DrainOutcome::Completed { dead_lettered: 1 },
                    ..
                }
            ),
            "expected the capture to dead-letter, got {outcome:?}"
        );

        // Tick 3: nothing queued, nothing returned, nothing to do — and now
        // nothing written either.
        let read = ScriptedRead::changes_only(&log, Ok(empty_body(1)));
        cycle
            .run(&read, &idle_write, "token", 3_000, Trigger::Timer, false, 0.0)
            .await;

        // A second capture: the journal rides along with it.
        cycle.enqueue(create_entry("m-2", "a-2"), 4_000).await.unwrap();

        let sizes = cycle.queue_store().write_sizes();
        assert_eq!(
            sizes.len(),
            3,
            "the first enqueue, the tick that dead-lettered it, and the second enqueue — the \
             two idle ticks either side wrote nothing; got {sizes:?}"
        );
        let (first_enqueue, dead_lettered, second_enqueue) = (sizes[0], sizes[1], sizes[2]);
        assert!(
            dead_lettered > 0 && first_enqueue > 0,
            "sanity: both of those writes really happened"
        );
        assert!(
            second_enqueue > first_enqueue,
            "the second capture's write carries the journal the first one's did not \
             ({second_enqueue} bytes against {first_enqueue}) — every capture pays for every \
             entry that ever failed permanently, and nothing prunes that"
        );

        let loaded: OutboundQueue = load_snapshot(cycle.queue_store())
            .await
            .unwrap()
            .unwrap()
            .payload;
        assert_eq!(loaded.len(), 1, "the second capture is still queued");
        assert_eq!(
            loaded.dead_letters().len(),
            1,
            "and the journal is durable — it is carried, not dropped, by the skip"
        );
    }

    /// The guard's failure mode is silent staleness on disk, and this is
    /// the test that would catch it: when each slot *genuinely* changed —
    /// one queued entry drains successfully, and a delta applies one row —
    /// each slot is written exactly once, as it always was.
    ///
    /// The second half is the subtler case: a delta carrying **zero rows at
    /// a higher version** still changes the mirror, because `version` is a
    /// field of [`SyncMirror`] and the apply advances it. A guard that
    /// compared only the row tables would leave the delta cursor on disk
    /// behind memory's, and the next boot would re-pull from the old one.
    #[tokio::test]
    async fn a_cycle_that_really_changed_a_slot_writes_that_slot_including_a_bare_cursor_move() {
        let log = CallLog::default();
        let mut cycle = cycle_seeded_with_a_watchline_mirror(&log).await;
        cycle.enqueue(create_entry("m-1", "a-1"), 1_500).await.unwrap();
        let queue_writes_before = cycle.queue_store().write_count();
        let mirror_writes_before = cycle.mirror_store().write_count();

        // Both halves move: the queued entry sends successfully (so the
        // FIFO empties) and the delta carries a row (so the mirror grows).
        let one_row_delta = serde_json::to_string(&ChangesResponse {
            version: 2,
            items: vec![item_fixture("a-new", hummingbird_domain::Stage::Ready)],
            ..ChangesResponse::empty(2)
        })
        .unwrap();
        let read = ScriptedRead::changes_only(&log, Ok(one_row_delta));
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(201, r#"{"id":"a-1","version":1}"#)].into()),
        };
        let outcome = cycle
            .run(&read, &write, "token", 2_000, Trigger::Timer, false, 0.0)
            .await;
        assert!(
            matches!(outcome, CycleOutcome::Completed { .. }),
            "expected a completed cycle, got {outcome:?}"
        );

        assert_eq!(
            cycle.queue_store().write_count() - queue_writes_before,
            1,
            "a queue that drained an entry is not the queue disk holds — it must be written"
        );
        assert_eq!(
            cycle.mirror_store().write_count() - mirror_writes_before,
            1,
            "a delta that applied a row must be written"
        );

        // Now the bare cursor move: no rows at all, but a higher version.
        let mirror_writes_before = cycle.mirror_store().write_count();
        let read = ScriptedRead::changes_only(&log, Ok(empty_body(3)));
        let write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        cycle
            .run(&read, &write, "token", 3_000, Trigger::Timer, false, 0.0)
            .await;

        assert_eq!(
            cycle.mirror_store().write_count() - mirror_writes_before,
            1,
            "a zero-row delta at a higher version still moved the delta cursor, so the mirror \
             on disk is stale until it is written"
        );
        let durable: SyncMirror = load_snapshot(cycle.mirror_store())
            .await
            .unwrap()
            .unwrap()
            .payload;
        assert_eq!(
            durable.version(),
            3,
            "and the cursor that is durable is the one memory holds"
        );
    }

    /// The escape hatch the payload compare needs. The guard compares
    /// payloads, but `save_snapshot` also stamps `schema_version` — and the
    /// queue's load is deliberately version-blind (#102: never lose
    /// captures), so a queue can be in memory at the current version's
    /// *shape* while disk still says the old version. Without the
    /// `queue_needs_write` flag an idle device would skip its own
    /// migration forever, leaving bytes on disk stamped at a version that
    /// no longer describes them.
    ///
    /// The mirror cannot reach this state: `load_snapshot_at_version`
    /// discards a version mismatch before the payload is parsed, so a
    /// loaded mirror is always at the current version.
    #[tokio::test]
    async fn a_queue_stored_at_an_old_schema_version_is_rewritten_once_then_left_alone() {
        let log = CallLog::default();
        let queue_store = InstrumentedSnapshotStore::new();
        // An identical payload — an empty queue — at the previous version:
        // nothing about the value changes, only the stamp on it.
        save_snapshot(&queue_store, QUEUE_SCHEMA_VERSION - 1, 0, &OutboundQueue::new())
            .await
            .unwrap();
        let writes_seeding = queue_store.write_count();

        let mut cycle = SyncCycle::load(queue_store, InstrumentedSnapshotStore::new())
            .await
            .expect("the queue loads whatever its version says — #102's asymmetry");

        let idle_write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        let read = ScriptedRead::sweep_only(&log, Ok(empty_body(0)));
        cycle
            .run(&read, &idle_write, "token", 1_000, Trigger::User, true, 0.0)
            .await;

        assert_eq!(
            cycle.queue_store().write_count() - writes_seeding,
            1,
            "an unchanged payload at a stale version must still be written once, or it skips \
             its own migration"
        );
        let envelope = load_snapshot::<OutboundQueue, _>(cycle.queue_store())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            envelope.schema_version,
            QUEUE_SCHEMA_VERSION,
            "and that write restamps it at the current version"
        );

        // The tick after that has nothing left to settle.
        let writes_before = cycle.queue_store().write_count();
        let read = ScriptedRead::changes_only(&log, Ok(empty_body(0)));
        cycle
            .run(&read, &idle_write, "token", 2_000, Trigger::Timer, false, 0.0)
            .await;
        assert_eq!(
            cycle.queue_store().write_count() - writes_before,
            0,
            "the escape hatch fires once, not on every tick forever"
        );
    }

    /// The write-skip guard's own failure mode, and the one case its safety
    /// argument does not cover on its own. The guard is safe because memory
    /// equals disk — but [`OutboundQueue::drain`] mutates `self.queue` in
    /// place (it cannot be persist-then-swap, having already sent what it
    /// drained), so a *failed* queue persist leaves memory one drain ahead
    /// of disk. The next cycle has nothing to drain, finds the queue equal
    /// to itself, and — without `queue_needs_write` — would skip the write
    /// that heals it, stranding the divergence for the process's lifetime.
    /// Before #165 the unconditional per-cycle write healed this for free;
    /// that is exactly the write the guard removed.
    ///
    /// The mirror needs no equivalent: `self.mirror` is only ever assigned
    /// a value that was just written, so its failed persist leaves memory
    /// and disk together — pinned by
    /// `a_failed_mirror_persist_leaves_the_mirror_and_its_cursor_where_the_store_is`.
    #[tokio::test]
    async fn a_failed_queue_persist_is_retried_next_cycle_not_stranded_by_the_write_skip() {
        let log = CallLog::default();
        let mut cycle = SyncCycle::new(InstrumentedSnapshotStore::new(), MemorySnapshotStore::default());
        cycle.enqueue(create_entry("m-1", "a-1"), 500).await.unwrap();

        // The store breaks, and this cycle's drain sends the entry — so the
        // in-memory queue empties while the store still holds it.
        cycle.queue_store().set_failing(true);
        let read = ScriptedRead::sweep_only(&log, Ok(empty_body(1)));
        let sending_write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![ok(201, r#"{"id":"a-1","version":1}"#)].into()),
        };
        let outcome = cycle
            .run(&read, &sending_write, "token", 1_000, Trigger::User, true, 1.0)
            .await;
        assert!(
            matches!(outcome, CycleOutcome::PersistFailed { .. }),
            "expected the queue persist to fail, got {outcome:?}"
        );
        assert!(cycle.queue().is_empty(), "sanity: the drain really happened in memory");
        let stranded: OutboundQueue = load_snapshot(cycle.queue_store())
            .await
            .unwrap()
            .unwrap()
            .payload;
        assert_eq!(
            stranded.len(),
            1,
            "sanity: disk is one drain behind memory — this is the divergence"
        );

        // The store recovers. The next cycle drains nothing at all, so the
        // payload compare alone would see no reason to write.
        cycle.queue_store().set_failing(false);
        let writes_before = cycle.queue_store().write_count();
        // Still the sweep path: the failed cycle returned before its pull,
        // so no full sweep has ever landed and the backstop is still due.
        let read = ScriptedRead::sweep_only(&log, Ok(empty_body(1)));
        let idle_write = ScriptedWrite {
            log: &log,
            responses: Mutex::new(vec![].into()),
        };
        cycle
            .run(&read, &idle_write, "token", 2_000, Trigger::User, false, 0.0)
            .await;

        assert_eq!(
            cycle.queue_store().write_count() - writes_before,
            1,
            "a queue whose last persist failed must be written again even though nothing \
             drained — the compare cannot see that divergence"
        );
        let healed: OutboundQueue = load_snapshot(cycle.queue_store())
            .await
            .unwrap()
            .unwrap()
            .payload;
        assert!(
            healed.is_empty(),
            "and disk is back level with memory, so a reload cannot replay an entry that \
             already reached the authority"
        );

        // Healed means healed: the tick after that has nothing to write.
        let writes_before = cycle.queue_store().write_count();
        let read = ScriptedRead::changes_only(&log, Ok(empty_body(1)));
        cycle
            .run(&read, &idle_write, "token", 3_000, Trigger::User, false, 0.0)
            .await;
        assert_eq!(
            cycle.queue_store().write_count() - writes_before,
            0,
            "the flag clears on the write that healed it, rather than pinning every later cycle"
        );
    }
}
