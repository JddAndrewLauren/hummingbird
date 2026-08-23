//! `PUT /api/settings/:key` — create-or-update under one CAS rule, typed
//! JSON stored canonically — and `GET /api/settings/:key`, the one read a
//! non-device scope reaches (#120).

use hummingbird_domain::{ConflictResponse, Setting};

use crate::rig::*;

#[test]
fn put_at_version_zero_creates_201_with_canonical_json() {
    let sql = RusqliteSql::new();
    let resp = put_setting(
        &sql,
        "race-series",
        r#"{"expected_version": 0, "value": {"followed": ["f1", "indycar"]}}"#,
        1000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created: Setting = body_as(&resp);
    assert_eq!(created.key, "race-series");
    assert_eq!(created.value, r#"{"followed":["f1","indycar"]}"#, "canonical serialization");
    assert_eq!(created.version, 1);
    assert_eq!(meta_version(&sql), 1);
}

#[test]
fn put_replay_at_version_zero_returns_stored_row_without_bump() {
    let sql = RusqliteSql::new();
    put_setting(&sql, "k", r#"{"expected_version": 0, "value": "original"}"#, 0);
    let resp = put_setting(&sql, "k", r#"{"expected_version": 0, "value": "divergent"}"#, 0);
    assert_eq!(resp.status, 200, "create replay is success");
    let stored: Setting = body_as(&resp);
    assert_eq!(stored.value, r#""original""#, "the stored row, not the divergent payload");
    assert_eq!(meta_version(&sql), 1, "no bump");
}

#[test]
fn put_updates_under_cas_and_identical_value_is_a_noop() {
    let sql = RusqliteSql::new();
    put_setting(&sql, "k", r#"{"expected_version": 0, "value": 1}"#, 0); // v1

    let resp = put_setting(&sql, "k", r#"{"expected_version": 1, "value": 2}"#, 5000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated: Setting = body_as(&resp);
    assert_eq!(updated.value, "2");
    assert_eq!(updated.version, 2);
    assert_eq!(updated.updated_at, 5000);

    let resp = put_setting(&sql, "k", r#"{"expected_version": 2, "value": 2}"#, 9000);
    assert_eq!(resp.status, 200);
    let unchanged: Setting = body_as(&resp);
    assert_eq!(unchanged.version, 2, "identical value = the empty-patch rule");
    assert_eq!(unchanged.updated_at, 5000, "no restamp");
    assert_eq!(meta_version(&sql), 2);
}

#[test]
fn put_stale_version_409_carries_current_setting() {
    let sql = RusqliteSql::new();
    put_setting(&sql, "k", r#"{"expected_version": 0, "value": 1}"#, 0);
    let resp = put_setting(&sql, "k", r#"{"expected_version": 9, "value": 2}"#, 0);
    assert_eq!(resp.status, 409);
    let conflict: ConflictResponse<Setting> = body_as(&resp);
    assert_eq!(conflict.error, "version_conflict");
    assert_eq!(conflict.current.value, "1");
    assert_eq!(meta_version(&sql), 1, "a stale write bumps nothing");
}

#[test]
fn put_absent_key_with_nonzero_version_404() {
    let sql = RusqliteSql::new();
    let resp = put_setting(&sql, "ghost", r#"{"expected_version": 3, "value": 1}"#, 0);
    assert_eq!(resp.status, 404);
}

#[test]
fn put_validation_400() {
    let sql = RusqliteSql::new();
    for (body, why) in [
        (r#"{"value": 1}"#, "missing expected_version"),
        (r#"{"expected_version": 0}"#, "missing value"),
        (r#"{"expected_version": 0, "value": 1, "extra": true}"#, "unknown field"),
    ] {
        let resp = put_setting(&sql, "k", body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 0);
}

// --- GET /api/settings/:key (#120) -------------------------------------

#[test]
fn get_reads_one_setting_by_key_and_404s_when_unset() {
    let sql = RusqliteSql::new();
    put_setting(
        &sql,
        "city-waste-page",
        r#"{"expected_version": 0, "value": "https://city.example/collection?addr=1"}"#,
        1000,
    );

    let resp = req(&sql, "GET", "/api/settings/city-waste-page", None, None, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let setting: Setting = body_as(&resp);
    assert_eq!(setting.key, "city-waste-page");
    assert_eq!(
        setting.value, r#""https://city.example/collection?addr=1""#,
        "the stored canonical JSON, quotes and all — the caller parses"
    );

    // "Nobody has set this" is a state the caller must handle, not a null
    // to squint at: a poller's correct response is to exit without writing
    // rather than to poll a guessed address.
    let missing = req(&sql, "GET", "/api/settings/trips-calendar", None, None, 0);
    assert_eq!(missing.status, 404, "{}", missing.body);

    assert_eq!(meta_version(&sql), 1, "a read writes nothing");
}

/// The scope widening, stated as a test. An ingest token may read a
/// setting — it needs the binding that tells it what to poll — and still
/// may not read anything else.
#[test]
fn an_ingest_token_may_read_a_setting_and_nothing_else() {
    let sql = RusqliteSql::new();
    put_setting(&sql, "city-waste-page", r#"{"expected_version": 0, "value": "u"}"#, 0);
    bind_ingest_token(&sql, "city-waste/v2");

    let resp = req_as(&sql, INGEST_TOKEN, "GET", "/api/settings/city-waste-page", None, None, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);

    // Still no route to the workspace itself, and still no write here.
    for (method, path, query, body) in [
        ("GET", "/api/changes", Some("since=0"), None),
        ("GET", "/api/sweep", None, None),
        ("PUT", "/api/settings/city-waste-page", None, Some(r#"{"expected_version": 1, "value": "x"}"#)),
    ] {
        let resp = req_as(&sql, INGEST_TOKEN, method, path, query, body, 0);
        assert_eq!(resp.status, 403, "{method} {path}: {}", resp.body);
        assert!(resp.body.is_empty(), "{method} {path} leaked: {}", resp.body);
    }
}

#[test]
fn a_sweeper_token_may_not_read_a_setting() {
    let sql = RusqliteSql::new();
    put_setting(&sql, "city-waste-page", r#"{"expected_version": 0, "value": "u"}"#, 0);
    let resp = req_as(&sql, SWEEPER_TOKEN, "GET", "/api/settings/city-waste-page", None, None, 0);
    assert_eq!(resp.status, 403, "{}", resp.body);
}

#[test]
fn an_unauthenticated_settings_read_is_401() {
    let sql = RusqliteSql::new();
    put_setting(&sql, "city-waste-page", r#"{"expected_version": 0, "value": "u"}"#, 0);
    let resp = req_anon(&sql, "GET", "/api/settings/city-waste-page", None, None);
    assert_eq!(resp.status, 401);
    assert!(resp.body.is_empty());
}

// --- the question off switch (#715, ADR-0034) --------------------------

/// #715 work item 2, which turned out to be **zero authority code**: the
/// off switch is a `settings` row like any other, so the poller half of
/// "off means unpolled" needs no new route and no fourth `Scope` — the
/// carve-out above already reaches it, and 404-when-unset is exactly the
/// shape absence-means-enabled wants.
///
/// Written as a test rather than left as a claim in a PR body, because
/// nothing else in this repo would fail if someone narrowed that gate to
/// the binding vocabulary alone: the poller that reads this row does not
/// exist yet (#718), so this test is standing in for its consumer.
#[test]
fn an_ingest_token_may_read_a_question_off_switch_the_same_way_it_reads_a_binding() {
    let sql = RusqliteSql::new();
    bind_ingest_token(&sql, "city-waste/v2");

    // Unset is the ordinary case and the one a poller meets first: 404,
    // which the caller reads as "this question is enabled" rather than as
    // an error (`client/core/src/question_switch.rs`).
    let unset = req_as(
        &sql,
        INGEST_TOKEN,
        "GET",
        "/api/settings/question-enabled-waste",
        None,
        None,
        0,
    );
    assert_eq!(unset.status, 404, "{}", unset.body);

    // Switched off, the poller reads the bare JSON boolean back — a
    // boolean, not the string "false", which is ADR-0034 decision 2's
    // whole objection to routing this through the binding vocabulary.
    put_setting(
        &sql,
        "question-enabled-waste",
        r#"{"expected_version": 0, "value": false}"#,
        1000,
    );
    let resp = req_as(
        &sql,
        INGEST_TOKEN,
        "GET",
        "/api/settings/question-enabled-waste",
        None,
        None,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let setting: Setting = body_as(&resp);
    assert_eq!(setting.value, "false");

    // And it is still only a read: the toggle is the operator's, written
    // from a device.
    let write = req_as(
        &sql,
        INGEST_TOKEN,
        "PUT",
        "/api/settings/question-enabled-waste",
        None,
        Some(r#"{"expected_version": 1, "value": true}"#),
        0,
    );
    assert_eq!(write.status, 403, "{}", write.body);
}
