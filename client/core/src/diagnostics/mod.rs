//! `DiagnosticEventV1` (#706): the shared diagnostic vocabulary every host
//! writes into — the PWA's SharedWorker journal (#707), Android's
//! process-wide recorder (#709), and the authority's request boundary
//! (#711) all serialize *this* envelope, never a host-local shape. This
//! module is the sole owner of [`DiagnosticEventV1`], [`DiagnosticEvent`]
//! (the closed event-family enum) and [`Source`] — #708 and #710 are
//! explicitly forbidden from redefining an owner enum of their own, per
//! their own briefs.
//!
//! **Payloads are closed types, never a string-keyed metadata map.** That is
//! what makes the redaction rule in [`failure`]/this module's own tests
//! checkable by grepping a serialized fixture rather than by review habit —
//! a map could carry anything; a fixed set of typed fields cannot silently
//! grow a `title` or a `token`.
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
//! - [`failure`]: the closed transport-failure classification.
//! - [`route`]: the pure route-templating function and the correlation
//!   header names/validation the transports below attach.
//! - [`clock`]: [`DiagnosticClock`], the caller-injected time/wait seam.
//! - [`context`]: [`DiagnosticsContext`], the per-cycle bundle (sink, clock,
//!   session id, cycle id, the per-cycle request-id ordinal) plus the
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
pub use context::DiagnosticsContext;
pub use failure::FailureClass;

use serde::{Deserialize, Serialize};

/// `DiagnosticEventV1`'s schema version — bumped only on a breaking change
/// to the envelope shape (never the event families; a new family or payload
/// field is additive and does not need this to move).
pub const DIAGNOSTIC_EVENT_SCHEMA_VERSION: u32 = 1;

/// Which host produced an event. `Core` is what this crate ever stamps —
/// the other three are stamped by the hosts whose own diagnostics slices
/// (#707/#709/#711) generate events without going through this crate's
/// [`DiagnosticSink`] at all (a browser SharedWorker or an Android process
/// has no reason to round-trip through Rust to record its own span).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Core,
    WebWorker,
    Android,
    Authority,
}

/// The envelope every diagnostic event is wrapped in, whatever host or
/// family produced it. `seq` and `elapsed_ms` are monotonic *within one
/// [`DiagnosticSink`]* (a single session's stream); `wall_clock_ms` is
/// caller-supplied (this crate reuses the sync cycle's own `now_ms` rather
/// than sampling a second clock) and exists for human correlation against
/// real-world time, not for ordering — `seq` is what orders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticEventV1 {
    pub schema_version: u32,
    pub seq: u64,
    pub wall_clock_ms: i64,
    /// Milliseconds since [`DiagnosticClock::monotonic_ms`]'s first reading
    /// this session took — a caller-supplied *origin*, not a duration this
    /// module samples on its own initiative. See [`DiagnosticsContext`].
    pub elapsed_ms: u64,
    pub session_id: String,
    pub source: Source,
    /// Which sync cycle this event belongs to, when it belongs to one.
    pub cycle_id: Option<String>,
    /// Which logical operation (a Core mutation, a background work item)
    /// this event belongs to, when it belongs to one. Independent of
    /// `cycle_id` — an operation can span cycles (queued offline, sent on a
    /// later one) and a cycle carries operations from more than one caller.
    pub operation_id: Option<String>,
    /// Which single HTTP round trip this event belongs to, when it belongs
    /// to one — `<cycle_id>-<ordinal>`, minted by
    /// [`DiagnosticsContext::next_request_id`].
    pub request_id: Option<String>,
    pub event: DiagnosticEvent,
}

/// One phase of a sync cycle (ADR-0007/ADR-0008) — the four boundaries
/// `sync/cycle.rs`'s own docs already name: drain, pull, and the two
/// persists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPhase {
    QueueDrain,
    Pull,
    QueuePersist,
    MirrorPersist,
}

/// How a whole sync cycle ended — [`crate::sync::cycle::CycleOutcome`]
/// collapsed to its discriminant, deliberately: the outcome's own fields
/// (`drain`, `message`, ...) can carry more than this redacted vocabulary
/// wants recorded (`PersistFailed`'s `message` is a raw store error string).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutcome {
    Skipped,
    Blocked,
    CredentialNeeded,
    PersistFailed,
    PullFailed,
    Completed,
}

/// The HTTP verb one `http.*` event's call used. A closed, local vocabulary
/// rather than reusing [`crate::sync::write::transport::HttpMethod`] — that
/// type has no `Get` (a write transport never needs one) and this family
/// covers both the read and write transports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiagnosticHttpMethod {
    Get,
    Post,
    Patch,
    Put,
}

/// How one `operation.*`-family unit of work ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOutcome {
    Success,
    Failure,
}

/// The closed event vocabulary (#706's acceptance list, verbatim). A
/// variant's payload carries only what is unique to that moment — every
/// correlation id already lives on [`DiagnosticEventV1`] itself, so no
/// payload repeats `cycle_id`/`operation_id`/`request_id`.
///
/// **Every family in this enum exists here, whether or not this slice ever
/// constructs one.** `core.*`, `operation.*`, `network.changed`, `worker.*`
/// and `push.received` are defined for #708/#709/#710 to emit — this slice
/// emits only `session.started` (never, in fact — no session lifecycle
/// lives in this crate yet, so the variant exists but nothing here builds
/// one) and the `sync.*`/`http.*`/`operation.slow`/`operation.stalled`
/// events the observed cycle in [`crate::sync::cycle`] actually produces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "payload")]
pub enum DiagnosticEvent {
    #[serde(rename = "session.started")]
    SessionStarted,

    #[serde(rename = "sync.started")]
    SyncStarted { force_full_sweep: bool },
    #[serde(rename = "sync.phase_started")]
    SyncPhaseStarted { phase: SyncPhase },
    #[serde(rename = "sync.phase_finished")]
    SyncPhaseFinished { phase: SyncPhase },
    #[serde(rename = "sync.finished")]
    SyncFinished { outcome: SyncOutcome },

    #[serde(rename = "http.started")]
    HttpStarted {
        method: DiagnosticHttpMethod,
        /// A route template (`/api/items/:id`) — never a concrete path.
        route: String,
    },
    #[serde(rename = "http.finished")]
    HttpFinished {
        method: DiagnosticHttpMethod,
        route: String,
        /// Present only when a response actually arrived.
        status: Option<u16>,
        /// Present only on failure — see [`failure::FailureClass`] for why
        /// this, and not the underlying error's message, is what gets kept.
        failure: Option<FailureClass>,
    },

    #[serde(rename = "core.wait_started")]
    CoreWaitStarted,
    #[serde(rename = "core.acquired")]
    CoreAcquired,
    #[serde(rename = "core.busy")]
    CoreBusy,
    #[serde(rename = "core.released")]
    CoreReleased,

    #[serde(rename = "operation.requested")]
    OperationRequested,
    #[serde(rename = "operation.local_commit")]
    OperationLocalCommit,
    #[serde(rename = "operation.finished")]
    OperationFinished { outcome: OperationOutcome },
    #[serde(rename = "operation.slow")]
    OperationSlow,
    #[serde(rename = "operation.stalled")]
    OperationStalled,

    #[serde(rename = "network.changed")]
    NetworkChanged { online: bool },

    #[serde(rename = "worker.started")]
    WorkerStarted,
    #[serde(rename = "worker.finished")]
    WorkerFinished { outcome: OperationOutcome },

    #[serde(rename = "push.received")]
    PushReceived,
}

/// A sink [`DiagnosticEventV1`]s are recorded to — infallible by
/// construction (no `Result`, nothing here to propagate a failure through)
/// so a sync cycle never behaves differently depending on whether one is
/// wired up. Implemented per host: a ring buffer, an IndexedDB writer, an
/// NDJSON file — none of that is this crate's concern.
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

/// Field names a payload must never carry (#706's redaction rule) —
/// checked structurally against a fixture covering every event variant,
/// not by review habit. Exact JSON key matches, case-insensitive, so a
/// legitimate `cycle_id`/`request_id`/`session_id`/`operation_id` (whose
/// key is not literally one of these words) never false-positives.
#[cfg(test)]
const FORBIDDEN_FIELD_NAMES: &[&str] = &[
    "authorization",
    "access_token",
    "api_key",
    "token",
    "credential",
    "password",
    "body",
    "request_body",
    "response_body",
    "title",
    "description",
    "url",
    "ip",
    "ip_address",
    "exception",
    "stack_trace",
    "message",
];

#[cfg(test)]
fn forbidden_keys_in(value: &serde_json::Value, found: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, nested) in map {
                if FORBIDDEN_FIELD_NAMES
                    .iter()
                    .any(|forbidden| forbidden.eq_ignore_ascii_case(key))
                {
                    found.push(key.clone());
                }
                forbidden_keys_in(nested, found);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                forbidden_keys_in(item, found);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `DiagnosticEventV1` must serialize stably — a round trip through
    /// `serde_json` is the whole claim, and pins the envelope's field names
    /// (a rename here breaks every host's stored history).
    #[test]
    fn a_diagnostic_event_v1_serializes_and_round_trips_stably() {
        let event = DiagnosticEventV1 {
            schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
            seq: 1,
            wall_clock_ms: 1_700_000_000_000,
            elapsed_ms: 42,
            session_id: "s-1".to_string(),
            source: Source::Core,
            cycle_id: Some("c-1".to_string()),
            operation_id: None,
            request_id: Some("c-1-0".to_string()),
            event: DiagnosticEvent::HttpStarted {
                method: DiagnosticHttpMethod::Get,
                route: "/api/changes".to_string(),
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"name\":\"http.started\""));
        assert!(json.contains("\"schema_version\":1"));

        let round_tripped: DiagnosticEventV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped, event);
    }

    /// One instance of every [`DiagnosticEvent`] family, so the field-name
    /// rejection test below is a structural claim about the whole enum, not
    /// one call site. **Mutation-tested**: adding a field named `title` to
    /// e.g. `SyncStarted` here made
    /// `no_payload_ever_carries_a_forbidden_field_name` fail, confirming
    /// the check actually inspects payload content rather than passing
    /// vacuously — reverted before landing this test.
    fn one_of_every_event_variant() -> Vec<DiagnosticEvent> {
        vec![
            DiagnosticEvent::SessionStarted,
            DiagnosticEvent::SyncStarted { force_full_sweep: true },
            DiagnosticEvent::SyncPhaseStarted { phase: SyncPhase::QueueDrain },
            DiagnosticEvent::SyncPhaseFinished { phase: SyncPhase::Pull },
            DiagnosticEvent::SyncFinished { outcome: SyncOutcome::Completed },
            DiagnosticEvent::HttpStarted {
                method: DiagnosticHttpMethod::Get,
                route: "/api/items/:id".to_string(),
            },
            DiagnosticEvent::HttpFinished {
                method: DiagnosticHttpMethod::Patch,
                route: "/api/items/:id".to_string(),
                status: Some(200),
                failure: None,
            },
            DiagnosticEvent::CoreWaitStarted,
            DiagnosticEvent::CoreAcquired,
            DiagnosticEvent::CoreBusy,
            DiagnosticEvent::CoreReleased,
            DiagnosticEvent::OperationRequested,
            DiagnosticEvent::OperationLocalCommit,
            DiagnosticEvent::OperationFinished { outcome: OperationOutcome::Success },
            DiagnosticEvent::OperationSlow,
            DiagnosticEvent::OperationStalled,
            DiagnosticEvent::NetworkChanged { online: true },
            DiagnosticEvent::WorkerStarted,
            DiagnosticEvent::WorkerFinished { outcome: OperationOutcome::Success },
            DiagnosticEvent::PushReceived,
        ]
    }

    /// #706 acceptance: "A redaction test rejects forbidden field names ...
    /// a future payload field called `title` or `token` fails a test
    /// rather than a review." See [`one_of_every_event_variant`]'s doc
    /// comment for the mutation-testing proof this check is not vacuous.
    #[test]
    fn no_payload_ever_carries_a_forbidden_field_name() {
        let mut offending = Vec::new();
        for event in one_of_every_event_variant() {
            let value = serde_json::to_value(&event).unwrap();
            forbidden_keys_in(&value, &mut offending);
        }
        assert!(
            offending.is_empty(),
            "forbidden field name(s) found on a payload: {offending:?}"
        );
    }

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
