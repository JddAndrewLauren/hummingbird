//! `MobileTaskHost`'s core-ownership diagnostics (#710) — the mobile-host
//! half of slice 3's web-host treatment: every acquisition of `inner:
//! tokio::sync::Mutex<Inner>` emits `core.wait_started`, then
//! `core.acquired`, and finally `core.released`, each carrying a closed
//! [`CoreOwner`] (`hummingbird_core::diagnostics::CoreOwner`, #710's
//! addition to the shared enum — never redefined here).
//!
//! **`wait_started`'s owner is the current holder, not the waiter.** The
//! acceptance this exists to satisfy is literally "a wait span identifying
//! `sync` as the owner" when a capture/triage/project-read starts while a
//! sync holds the mutex behind a hung transport — so `owner` on
//! `core.wait_started` names whoever is *already* holding the mutex (or,
//! when it is free, the waiter's own identity, since the wait resolves
//! immediately and there is nothing else to report). `core.acquired`/
//! `core.released` both name the caller itself, since by then that caller
//! *is* the holder. A tiny `std::sync::Mutex<Option<CoreOwner>>` sits
//! beside the real `tokio::sync::Mutex` purely to make that snapshot
//! observable *before* `.lock().await` — it is never itself a source of
//! truth about who holds the async lock, only a diagnostic breadcrumb.
//!
//! **Release goes through a `Drop` guard**, not an explicit call at the end
//! of every method — the acceptance's own "a cancelled in-flight operation
//! still records `core.released`" criterion. A uniffi async call that gets
//! cancelled (the Kotlin coroutine calling it is cancelled, or the whole
//! process races down) drops the in-flight future, which drops
//! [`OwnedGuard`], which records the release and clears the owner tracker
//! before the wrapped `tokio::sync::MutexGuard` itself drops and actually
//! unlocks — see [`OwnedGuard::drop`].
//!
//! **Source and clock, deliberately self-contained — but `seq` is
//! borrowed, not owned.** These events are stamped
//! [`hummingbird_core::diagnostics::Source::Android`] and share both the
//! `session_id` *and* the `seq` counter `lib.rs`'s process-wide
//! `DIAGNOSTIC_SESSION` static already uses for `session.started`/
//! `worker.*`/`push.received`/`network.changed` — [`CoreLockSession`]
//! holds a `&'static AtomicU64` reference to that same counter rather than
//! minting its own (review round 1 caught an earlier version doing
//! exactly that: two independent 0-based counters under one `session_id`,
//! landing in one exported journal with no total order between them).
//! *Not* `hummingbird_core::diagnostics::DiagnosticsContext` — that type is
//! built for one *sync cycle* (it hardcodes `cycle_id: Some(..)`), and a
//! mutex acquisition around `capture`/`triage`/a project read is not a
//! cycle. `elapsed_ms` for these events is measured from this module's own
//! first-use instant (a `std::time::Instant`, captured on first acquisition
//! attempt in this process), not from `DIAGNOSTIC_SESSION`'s
//! Android-`SystemClock`-relative origin — there is no way to read
//! `SystemClock` from native Rust, and no Kotlin call boundary at the exact
//! moment a mutex is contended to ask it for one. `wall_clock_ms` (from
//! `SystemTime::now()`) stays the authoritative cross-host correlation
//! field; `elapsed_ms` here is only for reading the *relative* gap between
//! two of this module's own events, which is what "how long did the wait
//! take" needs.
//!
//! **Production wiring is intentionally partial — stated here, not hidden.**
//! [`MobileTaskHost`] holds a [`BufferingSink`] (bounded, drops oldest on
//! overflow) rather than [`hummingbird_core::diagnostics::NullSink`], and
//! [`MobileTaskHost::take_diagnostic_events`] lets a host drain it into the
//! real journal (`DiagnosticsRecorder.appendRaw`, wired from `SyncWorker`
//! only, in this slice). A capture/triage/projects call with no sync
//! anywhere near it still buffers its spans correctly and they still
//! export next time a sync runs — nothing is lost — but they do not reach
//! the on-disk journal the instant they happen. Every acceptance criterion
//! this module exists for is proven directly against the sink in `cargo
//! test`, per this issue's own "Verify" section (which never asks for a
//! Kotlin-level assertion on this half at all): the on-device confirmation
//! it does ask for is `worker.started`/`worker.finished`, already wired to
//! the real recorder before this slice.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::Instant;

use hummingbird_core::diagnostics::{
    CoreOwner, DiagnosticEvent, DiagnosticEventV1, DiagnosticSink, Source,
    DIAGNOSTIC_EVENT_SCHEMA_VERSION,
};

/// The instant this module's `elapsed_ms` is measured from — set once, on
/// this process's first mutex acquisition of any kind. See the module doc
/// for why this is a separate clock from `DIAGNOSTIC_SESSION`'s.
static ANCHOR: OnceLock<Instant> = OnceLock::new();

fn elapsed_ms_since_anchor() -> u64 {
    let anchor = ANCHOR.get_or_init(Instant::now);
    anchor.elapsed().as_millis() as u64
}

fn wall_clock_ms_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The maximum number of events [`BufferingSink`] keeps before it starts
/// dropping the oldest — a diagnostics feature must never let an
/// un-drained host grow this without bound. 500 is generous for "since the
/// last sync" on any real cadence (#710's brief: an hourly WorkManager
/// refresh, plus whatever foreground activity happens in between).
const BUFFER_CAPACITY: usize = 500;

/// A [`DiagnosticSink`] that keeps what it is given, in order, up to
/// [`BUFFER_CAPACITY`] — the mobile host's own production sink (see the
/// module doc's "production wiring" note) and this module's own test
/// fixture, sharing one implementation so what a test proves is exactly
/// what production runs.
#[derive(Debug, Default)]
pub struct BufferingSink {
    events: StdMutex<Vec<DiagnosticEventV1>>,
}

impl BufferingSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every buffered event, oldest first, emptying the buffer — the drain
    /// [`crate::MobileTaskHost::take_diagnostic_events`] exposes.
    ///
    /// **Never `.unwrap()`s a poisoned lock.** A diagnostic that panics
    /// while observing the app it watches is worse than one that drops an
    /// event — review round 1's own finding — so a poisoned `std::sync
    /// ::Mutex` (some *other* panic happened while it was held; nothing in
    /// this small critical section itself panics) recovers the guard via
    /// `into_inner` rather than propagating. The buffered events are still
    /// perfectly readable; only the *poisoned* flag is discarded.
    pub fn drain(&self) -> Vec<DiagnosticEventV1> {
        let mut events = self.events.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::take(&mut *events)
    }

    /// Test-only convenience: a copy of what is buffered, without draining.
    #[cfg(test)]
    pub fn snapshot(&self) -> Vec<DiagnosticEventV1> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl DiagnosticSink for BufferingSink {
    /// Same poison-recovery rule as [`Self::drain`] — see that doc. This is
    /// the path [`OwnedGuard::drop`] reaches on every release, so a panic
    /// escaping from here would abort the very process this sink exists to
    /// observe.
    fn record(&self, event: DiagnosticEventV1) {
        let mut events = self.events.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if events.len() >= BUFFER_CAPACITY {
            events.remove(0);
        }
        events.push(event);
    }
}

/// The running `seq` counter these events are stamped with — constructed
/// once per [`crate::MobileTaskHost`] and reused for every acquisition, so
/// a whole process's core-ownership spans order correctly against each
/// other. **Not** the session id: that is looked up fresh at every `emit`
/// call (a `Fn() -> String` the caller supplies, e.g. `lib.rs`'s
/// `DIAGNOSTIC_SESSION`) rather than captured once here, because
/// `MobileTaskHost::init` can run *before* the host has ever called
/// `diagnostic_init_session` — `CoreHolder.create`'s own doc: `MobileTaskHost
/// .init` happens first, `session.started` (which is what actually sets
/// the identity) right after. Capturing a session id at construction time
/// would risk minting a second, disagreeing one.
pub struct CoreLockSession {
    /// **Not owned.** A reference to the *one* process-wide `seq` counter
    /// every Android-sourced event under one session shares —
    /// `lib.rs`'s `DIAGNOSTIC_SESSION.seq`, the same counter
    /// `diagnostic_event_json` advances for `session.started`/`worker.*`/
    /// `push.received`/`network.changed`. Review round 1 caught an
    /// earlier version of this type owning its own `AtomicU64::new(0)`:
    /// two counters under one `session_id`, landing in one journal via
    /// `record`/`appendRaw`, mint colliding `seq` values with no total
    /// order — exactly the failure `DiagnosticSession`'s own contract
    /// ("`seq` keeps counting … for one session") forbids. There must be
    /// exactly one counter per session; this is a borrow of it, not a
    /// second one.
    seq: &'static AtomicU64,
}

impl CoreLockSession {
    /// `seq` is the caller's counter — production hands in
    /// `&DIAGNOSTIC_SESSION.seq`; a test hands in its own `'static`
    /// (a `static` local) so two tests never share state, the same
    /// isolation the old owned counter gave without the cross-family
    /// collision.
    pub fn new(seq: &'static AtomicU64) -> Self {
        Self { seq }
    }

    fn emit(&self, sink: &dyn DiagnosticSink, session_id: &str, event: DiagnosticEvent) {
        sink.record(DiagnosticEventV1 {
            schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
            seq: self.seq.fetch_add(1, Ordering::Relaxed),
            wall_clock_ms: wall_clock_ms_now(),
            elapsed_ms: elapsed_ms_since_anchor(),
            session_id: session_id.to_string(),
            source: Source::Android,
            cycle_id: None,
            operation_id: None,
            request_id: None,
            event,
        });
    }

    /// Same as [`Self::emit`], but for an operation-scoped event
    /// (`operation.requested`/`operation.local_commit`) — the one place
    /// this session stamps `operation_id` rather than leaving it `None`.
    fn emit_for_operation(
        &self,
        sink: &dyn DiagnosticSink,
        session_id: &str,
        operation_id: &str,
        event: DiagnosticEvent,
    ) {
        sink.record(DiagnosticEventV1 {
            schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
            seq: self.seq.fetch_add(1, Ordering::Relaxed),
            wall_clock_ms: wall_clock_ms_now(),
            elapsed_ms: elapsed_ms_since_anchor(),
            session_id: session_id.to_string(),
            source: Source::Android,
            cycle_id: None,
            operation_id: Some(operation_id.to_string()),
            request_id: None,
            event,
        });
    }

    pub fn emit_operation_requested(&self, sink: &dyn DiagnosticSink, session_id: &str, operation_id: &str) {
        self.emit_for_operation(sink, session_id, operation_id, DiagnosticEvent::OperationRequested);
    }

    pub fn emit_operation_local_commit(&self, sink: &dyn DiagnosticSink, session_id: &str, operation_id: &str) {
        self.emit_for_operation(sink, session_id, operation_id, DiagnosticEvent::OperationLocalCommit);
    }
}

/// The tiny "who holds it right now" breadcrumb beside the real
/// `tokio::sync::Mutex` — see the module doc for why this exists and what
/// it is not.
#[derive(Debug, Default)]
pub struct CoreOwnershipTracker {
    current: StdMutex<Option<CoreOwner>>,
}

impl CoreOwnershipTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Poison-recovering, same rule as [`BufferingSink::record`] — this is
    /// reached from [`OwnedGuard::drop`] on every release, so it must never
    /// panic.
    fn snapshot(&self) -> Option<CoreOwner> {
        *self.current.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Poison-recovering — see [`Self::snapshot`].
    fn set(&self, owner: Option<CoreOwner>) {
        *self.current.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = owner;
    }
}

/// Acquires `mutex`, emitting `core.wait_started` (naming whoever
/// [`CoreOwnershipTracker`] currently says holds it, or `owner` itself when
/// it is free) then `core.acquired` (naming `owner`), and returning a guard
/// whose [`Drop`] records `core.released` unconditionally — including when
/// the future this call is part of is dropped before ever reaching the
/// caller's own code after the `.await` (a cancelled uniffi call).
pub async fn lock_with_diagnostics<'a, T>(
    mutex: &'a tokio::sync::Mutex<T>,
    tracker: &'a CoreOwnershipTracker,
    session: &'a CoreLockSession,
    sink: &'a dyn DiagnosticSink,
    session_id: &str,
    owner: CoreOwner,
) -> OwnedGuard<'a, T> {
    let waiting_on = tracker.snapshot().unwrap_or(owner);
    session.emit(sink, session_id, DiagnosticEvent::CoreWaitStarted { owner: Some(waiting_on) });
    let guard = mutex.lock().await;
    tracker.set(Some(owner));
    session.emit(sink, session_id, DiagnosticEvent::CoreAcquired { owner: Some(owner) });
    OwnedGuard {
        guard: Some(guard),
        owner,
        tracker,
        session,
        sink,
        session_id: session_id.to_string(),
    }
}

/// The `tokio::sync::MutexGuard` wrapper [`lock_with_diagnostics`] hands
/// back — transparent `Deref`/`DerefMut` onto the wrapped value, so every
/// existing `inner.core.…`/`inner.api_key` call site is unchanged, plus a
/// `Drop` that always records `core.released` and clears the ownership
/// breadcrumb before the real guard (held in the `Option` below) drops and
/// actually releases `tokio::sync::Mutex`'s lock.
pub struct OwnedGuard<'a, T> {
    guard: Option<tokio::sync::MutexGuard<'a, T>>,
    owner: CoreOwner,
    tracker: &'a CoreOwnershipTracker,
    session: &'a CoreLockSession,
    sink: &'a dyn DiagnosticSink,
    session_id: String,
}

impl<'a, T> std::ops::Deref for OwnedGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.guard.as_ref().expect("OwnedGuard drops its inner guard exactly once, in Drop")
    }
}

impl<'a, T> std::ops::DerefMut for OwnedGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.guard.as_mut().expect("OwnedGuard drops its inner guard exactly once, in Drop")
    }
}

impl<'a, T> Drop for OwnedGuard<'a, T> {
    fn drop(&mut self) {
        self.tracker.set(None);
        self.session
            .emit(self.sink, &self.session_id, DiagnosticEvent::CoreReleased { owner: self.owner });
        // Dropping the real guard (still held in `self.guard`) happens
        // after this fn returns, when `self` itself finishes dropping —
        // `core.released` is recorded before the async mutex is actually
        // unlocked, matching `core.acquired`/`core.wait_started`'s own
        // "diagnostic first, real effect follows" order.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    fn events_of(sink: &BufferingSink) -> Vec<DiagnosticEvent> {
        sink.snapshot().into_iter().map(|e| e.event).collect()
    }

    /// A fresh, test-isolated `seq` counter — [`CoreLockSession`] now
    /// borrows one rather than owning it (review round 1's fix for the
    /// two-counters-one-session collision), so each test that needs its
    /// own independent counter leaks a fresh `AtomicU64` rather than
    /// sharing `lib.rs`'s process-wide production one.
    fn test_seq() -> &'static AtomicU64 {
        Box::leak(Box::new(AtomicU64::new(0)))
    }

    #[tokio::test]
    async fn an_uncontended_acquisition_emits_wait_started_acquired_released_in_order() {
        let mutex = tokio::sync::Mutex::new(0u32);
        let tracker = CoreOwnershipTracker::new();
        let session = CoreLockSession::new(test_seq());
        let sink = BufferingSink::new();

        {
            let guard = lock_with_diagnostics(&mutex, &tracker, &session, &sink, "s-1", CoreOwner::Capture).await;
            assert_eq!(*guard, 0);
        }

        let events = events_of(&sink);
        assert_eq!(
            events,
            vec![
                DiagnosticEvent::CoreWaitStarted { owner: Some(CoreOwner::Capture) },
                DiagnosticEvent::CoreAcquired { owner: Some(CoreOwner::Capture) },
                DiagnosticEvent::CoreReleased { owner: CoreOwner::Capture },
            ]
        );
    }

    /// The acceptance criterion, almost verbatim: a capture/triage/project
    /// read that starts while a sync holds the mutex (behind a
    /// never-resolving await, standing in for #706/#708's own
    /// never-resolving transport fixture — the general mechanism under
    /// test here is the mutex wrapper, not `Core::run` itself) sees a
    /// `core.wait_started` naming `sync` as the current owner.
    #[tokio::test]
    async fn a_wait_behind_a_never_resolving_sync_hold_names_sync_as_the_owner() {
        let mutex = Arc::new(tokio::sync::Mutex::new(0u32));
        let tracker = Arc::new(CoreOwnershipTracker::new());
        let session = Arc::new(CoreLockSession::new(test_seq()));
        let sink = Arc::new(BufferingSink::new());

        // "Sync" acquires the mutex and then hangs forever inside it,
        // mirroring `MobileTaskHost::run` awaiting a never-resolving
        // transport with the lock held.
        let (m, t, s, k) = (mutex.clone(), tracker.clone(), session.clone(), sink.clone());
        let sync_task = tokio::spawn(async move {
            let _guard = lock_with_diagnostics(&m, &t, &s, k.as_ref(), "s-1", CoreOwner::Sync).await;
            std::future::pending::<()>().await;
        });

        // Give the spawned task a real chance to acquire before the three
        // waiters below start — a channel would be more precise, but a
        // short sleep is simple and this assertion only needs "acquired
        // before we check", not an exact race-free bound.
        tokio::time::sleep(Duration::from_millis(20)).await;

        for owner in [CoreOwner::Capture, CoreOwner::Triage, CoreOwner::Read] {
            let (m, t, s, k) = (mutex.clone(), tracker.clone(), session.clone(), sink.clone());
            // Each waiter would block forever behind the still-held mutex,
            // so this only needs to observe the `core.wait_started` it
            // records *before* `.lock().await` — never the acquisition
            // itself, which never comes.
            let waiter = tokio::spawn(async move {
                let _ = lock_with_diagnostics(&m, &t, &s, k.as_ref(), "s-1", owner).await;
            });
            tokio::time::sleep(Duration::from_millis(10)).await;
            waiter.abort();
        }

        sync_task.abort();

        let waits: Vec<Option<CoreOwner>> = events_of(&sink)
            .into_iter()
            .filter_map(|event| match event {
                DiagnosticEvent::CoreWaitStarted { owner } => Some(owner),
                _ => None,
            })
            .collect();
        // The first wait_started is sync's own (the mutex was free, so it
        // names itself); the following three are capture/triage/project
        // read's, each naming `sync` as the owner they found already
        // holding it. Every producer here is this crate's own Rust
        // `lock_with_diagnostics`, which always knows its owner, so every
        // entry is `Some` — only a non-Rust writer (the web SharedWorker)
        // ever emits `None`.
        assert_eq!(
            waits,
            vec![
                Some(CoreOwner::Sync),
                Some(CoreOwner::Sync),
                Some(CoreOwner::Sync),
                Some(CoreOwner::Sync),
            ]
        );
    }

    /// #710's other central proof: a cancelled in-flight acquisition still
    /// records `core.released`. Dropping the `JoinHandle` after `.abort()`
    /// drops the task's future, which drops `OwnedGuard`, which records
    /// the release — without ever reaching any code after the `.await` in
    /// the task body.
    #[tokio::test]
    async fn a_cancelled_holder_still_records_core_released() {
        let mutex = Arc::new(tokio::sync::Mutex::new(0u32));
        let tracker = Arc::new(CoreOwnershipTracker::new());
        let session = Arc::new(CoreLockSession::new(test_seq()));
        let sink = Arc::new(BufferingSink::new());

        let (m, t, s, k) = (mutex.clone(), tracker.clone(), session.clone(), sink.clone());
        let task = tokio::spawn(async move {
            let _guard = lock_with_diagnostics(&m, &t, &s, k.as_ref(), "s-1", CoreOwner::Sync).await;
            std::future::pending::<()>().await;
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        task.abort();
        // Let the abort actually run and drop the future.
        let _ = task.await;

        let events = events_of(&sink);
        assert!(
            events.contains(&DiagnosticEvent::CoreReleased { owner: CoreOwner::Sync }),
            "a cancelled holder must still record core.released: {events:?}"
        );
        // The ownership tracker must also have been cleared, or every
        // later acquisition would wrongly report `sync` as still holding
        // it forever.
        assert_eq!(tracker.snapshot(), None);
    }

    /// Mutation check on the guard itself (not run in CI — a manual proof
    /// this test file's own assertions are load-bearing): commenting out
    /// `self.tracker.set(None)` in `OwnedGuard::drop` makes the assertion
    /// above fail, since the tracker would still read
    /// `Some(CoreOwner::Sync)` after the cancelled task's guard dropped.
    /// Left as a doc note rather than a runnable test because there is no
    /// way to assert "the source *would* fail a specific mutation" from
    /// within the suite that source compiles into.
    #[test]
    fn mutation_note_tracker_clear_on_drop() {}

    #[tokio::test]
    async fn a_second_acquisition_by_the_same_owner_after_release_finds_the_lock_free() {
        let mutex = tokio::sync::Mutex::new(0u32);
        let tracker = CoreOwnershipTracker::new();
        let session = CoreLockSession::new(test_seq());
        let sink = BufferingSink::new();

        {
            let _guard = lock_with_diagnostics(&mutex, &tracker, &session, &sink, "s-1", CoreOwner::Triage).await;
        }
        {
            let _guard = lock_with_diagnostics(&mutex, &tracker, &session, &sink, "s-1", CoreOwner::Triage).await;
        }

        let waits: Vec<Option<CoreOwner>> = events_of(&sink)
            .into_iter()
            .filter_map(|event| match event {
                DiagnosticEvent::CoreWaitStarted { owner } => Some(owner),
                _ => None,
            })
            .collect();
        assert_eq!(waits, vec![Some(CoreOwner::Triage), Some(CoreOwner::Triage)]);
    }

    #[test]
    fn the_buffering_sink_drops_the_oldest_event_once_over_capacity() {
        let sink = BufferingSink::new();
        for _ in 0..(BUFFER_CAPACITY + 10) {
            sink.record(DiagnosticEventV1 {
                schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
                seq: 0,
                wall_clock_ms: 0,
                elapsed_ms: 0,
                session_id: "s".to_string(),
                source: Source::Android,
                cycle_id: None,
                operation_id: None,
                request_id: None,
                event: DiagnosticEvent::OperationRequested,
            });
        }
        assert_eq!(sink.snapshot().len(), BUFFER_CAPACITY);
    }

    #[test]
    fn drain_empties_the_buffer() {
        let sink = BufferingSink::new();
        sink.record(DiagnosticEventV1 {
            schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
            seq: 0,
            wall_clock_ms: 0,
            elapsed_ms: 0,
            session_id: "s".to_string(),
            source: Source::Android,
            cycle_id: None,
            operation_id: None,
            request_id: None,
            event: DiagnosticEvent::OperationRequested,
        });
        assert_eq!(sink.drain().len(), 1);
        assert_eq!(sink.snapshot().len(), 0);
    }

    /// Review round 1, finding 7: a poisoned `std::sync::Mutex` must never
    /// panic on [`CoreOwnershipTracker::snapshot`]/`set` — both are
    /// reached from [`OwnedGuard::drop`] on every release, and a panic
    /// escaping a `Drop` during another unwind aborts the whole process
    /// (the exact "a diagnostic that breaks what it observes" failure
    /// mode #707 was rejected for). Poisons the tracker's own mutex from a
    /// second thread (lock it, then panic while it is held — the standard
    /// way to poison a `std::sync::Mutex` in a test) and proves both
    /// methods still answer normally afterward.
    #[test]
    fn a_poisoned_tracker_mutex_never_panics_on_snapshot_or_set() {
        let tracker = Arc::new(CoreOwnershipTracker::new());
        let poisoning = tracker.clone();
        // `.join()`'s `Err` (the panic payload) is deliberately dropped,
        // not unwrapped — unwrapping it would re-raise the very panic this
        // test is using to poison the mutex.
        let _ = std::thread::spawn(move || {
            let _guard = poisoning.current.lock().unwrap();
            panic!("deliberately poisoning CoreOwnershipTracker's mutex");
        })
        .join();

        let snapshot_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tracker.snapshot()));
        assert!(snapshot_result.is_ok(), "snapshot() must not panic on a poisoned mutex");

        let set_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tracker.set(Some(CoreOwner::Sync))
        }));
        assert!(set_result.is_ok(), "set() must not panic on a poisoned mutex");
        assert_eq!(tracker.snapshot(), Some(CoreOwner::Sync), "the write through the recovered guard must still take");
    }

    /// Same proof as the tracker test above, for [`BufferingSink::record`]
    /// — the method [`DiagnosticSink::record`] calls on every emission,
    /// including from inside [`OwnedGuard::drop`].
    #[test]
    fn a_poisoned_sink_mutex_never_panics_on_record_or_drain() {
        let sink = Arc::new(BufferingSink::new());
        let poisoning = sink.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoning.events.lock().unwrap();
            panic!("deliberately poisoning BufferingSink's mutex");
        })
        .join();

        let record_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sink.record(DiagnosticEventV1 {
                schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
                seq: 0,
                wall_clock_ms: 0,
                elapsed_ms: 0,
                session_id: "s".to_string(),
                source: Source::Android,
                cycle_id: None,
                operation_id: None,
                request_id: None,
                event: DiagnosticEvent::OperationRequested,
            })
        }));
        assert!(record_result.is_ok(), "record() must not panic on a poisoned mutex");

        let drain_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink.drain()));
        assert!(drain_result.is_ok(), "drain() must not panic on a poisoned mutex");
        assert_eq!(drain_result.unwrap().len(), 1, "the event recorded through the recovered guard must still be there");
    }

    /// End-to-end version of the two tests above: a real `OwnedGuard` drop
    /// (the actual call site review round 1 flagged) with the tracker's
    /// mutex already poisoned, proving the release path itself — not just
    /// the two methods it calls — survives.
    #[tokio::test]
    async fn a_guard_release_survives_a_poisoned_tracker_mutex() {
        let mutex = tokio::sync::Mutex::new(0u32);
        let tracker = Arc::new(CoreOwnershipTracker::new());
        let session = CoreLockSession::new(test_seq());
        let sink = BufferingSink::new();

        let guard = lock_with_diagnostics(&mutex, &tracker, &session, &sink, "s-1", CoreOwner::Sync).await;

        let poisoning = tracker.clone();
        let _ = std::thread::spawn(move || {
            let _inner_guard = poisoning.current.lock().unwrap();
            panic!("deliberately poisoning CoreOwnershipTracker's mutex before release");
        })
        .join();

        let drop_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(guard)));
        assert!(drop_result.is_ok(), "dropping the guard must not panic even with the tracker's mutex poisoned");

        let events = events_of(&sink);
        assert!(
            events.contains(&DiagnosticEvent::CoreReleased { owner: CoreOwner::Sync }),
            "core.released must still be recorded despite the poisoned tracker mutex: {events:?}",
        );
    }
}
