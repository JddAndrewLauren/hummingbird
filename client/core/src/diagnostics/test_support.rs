//! Fixtures this slice's own tests need, kept `pub` (not `#[cfg(test)]`) so
//! the host slices that consume this contract — #707 (PWA), #709 (Android),
//! #710 (Android transports/spans), #711 (authority) — can write their own
//! tests against the identical never-resolving transport and controllable
//! clock rather than each inventing one.

use std::future::pending;
use std::sync::Mutex;

use super::clock::DiagnosticClock;
use super::{DiagnosticEventV1, DiagnosticSink};
use crate::sync::transport::{ChangesTransport, TransportError};

/// A [`ChangesTransport`] whose calls never resolve — the central fixture
/// for proving a `*_started` event survives a cycle that never finishes.
/// Every method returns a future that is pending forever; nothing here
/// ever panics, since a caller that awaits it is expected to never observe
/// completion at all.
#[derive(Debug, Default)]
pub struct NeverResolvingChangesTransport;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl ChangesTransport for NeverResolvingChangesTransport {
    async fn fetch_changes(&self, _access_token: &str, _since: i64) -> Result<String, TransportError> {
        pending().await
    }

    async fn fetch_sweep(&self, _access_token: &str) -> Result<String, TransportError> {
        pending().await
    }
}

/// A [`DiagnosticSink`] that keeps every event it is given, in order — for
/// assertions.
#[derive(Debug, Default)]
pub struct RecordingSink {
    events: Mutex<Vec<DiagnosticEventV1>>,
}

impl RecordingSink {
    pub fn events(&self) -> Vec<DiagnosticEventV1> {
        self.events.lock().unwrap().clone()
    }
}

impl DiagnosticSink for RecordingSink {
    fn record(&self, event: DiagnosticEventV1) {
        self.events.lock().unwrap().push(event);
    }
}

/// A [`DiagnosticClock`] under full manual control: `monotonic_ms` reads
/// whatever [`RecordingClock::advance`] last set it to, and `sleep_ms`
/// resolves immediately while recording the duration it was asked to wait
/// — the "controllable clock" the 5s/30s watchdog acceptance needs, without
/// a real wall-clock wait anywhere in the test suite.
#[derive(Debug, Default)]
pub struct RecordingClock {
    monotonic_ms: Mutex<u64>,
    sleep_requests_ms: Mutex<Vec<u64>>,
}

impl RecordingClock {
    pub fn advance(&self, by_ms: u64) {
        *self.monotonic_ms.lock().unwrap() += by_ms;
    }

    /// Every duration a caller asked [`DiagnosticClock::sleep_ms`] for, in
    /// order — what the slow/stalled watchdog's own tests assert against
    /// to pin the 5s/25s (5s-to-30s) thresholds exactly, not just that
    /// *some* delay was requested.
    pub fn sleep_requests_ms(&self) -> Vec<u64> {
        self.sleep_requests_ms.lock().unwrap().clone()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DiagnosticClock for RecordingClock {
    fn monotonic_ms(&self) -> u64 {
        *self.monotonic_ms.lock().unwrap()
    }

    async fn sleep_ms(&self, ms: u64) {
        self.sleep_requests_ms.lock().unwrap().push(ms);
        self.advance(ms);
    }
}

/// A [`DiagnosticSink`] proving [`DiagnosticSink::record`]'s infallibility
/// end to end: it "would" fail if its own backing store errored (imagine an
/// IndexedDB quota, a closed channel), but the trait gives it no `Result`
/// to report that through, so it just counts what it dropped instead of
/// ever surfacing anything to a caller.
#[derive(Debug, Default)]
pub struct FailingSink {
    dropped: Mutex<u64>,
}

impl FailingSink {
    pub fn dropped_count(&self) -> u64 {
        *self.dropped.lock().unwrap()
    }
}

impl DiagnosticSink for FailingSink {
    fn record(&self, _event: DiagnosticEventV1) {
        *self.dropped.lock().unwrap() += 1;
    }
}
