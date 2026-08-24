//! `DiagnosticEventV1` (#706): the shared diagnostic vocabulary every host
//! writes into — the PWA's SharedWorker journal (#707), Android's
//! process-wide recorder (#709), and the authority's request boundary
//! (#711) all serialize *this* envelope, never a host-local shape.
//!
//! **The envelope itself moved to `hummingbird-domain` in #711** (review
//! round 1). The envelope was first drafted here (#706), in a member of the
//! *client* Cargo workspace; `hummingbird-authority` — the authority's
//! request boundary — is a member of the *server* workspace, and a crate
//! cannot depend across that workspace boundary. `hummingbird-domain` is
//! the one crate both workspaces already compile (client: this crate,
//! `ffi-web`, `ffi-mobile`; server: `hummingbird-authority` and
//! transitively `hummingbird-authority-worker`) and carries nothing but
//! `serde`/`serde_json`, so moving the envelope there costs the wasm32
//! worker build nothing it wasn't already paying for `hummingbird-domain`
//! itself. That is where [`DiagnosticEventV1`], [`DiagnosticEvent`],
//! [`Source`] and [`route::route_template`] now live — this module
//! re-exports them so every existing call site in this crate is unchanged.
//! `hummingbird_authority::diagnostics` constructs real
//! [`DiagnosticEventV1`] values from the same shared type; see that
//! module's own docs for how (a fixed per-instance session id and a `seq`
//! counter held in the `wasm32` shim's own state, since the authority has
//! no session concept of its own to reuse).
//!
//! [`DiagnosticEventV1`], [`DiagnosticEvent`] (the closed event-family
//! enum) and [`Source`] have exactly one owner (`hummingbird_domain::diagnostics`)
//! — #707/#708/#709/#710 are explicitly forbidden from redefining an owner
//! enum of their own, per their own briefs, and #711 does not either.
//!
//! **Payloads are closed types, never a string-keyed metadata map.** That is
//! what makes the redaction rule checkable in `hummingbird_domain::diagnostics`'s
//! own tests — which is where all of them live, none in [`failure`] — by scanning [`DiagnosticEvent`]'s own declaration
//! and by grepping serialized fixtures — rather than by review habit: a map
//! could carry anything; a fixed set of typed fields cannot silently grow a
//! `title` or a `token`.
//!
//! **This module has no clock or RNG of its own.** Bare
//! `wasm32-unknown-unknown` has neither (see `sync/mod.rs`'s seed-minting
//! rule), so every timestamp, id and delay in here is either a plain value
//! the caller already had (`wall_clock_ms` reuses the cycle's own `now_ms`)
//! or comes from a caller-injected trait — [`DiagnosticClock`] for
//! monotonic time and the slow/stalled watchdog's waiting, exactly the same
//! shape [`crate::sync::transport::ChangesTransport`] already uses for HTTP.
//!
//! **The sink cannot fail from the caller's point of view.** [`DiagnosticSink::record`]
//! returns nothing and cannot panic on this crate's side — a host whose
//! backing store errors (a full IndexedDB quota, a closed channel) swallows
//! that failure itself; nothing here ever propagates it to a sync cycle,
//! which must keep running whether or not diagnostics happen to be wired up.
//!
//! ## Module layout
//!
//! - [`failure`]: the closed transport-failure classification's
//!   *classifiers* — [`FailureClass`] itself is re-exported from
//!   `hummingbird_domain::diagnostics` (it is a payload field of
//!   [`DiagnosticEvent::HttpFinished`], so both workspaces must name it),
//!   but `classify_transport_error`/`from_adapter_error` take client-only
//!   types domain has no business naming, so they stay here.
//! - [`route`]: a thin re-export of [`route::route_template`] (shared, see
//!   above) plus the correlation header names/validation the transports
//!   below attach — those stay client-only (only this crate ever attaches
//!   an HTTP header).
//! - [`clock`]: [`DiagnosticClock`], the caller-injected time/wait seam.
//! - [`context`]: [`context::DiagnosticSession`] (the session-scoped `seq`
//!   counter and monotonic origin, built once per session) and
//!   [`DiagnosticsContext`] (the per-cycle bundle built from a session for
//!   each cycle: sink, clock, cycle id, the per-cycle request-id ordinal)
//!   plus the
//!   [`context::InstrumentedChangesTransport`]/[`context::InstrumentedMutationTransport`]
//!   decorators that attach headers and emit `http.*` around a real call —
//!   used only by [`crate::sync::cycle::SyncCycle::run_observed`], so the
//!   no-observer [`crate::sync::cycle::SyncCycle::run`] path never pays for
//!   or risks any of this.
//! - [`test_support`]: the never-resolving fake transport and the other
//!   fixtures this slice's own tests need, kept `pub` (not `#[cfg(test)]`)
//!   so the host slices that consume this contract (#707/#708/#709/#710/#711)
//!   can write their own tests against the identical fixtures rather than
//!   re-inventing a never-resolving transport per host.

pub mod clock;
pub mod context;
pub mod failure;
pub mod route;
pub mod test_support;

pub use clock::DiagnosticClock;
pub use context::{DiagnosticSession, DiagnosticsContext};
pub use failure::FailureClass;
pub use hummingbird_domain::diagnostics::{
    CoreOwner, DiagnosticEvent, DiagnosticEventV1, DiagnosticHttpMethod, OperationOutcome, Source,
    SyncOutcome, SyncPhase, DIAGNOSTIC_EVENT_SCHEMA_VERSION,
};

/// A sink [`DiagnosticEventV1`]s are recorded to — infallible by
/// construction (no `Result`, nothing here to propagate a failure through)
/// so a sync cycle never behaves differently depending on whether one is
/// wired up. Implemented per host: a ring buffer, an IndexedDB writer, an
/// NDJSON file — none of that is this crate's concern.
///
/// #711 review round 2: this trait stays here rather than travelling to
/// `hummingbird-domain` with the envelope. A sink is a *client-host*
/// concept — nothing server-side records into one (the authority's request
/// boundary writes `console_log!` straight to Workers Logs), and
/// `hummingbird-domain` is the owned schema plus the wire DTOs. Only the
/// envelope had to be shared across the workspace boundary; the sink did
/// not, so it isn't.
pub trait DiagnosticSink: Send + Sync {
    fn record(&self, event: DiagnosticEventV1);
}

/// The default sink for a caller with nothing wired up yet — drops every
/// event. Distinct from a test's [`test_support::RecordingSink`], which
/// keeps them for assertions.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSink;

impl DiagnosticSink for NullSink {
    fn record(&self, _event: DiagnosticEventV1) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The null sink is exactly that — proving it does not require a caller
    /// to do anything to satisfy [`DiagnosticSink`]'s infallibility.
    #[test]
    fn the_null_sink_drops_every_event_without_a_result() {
        let sink = NullSink;
        sink.record(DiagnosticEventV1 {
            schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
            seq: 0,
            wall_clock_ms: 0,
            elapsed_ms: 0,
            session_id: "s".to_string(),
            source: Source::Core,
            cycle_id: None,
            operation_id: None,
            request_id: None,
            event: DiagnosticEvent::SessionStarted,
        });
    }
}
