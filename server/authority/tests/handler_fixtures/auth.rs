//! The auth contract (#114 acceptance criterion 3): 401 = bad credential
//! (empty body, revoked and never-existed indistinguishable), 403 = valid
//! token outside its scope (empty body), and the scope matrix — device is
//! everything but ingest, sweeper creates items only, ingest posts alerts
//! only.

use crate::rig::*;

#[test]
fn missing_or_garbage_bearer_401_with_empty_body() {
    let sql = RusqliteSql::new();
    let anon = req_anon(&sql, "GET", "/api/changes", Some("since=0"), None);
    assert_eq!(anon.status, 401);
    assert!(anon.body.is_empty(), "no leakage: {}", anon.body);

    for header in ["Bearer hb_never_minted", "Basic dXNlcjpwdw==", "Bearer", "  "] {
        let resp = req_with(
            &sql,
            Some(header),
            Some(ADMIN_SECRET),
            "GET",
            "/api/changes",
            Some("since=0"),
            None,
            0,
        );
        assert_eq!(resp.status, 401, "header {header:?}");
        assert!(resp.body.is_empty(), "header {header:?} leaked: {}", resp.body);
    }
}

#[test]
fn unauthenticated_writes_never_reach_a_table() {
    let sql = RusqliteSql::new();
    let resp = req_anon(&sql, "POST", "/api/items", None, Some(r#"{"id": "a", "title": "t"}"#));
    assert_eq!(resp.status, 401);
    let rows = sql.exec("SELECT id FROM items", &[]).unwrap();
    assert!(rows.is_empty(), "the 401 short-circuits before any write");
    assert_eq!(meta_version(&sql), 0);
}

#[test]
fn device_token_cannot_post_alerts() {
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
    assert!(resp.body.is_empty());
}

#[test]
fn ingest_token_cannot_touch_items_or_read() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    for (method, path, body) in [
        ("POST", "/api/items", Some(r#"{"id": "x", "title": "t"}"#)),
        ("PATCH", "/api/items/a-1", Some(r#"{"expected_version": 1}"#)),
        ("GET", "/api/changes", None),
        ("GET", "/api/sweep", None),
        ("POST", "/api/projects", Some(r#"{"id": "p", "name": "n"}"#)),
    ] {
        let query = (path == "/api/changes").then_some("since=0");
        let resp = req_as(&sql, INGEST_TOKEN, method, path, query, body, 0);
        assert_eq!(resp.status, 403, "{method} {path}");
        assert!(resp.body.is_empty(), "{method} {path} leaked: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 1, "nothing was written");
}

#[test]
fn sweeper_token_creates_items_and_nothing_else() {
    let sql = RusqliteSql::new();
    let resp = req_as(
        &sql,
        SWEEPER_TOKEN,
        "POST",
        "/api/items",
        None,
        Some(r#"{"id": "swept-1", "title": "from the funnel", "source": "google-tasks/v1"}"#),
        0,
    );
    assert_eq!(resp.status, 201, "the sweeper's whole contract: {}", resp.body);

    for (method, path, body) in [
        ("PATCH", "/api/items/swept-1", Some(r#"{"expected_version": 1}"#)),
        ("GET", "/api/changes", None),
        ("POST", "/api/alerts", Some(r#"{"source": "s", "source_key": "k", "title": "t"}"#)),
    ] {
        let query = (path == "/api/changes").then_some("since=0");
        let resp = req_as(&sql, SWEEPER_TOKEN, method, path, query, body, 0);
        assert_eq!(resp.status, 403, "{method} {path}");
    }
}

#[test]
fn scope_matrix_default_arm_denies_every_other_route() {
    let sql = RusqliteSql::new();
    // The matrix names two non-device allowances and defaults everything
    // else to device-only; these representatives pin the default arm for
    // both narrow scopes across the route families their allowances sit
    // nearest to.
    for (token, scope, method, path, body) in [
        (
            SWEEPER_TOKEN,
            "sweeper",
            "PATCH",
            "/api/alerts/al-1",
            r#"{"expected_version": 1, "dismissed_at": 1}"#,
        ),
        (
            SWEEPER_TOKEN,
            "sweeper",
            "POST",
            "/api/steps",
            r#"{"id": "s", "item_id": "a", "body": "b", "position": 1}"#,
        ),
        (
            SWEEPER_TOKEN,
            "sweeper",
            "PUT",
            "/api/settings/k",
            r#"{"expected_version": 0, "value": true}"#,
        ),
        (
            INGEST_TOKEN,
            "ingest",
            "PATCH",
            "/api/routes/x",
            r#"{"expected_version": 1, "destination": "d"}"#,
        ),
        (
            INGEST_TOKEN,
            "ingest",
            "POST",
            "/api/blocked_by",
            r#"{"item_id": "a", "blocker_id": "b"}"#,
        ),
    ] {
        let resp = req_as(&sql, token, method, path, None, Some(body), 0);
        assert_eq!(resp.status, 403, "{scope} {method} {path}: {}", resp.body);
        assert!(resp.body.is_empty(), "{scope} {method} {path} leaked: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 0, "nothing was written");
}

#[test]
fn bound_ingest_token_posting_for_its_own_source_succeeds() {
    let sql = RusqliteSql::new();
    bind_ingest_token(&sql, "hc");
    let resp = req_as(
        &sql,
        INGEST_TOKEN,
        "POST",
        "/api/alerts",
        None,
        Some(r#"{"source": "hc", "source_key": "k", "title": "t"}"#),
        0,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
}

#[test]
fn ingest_token_posting_for_a_different_source_is_403_with_an_empty_body() {
    let sql = RusqliteSql::new();
    bind_ingest_token(&sql, "hc");
    let resp = req_as(
        &sql,
        INGEST_TOKEN,
        "POST",
        "/api/alerts",
        None,
        Some(r#"{"source": "not-hc", "source_key": "k", "title": "t"}"#),
        0,
    );
    assert_eq!(resp.status, 403, "{}", resp.body);
    assert!(resp.body.is_empty(), "leaked: {}", resp.body);
    let rows = sql.exec("SELECT id FROM alerts", &[]).unwrap();
    assert!(rows.is_empty(), "the mismatch never reaches the upsert");
}

#[test]
fn revoked_token_gets_401_on_its_next_request() {
    let sql = RusqliteSql::new();
    assert_eq!(changes(&sql, "since=0").status, 200, "live token works");

    let resp = req_admin(&sql, "DELETE", "/api/admin/tokens/rig-device", None, 5000);
    assert_eq!(resp.status, 200, "{}", resp.body);

    let resp = changes(&sql, "since=0");
    assert_eq!(resp.status, 401, "revoked on the very next request");
    assert!(
        resp.body.is_empty(),
        "revoked is indistinguishable from never-existed: {}",
        resp.body
    );
}

#[test]
fn last_seen_is_stamped_without_a_meta_bump() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1"); // meta 1
    let before = meta_version(&sql);
    assert_eq!(changes(&sql, "since=0").status, 200);
    assert_eq!(
        meta_version(&sql),
        before,
        "an authed read must not dirty the delta cursor"
    );
    let last_seen = sql
        .exec("SELECT last_seen FROM tokens WHERE id = 'rig-device'", &[])
        .unwrap()[0]
        .get("last_seen")
        .unwrap()
        .as_i64();
    assert!(last_seen.is_some(), "last_seen was stamped");
}

#[test]
fn admin_routes_reject_wrong_and_missing_secret_and_fail_closed() {
    let sql = RusqliteSql::new();
    // Wrong secret.
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
    assert!(resp.body.is_empty());

    // A device token is not the admin secret.
    let resp = req_with(
        &sql,
        Some(&format!("Bearer {DEVICE_TOKEN}")),
        Some(ADMIN_SECRET),
        "GET",
        "/api/admin/tokens",
        None,
        None,
        0,
    );
    assert_eq!(resp.status, 401, "token auth never opens the admin lane");

    // Unconfigured ADMIN_SECRET fails closed even with a matching header.
    let resp = req_with(
        &sql,
        Some(&format!("Bearer {ADMIN_SECRET}")),
        None,
        "GET",
        "/api/admin/tokens",
        None,
        None,
        0,
    );
    assert_eq!(resp.status, 401, "no secret configured = no admin lane");
    assert!(resp.body.is_empty());
}
