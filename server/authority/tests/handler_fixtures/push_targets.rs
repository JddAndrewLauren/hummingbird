//! `POST /api/push_targets` and `DELETE /api/push_targets/:id` (#139): the
//! HTTP surface #131 deferred here. Idempotent create by client id, the
//! same shape as `rules.rs`'s create tests; revoke is a flag, individually
//! scoped, and idempotent.

use crate::rig::*;

fn post_push_target(sql: &dyn Sql, body: &str, now_ms: i64) -> hummingbird_authority::ApiResponse {
    post_to(sql, "/api/push_targets", body, now_ms)
}

fn delete_push_target(sql: &dyn Sql, id: &str, now_ms: i64) -> hummingbird_authority::ApiResponse {
    req(sql, "DELETE", &format!("/api/push_targets/{id}"), None, None, now_ms)
}

#[test]
fn register_returns_201_with_the_stamped_target() {
    let sql = RusqliteSql::new();
    let resp = post_push_target(
        &sql,
        r#"{"id": "pt-1", "name": "pixel-9", "platform": "android", "fcm_token": "tok-1"}"#,
        1000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let target: hummingbird_domain::PushTarget = body_as(&resp);
    assert_eq!(target.id, "pt-1");
    assert_eq!(target.name, "pixel-9");
    assert_eq!(target.fcm_token, "tok-1");
    assert_eq!(target.created_at, 1000);
    assert!(target.last_seen.is_none());
    assert!(target.revoked_at.is_none());
}

#[test]
fn register_replay_same_id_returns_200_current_target_without_a_duplicate_row() {
    let sql = RusqliteSql::new();
    post_push_target(
        &sql,
        r#"{"id": "pt-1", "name": "pixel-9", "platform": "android", "fcm_token": "tok-1"}"#,
        1000,
    );
    let resp = post_push_target(
        &sql,
        r#"{"id": "pt-1", "name": "pixel-9", "platform": "android", "fcm_token": "tok-1"}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "replay is success, not conflict");
    let target: hummingbird_domain::PushTarget = body_as(&resp);
    assert_eq!(target.created_at, 1000, "the original row, untouched");
    let rows = sql.exec("SELECT id FROM push_targets", &[]).unwrap();
    assert_eq!(rows.len(), 1, "no duplicate row");
}

#[test]
fn register_rejects_an_empty_name_or_token() {
    let sql = RusqliteSql::new();
    let resp = post_push_target(
        &sql,
        r#"{"id": "pt-1", "name": "", "platform": "android", "fcm_token": "tok-1"}"#,
        0,
    );
    assert_eq!(resp.status, 400);
    let resp = post_push_target(
        &sql,
        r#"{"id": "pt-1", "name": "pixel-9", "platform": "android", "fcm_token": ""}"#,
        0,
    );
    assert_eq!(resp.status, 400);
    let rows = sql.exec("SELECT id FROM push_targets", &[]).unwrap();
    assert!(rows.is_empty());
}

#[test]
fn revoking_one_target_leaves_a_sibling_untouched() {
    let sql = RusqliteSql::new();
    post_push_target(
        &sql,
        r#"{"id": "pt-1", "name": "pixel-9", "platform": "android", "fcm_token": "tok-1"}"#,
        0,
    );
    post_push_target(
        &sql,
        r#"{"id": "pt-2", "name": "pixel-watch", "platform": "android", "fcm_token": "tok-2"}"#,
        0,
    );

    let resp = delete_push_target(&sql, "pt-1", 5000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let revoked: hummingbird_domain::PushTarget = body_as(&resp);
    assert_eq!(revoked.revoked_at, Some(5000));

    let rows = sql
        .exec("SELECT id, revoked_at FROM push_targets ORDER BY id", &[])
        .unwrap();
    assert_eq!(rows.len(), 2, "revoking never deletes the row");
    assert!(rows[1].get("revoked_at").unwrap().as_i64().is_none(), "pt-2 untouched");
}

#[test]
fn revoke_is_idempotent() {
    let sql = RusqliteSql::new();
    post_push_target(
        &sql,
        r#"{"id": "pt-1", "name": "pixel-9", "platform": "android", "fcm_token": "tok-1"}"#,
        0,
    );
    delete_push_target(&sql, "pt-1", 5000);
    let resp = delete_push_target(&sql, "pt-1", 9000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let target: hummingbird_domain::PushTarget = body_as(&resp);
    assert_eq!(target.revoked_at, Some(5000), "the original revocation, unchanged");
}

#[test]
fn revoke_missing_target_is_404() {
    let sql = RusqliteSql::new();
    let resp = delete_push_target(&sql, "no-such-target", 0);
    assert_eq!(resp.status, 404);
}

#[test]
fn a_bad_credential_gets_401_and_a_wrong_scope_gets_403_both_empty_bodied() {
    let sql = RusqliteSql::new();
    let anon = req_anon(
        &sql,
        "POST",
        "/api/push_targets",
        None,
        Some(r#"{"id": "pt-1", "name": "n", "platform": "android", "fcm_token": "t"}"#),
    );
    assert_eq!(anon.status, 401);
    assert!(anon.body.is_empty());

    let ingest = req_as(
        &sql,
        INGEST_TOKEN,
        "POST",
        "/api/push_targets",
        None,
        Some(r#"{"id": "pt-1", "name": "n", "platform": "android", "fcm_token": "t"}"#),
        0,
    );
    assert_eq!(ingest.status, 403, "{}", ingest.body);
    assert!(ingest.body.is_empty());
    let rows = sql.exec("SELECT id FROM push_targets", &[]).unwrap();
    assert!(rows.is_empty());
}
