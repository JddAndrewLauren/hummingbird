//! The authority's half of #706's shared diagnostic contract
//! (`hummingbird_domain::diagnostics`): `request.received` and
//! `request.finished`, structured JSON at the Durable Object boundary
//! (#711, part of #705).
//!
//! **This module constructs real `DiagnosticEventV1` values** — the shared
//! envelope moved from `hummingbird-core` (a client-workspace crate this
//! server-workspace crate cannot depend on) into `hummingbird-domain` in
//! #711's review round 1, precisely so this could be true rather than a
//! hand-shaped lookalike. See `hummingbird_domain::diagnostics`'s own module
//! docs for the full reasoning and for why `Source::Authority`,
//! `DiagnosticEvent::RequestReceived`/`RequestFinished` and
//! [`hummingbird_domain::diagnostics::route_template`] live there now
//! instead of being redefined here.
//!
//! **What the authority supplies that the envelope doesn't have a session
//! for.** `DiagnosticEventV1::session_id`/`seq`/`elapsed_ms` were designed
//! for `hummingbird-core`'s session model (one per client process,
//! constructed once at launch). The authority has no equivalent — a
//! Durable Object instance is ADR-0008's "one workspace singleton", the
//! closest thing it has to a session — so [`SESSION_ID`] is a fixed
//! constant and `seq`/the elapsed-time origin are read by the `wasm32`
//! shim off its own per-instance `Cell` state (the same pattern
//! `worker/src/lib.rs`'s `schema_ready: Cell<bool>` already uses) and
//! handed into [`request_received_event`]/[`request_finished_event`] as
//! plain values — this module holds no state of its own to compute them
//! from.
//!
//! Everything decidable is natively tested here, per CLAUDE.md's thin-shim
//! rule: `server/worker` has no test harness, so id validation, route
//! normalization (`route_template`), auth classification and event shaping
//! all live in this crate; the `wasm32` shim adds only the `Cell` reads and
//! the `console_log!` call.

use hummingbird_domain::diagnostics::{
    DiagnosticEvent, DiagnosticEventV1, DiagnosticHttpMethod, Source,
    DIAGNOSTIC_EVENT_SCHEMA_VERSION,
};
pub use hummingbird_domain::diagnostics::{is_valid_header_value, route_template, AuthResult};

/// The authority's fixed session id (see this module's own docs on why) —
/// one Durable Object instance, one "session" for as long as it lives.
pub const SESSION_ID: &str = "authority";

/// `[A-Za-z0-9_-]{1,80}` — re-checked here under the name this module's
/// callers use; the underlying pattern is
/// [`hummingbird_domain::diagnostics::is_valid_header_value`] (shared with
/// the client's own enforcement, `client/core/src/diagnostics/route.rs`).
/// Checked again here because a correlation id is an attacker-supplied
/// string riding an HTTP header: the client is not a trust boundary, and a
/// value that failed validation there might never have come from this
/// app's own client at all.
pub fn is_valid_correlation_id(value: &str) -> bool {
    is_valid_header_value(value)
}

/// The cycle id a client attached, or `None`. **An invalid value is
/// dropped whole, never repaired or logged** — truncating or
/// sanitizing a rejected value "for debugging" is exactly the injection
/// this validator exists to prevent (a value that failed the charset
/// check can carry a log-injection payload, a control character, or
/// simply garbage no operator can act on).
pub fn accept_cycle_id(raw: Option<&str>) -> Option<String> {
    raw.filter(|v| is_valid_correlation_id(v)).map(|v| v.to_string())
}

/// The request id to log and echo back: the client's own value when it
/// validates, otherwise a fresh server-generated one — never a repaired or
/// partially-logged copy of the rejected value (see [`accept_cycle_id`]'s
/// docs; the same rule applies here).
///
/// `entropy` is the same seam token minting already uses
/// ([`crate::Entropy`]); 16 random bytes hex-encoded are 32 characters of
/// `[0-9a-f]`, comfortably inside the 80-character budget and always valid
/// by construction, so a generated id never itself needs re-validating —
/// and this function draws entropy **only** on that fallback branch: a
/// caller that hands in an already-valid value (the `wasm32` shim's own
/// pre-`handle()` resolution, see `worker/src/lib.rs::fetch`) gets it back
/// unchanged with no second random draw, which is what keeps a request's
/// `request.received` log, its `request.finished` log and its echoed
/// `X-Hummingbird-Request-Id` header all naming the exact same id even
/// though the shim and `route()` each call this function once.
pub fn accept_request_id(raw: Option<&str>, entropy: &dyn crate::Entropy) -> String {
    if let Some(value) = raw {
        if is_valid_correlation_id(value) {
            return value.to_string();
        }
    }
    let mut buf = [0u8; 16];
    entropy.fill(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether `path` is the admin lane (`/api/admin/...`) — the one branch
/// `route()` authenticates differently (against `ADMIN_SECRET`, never the
/// `tokens` table), which is what [`classify_auth_result`] needs to tell a
/// clean-secret admin success apart from a device-token success.
pub fn is_admin_path(path: &str) -> bool {
    path.strip_prefix("/api/")
        .and_then(|rest| rest.split('/').next())
        == Some("admin")
}

/// Derives the auth result purely from the request path and the final
/// response status — no threading of extra state through every branch of
/// `route()` is needed, because the DO only ever ends up in one of these
/// four shapes: an admin-lane 401 (bad `ADMIN_SECRET`) or non-401 (a
/// successful admin operation); a device/sweeper/ingest-lane 401 (no valid
/// token) or 403 (valid token, out of scope); or anything else (valid
/// token, in scope).
///
/// **This is a classification of the *auth* outcome, not of every 403 this
/// crate can answer.** A handful of routes return 403 for reasons that have
/// nothing to do with the scope matrix and happen after a token has
/// already been accepted and permitted — `alerts::ingest`'s
/// source-binding check (`handlers/alerts.rs`), `snapshots::get`/`ingest`'s
/// equivalent (`handlers/snapshots.rs`), and
/// `calendar_token::write_verdict`'s allowed-holder list
/// (`handlers/calendar_token.rs`) all answer 403 for an authenticated,
/// in-scope device/ingest token that is simply the wrong *source* or the
/// wrong *device*. Those still classify as `Forbidden` here — the same
/// bucket the scope-matrix 403 uses — because from the log line's point of
/// view they are the same fact worth recording ("this token could not do
/// this"); this function does not and cannot distinguish *why* within that
/// bucket, since it sees only the path and the status, never which
/// in-handler check produced the 403.
///
/// This also assumes every request classified here already passed
/// `handlers::route`'s own `/api/` prefix check — true for every request
/// that reaches the Durable Object in production, since the Worker's own
/// `#[event(fetch)]` routes only `/api/*` paths to it (`worker/src/lib.rs`).
/// A synthetic non-`/api/` path (reachable only from this crate's own
/// tests, never from real traffic) falls into the `Accepted` bucket below
/// for lack of a better one; it is never logged for a real request.
pub fn classify_auth_result(path: &str, status: u16) -> AuthResult {
    let is_admin = is_admin_path(path);
    match (is_admin, status) {
        (true, 401) => AuthResult::Rejected,
        (true, _) => AuthResult::Admin,
        (false, 401) => AuthResult::Rejected,
        (false, 403) => AuthResult::Forbidden,
        (false, _) => AuthResult::Accepted,
    }
}

/// Maps the authority's uppercase method string
/// (`ApiRequest::method`/`req.method().to_string().to_uppercase()` in the
/// shim) onto the shared [`DiagnosticHttpMethod`]. The route table in
/// `handlers/mod.rs` only ever dispatches `GET`/`POST`/`PATCH`/`PUT`/`DELETE`
/// — anything else reaching this function is a method the router itself
/// would answer 404/405 for, so it is classified `Get` rather than adding a
/// speculative `Other` variant to a shared, wire-committed enum for a verb
/// this API never actually answers.
fn parse_method(method: &str) -> DiagnosticHttpMethod {
    match method {
        "POST" => DiagnosticHttpMethod::Post,
        "PATCH" => DiagnosticHttpMethod::Patch,
        "PUT" => DiagnosticHttpMethod::Put,
        "DELETE" => DiagnosticHttpMethod::Delete,
        _ => DiagnosticHttpMethod::Get,
    }
}

/// Builds the `request.received` event — written by the `wasm32` shim
/// before `handle()` runs (before schema init, alarm scheduling, or
/// reading the body), so an incomplete span survives a hang.
#[allow(clippy::too_many_arguments)]
pub fn request_received_event(
    seq: u64,
    wall_clock_ms: i64,
    elapsed_ms: u64,
    cycle_id: Option<String>,
    request_id: String,
    route: String,
    method: &str,
) -> DiagnosticEventV1 {
    DiagnosticEventV1 {
        schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
        seq,
        wall_clock_ms,
        elapsed_ms,
        session_id: SESSION_ID.to_string(),
        source: Source::Authority,
        cycle_id,
        operation_id: None,
        request_id: Some(request_id),
        event: DiagnosticEvent::RequestReceived {
            method: parse_method(method),
            route,
        },
    }
}

/// Builds the `request.finished` event — written after the response is
/// built, or (for a request the DO fails on before ever calling `handle()`
/// — schema init, the alarm-scheduling await, the body-read await) written
/// immediately before that failure propagates, so this event is never
/// skipped on an exit path that already emitted `request.received`. Never
/// carries a token value, an `authorization` header, or a response body —
/// only the non-secret `token_id` and the closed [`AuthResult`].
#[allow(clippy::too_many_arguments)]
pub fn request_finished_event(
    seq: u64,
    wall_clock_ms: i64,
    elapsed_ms: u64,
    cycle_id: Option<String>,
    request_id: String,
    route: String,
    method: &str,
    status: u16,
    duration_ms: i64,
    response_bytes: usize,
    token_id: Option<String>,
    auth_result: AuthResult,
) -> DiagnosticEventV1 {
    DiagnosticEventV1 {
        schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
        seq,
        wall_clock_ms,
        elapsed_ms,
        session_id: SESSION_ID.to_string(),
        source: Source::Authority,
        cycle_id,
        operation_id: None,
        request_id: Some(request_id),
        event: DiagnosticEvent::RequestFinished {
            method: parse_method(method),
            route,
            status,
            duration_ms,
            response_bytes,
            token_id,
            auth_result,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------- id validation

    #[test]
    fn a_valid_correlation_id_accepts_letters_digits_underscore_and_hyphen() {
        assert!(is_valid_correlation_id("cycle-1_ABC123"));
    }

    #[test]
    fn an_empty_correlation_id_is_rejected() {
        assert!(!is_valid_correlation_id(""));
    }

    #[test]
    fn a_correlation_id_over_eighty_characters_is_rejected() {
        let too_long = "a".repeat(81);
        assert!(!is_valid_correlation_id(&too_long));
    }

    /// Percent-decoding never happens at this layer (`ApiRequest::path`'s
    /// own doc: segments are matched exactly as received) — this pins that
    /// the validator sees the raw, still-encoded bytes and rejects them,
    /// rather than a decoded form that might validate.
    #[test]
    fn a_percent_encoded_payload_is_rejected() {
        assert!(!is_valid_correlation_id("cycle%2F1"));
    }

    #[test]
    fn accept_cycle_id_keeps_a_valid_value() {
        assert_eq!(accept_cycle_id(Some("cycle-1")), Some("cycle-1".to_string()));
    }

    /// The invalid value itself must never appear in what
    /// `accept_cycle_id` returns — dropped whole, not truncated or
    /// repaired.
    #[test]
    fn accept_cycle_id_drops_an_invalid_value_entirely() {
        assert_eq!(accept_cycle_id(Some("has a space")), None);
        assert_eq!(accept_cycle_id(Some("")), None);
        assert_eq!(accept_cycle_id(None), None);
    }

    struct FixedEntropy(u8);
    impl crate::Entropy for FixedEntropy {
        fn fill(&self, buf: &mut [u8]) {
            for (i, byte) in buf.iter_mut().enumerate() {
                *byte = self.0.wrapping_add(i as u8);
            }
        }
    }

    #[test]
    fn accept_request_id_echoes_a_valid_client_value() {
        let entropy = FixedEntropy(1);
        assert_eq!(
            accept_request_id(Some("cycle-1-0"), &entropy),
            "cycle-1-0"
        );
    }

    #[test]
    fn accept_request_id_generates_one_when_missing() {
        let entropy = FixedEntropy(7);
        let generated = accept_request_id(None, &entropy);
        assert!(is_valid_correlation_id(&generated));
        assert_eq!(generated.len(), 32, "16 bytes hex-encoded");
    }

    /// The rejected raw value must never leak into the generated id.
    #[test]
    fn accept_request_id_generates_one_when_invalid_and_drops_the_rejected_value() {
        let entropy = FixedEntropy(3);
        let generated = accept_request_id(Some("has a space"), &entropy);
        assert!(is_valid_correlation_id(&generated));
        assert!(!generated.contains(' '));
    }

    /// The exact coordination claim `accept_request_id`'s own doc makes:
    /// calling it a second time on an already-valid value never touches
    /// entropy. Proven by handing it an entropy source that panics if
    /// asked to fill anything — if the fallback branch ran, this test
    /// would panic instead of returning.
    #[test]
    fn accept_request_id_never_draws_entropy_for_an_already_valid_value() {
        struct PanicsIfDrawn;
        impl crate::Entropy for PanicsIfDrawn {
            fn fill(&self, _buf: &mut [u8]) {
                panic!("entropy drawn for an already-valid request id");
            }
        }
        assert_eq!(
            accept_request_id(Some("already-valid-1"), &PanicsIfDrawn),
            "already-valid-1"
        );
    }

    // ------------------------------------------------- auth classification

    #[test]
    fn a_device_success_classifies_accepted() {
        assert_eq!(classify_auth_result("/api/items", 200), AuthResult::Accepted);
        assert_eq!(classify_auth_result("/api/items", 404), AuthResult::Accepted);
    }

    #[test]
    fn a_401_off_the_device_lane_classifies_rejected() {
        assert_eq!(classify_auth_result("/api/items", 401), AuthResult::Rejected);
    }

    #[test]
    fn a_403_off_the_device_lane_classifies_forbidden() {
        assert_eq!(classify_auth_result("/api/alerts", 403), AuthResult::Forbidden);
    }

    /// The non-blocking finding from review round 1: a non-auth 403 (an
    /// ingest token bound to the wrong source, `alerts::ingest`'s own
    /// check) still classifies `Forbidden` — see this function's own doc
    /// for why that is the intended, not merely tolerated, behaviour.
    #[test]
    fn a_source_binding_403_unrelated_to_the_scope_matrix_still_classifies_forbidden() {
        assert_eq!(classify_auth_result("/api/alerts", 403), AuthResult::Forbidden);
        assert_eq!(
            classify_auth_result("/api/google/calendar_write_token", 403),
            AuthResult::Forbidden
        );
    }

    #[test]
    fn a_successful_admin_call_classifies_admin() {
        assert_eq!(
            classify_auth_result("/api/admin/tokens", 201),
            AuthResult::Admin
        );
    }

    #[test]
    fn a_bad_admin_secret_classifies_rejected_not_admin() {
        assert_eq!(
            classify_auth_result("/api/admin/tokens", 401),
            AuthResult::Rejected
        );
    }

    // ------------------------------------------------- event shaping

    #[test]
    fn request_received_event_carries_source_authority_and_no_status() {
        let event = request_received_event(
            0,
            1_700_000_000_000,
            0,
            Some("cycle-1".to_string()),
            "cycle-1-0".to_string(),
            "/api/items".to_string(),
            "POST",
        );
        assert_eq!(event.source, Source::Authority);
        assert_eq!(event.session_id, SESSION_ID);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"name\":\"request.received\""));
        assert!(json.contains("\"cycle_id\":\"cycle-1\""));
        assert!(!json.contains("\"status\""));
    }

    #[test]
    fn request_finished_event_serializes_every_required_field() {
        let event = request_finished_event(
            1,
            1_700_000_000_000,
            5,
            Some("cycle-1".to_string()),
            "cycle-1-0".to_string(),
            "/api/items/:id".to_string(),
            "PATCH",
            200,
            42,
            128,
            Some("device-mac".to_string()),
            AuthResult::Accepted,
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"name\":\"request.finished\""));
        assert!(json.contains("\"status\":200"));
        assert!(json.contains("\"duration_ms\":42"));
        assert!(json.contains("\"response_bytes\":128"));
        assert!(json.contains("\"token_id\":\"device-mac\""));
        assert!(json.contains("\"auth_result\":\"accepted\""));
    }

    /// The whole risk the brief names explicitly: the principal id is
    /// metadata for the shim's log call, never the HTTP response body. This
    /// test lives beside `request_finished_event` (which carries
    /// `token_id`) as a reminder of the field it is *not* the same as —
    /// `ApiResponse`'s own pin (`handlers/mod.rs`) is the one that actually
    /// protects the body.
    #[test]
    fn a_token_id_absent_from_the_event_serializes_as_null_not_a_missing_key() {
        let event = request_finished_event(
            2,
            1_700_000_000_000,
            5,
            None,
            "generated-1".to_string(),
            "/api/admin/tokens".to_string(),
            "POST",
            401,
            5,
            0,
            None,
            AuthResult::Rejected,
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"token_id\":null"));
        assert!(json.contains("\"cycle_id\":null"));
    }

    #[test]
    fn a_delete_method_maps_to_the_shared_delete_variant() {
        let event = request_received_event(
            0, 0, 0, None, "r-1".to_string(), "/api/admin/tokens/:id".to_string(), "DELETE",
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"method\":\"DELETE\""));
    }
}
