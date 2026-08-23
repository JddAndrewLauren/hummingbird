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
    use hummingbird_core::diagnostics::OperationOutcome;

    #[test]
    fn worker_started_serializes_with_no_payload_and_android_source() {
        let json = event_json(
            "s-1",
            0,
            1_000,
            1_700_000_000_000,
            1_500,
            DiagnosticEvent::WorkerStarted,
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["seq"], 0);
        assert_eq!(value["session_id"], "s-1");
        assert_eq!(value["source"], "android");
        assert_eq!(value["elapsed_ms"], 500);
        assert_eq!(value["event"]["name"], "worker.started");
        assert_eq!(value["cycle_id"], serde_json::Value::Null);
        assert_eq!(value["operation_id"], serde_json::Value::Null);
        assert_eq!(value["request_id"], serde_json::Value::Null);
    }

    #[test]
    fn worker_finished_carries_its_outcome_payload() {
        let json = event_json(
            "s-1",
            1,
            0,
            0,
            0,
            DiagnosticEvent::WorkerFinished {
                outcome: OperationOutcome::Failure,
            },
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["event"]["name"], "worker.finished");
        assert_eq!(value["event"]["payload"]["outcome"], "failure");
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
}
