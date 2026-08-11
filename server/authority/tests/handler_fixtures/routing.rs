//! Routing edges: unknown paths 404, known paths with the wrong method 405,
//! missing bodies 400.

use crate::rig::*;

#[test]
fn unknown_route_404_and_wrong_method_405() {
    let sql = RusqliteSql::new();
    let unknown = req(&sql, "GET", "/api/nope", None, None, 0);
    assert_eq!(unknown.status, 404);

    for (method, path) in [
        ("GET", "/api/items"),
        ("POST", "/api/items/a-1"),
        ("PATCH", "/api/changes"),
        ("GET", "/api/projects"),
        ("DELETE", "/api/projects/p-1"),
        ("POST", "/api/routes/p-1"),
        ("GET", "/api/fog"),
        ("DELETE", "/api/steps/s-1"),
        ("GET", "/api/blocked_by"),
        ("DELETE", "/api/blocked_by/a/b"),
        // GET /api/rules is no longer wrong here (#135-137) — see rules.rs.
        ("DELETE", "/api/rules/r-1"),
    ] {
        let resp = req(&sql, method, path, None, Some("{}"), 0);
        assert_eq!(resp.status, 405, "{method} {path}: {}", resp.body);
    }
}

#[test]
fn post_and_patch_with_no_body_400() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 0);
    for (method, path) in [
        ("POST", "/api/items"),
        ("PATCH", "/api/items/a-1"),
        ("POST", "/api/projects"),
        ("POST", "/api/steps"),
        ("POST", "/api/blocked_by"),
        ("POST", "/api/rules"),
    ] {
        let resp = req(&sql, method, path, None, None, 0);
        assert_eq!(resp.status, 400, "{method} {path}: {}", resp.body);
    }
}

#[test]
fn trailing_slash_empty_id_404() {
    let sql = RusqliteSql::new();
    for path in [
        "/api/items/",
        "/api/projects/",
        "/api/routes/",
        "/api/fog/",
        "/api/steps/",
        "/api/alerts/",
        "/api/rules/",
    ] {
        let resp = req(&sql, "PATCH", path, None, Some(r#"{"expected_version": 1}"#), 0);
        assert_eq!(resp.status, 404, "PATCH {path}");
    }
    // Settings takes PUT, not PATCH — the empty key still falls to 404.
    let resp = req(
        &sql,
        "PUT",
        "/api/settings/",
        None,
        Some(r#"{"expected_version": 0, "value": true}"#),
        0,
    );
    assert_eq!(resp.status, 404, "PUT /api/settings/");
}
