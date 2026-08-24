//! `DiagnosticEventV1` (#706): the shared diagnostic vocabulary every host
//! writes into — the PWA's SharedWorker journal (#707), Android's
//! process-wide recorder (#709), and the authority's request boundary
//! (#711) all serialize *this* envelope, never a host-local shape.
//!
//! **The envelope itself moved to `hummingbird-domain` in #711** (review
//! round 1): `hummingbird-authority` — the authority's request boundary —
//! is a member of the *server* Cargo workspace and cannot depend on this
//! crate (a member of the *client* workspace) without dragging `chrono`,
//! `futures-util` and (optionally) `reqwest` into
//! `hummingbird-authority-worker`'s wasm32 build, which CLAUDE.md's
//! thin-shim rule forbids. `hummingbird-domain` is the one crate both
//! workspaces already compile (client: this crate, `ffi-web`, `ffi-mobile`;
//! server: `hummingbird-authority` and transitively
//! `hummingbird-authority-worker`) and carries nothing but
//! `serde`/`serde_json`, so it is where [`DiagnosticEventV1`],
//! [`DiagnosticEvent`], [`Source`] and [`route::route_template`] now live —
//! this module re-exports them so every existing call site in this crate is
//! unchanged. `hummingbird_authority::diagnostics` constructs real
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
//! what makes the redaction rule in [`failure`]/`hummingbird_domain::diagnostics`'s
//! own tests checkable — by scanning [`DiagnosticEvent`]'s own declaration
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
    DiagnosticEvent, DiagnosticEventV1, DiagnosticHttpMethod, DiagnosticSink, NullSink,
    OperationOutcome, Source, SyncOutcome, SyncPhase, DIAGNOSTIC_EVENT_SCHEMA_VERSION,
};
