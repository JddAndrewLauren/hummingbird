//! The pure half of Android's diagnostic-event minting (#709): builds one
//! `DiagnosticEventV1` (owned by `hummingbird_core::diagnostics`, #706) for
//! an event Android mints itself — outside any observed sync cycle, which
//! is #710's job to wire through `DiagnosticSink` — and serializes it to
//! the exact NDJSON line `net.twinion.hummingbird.diagnostics.
//! DiagnosticsRecorder` appends verbatim.
//!
//! **Why this exists at all, rather than a Kotlin-side event builder.**
//! #706's module header forbids a host redefining the closed event family
//! or the envelope shape — Android's `session.started`/`worker.*`/
//! `push.received` events still have to come from *somewhere*, since they
//! are not produced by a `Core::run_observed` cycle (`CoreHolder` never
//! calls the observed path; that wiring is #710's). Reusing the real
//! `DiagnosticEventV1`/`DiagnosticEvent`/`Source` types here — rather than
//! a parallel Kotlin `data class` guessing at the same field names and
//! `serde` renames — is what keeps Android's own NDJSON lines byte-for-byte
//! the shape every other host's export already is. `lib.rs`'s
//! `MobileDiagnosticEvent` mirrors only the handful of variants Android
//! mints locally, the same "mirror, never redefine" shape as
//! `MobileUrgencyBand`/`MobileFrontierAxis` already use for a Rust-owned
//! enum crossing to Kotlin.
//!
//! **No process-wide state lives in this module.** `seq`, the session id
//! and the monotonic origin are `lib.rs`'s statics (`DIAGNOSTIC_SESSION`) —
//! this module only ever builds one envelope from values a caller already
//! has, which is what makes [`event_json`] trivially testable with a fresh
//! counter per test rather than fighting a `OnceLock` only one test could
//! ever `set`.

use hummingbird_core::diagnostics::{
    DiagnosticEvent, DiagnosticEventV1, Source, DIAGNOSTIC_EVENT_SCHEMA_VERSION,
};

/// Builds one Android-sourced [`DiagnosticEventV1`]. `cycle_id`/
/// `operation_id`/`request_id` are always `None` here: none of Android's
/// own four mints (`session.started`, `worker.started`/`finished`,
/// `push.received`) belongs to a sync cycle or a correlated HTTP call —
/// only `Core::run_observed`'s own emissions ever set those, and Android
/// does not call that path (yet — #710).
pub(crate) fn event_envelope(
    session_id: &str,
    seq: u64,
    origin_monotonic_ms: u64,
    wall_clock_ms: i64,
    monotonic_ms: u64,
    event: DiagnosticEvent,
) -> DiagnosticEventV1 {
    DiagnosticEventV1 {
        schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
        seq,
        wall_clock_ms,
        elapsed_ms: monotonic_ms.saturating_sub(origin_monotonic_ms),
        session_id: session_id.to_string(),
        source: Source::Android,
        cycle_id: None,
        operation_id: None,
        request_id: None,
        event,
    }
}

/// [`event_envelope`], serialized to the one NDJSON line the Kotlin
/// recorder appends. `DiagnosticEventV1` is plain strings/numbers/enums —
/// there is no realistic serialization failure here — but the caller
/// crosses a uniffi boundary, so this still never panics: a failure (which
/// nothing in this crate's test suite can actually provoke) falls back to
/// `"{}"`, an empty-but-valid JSON object line rather than a malformed one
/// or a poisoned FFI call.
pub(crate) fn event_json(
    session_id: &str,
    seq: u64,
    origin_monotonic_ms: u64,
    wall_clock_ms: i64,
    monotonic_ms: u64,
    event: DiagnosticEvent,
) -> String {
    let envelope = event_envelope(
        session_id,
        seq,
        origin_monotonic_ms,
        wall_clock_ms,
        monotonic_ms,
        event,
    );
    serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_core::diagnostics::{OperationOutcome, WorkerTrigger};

    #[test]
    fn worker_started_serializes_with_its_trigger_and_attempt_count() {
        let json = event_json(
            "s-1",
            0,
            1_000,
            1_700_000_000_000,
            1_500,
            DiagnosticEvent::WorkerStarted {
                trigger: WorkerTrigger::Timer,
                attempt_count: 1,
            },
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["seq"], 0);
        assert_eq!(value["session_id"], "s-1");
        assert_eq!(value["source"], "android");
        assert_eq!(value["elapsed_ms"], 500);
        assert_eq!(value["event"]["name"], "worker.started");
        assert_eq!(value["event"]["payload"]["trigger"], "timer");
        assert_eq!(value["event"]["payload"]["attempt_count"], 1);
        assert_eq!(value["cycle_id"], serde_json::Value::Null);
        assert_eq!(value["operation_id"], serde_json::Value::Null);
        assert_eq!(value["request_id"], serde_json::Value::Null);
    }

    #[test]
    fn worker_finished_carries_its_trigger_attempt_count_and_outcome_payload() {
        let json = event_json(
            "s-1",
            1,
            0,
            0,
            0,
            DiagnosticEvent::WorkerFinished {
                trigger: WorkerTrigger::Push,
                attempt_count: 4,
                outcome: OperationOutcome::Failure,
            },
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["event"]["name"], "worker.finished");
        assert_eq!(value["event"]["payload"]["outcome"], "failure");
        assert_eq!(value["event"]["payload"]["trigger"], "push");
        assert_eq!(value["event"]["payload"]["attempt_count"], 4);
    }

    #[test]
    fn elapsed_ms_is_monotonic_ms_minus_the_session_origin() {
        let json = event_json("s-1", 0, 10_000, 0, 10_750, DiagnosticEvent::PushReceived);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["elapsed_ms"], 750);
    }

    /// A monotonic reading that ever comes in *behind* the session's origin
    /// (clock oddities on some devices) must never underflow — `elapsed_ms`
    /// floors at 0 rather than wrapping to a huge `u64`.
    #[test]
    fn elapsed_ms_saturates_at_zero_rather_than_underflowing() {
        let json = event_json("s-1", 0, 10_000, 0, 1_000, DiagnosticEvent::SessionStarted);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["elapsed_ms"], 0);
    }

    #[test]
    fn each_call_is_independent_seq_is_the_callers_to_advance() {
        let first = event_json("s-1", 5, 0, 0, 0, DiagnosticEvent::PushReceived);
        let second = event_json("s-1", 6, 0, 0, 0, DiagnosticEvent::PushReceived);
        let first: serde_json::Value = serde_json::from_str(&first).unwrap();
        let second: serde_json::Value = serde_json::from_str(&second).unwrap();
        assert_eq!(first["seq"], 5);
        assert_eq!(second["seq"], 6);
    }

    /// A unit variant (no payload field on the Rust side at all) serializes
    /// with **no `payload` key**, not `"payload":null` — `#[serde(tag =
    /// "name", content = "payload")]` omits the content key entirely for a
    /// fieldless variant. Pinned here because review round 1 caught the
    /// Android-side test suite assuming the opposite (a hand-built fixture
    /// emitting `"payload":null`, which real production output never
    /// produces) — this is the one place that could actually go stale
    /// silently, since nothing on the Kotlin side can call the real
    /// `event_json` to notice.
    #[test]
    fn a_unit_variant_serializes_with_no_payload_key_at_all() {
        let json = event_json("s-1", 0, 0, 0, 0, DiagnosticEvent::SessionStarted);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let event_object = value["event"].as_object().unwrap();
        assert!(
            !event_object.contains_key("payload"),
            "a fieldless variant must omit `payload`, not null it: {json}",
        );
    }

    /// Mirrors `hummingbird_domain::diagnostics::FORBIDDEN_FIELD_NAMES`
    /// (`server/domain/src/diagnostics.rs` — #711 moved the contract there,
    /// and that list is `#[cfg(test)]`-private, so `client/core` does not
    /// re-export it). This is a deliberate copy for the reason the owning
    /// doc gives: a fixed, named list a reviewer can diff against the
    /// source of truth, not a guess. Nothing gates the two against drift
    /// (#741), so that diff is the only control — keep this pointer exact.
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

    /// **The real redaction guarantee, over real production output.** Every
    /// event Android mints on its own — `event_json`, not a Kotlin fixture —
    /// scanned for a forbidden field name. This is the only place that
    /// guarantee can actually be checked against what ships: a plain JVM
    /// Android unit test cannot call `event_json` at all (it is a
    /// `#[uniffi::export]` function behind the native `.so`), so any
    /// redaction assertion written in Kotlin is necessarily over a
    /// hand-built fixture standing in for this function's output — useful
    /// for the journal/export *pipeline* (does storage introduce or drop
    /// anything), but not evidence about what this function itself
    /// produces. That evidence lives here.
    #[test]
    fn no_android_minted_event_ever_carries_a_forbidden_field() {
        use hummingbird_core::diagnostics::{CoreOwner, NetworkTransport, WorkerTrigger};
        let events = [
            DiagnosticEvent::SessionStarted,
            DiagnosticEvent::WorkerStarted {
                trigger: WorkerTrigger::Timer,
                attempt_count: 1,
            },
            DiagnosticEvent::WorkerFinished {
                trigger: WorkerTrigger::Timer,
                attempt_count: 1,
                outcome: OperationOutcome::Success,
            },
            DiagnosticEvent::WorkerFinished {
                trigger: WorkerTrigger::Push,
                attempt_count: 2,
                outcome: OperationOutcome::Failure,
            },
            DiagnosticEvent::PushReceived,
            DiagnosticEvent::NetworkChanged {
                online: true,
                transport: Some(NetworkTransport::Wifi),
                internet_capable: Some(true),
                validated: Some(true),
                metered: Some(false),
                roaming: Some(false),
            },
            DiagnosticEvent::CoreWaitStarted { owner: Some(CoreOwner::Sync) },
            DiagnosticEvent::CoreAcquired { owner: Some(CoreOwner::Capture) },
            DiagnosticEvent::CoreReleased { owner: CoreOwner::Triage },
            DiagnosticEvent::OperationRequested,
            DiagnosticEvent::OperationLocalCommit,
        ];
        for (seq, event) in events.into_iter().enumerate() {
            let json = event_json("s-1", seq as u64, 0, 1_700_000_000_000, 1_000, event);
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            let mut found = Vec::new();
            forbidden_keys_in(&value, &mut found);
            assert!(found.is_empty(), "forbidden field(s) {found:?} in {json}");
        }
    }
}
