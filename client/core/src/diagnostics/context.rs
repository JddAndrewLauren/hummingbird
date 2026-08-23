//! [`DiagnosticsContext`]: the per-cycle bundle an observed sync cycle
//! carries through [`crate::sync::cycle::SyncCycle::run_observed`] — the
//! sink, the clock, the session/cycle ids, and the per-cycle request-id
//! ordinal every `http.*` event and correlation header needs. Also owns the
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
use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering};

use futures_util::future::{select, Either};

use crate::sync::transport::{ChangesTransport, TransportError};
use crate::sync::write::transport::{MutationRequest, MutationTransport, RawResponse};

use super::clock::{DiagnosticClock, SLOW_AFTER_MS, STALLED_AFTER_MS};
use super::failure::classify_transport_error;
use super::route::{route_template, CorrelationHeaders};
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

/// The per-cycle context every observed sync cycle carries. `session_id`
/// and `cycle_id` are caller-minted (this crate has no RNG — see the module
/// docs); `wall_clock_ms` is the cycle's own `now_ms`, reused rather than
/// sampled a second time, so every event in one cycle shares it and only
/// `elapsed_ms` (via `clock`) actually varies event to event.
pub struct DiagnosticsContext<'a> {
    sink: &'a dyn DiagnosticSink,
    clock: &'a dyn DiagnosticClock,
    session_id: &'a str,
    cycle_id: &'a str,
    platform: &'a str,
    build: &'a str,
    wall_clock_ms: i64,
    seq: AtomicU64,
    ordinal: AtomicU32,
    /// `-1` means "not yet taken" — an `AtomicI64` sentinel rather than
    /// `Cell<Option<u64>>` because [`DiagnosticSink`]/[`ChangesTransport`]
    /// require `Sync`, which `Cell` (single-threaded interior mutability)
    /// cannot give; every real monotonic reading fits comfortably under
    /// `i64::MAX`.
    origin_monotonic_ms: AtomicI64,
}

impl<'a> DiagnosticsContext<'a> {
    /// `platform`/`build` are the identity the correlation headers carry —
    /// see `sync::reqwest_transport::ReqwestSyncTransport::with_client_identity`
    /// for the transport-level equivalent this context's headers must
    /// agree with when both are supplied by the same host.
    pub fn new(
        sink: &'a dyn DiagnosticSink,
        clock: &'a dyn DiagnosticClock,
        session_id: &'a str,
        cycle_id: &'a str,
        platform: &'a str,
        build: &'a str,
        wall_clock_ms: i64,
    ) -> Self {
        Self {
            sink,
            clock,
            session_id,
            cycle_id,
            platform,
            build,
            wall_clock_ms,
            seq: AtomicU64::new(0),
            ordinal: AtomicU32::new(0),
            origin_monotonic_ms: AtomicI64::new(-1),
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

    fn correlation_headers<'r>(&'r self, request_id: &'r str) -> CorrelationHeaders<'r> {
        CorrelationHeaders {
            cycle_id: self.cycle_id,
            request_id,
            platform: self.platform,
            build: self.build,
        }
    }

    /// The origin every `elapsed_ms` this context records is measured from
    /// — the first monotonic reading this context ever took, captured once
    /// and reused, per the module docs' "caller-supplied origin, not
    /// sampled [per event]" rule.
    fn elapsed_ms(&self) -> u64 {
        let now = self.clock.monotonic_ms();
        // `compare_exchange` rather than an unconditional store: only the
        // first caller's reading wins as the origin, whichever call reaches
        // here first.
        let origin = match self.origin_monotonic_ms.compare_exchange(
            -1,
            now as i64,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => now,
            Err(existing) => existing as u64,
        };
        now.saturating_sub(origin)
    }

    fn emit(&self, cycle_id: Option<&str>, event: DiagnosticEvent) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        self.sink.record(DiagnosticEventV1 {
            schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
            seq,
            wall_clock_ms: self.wall_clock_ms,
            elapsed_ms: self.elapsed_ms(),
            session_id: self.session_id.to_string(),
            source: Source::Core,
            cycle_id: cycle_id.map(str::to_string),
            operation_id: None,
            request_id: None,
            event,
        });
    }

    pub fn emit_sync_started(&self, force_full_sweep: bool) {
        self.emit(Some(self.cycle_id), DiagnosticEvent::SyncStarted { force_full_sweep });
    }

    pub fn emit_sync_phase_started(&self, phase: SyncPhase) {
        self.emit(Some(self.cycle_id), DiagnosticEvent::SyncPhaseStarted { phase });
    }

    pub fn emit_sync_phase_finished(&self, phase: SyncPhase) {
        self.emit(Some(self.cycle_id), DiagnosticEvent::SyncPhaseFinished { phase });
    }

    pub fn emit_sync_finished(&self, outcome: SyncOutcome) {
        self.emit(Some(self.cycle_id), DiagnosticEvent::SyncFinished { outcome });
    }

    fn emit_http_started(&self, method: DiagnosticHttpMethod, route: &str) {
        self.emit(
            Some(self.cycle_id),
            DiagnosticEvent::HttpStarted {
                method,
                route: route.to_string(),
            },
        );
    }

    fn emit_http_finished(
        &self,
        method: DiagnosticHttpMethod,
        route: &str,
        status: Option<u16>,
        failure: Option<super::FailureClass>,
    ) {
        self.emit(
            Some(self.cycle_id),
            DiagnosticEvent::HttpFinished {
                method,
                route: route.to_string(),
                status,
                failure,
            },
        );
    }

    fn emit_operation_slow(&self) {
        self.emit(Some(self.cycle_id), DiagnosticEvent::OperationSlow);
    }

    fn emit_operation_stalled(&self) {
        self.emit(Some(self.cycle_id), DiagnosticEvent::OperationStalled);
    }

    /// Races `op` against the 5s/30s thresholds, emitting `operation.slow`
    /// / `operation.stalled` if `op` has not resolved by then, and always
    /// returning whatever `op` eventually produces. Entirely driven by
    /// `self.clock.sleep_ms` — see [`DiagnosticClock`]'s docs on why this
    /// crate has no timer of its own to drive it with instead.
    async fn watch_slow_stalled<'f, F, T>(&'f self, op: F) -> T
    where
        F: Future<Output = T> + MaybeSend + 'f,
        T: MaybeSend,
    {
        let op: BoxFuture<'f, T> = Box::pin(op);
        let slow_sleep: BoxFuture<'f, ()> = Box::pin(self.clock.sleep_ms(SLOW_AFTER_MS));
        match select(op, slow_sleep).await {
            Either::Left((value, _)) => value,
            Either::Right((_, remaining_op)) => {
                self.emit_operation_slow();
                let stalled_sleep: BoxFuture<'f, ()> =
                    Box::pin(self.clock.sleep_ms(STALLED_AFTER_MS - SLOW_AFTER_MS));
                match select(remaining_op, stalled_sleep).await {
                    Either::Left((value, _)) => value,
                    Either::Right((_, remaining_op)) => {
                        self.emit_operation_stalled();
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
        self.diagnostics.emit_http_started(method, ROUTE);
        let headers = self.diagnostics.correlation_headers(&request_id);
        let result = self
            .diagnostics
            .watch_slow_stalled(self.inner.fetch_changes_with_headers(access_token, since, &headers))
            .await;
        self.report(method, ROUTE, &result);
        result
    }

    async fn fetch_sweep(&self, access_token: &str) -> Result<String, TransportError> {
        const ROUTE: &str = "/api/sweep";
        let method = DiagnosticHttpMethod::Get;
        let request_id = self.diagnostics.next_request_id();
        self.diagnostics.emit_http_started(method, ROUTE);
        let headers = self.diagnostics.correlation_headers(&request_id);
        let result = self
            .diagnostics
            .watch_slow_stalled(self.inner.fetch_sweep_with_headers(access_token, &headers))
            .await;
        self.report(method, ROUTE, &result);
        result
    }
}

impl<'a, R: ChangesTransport> InstrumentedChangesTransport<'a, R> {
    fn report(&self, method: DiagnosticHttpMethod, route: &str, result: &Result<String, TransportError>) {
        match result {
            Ok(_) => self.diagnostics.emit_http_finished(method, route, None, None),
            Err(error) => self.diagnostics.emit_http_finished(
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
        self.diagnostics.emit_http_started(method, &route);
        let headers = self.diagnostics.correlation_headers(&request_id);
        let result = self
            .diagnostics
            .watch_slow_stalled(self.inner.send_with_headers(access_token, request, &headers))
            .await;
        match &result {
            Ok(response) => self
                .diagnostics
                .emit_http_finished(method, &route, Some(response.status), None),
            Err(error) => self.diagnostics.emit_http_finished(
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
