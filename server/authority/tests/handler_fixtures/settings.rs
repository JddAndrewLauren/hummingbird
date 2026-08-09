//! `PUT /api/settings/:key` — create-or-update under one CAS rule, typed
//! JSON stored canonically.

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
