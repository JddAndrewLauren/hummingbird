//! [`DiagnosticSession`]/[`DiagnosticsContext`]: the two-tier bundle an
//! observed sync cycle carries through
//! [`crate::sync::cycle::SyncCycle::run_observed`].
//!
//! **The split, and why it exists (review round 1, finding 2/3).** `seq`
//! and `elapsed_ms`'s origin are documented on [`super::DiagnosticEventV1`]
//! as monotonic *within one session* — a session outliving any single sync
//! cycle. [`DiagnosticsContext`] itself is necessarily per-cycle (one
//! `cycle_id`, one `wall_clock_ms` reused from that cycle's own `now_ms`),
//! so the session-scoped counters cannot live on it without restarting
//! every cycle — which is exactly what round 1 found: `seq` reset to 0 and
//! the monotonic origin re-sampled on every `DiagnosticsContext::new` call.
//! [`DiagnosticSession`] is what a host constructs **once**, for the life
//! of one session, and hands to every `DiagnosticsContext::new` it builds
//! across however many cycles that session runs — `seq` keeps counting
//! across cycles, and `origin_monotonic_ms` is the one reading
//! [`DiagnosticSession::new`]'s caller supplies, never sampled by this
//! module (the brief's "measured from a caller-supplied origin, not
//! sampled" is now literal: the origin arrives as a constructor argument,
//! not a `compare_exchange` against the clock's first call).
//!
//! [`DiagnosticsContext`] also owns the
//! [`InstrumentedChangesTransport`]/[`InstrumentedMutationTransport`]
//! decorators that wrap a caller's transport for exactly one observed
//! cycle, attaching headers and emitting `http.started`/`http.finished`
//! (plus the slow/stalled watchdog) around the real call — with **zero**
//! change to [`crate::sync::adapter::fetch_delta`]/`fetch_sweep` or
//! [`crate::sync::queue::OutboundQueue::drain`], which see only an
//! ordinary `&impl ChangesTransport`/`&impl MutationTransport` and have no
//! idea one is instrumented.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use futures_util::future::{select, Either};

use crate::sync::transport::{ChangesTransport, TransportError};
use crate::sync::write::transport::{MutationRequest, MutationTransport, RawResponse};

use super::clock::{DiagnosticClock, SLOW_AFTER_MS, STALLED_AFTER_MS};
use super::failure::classify_transport_error;
use super::route::{route_template, sanitize_header_value, CorrelationHeaders};
use super::{
    DiagnosticEvent, DiagnosticEventV1, DiagnosticHttpMethod, DiagnosticSink, Source,
    SyncOutcome, SyncPhase, DIAGNOSTIC_EVENT_SCHEMA_VERSION,
};

// A boxed, `'a`-scoped future. `?Send` on `wasm32` (single-threaded, and
// nothing there is ever `Send` anyway) matching every other trait in this
// crate's dual `#[cfg_attr]` pattern; `Send`-bound elsewhere because
// `async_trait`'s default (non-`?Send`) mode — what `ChangesTransport`'s own
// native build uses — requires the futures it returns to be `Send`, even
// though nothing here is ever actually moved across a thread.
#[cfg(target_arch = "wasm32")]
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
#[cfg(not(target_arch = "wasm32"))]
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// `Send` on native, a no-op bound on `wasm32` — the same dual shape as
/// [`BoxFuture`], for the generic `F` [`DiagnosticsContext::watch_slow_stalled`]
/// boxes.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSend for T {}
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> MaybeSend for T {}

/// The session-scoped state every [`DiagnosticsContext`] a session builds
/// shares: the session id, the running `seq` counter, and the monotonic
/// origin `elapsed_ms` is measured from. A host constructs exactly one of
/// these per session (app launch to app close, roughly) and passes `&self`
/// into every `DiagnosticsContext::new` for however many sync cycles that
/// session runs — see the module docs for why this had to be split out of
/// `DiagnosticsContext` itself.
pub struct DiagnosticSession<'a> {
    session_id: &'a str,
    /// The one monotonic reading this session is measured from, taken by
    /// the caller (e.g. at session/process start) and handed in here —
    /// never sampled by this module. `elapsed_ms` on every event this
    /// session ever records is `clock.monotonic_ms() - origin_monotonic_ms`.
    origin_monotonic_ms: u64,
    seq: AtomicU64,
}

impl<'a> DiagnosticSession<'a> {
    pub fn new(session_id: &'a str, origin_monotonic_ms: u64) -> Self {
        Self {
            session_id,
            origin_monotonic_ms,
            seq: AtomicU64::new(0),
        }
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }
}

/// The per-cycle context every observed sync cycle carries. `cycle_id` is
/// caller-minted (this crate has no RNG — see the module docs); `wall_clock_ms`
/// is the cycle's own `now_ms`, reused rather than sampled a second time, so
/// every event in one cycle shares it and only `elapsed_ms` (via `clock`,
/// off `session`'s origin) actually varies event to event.
pub struct DiagnosticsContext<'a> {
    sink: &'a dyn DiagnosticSink,
    clock: &'a dyn DiagnosticClock,
    session: &'a DiagnosticSession<'a>,
    cycle_id: &'a str,
    platform: &'a str,
    build: &'a str,
    wall_clock_ms: i64,
    ordinal: AtomicU32,
}

impl<'a> DiagnosticsContext<'a> {
    /// `platform`/`build` are the identity the correlation headers carry,
    /// and **this is the only place a host supplies it** — the transports
    /// hold no copy of their own (see
    /// `sync::reqwest_transport::ReqwestSyncTransport`'s struct docs), so a
    /// host slice wiring identity in has exactly one seam and no second one
    /// that can disagree with it. Both values pass through
    /// [`crate::diagnostics::route::sanitize_header_value`] in
    /// [`Self::correlation_headers`] before they ever reach a header.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sink: &'a dyn DiagnosticSink,
        clock: &'a dyn DiagnosticClock,
        session: &'a DiagnosticSession<'a>,
        cycle_id: &'a str,
        platform: &'a str,
        build: &'a str,
        wall_clock_ms: i64,
    ) -> Self {
        Self {
            sink,
            clock,
            session,
            cycle_id,
            platform,
            build,
            wall_clock_ms,
            ordinal: AtomicU32::new(0),
        }
    }

    pub fn cycle_id(&self) -> &str {
        self.cycle_id
    }

    /// `<cycle_id>-<ordinal>` — the ordinal is per-context (per cycle) and
    /// monotonic, so two calls within the same observed cycle never collide
    /// and the order they were minted in is recoverable from the suffix.
    pub fn next_request_id(&self) -> String {
        let ordinal = self.ordinal.fetch_add(1, Ordering::Relaxed);
        format!("{}-{ordinal}", self.cycle_id)
    }

    /// The four correlation headers for one call, `request_id` already
    /// minted by [`DiagnosticsContext::next_request_id`]. Every value is
    /// routed through [`sanitize_header_value`] here — the one place every
    /// transport's attached header passes through (review round 1, finding
    /// 4) — so a malformed caller-minted or host-supplied value becomes the
    /// `"invalid"` sentinel rather than an out-of-pattern header the
    /// authority silently rejects.
    fn correlation_headers<'r>(&'r self, request_id: &'r str) -> CorrelationHeaders<'r> {
        CorrelationHeaders {
            cycle_id: sanitize_header_value(self.cycle_id),
            request_id: sanitize_header_value(request_id),
            platform: sanitize_header_value(self.platform),
            build: sanitize_header_value(self.build),
        }
    }

    fn elapsed_ms(&self) -> u64 {
        self.clock
            .monotonic_ms()
            .saturating_sub(self.session.origin_monotonic_ms)
    }

    fn emit(&self, request_id: Option<&str>, event: DiagnosticEvent) {
        self.sink.record(DiagnosticEventV1 {
            schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
            seq: self.session.next_seq(),
            wall_clock_ms: self.wall_clock_ms,
            elapsed_ms: self.elapsed_ms(),
            session_id: self.session.session_id.to_string(),
            source: Source::Core,
            cycle_id: Some(self.cycle_id.to_string()),
            operation_id: None,
            request_id: request_id.map(str::to_string),
            event,
        });
    }

    pub fn emit_sync_started(&self, force_full_sweep: bool) {
        self.emit(None, DiagnosticEvent::SyncStarted { force_full_sweep });
    }

    pub fn emit_sync_phase_started(&self, phase: SyncPhase) {
        self.emit(None, DiagnosticEvent::SyncPhaseStarted { phase });
    }

    pub fn emit_sync_phase_finished(&self, phase: SyncPhase) {
        self.emit(None, DiagnosticEvent::SyncPhaseFinished { phase });
    }

    pub fn emit_sync_finished(&self, outcome: SyncOutcome) {
        self.emit(None, DiagnosticEvent::SyncFinished { outcome });
    }

    fn emit_http_started(&self, request_id: &str, method: DiagnosticHttpMethod, route: &str) {
        self.emit(
            Some(request_id),
            DiagnosticEvent::HttpStarted {
                method,
                route: route.to_string(),
            },
        );
    }

    fn emit_http_finished(
        &self,
        request_id: &str,
        method: DiagnosticHttpMethod,
        route: &str,
        status: Option<u16>,
        failure: Option<super::FailureClass>,
    ) {
        self.emit(
            Some(request_id),
            DiagnosticEvent::HttpFinished {
                method,
                route: route.to_string(),
                status,
                failure,
            },
        );
    }

    fn emit_operation_slow(&self, request_id: &str) {
        self.emit(Some(request_id), DiagnosticEvent::OperationSlow);
    }

    fn emit_operation_stalled(&self, request_id: &str) {
        self.emit(Some(request_id), DiagnosticEvent::OperationStalled);
    }

    /// Races `op` against the 5s/30s thresholds, emitting `operation.slow`
    /// / `operation.stalled` (each carrying `request_id`, so a slow/stalled
    /// event is attributable to the exact call it watched — review round 1,
    /// finding 1) if `op` has not resolved by then, and always returning
    /// whatever `op` eventually produces. Entirely driven by
    /// `self.clock.sleep_ms` — see [`DiagnosticClock`]'s docs on why this
    /// crate has no timer of its own to drive it with instead.
    async fn watch_slow_stalled<'f, F, T>(&'f self, request_id: &'f str, op: F) -> T
    where
        F: Future<Output = T> + MaybeSend + 'f,
        T: MaybeSend,
    {
        let op: BoxFuture<'f, T> = Box::pin(op);
        let slow_sleep: BoxFuture<'f, ()> = Box::pin(self.clock.sleep_ms(SLOW_AFTER_MS));
        match select(op, slow_sleep).await {
            Either::Left((value, _)) => value,
            Either::Right((_, remaining_op)) => {
                self.emit_operation_slow(request_id);
                let stalled_sleep: BoxFuture<'f, ()> =
                    Box::pin(self.clock.sleep_ms(STALLED_AFTER_MS - SLOW_AFTER_MS));
                match select(remaining_op, stalled_sleep).await {
                    Either::Left((value, _)) => value,
                    Either::Right((_, remaining_op)) => {
                        self.emit_operation_stalled(request_id);
                        remaining_op.await
                    }
                }
            }
        }
    }
}

/// Wraps a `&'a R: ChangesTransport` for exactly one observed cycle —
/// attaches correlation headers via [`ChangesTransport::fetch_changes_with_headers`]/
/// `fetch_sweep_with_headers` and emits `http.started` before the awaited
/// call, `http.finished` after (or never, if the call never resolves —
/// this issue's central proof).
pub struct InstrumentedChangesTransport<'a, R> {
    inner: &'a R,
    diagnostics: &'a DiagnosticsContext<'a>,
}

impl<'a, R> InstrumentedChangesTransport<'a, R> {
    pub fn new(inner: &'a R, diagnostics: &'a DiagnosticsContext<'a>) -> Self {
        Self { inner, diagnostics }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<'a, R: ChangesTransport> ChangesTransport for InstrumentedChangesTransport<'a, R> {
    async fn fetch_changes(&self, access_token: &str, since: i64) -> Result<String, TransportError> {
        const ROUTE: &str = "/api/changes";
        let method = DiagnosticHttpMethod::Get;
        let request_id = self.diagnostics.next_request_id();
        self.diagnostics.emit_http_started(&request_id, method, ROUTE);
        let headers = self.diagnostics.correlation_headers(&request_id);
        let result = self
            .diagnostics
            .watch_slow_stalled(
                &request_id,
                self.inner.fetch_changes_with_headers(access_token, since, &headers),
            )
            .await;
        self.report(&request_id, method, ROUTE, &result);
        result
    }

    async fn fetch_sweep(&self, access_token: &str) -> Result<String, TransportError> {
        const ROUTE: &str = "/api/sweep";
        let method = DiagnosticHttpMethod::Get;
        let request_id = self.diagnostics.next_request_id();
        self.diagnostics.emit_http_started(&request_id, method, ROUTE);
        let headers = self.diagnostics.correlation_headers(&request_id);
        let result = self
            .diagnostics
            .watch_slow_stalled(&request_id, self.inner.fetch_sweep_with_headers(access_token, &headers))
            .await;
        self.report(&request_id, method, ROUTE, &result);
        result
    }
}

impl<'a, R: ChangesTransport> InstrumentedChangesTransport<'a, R> {
    fn report(
        &self,
        request_id: &str,
        method: DiagnosticHttpMethod,
        route: &str,
        result: &Result<String, TransportError>,
    ) {
        match result {
            Ok(_) => self.diagnostics.emit_http_finished(request_id, method, route, None, None),
            Err(error) => self.diagnostics.emit_http_finished(
                request_id,
                method,
                route,
                error.status,
                Some(classify_transport_error(error)),
            ),
        }
    }
}

/// The [`InstrumentedChangesTransport`] mirror for the write side — wraps a
/// `&'a W: MutationTransport`, attaching headers via
/// [`MutationTransport::send_with_headers`] and emitting the same
/// `http.*`/slow/stalled events around each queued mutation's actual send.
pub struct InstrumentedMutationTransport<'a, W> {
    inner: &'a W,
    diagnostics: &'a DiagnosticsContext<'a>,
}

impl<'a, W> InstrumentedMutationTransport<'a, W> {
    pub fn new(inner: &'a W, diagnostics: &'a DiagnosticsContext<'a>) -> Self {
        Self { inner, diagnostics }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<'a, W: MutationTransport> MutationTransport for InstrumentedMutationTransport<'a, W> {
    async fn send(&self, access_token: &str, request: MutationRequest) -> Result<RawResponse, TransportError> {
        let method = match request.method {
            crate::sync::write::transport::HttpMethod::Post => DiagnosticHttpMethod::Post,
            crate::sync::write::transport::HttpMethod::Patch => DiagnosticHttpMethod::Patch,
            crate::sync::write::transport::HttpMethod::Put => DiagnosticHttpMethod::Put,
        };
        let route = route_template(&request.path);
        let request_id = self.diagnostics.next_request_id();
        self.diagnostics.emit_http_started(&request_id, method, &route);
        let headers = self.diagnostics.correlation_headers(&request_id);
        let result = self
            .diagnostics
            .watch_slow_stalled(&request_id, self.inner.send_with_headers(access_token, request, &headers))
            .await;
        match &result {
            Ok(response) => {
                self.diagnostics
                    .emit_http_finished(&request_id, method, &route, Some(response.status), None)
            }
            Err(error) => self.diagnostics.emit_http_finished(
                &request_id,
                method,
                &route,
                error.status,
                Some(classify_transport_error(error)),
            ),
        }
        result
    }
}

/// Collapses [`crate::sync::cycle::CycleOutcome`] to the redacted
/// [`SyncOutcome`] `sync.finished` records.
pub fn sync_outcome_of(outcome: &crate::sync::cycle::CycleOutcome) -> SyncOutcome {
    use crate::sync::cycle::CycleOutcome;
    match outcome {
        CycleOutcome::Skipped => SyncOutcome::Skipped,
        CycleOutcome::Blocked { .. } => SyncOutcome::Blocked,
        CycleOutcome::CredentialNeeded { .. } => SyncOutcome::CredentialNeeded,
        CycleOutcome::PersistFailed { .. } => SyncOutcome::PersistFailed,
        CycleOutcome::PullFailed { .. } => SyncOutcome::PullFailed,
        CycleOutcome::Completed { .. } => SyncOutcome::Completed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::test_support::RecordingClock;

    /// Review round 1, finding 3: the origin must be exactly what the
    /// caller passed to `DiagnosticSession::new`, never a value this module
    /// samples — pinned by using a clock whose very first `monotonic_ms()`
    /// reading (20) differs from the supplied origin (10), so a
    /// self-sampling implementation (the old `compare_exchange`) and a
    /// truly-caller-supplied one disagree on `elapsed_ms`.
    #[test]
    fn elapsed_ms_is_measured_from_the_session_supplied_origin_not_the_clocks_first_reading() {
        let sink = crate::diagnostics::test_support::RecordingSink::default();
        let clock = RecordingClock::default();
        clock.advance(20);
        let session = DiagnosticSession::new("s-1", 10);
        let diagnostics = DiagnosticsContext::new(&sink, &clock, &session, "c-1", "core", "test", 1_000);

        diagnostics.emit_sync_started(true);

        let events = sink.events();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].elapsed_ms, 10,
            "elapsed_ms must be clock(20) - the caller-supplied origin(10), not clock(20) - clock(20)"
        );
    }

    /// Review round 1, finding 2: `seq` must keep counting across cycles
    /// that share one `DiagnosticSession`, not reset per
    /// `DiagnosticsContext`. Two contexts, two "cycles", one session.
    #[test]
    fn seq_keeps_counting_across_multiple_cycles_sharing_one_session() {
        let sink = crate::diagnostics::test_support::RecordingSink::default();
        let clock = RecordingClock::default();
        let session = DiagnosticSession::new("s-1", 0);

        {
            let first_cycle = DiagnosticsContext::new(&sink, &clock, &session, "c-1", "core", "test", 1_000);
            first_cycle.emit_sync_started(true);
            first_cycle.emit_sync_finished(SyncOutcome::Completed);
        }
        {
            let second_cycle = DiagnosticsContext::new(&sink, &clock, &session, "c-2", "core", "test", 2_000);
            second_cycle.emit_sync_started(true);
        }

        let events = sink.events();
        let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
        assert_eq!(
            seqs,
            vec![0, 1, 2],
            "seq must be monotonic across the whole session's stream, not restart at each cycle boundary"
        );
    }

    /// Review round 1, finding 1: every `http.*`/slow/stalled event must
    /// carry the minted request id, not `None`.
    #[test]
    fn http_events_carry_the_minted_request_id() {
        let sink = crate::diagnostics::test_support::RecordingSink::default();
        let clock = RecordingClock::default();
        let session = DiagnosticSession::new("s-1", 0);
        let diagnostics = DiagnosticsContext::new(&sink, &clock, &session, "cycle-9", "core", "test", 1_000);

        diagnostics.emit_http_started("cycle-9-0", DiagnosticHttpMethod::Get, "/api/sweep");
        diagnostics.emit_http_finished("cycle-9-0", DiagnosticHttpMethod::Get, "/api/sweep", Some(200), None);

        let events = sink.events();
        assert_eq!(events.len(), 2);
        for event in &events {
            assert_eq!(event.request_id.as_deref(), Some("cycle-9-0"));
        }
    }

    /// Review round 1, finding 4: an invalid platform/build must never
    /// reach a header verbatim. The `build` is the invalid one here (a
    /// version string with spaces and parentheses) — `platform` is valid
    /// and must survive untouched alongside it.
    #[test]
    fn an_invalid_build_is_sanitized_before_it_reaches_the_correlation_headers() {
        let sink = crate::diagnostics::test_support::RecordingSink::default();
        let clock = RecordingClock::default();
        let session = DiagnosticSession::new("s-1", 0);
        let diagnostics =
            DiagnosticsContext::new(&sink, &clock, &session, "cycle-1", "core", "1.2.3 (dev build)", 1_000);

        let headers = diagnostics.correlation_headers("cycle-1-0");

        assert_eq!(headers.build, "invalid");
        assert_eq!(headers.platform, "core");
        assert!(super::super::route::is_valid_header_value(headers.build));
    }
}
