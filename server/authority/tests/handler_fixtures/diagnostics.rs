//! #711: the authority's request-boundary diagnostics — id validation,
//! route normalization, auth classification and event shaping are
//! unit-tested directly against `hummingbird_authority::diagnostics` and
//! `hummingbird_domain::diagnostics` (see each module's own `#[cfg(test)]`
//! block for the exhaustive id/route/event fixtures); this file covers the
//! route-level behaviour that needs a real `handle()` call: the four auth
//! results against real tokens/scopes, `ApiResponse::principal_id` as
//! metadata that never leaks into a body, and — since review round 1 caught
//! `ApiRequest::cycle_id`/`request_id` as write-only dead fields —
//! `ApiResponse::cycle_id`/`request_id` actually reflecting `handle()`'s own
//! validate-or-generate parsing of them.
//!
//! **What this file cannot cover.** The actual `X-Hummingbird-Request-Id`
//! echo header, the `request.received`/`request.finished` `console_log!`
//! calls and the byte-count/duration measurement all live in
//! `hummingbird-authority-worker`'s `wasm32`-only shim, which — per
//! CLAUDE.md's thin-shim rule — has no test harness. Those are reviewed by
//! reading `worker/src/lib.rs::fetch` and confirmed only by the
//! `wasm32-unknown-unknown` build succeeding; `../../src/lib.rs`'s
//! `no_bare_await_question_mark_follows_the_shims_request_received_log`
//! text-scans that file for the one class of bug review round 1 actually
//! found there (an exit path that logs `request.received` but can never
//! reach `request.finished`).

use hummingbird_authority::diagnostics::classify_auth_result;
use hummingbird_authority::{Row, SqlError, SqlValue};

use crate::rig::*;

// ------------------------------------------------- auth result, end to end

#[test]
fn an_authenticated_in_scope_device_request_classifies_accepted() {
    let sql = RusqliteSql::new();
    let resp = req(&sql, "GET", "/api/changes", Some("since=0"), None, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(
        classify_auth_result("/api/changes", resp.status),
        hummingbird_authority::diagnostics::AuthResult::Accepted
    );
    assert_eq!(resp.principal_id.as_deref(), Some("rig-device"));
}

#[test]
fn an_unauthenticated_request_classifies_rejected_and_carries_no_principal() {
    let sql = RusqliteSql::new();
    let resp = req_anon(&sql, "GET", "/api/changes", Some("since=0"), None);
    assert_eq!(resp.status, 401);
    assert_eq!(
        classify_auth_result("/api/changes", resp.status),
        hummingbird_authority::diagnostics::AuthResult::Rejected
    );
    assert_eq!(resp.principal_id, None);
}

#[test]
fn a_valid_token_out_of_scope_classifies_forbidden_and_still_names_the_token() {
    let sql = RusqliteSql::new();
    let resp = req_as(
        &sql,
        DEVICE_TOKEN,
        "POST",
        "/api/alerts",
        None,
        Some(r#"{"source": "s", "source_key": "k", "title": "t"}"#),
        0,
    );
    assert_eq!(resp.status, 403, "{}", resp.body);
    assert_eq!(
        classify_auth_result("/api/alerts", resp.status),
        hummingbird_authority::diagnostics::AuthResult::Forbidden
    );
    assert_eq!(
        resp.principal_id.as_deref(),
        Some("rig-device"),
        "a 403 still names which token was out of scope",
    );
}

#[test]
fn a_successful_admin_operation_classifies_admin() {
    let sql = RusqliteSql::new();
    let resp = req_admin(
        &sql,
        "POST",
        "/api/admin/tokens",
        Some(r#"{"id": "t-diag", "name": "diag", "scope": "device"}"#),
        0,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert_eq!(
        classify_auth_result("/api/admin/tokens", resp.status),
        hummingbird_authority::diagnostics::AuthResult::Admin
    );
    // The admin lane authenticates against ADMIN_SECRET, never a `tokens`
    // row — there is no per-caller id to name.
    assert_eq!(resp.principal_id, None);
}

#[test]
fn a_bad_admin_secret_classifies_rejected_not_admin() {
    let sql = RusqliteSql::new();
    let resp = req_with(
        &sql,
        Some("Bearer not-the-secret"),
        Some(ADMIN_SECRET),
        "GET",
        "/api/admin/tokens",
        None,
        None,
        0,
    );
    assert_eq!(resp.status, 401);
    assert_eq!(
        classify_auth_result("/api/admin/tokens", resp.status),
        hummingbird_authority::diagnostics::AuthResult::Rejected
    );
}

// ------------------------------------------------- principal id never leaks

/// The whole risk the brief names explicitly: `principal_id` is response
/// *metadata* (like `deliveries`), never serialized into the body. Checked
/// across every response shape a real request can produce — a plain
/// success, a validation 400, a conflict 409, and the admin lane's own
/// success — because the risk is a `serde` attribute away on any one of
/// them, not just the happy path.
#[test]
fn the_principal_id_never_appears_in_any_response_body() {
    let sql = RusqliteSql::new();

    let ok = post(&sql, r#"{"id": "diag-1", "title": "t"}"#, 0);
    assert_eq!(ok.status, 201, "{}", ok.body);
    assert_eq!(ok.principal_id.as_deref(), Some("rig-device"));
    assert!(!ok.body.contains("rig-device"), "leaked: {}", ok.body);
    assert!(!ok.body.contains("principal"), "leaked: {}", ok.body);

    let bad_request = post(&sql, r#"{"id": "", "title": "t"}"#, 0);
    assert_eq!(bad_request.status, 400, "{}", bad_request.body);
    assert!(bad_request.principal_id.is_some());
    assert!(!bad_request.body.contains("rig-device"));

    let conflict = patch(&sql, "diag-1", r#"{"expected_version": 999, "title": "x"}"#, 0);
    assert_eq!(conflict.status, 409, "{}", conflict.body);
    assert!(conflict.principal_id.is_some());
    assert!(!conflict.body.contains("rig-device"));

    let admin_ok = req_admin(
        &sql,
        "POST",
        "/api/admin/tokens",
        Some(r#"{"id": "t-diag-2", "name": "diag2", "scope": "device"}"#),
        0,
    );
    assert_eq!(admin_ok.status, 201, "{}", admin_ok.body);
    // Admin lane: no principal at all, so nothing to leak — pinned above
    // by `a_successful_admin_operation_classifies_admin`.
    assert!(!admin_ok.body.contains("principal"));
}

// --------------------------------------------- the 500 still names its token

/// Fails any statement whose text contains `fail_on`, passing everything
/// else through — so the token lookup (which touches `tokens`) succeeds and
/// a handler *below* the auth layer is the thing that raises `SqlError`.
struct FailingAfterAuthSql<'a> {
    inner: &'a dyn Sql,
    fail_on: &'static str,
}

impl Sql for FailingAfterAuthSql<'_> {
    fn exec(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, SqlError> {
        if sql.contains(self.fail_on) {
            return Err(SqlError {
                message: "injected storage failure".to_string(),
            });
        }
        self.inner.exec(sql, params)
    }
}

/// #711 review round 2, finding 2: a `SqlError` raised by a handler *after*
/// auth succeeded still names its acting token. Before this, `route()`
/// propagated that error past the `principal_id` stamp, so `handle()`'s
/// fallback produced the one 500 that looked — to an operator reading
/// `request.finished` — exactly like a request whose auth layer never ran.
/// That is the misdiagnosis this whole slice exists to prevent, so it is
/// pinned rather than documented.
#[test]
fn a_handler_raised_500_after_a_successful_auth_still_names_the_token() {
    let real = RusqliteSql::new();
    let sql = FailingAfterAuthSql {
        inner: &real,
        fail_on: "items",
    };

    let resp = post_to(&sql, "/api/items", r#"{"id": "diag-500", "title": "t"}"#, 0);

    assert_eq!(resp.status, 500, "{}", resp.body);
    assert_eq!(
        resp.principal_id.as_deref(),
        Some("rig-device"),
        "a 500 raised below the auth layer must still name the token that got there"
    );
    assert!(!resp.body.contains("rig-device"), "leaked: {}", resp.body);
}

/// The other half of the same claim: a `SqlError` raised by the token
/// lookup *itself* has no principal to name, and must not invent one.
#[test]
fn a_500_raised_by_the_token_lookup_itself_names_no_token() {
    let real = RusqliteSql::new();
    let sql = FailingAfterAuthSql {
        inner: &real,
        fail_on: "tokens",
    };

    let resp = post_to(&sql, "/api/items", r#"{"id": "diag-501", "title": "t"}"#, 0);

    assert_eq!(resp.status, 500, "{}", resp.body);
    assert_eq!(resp.principal_id, None);
}

// ------------------------------------------------- correlation headers, end to end

/// Correlation headers are diagnostics, not credentials: a request that
/// carries a bogus `X-Hummingbird-Cycle-Id`/`-Request-Id` must be routed
/// and authorized exactly as it would be with none at all — the response
/// *body* is identical either way. This is deliberately not the whole
/// claim any more (review round 1): the tests below it assert on the
/// *metadata* `handle()` actually derives from these headers, so this one
/// stays scoped to what it says — headers don't change authorization.
#[test]
fn correlation_headers_never_change_what_a_request_may_do() {
    let sql = RusqliteSql::new();
    let header = format!("Bearer {DEVICE_TOKEN}");

    let plain = req_with_correlation(
        &sql, Some(&header), Some(ADMIN_SECRET), "GET", "/api/changes", Some("since=0"), None, 0,
        None, None,
    );
    let with_valid = req_with_correlation(
        &sql, Some(&header), Some(ADMIN_SECRET), "GET", "/api/changes", Some("since=0"), None, 0,
        Some("cycle-1"), Some("cycle-1-0"),
    );
    let with_garbage = req_with_correlation(
        &sql, Some(&header), Some(ADMIN_SECRET), "GET", "/api/changes", Some("since=0"), None, 0,
        Some("has a space"), Some(""),
    );

    assert_eq!(plain.status, 200);
    assert_eq!(with_valid.status, 200);
    assert_eq!(with_garbage.status, 200);
    assert_eq!(plain.body, with_valid.body);
    assert_eq!(plain.body, with_garbage.body);
}

/// The fix for review round 1's dead-field finding: `handle()` actually
/// parses `ApiRequest::cycle_id`/`request_id` (via
/// `diagnostics::accept_cycle_id`/`accept_request_id`) and the *resolved*
/// values come back on `ApiResponse` — a valid header is echoed unchanged.
#[test]
fn a_valid_cycle_and_request_id_are_echoed_on_the_response_metadata() {
    let sql = RusqliteSql::new();
    let header = format!("Bearer {DEVICE_TOKEN}");
    let resp = req_with_correlation(
        &sql, Some(&header), Some(ADMIN_SECRET), "GET", "/api/changes", Some("since=0"), None, 0,
        Some("cycle-9"), Some("cycle-9-0"),
    );
    assert_eq!(resp.cycle_id, Some("cycle-9".to_string()));
    assert_eq!(resp.request_id, "cycle-9-0");
}

/// The other half: an invalid cycle id is dropped to `None` (never
/// repaired), and an invalid/absent request id is replaced with a freshly
/// generated one — never the rejected raw value.
#[test]
fn an_invalid_cycle_id_is_dropped_and_an_invalid_request_id_is_replaced() {
    let sql = RusqliteSql::new();
    let header = format!("Bearer {DEVICE_TOKEN}");
    let resp = req_with_correlation(
        &sql, Some(&header), Some(ADMIN_SECRET), "GET", "/api/changes", Some("since=0"), None, 0,
        Some("has a space"), Some("also invalid"),
    );
    assert_eq!(resp.cycle_id, None, "an invalid cycle id is dropped, never repaired");
    assert_ne!(resp.request_id, "also invalid", "the rejected value must never be echoed");
    assert!(
        hummingbird_authority::diagnostics::is_valid_header_value(&resp.request_id),
        "the generated replacement must itself be a valid header value: {}",
        resp.request_id,
    );
}

/// `ApiResponse::cycle_id`/`request_id` are set on *every* exit path,
/// including a route this crate's own auth layer rejects outright — the
/// same "unconditional metadata" claim `principal_id`'s absence-on-401
/// pins for that field.
#[test]
fn correlation_metadata_is_present_even_on_a_401() {
    let sql = RusqliteSql::new();
    let resp = req_with_correlation(
        &sql, None, Some(ADMIN_SECRET), "GET", "/api/changes", Some("since=0"), None, 0,
        Some("cycle-401"), None,
    );
    assert_eq!(resp.status, 401);
    assert_eq!(resp.cycle_id, Some("cycle-401".to_string()));
    assert!(!resp.request_id.is_empty());
}
