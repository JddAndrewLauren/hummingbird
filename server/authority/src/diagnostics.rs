//! The authority's half of #706's shared diagnostic contract
//! (`client/core/src/diagnostics/mod.rs`): `request.received` and
//! `request.finished`, structured JSON at the Durable Object boundary
//! (#711, part of #705).
//!
//! **This module cannot literally reuse `DiagnosticEventV1`/`DiagnosticEvent`.**
//! They live in `hummingbird-core`, a member of the *client* Cargo
//! workspace (`client/Cargo.toml`); `hummingbird-authority` is a member of
//! the *server* workspace and has no dependency on it today. Adding one
//! would drag `chrono`, `futures-util` and (unless disabled) `reqwest` into
//! `hummingbird-authority-worker`'s `wasm32` build — exactly what
//! CLAUDE.md's thin-shim rule forbids, for a dependency this module does
//! not otherwise need. So [`RequestReceived`]/[`RequestFinished`] are
//! hand-shaped structs of their own, carrying the field *names* the
//! client-side contract already established wherever the concept is
//! shared (`cycle_id`, `request_id`, `route`, `method`), plus the
//! boundary-specific fields the brief calls for. Reported as a finding on
//! #711 rather than forked into a redefinition of the client's enum.
//!
//! Everything here is decidable and natively tested, per CLAUDE.md's
//! thin-shim rule: the `wasm32` shim (`hummingbird-authority-worker`) calls
//! [`accept_cycle_id`]/[`accept_request_id`] before invoking `handle()` (so
//! a `request.received` line can be written before the possibly-slow work
//! starts — the same "span survives a hang" shape the client's own
//! `DiagnosticsContext` uses), classifies the outcome with
//! [`classify_auth_result`] after `handle()` returns, and does nothing but
//! serialize the two structs below and call the platform's log function.

use serde::Serialize;

/// `[A-Za-z0-9_-]{1,80}` — the same shape
/// `client/core/src/diagnostics/route.rs::is_valid_header_value` enforces
/// client-side. Checked again here because a correlation id is an
/// attacker-supplied string riding an HTTP header: the client is not a
/// trust boundary, and a value that failed validation there might never
/// have come from this app's own client at all.
pub fn is_valid_correlation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
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
/// by construction, so a generated id never itself needs re-validating.
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

/// Reduces a concrete request path to its route template — every path
/// segment that is not made up entirely of ASCII letters and underscores
/// becomes `:id`, except the segment immediately after `settings` (a
/// settings key, drawn from a small fixed non-secret vocabulary, some of
/// whose entries are hyphenated — e.g. `race-series` — so it must stay
/// concrete rather than being treated as an entity id).
///
/// **Not a second, drifting copy of `handlers::route`'s match table.** This
/// makes no reference to any literal segment name at all: it is a
/// structural rule over segment shape, so a new literal route segment
/// (always lowercase letters/underscores, per every existing route in
/// `handlers/mod.rs`) is classified correctly with no update needed here,
/// and a new entity id (every id this app mints contains a digit or a
/// hyphen — `sweep.py`'s `deterministic_v4`, the authority's own uuids, and
/// hex-encoded generated ids) is templated correctly for the same reason.
/// This is the identical rule (and identical known limit — a purely
/// alphabetic id would be left concrete) as the client's own
/// `client/core/src/diagnostics/route.rs::route_template`; the two cannot
/// share code (different Cargo workspaces) but should never disagree, and
/// this module's tests pin the same fixtures across both authority routes
/// and the settings carve-out.
pub fn route_template(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').collect();
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let previous_is_settings = index > 0 && segments[index - 1] == "settings";
            if previous_is_settings
                || segment.is_empty()
                || segment.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
            {
                segment.to_string()
            } else {
                ":id".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
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

/// The closed auth-result vocabulary (#711's acceptance list, verbatim).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthResult {
    Accepted,
    Rejected,
    Forbidden,
    Admin,
}

impl AuthResult {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthResult::Accepted => "accepted",
            AuthResult::Rejected => "rejected",
            AuthResult::Forbidden => "forbidden",
            AuthResult::Admin => "admin",
        }
    }
}

/// Derives the auth result purely from the request path and the final
/// response status — no threading of extra state through every branch of
/// `route()` is needed, because the DO only ever ends up in one of these
/// four shapes: an admin-lane 401 (bad `ADMIN_SECRET`) or non-401 (a
/// successful admin operation); a device/sweeper/ingest-lane 401 (no valid
/// token) or 403 (valid token, out of scope); or anything else (valid
/// token, in scope).
///
/// This assumes every request classified here already passed
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

/// Written before `handle()` runs, so an incomplete span survives a hang —
/// the same reason the client's own `http.started` is emitted before its
/// awaited call (`client/core/src/diagnostics/context.rs`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RequestReceived {
    pub event: &'static str,
    pub cycle_id: Option<String>,
    pub request_id: String,
    pub route: String,
    pub method: String,
}

impl RequestReceived {
    pub fn new(cycle_id: Option<String>, request_id: String, route: String, method: String) -> Self {
        RequestReceived {
            event: "request.received",
            cycle_id,
            request_id,
            route,
            method,
        }
    }
}

/// Written after the response is built. **Never carries a token value, an
/// `authorization` header, or a response body** — only the non-secret
/// token id ([`crate::ApiResponse::principal_id`]) and the closed
/// [`AuthResult`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RequestFinished {
    pub event: &'static str,
    pub cycle_id: Option<String>,
    pub request_id: String,
    pub route: String,
    pub method: String,
    pub status: u16,
    pub duration_ms: i64,
    pub response_bytes: usize,
    pub token_id: Option<String>,
    pub auth_result: AuthResult,
}

#[allow(clippy::too_many_arguments)]
impl RequestFinished {
    pub fn new(
        cycle_id: Option<String>,
        request_id: String,
        route: String,
        method: String,
        status: u16,
        duration_ms: i64,
        response_bytes: usize,
        token_id: Option<String>,
        auth_result: AuthResult,
    ) -> Self {
        RequestFinished {
            event: "request.finished",
            cycle_id,
            request_id,
            route,
            method,
            status,
            duration_ms,
            response_bytes,
            token_id,
            auth_result,
        }
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

    #[test]
    fn an_eighty_character_correlation_id_is_accepted() {
        let boundary = "a".repeat(80);
        assert!(is_valid_correlation_id(&boundary));
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
    fn a_correlation_id_with_a_disallowed_character_is_rejected() {
        assert!(!is_valid_correlation_id("has a space"));
        assert!(!is_valid_correlation_id("has/a/slash"));
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

    // ------------------------------------------------- route templating

    #[test]
    fn a_bare_collection_path_is_unchanged() {
        assert_eq!(route_template("/api/items"), "/api/items");
    }

    #[test]
    fn a_concrete_entity_path_is_templated() {
        let template = route_template("/api/items/9f1c2e40-aaaa-4b2b-8c3d-000000000001");
        assert!(!template.contains("9f1c2e40"));
        assert_eq!(template, "/api/items/:id");
    }

    #[test]
    fn two_entity_ids_in_one_path_are_both_templated() {
        assert_eq!(
            route_template("/api/blocked_by/a-1/a-2"),
            "/api/blocked_by/:id/:id"
        );
    }

    #[test]
    fn a_hyphenated_settings_key_survives_concrete() {
        assert_eq!(
            route_template("/api/settings/race-series"),
            "/api/settings/race-series"
        );
    }

    #[test]
    fn only_the_segment_immediately_after_settings_is_exempt() {
        assert_eq!(
            route_template("/api/settings/race-series/a-1"),
            "/api/settings/race-series/:id"
        );
    }

    #[test]
    fn purely_alphabetic_literal_segments_survive_untouched() {
        assert_eq!(
            route_template("/api/google/calendar_token"),
            "/api/google/calendar_token"
        );
        assert_eq!(route_template("/api/skills/run"), "/api/skills/run");
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
    fn request_received_serializes_with_the_event_name_and_no_status() {
        let event = RequestReceived::new(
            Some("cycle-1".to_string()),
            "cycle-1-0".to_string(),
            "/api/items".to_string(),
            "POST".to_string(),
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"request.received\""));
        assert!(json.contains("\"cycle_id\":\"cycle-1\""));
        assert!(!json.contains("status"));
    }

    #[test]
    fn request_finished_serializes_every_required_field() {
        let event = RequestFinished::new(
            Some("cycle-1".to_string()),
            "cycle-1-0".to_string(),
            "/api/items/:id".to_string(),
            "PATCH".to_string(),
            200,
            42,
            128,
            Some("device-mac".to_string()),
            AuthResult::Accepted,
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"request.finished\""));
        assert!(json.contains("\"status\":200"));
        assert!(json.contains("\"duration_ms\":42"));
        assert!(json.contains("\"response_bytes\":128"));
        assert!(json.contains("\"token_id\":\"device-mac\""));
        assert!(json.contains("\"auth_result\":\"accepted\""));
    }

    /// The whole risk the brief names explicitly: the principal id is
    /// metadata for the shim's log call, never the HTTP response body. This
    /// test lives beside `RequestFinished` (which carries `token_id`) as a
    /// reminder of the field it is *not* the same as — `ApiResponse`'s own
    /// pin (`handlers/mod.rs`) is the one that actually protects the body.
    #[test]
    fn a_token_id_absent_from_the_request_serializes_as_null_not_a_missing_key() {
        let event = RequestFinished::new(
            None,
            "generated-1".to_string(),
            "/api/admin/tokens".to_string(),
            "POST".to_string(),
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
}
