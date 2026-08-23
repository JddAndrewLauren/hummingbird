//! [`DiagnosticClock`]: the one caller-injected seam this module needs for
//! time, modelled on [`crate::sync::transport::ChangesTransport`] — bare
//! `wasm32-unknown-unknown` has no clock and no timer of its own (the same
//! reasoning `sync/mod.rs`'s seed-minting rule and `sync/cycle.rs`'s
//! `now_ms`/`jitter_unit` parameters already document), so both a
//! monotonic reading and a wait are things this crate asks a host-supplied
//! implementation for rather than sampling `std::time` or spawning a real
//! timer itself.

/// Milliseconds after which an unfinished operation is `operation.slow`.
pub const SLOW_AFTER_MS: u64 = 5_000;
/// Milliseconds after which an unfinished operation is `operation.stalled`
/// (measured from the same start the slow threshold is, not from when the
/// slow event fired).
pub const STALLED_AFTER_MS: u64 = 30_000;

/// A monotonic clock plus the one wait primitive the slow/stalled watchdog
/// needs. `monotonic_ms` is called synchronously and often (once per
/// recorded event, to compute `elapsed_ms`); `sleep_ms` is the only async
/// method in this crate that exists purely to wait rather than to reach the
/// network or a store, and a test's implementation can resolve it
/// immediately (or on a manual trigger) to assert the 5s/30s thresholds
/// without a real wall-clock wait — see
/// [`crate::diagnostics::test_support::RecordingClock`].
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DiagnosticClock: Send + Sync {
    /// Milliseconds from an arbitrary, caller-chosen origin — never
    /// wall-clock, so a system clock adjustment mid-cycle cannot perturb an
    /// `elapsed_ms` this crate computed from it.
    fn monotonic_ms(&self) -> u64;

    /// Resolves after roughly `ms` milliseconds of the same monotonic time
    /// `monotonic_ms` reports.
    async fn sleep_ms(&self, ms: u64);
}
