//! `POST /api/skills/run` (#273, ADR-0018): the DO's authorization verdict,
//! and every word the proxy can say.
//!
//! The egress lives in the `wasm32` shim and cannot be tested natively, so
//! what is proven here is everything the shim is forbidden to decide for
//! itself: who reaches the verdict, that reaching it writes nothing, which
//! upstream statuses may be forwarded, and the exact prose of each failure.

use hummingbird_authority::{
    credential_rejected, forwardable, run_url, unconfigured, unreachable, upstream_status,
    ProxyFailure,
};
use serde_json::Value;

use crate::rig::*;

const RUN: &str = "/api/skills/run";

// ------------------------------------------------------------- the verdict

#[test]
fn a_device_token_reaches_the_verdict() {
    let sql = RusqliteSql::new();
    let resp = req_as(&sql, DEVICE_TOKEN, "POST", RUN, None, None, 1_000);
    assert_eq!(resp.status, 204);
    assert_eq!(resp.body, "");
    assert!(resp.deliveries.is_empty());
}

/// The preflight is bodiless by design (the shim must never `clone()` the
/// request, which would buffer the stream in the DO), but a body must not
/// change the verdict either — the DO simply never looks at it.
#[test]
fn the_verdict_is_the_same_with_a_body() {
    let sql = RusqliteSql::new();
    let body = r#"{"skill":"microtask","args":{"ref":"abc"}}"#;
    let resp = req_as(&sql, DEVICE_TOKEN, "POST", RUN, None, Some(body), 1_000);
    assert_eq!(resp.status, 204);
    assert_eq!(resp.body, "");
}

/// The route is device-only through `permitted`'s default arm — no arm of
/// its own, which would be dead code duplicating the default.
#[test]
fn every_other_scope_is_out_of_scope() {
    let sql = RusqliteSql::new();
    for token in [SWEEPER_TOKEN, INGEST_TOKEN] {
        let resp = req_as(&sql, token, "POST", RUN, None, None, 1_000);
        assert_eq!(resp.status, 403, "{token} should be out of scope");
        assert_eq!(resp.body, "", "403 must leak no body");
    }
}

#[test]
fn an_unauthenticated_or_garbage_caller_is_a_clean_401() {
    let sql = RusqliteSql::new();
    let anon = req_anon(&sql, "POST", RUN, None, None);
    assert_eq!(anon.status, 401);
    assert_eq!(anon.body, "");

    let garbage = req_as(&sql, "hb_not_a_real_token", "POST", RUN, None, None, 1_000);
    assert_eq!(garbage.status, 401);
    assert_eq!(garbage.body, "");
}

#[test]
fn the_wrong_method_is_a_405_not_a_404() {
    let sql = RusqliteSql::new();
    for method in ["GET", "DELETE", "PATCH", "PUT"] {
        let resp = req_as(&sql, DEVICE_TOKEN, method, RUN, None, None, 1_000);
        assert_eq!(resp.status, 405, "{method} {RUN}");
    }
}

/// A tap must not dirty the sync cursor. `authenticate` stamps `last_seen`
/// without bumping `meta` on purpose; this pins that the verdict adds
/// nothing on top of it.
#[test]
fn the_verdict_does_not_bump_meta_version() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "item-1");
    let before = meta_version(&sql);
    req_as(&sql, DEVICE_TOKEN, "POST", RUN, None, None, 1_000);
    req_as(&sql, DEVICE_TOKEN, "POST", RUN, None, None, 2_000);
    assert_eq!(meta_version(&sql), before);
}

// ------------------------------------------------------- the failure prose

fn line(failure: &ProxyFailure) -> Value {
    assert!(
        failure.body.ends_with('\n'),
        "an NDJSON line must be newline-terminated: {:?}",
        failure.body,
    );
    serde_json::from_str(failure.body.trim_end()).expect("the body is one JSON object")
}

#[test]
fn every_failure_is_one_unstamped_ndjson_envelope() {
    for failure in [
        unconfigured(),
        unreachable(),
        credential_rejected(),
        upstream_status(500),
    ] {
        assert_eq!(
            failure.body.matches('\n').count(),
            1,
            "exactly one line: {:?}",
            failure.body,
        );
        let value = line(&failure);
        assert_eq!(value["ok"], Value::Bool(false));
        assert_eq!(value["skill"], Value::Null);
        assert!(value["error"].as_str().is_some_and(|e| !e.is_empty()));
        // Nothing was attempted, so there is no backend and no model to
        // name — and an absent key is what the client's classifier reads
        // as "no stamp", never a literal.
        assert!(value.get("backend").is_none(), "{:?}", failure.body);
        assert!(value.get("model").is_none(), "{:?}", failure.body);
    }
}

#[test]
fn the_failure_statuses_and_words_are_exact() {
    let cases = [
        (
            unconfigured(),
            503,
            "The cloud runner is not configured on this server.",
        ),
        (unreachable(), 502, "Cloud runner unreachable."),
        (
            credential_rejected(),
            502,
            "The cloud runner rejected this server's credential.",
        ),
        (upstream_status(502), 502, "The cloud runner answered 502."),
        (upstream_status(404), 502, "The cloud runner answered 404."),
    ];
    for (failure, status, error) in cases {
        assert_eq!(failure.status, status, "{error}");
        assert_eq!(line(&failure)["error"], Value::String(error.to_string()));
    }
}

/// Unset secrets fail closed as a 503 and never as a 401: a 401 would make
/// the client re-prompt a device token that is perfectly fine.
#[test]
fn unconfigured_is_a_503_never_a_401() {
    assert_eq!(unconfigured().status, 503);
}

// ------------------------------------------------------------- forwarding

#[test]
fn only_the_runners_own_json_statuses_forward() {
    for status in [200, 400, 413] {
        assert!(forwardable(status), "{status} is the runner's own NDJSON");
    }
    // 401 is the trap: forwarding it would surface this server's rotated
    // bearer as the *user's* credential being bad.
    for status in [401, 403, 404, 429, 500, 502, 503, 504] {
        assert!(!forwardable(status), "{status} must not forward verbatim");
    }
}

// ---------------------------------------------------------------- run_url

#[test]
fn run_url_tolerates_a_trailing_slash_on_the_secret() {
    assert_eq!(run_url("https://runner.example"), "https://runner.example/run");
    assert_eq!(run_url("https://runner.example/"), "https://runner.example/run");
    assert_eq!(run_url("https://runner.example///"), "https://runner.example/run");
}
