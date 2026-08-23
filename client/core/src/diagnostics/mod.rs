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
//! checkable — by scanning [`DiagnosticEvent`]'s own declaration and by
//! grepping serialized fixtures — rather than by review habit: a map could
//! carry anything; a fixed set of typed fields cannot silently grow a
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
//! - [`failure`]: the closed transport-failure classification.
//! - [`route`]: the pure route-templating function and the correlation
//!   header names/validation the transports below attach.
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
/// [`context::DiagnosticSession`]* — a session outlives any single sync
/// cycle, so `seq` keeps counting and `elapsed_ms`'s origin stays fixed
/// across every [`DiagnosticsContext`] (one per cycle) that session builds;
/// see [`context::DiagnosticSession`]'s own docs for why this crate needed
/// that two-tier split rather than counting per cycle. `wall_clock_ms` is
/// caller-supplied (this crate reuses the sync cycle's own `now_ms` rather
/// than sampling a second clock) and exists for human correlation against
/// real-world time, not for ordering — `seq` is what orders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticEventV1 {
    pub schema_version: u32,
    pub seq: u64,
    pub wall_clock_ms: i64,
    /// Milliseconds since [`context::DiagnosticSession::new`]'s
    /// caller-supplied origin — never sampled by this module itself. See
    /// [`context::DiagnosticSession`].
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
/// checked structurally, not by review habit, by two tests that cover
/// different halves of the claim: `no_variant_declares_a_forbidden_field_name`
/// scans this enum's whole declaration (every variant, including future
/// ones), and `no_payload_ever_carries_a_forbidden_field_name` checks real
/// serialized JSON for the variants that have a fixture. Each test's docs
/// state what it does not cover. Exact JSON key matches, case-insensitive, so a
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

    /// One instance of every [`DiagnosticEvent`] family known when this was
    /// written, so [`no_payload_ever_carries_a_forbidden_field_name`] checks
    /// real serialized JSON — including keys that come from a *nested* type
    /// (a payload field whose own struct grows a bad field name) rather than
    /// from this enum's own declaration.
    ///
    /// **What the compiler does and does not force here.** `canonical`
    /// re-matches each fixture against `DiagnosticEvent` with **no wildcard
    /// arm**, so adding a variant fails to compile — `error[E0004]:
    /// non-exhaustive patterns` — until an arm exists for it. That forces an
    /// *arm*; it does **not** force a *fixture*. Review round 2 disproved
    /// the earlier claim that it did: adding `BrandNewFamily { title:
    /// String }` plus only the arm the compiler demanded left this array
    /// untouched and the redaction test still green. Stable Rust cannot
    /// count an enum's variants (`std::mem::variant_count` is nightly), so
    /// there is no way to assert this array is complete without either a
    /// derive dependency or macro-generating the enum — and the enum's
    /// declaration is the wire contract, which stays hand-written and
    /// readable.
    ///
    /// So the whole-enum guarantee lives in
    /// [`no_variant_declares_a_forbidden_field_name`] instead, which reads
    /// this enum's own source text and therefore covers every variant that
    /// exists, including ones nobody added a fixture for. This array carries
    /// the value-level half; that test carries the coverage half. **Neither
    /// one alone closes the rule** — see that test's docs for the residual
    /// hole it leaves.
    ///
    /// **Mutation-tested**: commenting out the `PushReceived` arm below
    /// reproduces `error[E0004]` at this function, pinning that the match
    /// really is exhaustive rather than accidentally carrying a stray
    /// wildcard. Reverted before landing.
    fn one_of_every_event_variant() -> Vec<DiagnosticEvent> {
        fn canonical(event: DiagnosticEvent) -> DiagnosticEvent {
            match event {
                DiagnosticEvent::SessionStarted => DiagnosticEvent::SessionStarted,
                DiagnosticEvent::SyncStarted { force_full_sweep } => {
                    DiagnosticEvent::SyncStarted { force_full_sweep }
                }
                DiagnosticEvent::SyncPhaseStarted { phase } => DiagnosticEvent::SyncPhaseStarted { phase },
                DiagnosticEvent::SyncPhaseFinished { phase } => DiagnosticEvent::SyncPhaseFinished { phase },
                DiagnosticEvent::SyncFinished { outcome } => DiagnosticEvent::SyncFinished { outcome },
                DiagnosticEvent::HttpStarted { method, route } => {
                    DiagnosticEvent::HttpStarted { method, route }
                }
                DiagnosticEvent::HttpFinished {
                    method,
                    route,
                    status,
                    failure,
                } => DiagnosticEvent::HttpFinished {
                    method,
                    route,
                    status,
                    failure,
                },
                DiagnosticEvent::CoreWaitStarted => DiagnosticEvent::CoreWaitStarted,
                DiagnosticEvent::CoreAcquired => DiagnosticEvent::CoreAcquired,
                DiagnosticEvent::CoreBusy => DiagnosticEvent::CoreBusy,
                DiagnosticEvent::CoreReleased => DiagnosticEvent::CoreReleased,
                DiagnosticEvent::OperationRequested => DiagnosticEvent::OperationRequested,
                DiagnosticEvent::OperationLocalCommit => DiagnosticEvent::OperationLocalCommit,
                DiagnosticEvent::OperationFinished { outcome } => {
                    DiagnosticEvent::OperationFinished { outcome }
                }
                DiagnosticEvent::OperationSlow => DiagnosticEvent::OperationSlow,
                DiagnosticEvent::OperationStalled => DiagnosticEvent::OperationStalled,
                DiagnosticEvent::NetworkChanged { online } => DiagnosticEvent::NetworkChanged { online },
                DiagnosticEvent::WorkerStarted => DiagnosticEvent::WorkerStarted,
                DiagnosticEvent::WorkerFinished { outcome } => DiagnosticEvent::WorkerFinished { outcome },
                DiagnosticEvent::PushReceived => DiagnosticEvent::PushReceived,
                // No `_` arm — see this function's doc comment.
            }
        }

        [
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
        .into_iter()
        .map(canonical)
        .collect()
    }

    /// The `DiagnosticEvent` declaration's own source text, from `pub enum
    /// DiagnosticEvent {` to the column-0 `}` that closes it. Deliberately
    /// *not* the whole file: [`FORBIDDEN_FIELD_NAMES`] itself lists every
    /// forbidden word as a literal, so a whole-file scan would always fail.
    fn diagnostic_event_declaration() -> &'static str {
        let source = include_str!("mod.rs");
        let start = source
            .find("pub enum DiagnosticEvent {")
            .expect("DiagnosticEvent's declaration is in this module's own source");
        let body = &source[start..];
        let end = body
            .find("\n}\n")
            .expect("DiagnosticEvent's declaration closes with a column-0 brace");
        &body[..end]
    }

    /// **The whole-enum half of #706's redaction rule.** Scans
    /// `DiagnosticEvent`'s own declaration and rejects any forbidden word
    /// appearing as a bare token on a non-comment line — which catches both
    /// a field named `title` and a `#[serde(rename = "title")]` on a
    /// differently-named one. Unlike
    /// [`no_payload_ever_carries_a_forbidden_field_name`] this enumerates no
    /// variants, so it covers every variant that exists by construction: a
    /// new family cannot slip past it by having no fixture, which is exactly
    /// how the earlier version of this rule was disproved in review round 2.
    ///
    /// **Mutation-tested:** adding `BrandNewFamily { title: String }` to the
    /// enum, with only the arm the compiler demands in
    /// [`one_of_every_event_variant`] and no fixture, fails *this* test
    /// (`forbidden field name(s) declared on DiagnosticEvent: ["title"]`)
    /// where the fixture test passed. Reverted before landing.
    ///
    /// **The residual hole, stated honestly.** This is a source-text check,
    /// so it sees only names written literally in *this* declaration. A
    /// forbidden key reaching the wire from a *nested* type — a payload
    /// field like `failure: Option<FailureClass>` whose own struct grows a
    /// `message` field in another module — is invisible here; that case is
    /// the fixture test's job, and only for variants that have a fixture. A
    /// new variant carrying a *new* nested type with a bad field inside it
    /// is covered by neither, and nothing in stable Rust closes that without
    /// generating the enum from a macro. All payload types today are the
    /// closed local enums above plus scalars, so the gap is unreachable at
    /// present.
    #[test]
    fn no_variant_declares_a_forbidden_field_name() {
        let mut offending = Vec::new();
        for line in diagnostic_event_declaration().lines() {
            let line = line.trim();
            if line.starts_with("//") {
                continue;
            }
            for token in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                if FORBIDDEN_FIELD_NAMES
                    .iter()
                    .any(|forbidden| forbidden.eq_ignore_ascii_case(token))
                {
                    offending.push(token.to_string());
                }
            }
        }
        assert!(
            offending.is_empty(),
            "forbidden field name(s) declared on DiagnosticEvent: {offending:?}"
        );
    }

    /// The scan above is only as good as the block it reads — pin that the
    /// extraction really found the enum body and stopped at its end, so a
    /// future edit that moves the declaration cannot silently reduce
    /// [`no_variant_declares_a_forbidden_field_name`] to scanning nothing.
    #[test]
    fn the_scanned_declaration_is_the_whole_enum_body_and_no_more() {
        let declaration = diagnostic_event_declaration();
        assert!(declaration.contains("SessionStarted"));
        assert!(declaration.contains("PushReceived"));
        assert!(declaration.contains("force_full_sweep"));
        // Stops at the enum's own closing brace — the next item in the file
        // is not in scope.
        assert!(!declaration.contains("pub trait DiagnosticSink"));
        assert!(!declaration.contains("FORBIDDEN_FIELD_NAMES"));
    }

    /// #706 acceptance: "A redaction test rejects forbidden field names ...
    /// a future payload field called `title` or `token` fails a test
    /// rather than a review." The value-level half — see
    /// [`one_of_every_event_variant`] for what this does and does not
    /// cover, and [`no_variant_declares_a_forbidden_field_name`] for the
    /// whole-enum half.
    ///
    /// **Mutation-tested:** adding a field named `title` to `SyncStarted`
    /// made this fail. Reverted before landing.
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
