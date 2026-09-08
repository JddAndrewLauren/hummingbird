//! File links (ADR-0036): created against a real item, removed by flagging,
//! never re-pointed. Mirrors `project_links.rs`'s shape minus the fields a
//! file link does not carry.

use hummingbird_domain::{ConflictResponse, FileLink};

use crate::rig::*;

#[test]
fn create_file_link_201_and_replay_200_without_bump() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1"); // version 1
    let body = r#"{"id": "fl-1", "item_id": "a-1", "path": "Finance/2026/receipt.pdf"}"#;
    let resp = post_to(&sql, "/api/file_links", body, 0);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created: FileLink = body_as(&resp);
    assert_eq!(created.path, "Finance/2026/receipt.pdf");
    assert_eq!(created.removed_at, None);
    assert_eq!(created.version, 2);

    let resp = post_to(&sql, "/api/file_links", body, 0);
    assert_eq!(resp.status, 200, "replay is success");
    assert_eq!(meta_version(&sql), 2, "no bump on replay");
}

/// The authority checks non-blank after trim and nothing else — the shape
/// of a path is client-side vendor knowledge, so an extension-less folder
/// path is accepted here.
#[test]
fn create_file_link_validation_400() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    for (body, why) in [
        (r#"{"id": "", "item_id": "a-1", "path": "x.pdf"}"#, "empty id"),
        (r#"{"id": "fl", "item_id": "a-1", "path": ""}"#, "empty path"),
        (r#"{"id": "fl", "item_id": "a-1", "path": "   "}"#, "blank path"),
        (r#"{"id": "fl", "item_id": "ghost", "path": "x.pdf"}"#, "unknown item"),
        (r#"{"id": "fl", "item_id": "a-1", "path": "x.pdf", "label": "no"}"#, "unknown field"),
    ] {
        let resp = post_to(&sql, "/api/file_links", body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 1, "no write happened");

    let resp = post_to(&sql, "/api/file_links", r#"{"id": "fl", "item_id": "a-1", "path": "House/Plumbing"}"#, 0);
    assert_eq!(resp.status, 201, "a folder path is a path: {}", resp.body);
}

#[test]
fn patch_file_link_removes_and_restores_under_cas() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    post_to(&sql, "/api/file_links", r#"{"id": "fl-1", "item_id": "a-1", "path": "x.pdf"}"#, 0); // version 2

    let resp = patch_at(&sql, "/api/file_links/fl-1", r#"{"expected_version": 2, "removed_at": 7000}"#, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let removed: FileLink = body_as(&resp);
    assert_eq!(removed.removed_at, Some(7000), "removal is a flag, not a delete");
    assert_eq!(removed.version, 3);

    // Un-removing: an explicit null clears the flag, same as fog's resolved_at.
    let resp = patch_at(&sql, "/api/file_links/fl-1", r#"{"expected_version": 3, "removed_at": null}"#, 0);
    let restored: FileLink = body_as(&resp);
    assert_eq!(restored.removed_at, None);
    assert_eq!(restored.version, 4);

    let resp = patch_at(&sql, "/api/file_links/fl-1", r#"{"expected_version": 99}"#, 0);
    assert_eq!(resp.status, 409);
    let conflict: ConflictResponse<FileLink> = body_as(&resp);
    assert_eq!(conflict.current.version, 4);
}

/// ADR-0036: never re-pointed. A `path` key on the patch is refused, and
/// the stored path is untouched.
#[test]
fn patch_file_link_refuses_to_repoint() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    post_to(&sql, "/api/file_links", r#"{"id": "fl-1", "item_id": "a-1", "path": "x.pdf"}"#, 0);
    let resp = patch_at(&sql, "/api/file_links/fl-1", r#"{"expected_version": 2, "path": "y.pdf"}"#, 0);
    assert_eq!(resp.status, 400, "{}", resp.body);
    assert_eq!(meta_version(&sql), 2, "no write happened");
}

#[test]
fn patch_file_link_value_identical_is_a_noop_and_unknown_id_404() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    post_to(&sql, "/api/file_links", r#"{"id": "fl-1", "item_id": "a-1", "path": "x.pdf"}"#, 0); // version 2
    let resp = patch_at(&sql, "/api/file_links/fl-1", r#"{"expected_version": 2, "removed_at": null}"#, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged: FileLink = body_as(&resp);
    assert_eq!(unchanged.version, 2, "no version bump for a value-identical patch");
    assert_eq!(meta_version(&sql), 2);

    assert_eq!(
        patch_at(&sql, "/api/file_links/ghost", r#"{"expected_version": 1}"#, 0).status,
        404
    );
}
