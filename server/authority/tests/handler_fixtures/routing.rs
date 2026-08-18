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

/// #548: every create route that takes a client-supplied id rejects one no
/// path segment could carry back. The authority matches segments verbatim
/// (`ApiRequest::path`), so an id with a space in it — the shape that
/// actually reached production — is stored fine and then addressable by
/// nothing: `%20` misses the stored space, and a literal space cannot
/// appear in a request line. There is no DELETE route for most of these
/// either, so such a row is unreachable for good.
///
/// The assertion is on the message, not just the 400: several of these
/// bodies also carry dangling referents, which 400 for their own reason.
#[test]
fn create_rejects_an_id_no_path_could_ever_address() {
    let sql = RusqliteSql::new();
    for (path, body, why) in [
        (
            "/api/items",
            r#"{"id": "test mint 526 repro B", "title": "t"}"#,
            "the literal id that produced #548",
        ),
        ("/api/projects", r#"{"id": "p 1", "name": "n"}"#, "space"),
        (
            "/api/fog",
            r#"{"id": "f/1", "project_id": "p-1", "question": "q", "position": 1}"#,
            "a slash would split into two segments",
        ),
        (
            "/api/steps",
            r#"{"id": "s%201", "item_id": "a-1", "body": "b", "position": 1}"#,
            "a pre-encoded id is stored encoded and means something else",
        ),
        (
            "/api/rules",
            r#"{"id": "r?1", "name": "n", "conditions": [], "severity": "high", "tier": "urgent"}"#,
            "a query delimiter never reaches the path at all",
        ),
        (
            "/api/grills",
            r#"{"id": "g#1", "item_id": "a-1", "expected_version": 1, "transcript": "t",
                "summary": "s", "verdict": "fog_remains", "model_proposal": "p",
                "applied_patch": "p"}"#,
            "a fragment delimiter is dropped before the request is sent",
        ),
        (
            "/api/push_targets",
            r#"{"id": "pt 1", "name": "pixel-9", "platform": "android", "fcm_token": "tok-1"}"#,
            "space",
        ),
    ] {
        let resp = post_to(&sql, path, body, 0);
        assert_eq!(resp.status, 400, "POST {path} ({why}): {}", resp.body);
        assert!(
            resp.body.contains("URL-safe"),
            "POST {path} ({why}) must 400 on the id, not for some other reason: {}",
            resp.body
        );
    }
    assert_eq!(meta_version(&sql), 0, "no write happened on any of them");

    // The admin lane mints its own ids and is addressed the same way.
    let resp = req_admin(
        &sql,
        "POST",
        "/api/admin/tokens",
        Some(r#"{"id": "t pixel", "name": "pixel-9", "scope": "device"}"#),
        0,
    );
    assert_eq!(resp.status, 400, "POST /api/admin/tokens: {}", resp.body);
    assert!(resp.body.contains("URL-safe"), "{}", resp.body);
}

/// The guard sits *ahead* of the replay select, not after it. Every create
/// answers already-exists with a 200 (ADR-0008), and that rule must not
/// become a way to keep re-affirming an id nothing can address — the row
/// #548 is about would otherwise read as a healthy create forever.
#[test]
fn an_unaddressable_id_is_rejected_rather_than_replayed() {
    let sql = RusqliteSql::new();
    // Seed the row the old code would have created, behind the route's back.
    sql.exec(
        "INSERT INTO items (id, title, stage, priority, agent, version, created_at, updated_at) \
         VALUES ('bad id', 't', 'ready', 0, 0, 1, 0, 0)",
        &[],
    )
    .expect("seed");

    let resp = post(&sql, r#"{"id": "bad id", "title": "t"}"#, 1000);
    assert_eq!(resp.status, 400, "already-exists must not rescue it: {}", resp.body);
    assert!(resp.body.contains("URL-safe"), "{}", resp.body);
}
