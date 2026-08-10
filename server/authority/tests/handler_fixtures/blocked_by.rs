//! Sequencing edges: composite identity, flag-removal, re-add on the same
//! row, and crash-replay no-ops at every stage.

use hummingbird_domain::{BlockedBy, ConflictResponse};

use crate::rig::*;

fn edge_body(item: &str, blocker: &str) -> String {
    format!(r#"{{"item_id": "{item}", "blocker_id": "{blocker}"}}"#)
}

#[test]
fn create_edge_201_and_replay_200_without_bump() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a"); // version 1
    seed_item(&sql, "b"); // version 2
    let resp = post_to(&sql, "/api/blocked_by", &edge_body("a", "b"), 0);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let edge: BlockedBy = body_as(&resp);
    assert_eq!((edge.item_id.as_str(), edge.blocker_id.as_str()), ("a", "b"));
    assert_eq!(edge.removed_at, None);
    assert_eq!(edge.version, 3);

    let resp = post_to(&sql, "/api/blocked_by", &edge_body("a", "b"), 0);
    assert_eq!(resp.status, 200, "replay is success");
    assert_eq!(meta_version(&sql), 3, "no bump on replay");
    let rows = sql.exec("SELECT item_id FROM blocked_by", &[]).unwrap();
    assert_eq!(rows.len(), 1, "no duplicate edge");
}

#[test]
fn create_edge_validation_400() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a");
    for (body, why) in [
        (edge_body("a", "a"), "self-edge"),
        (edge_body("a", "ghost"), "unknown blocker"),
        (edge_body("ghost", "a"), "unknown item"),
        (edge_body("", "a"), "empty item_id"),
    ] {
        let resp = post_to(&sql, "/api/blocked_by", &body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 1, "no write happened");
}

#[test]
fn remove_readd_and_replay_cycle() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a");
    seed_item(&sql, "b");
    post_to(&sql, "/api/blocked_by", &edge_body("a", "b"), 0); // version 3

    // Remove: flag, never DELETE.
    let resp = patch_at(
        &sql,
        "/api/blocked_by/a/b",
        r#"{"expected_version": 3, "removed_at": 5000}"#,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let removed: BlockedBy = body_as(&resp);
    assert_eq!(removed.removed_at, Some(5000));
    assert_eq!(removed.version, 4);
    let rows = sql.exec("SELECT item_id FROM blocked_by", &[]).unwrap();
    assert_eq!(rows.len(), 1, "the row survives removal");

    // Re-add: the POST clears the flag on the same row — a real write.
    let resp = post_to(&sql, "/api/blocked_by", &edge_body("a", "b"), 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let readded: BlockedBy = body_as(&resp);
    assert_eq!(readded.removed_at, None);
    assert_eq!(readded.version, 5, "re-add is a real write");
    assert_eq!(meta_version(&sql), 5);

    // Crash-replay of the re-add finds a live edge and no-ops.
    let resp = post_to(&sql, "/api/blocked_by", &edge_body("a", "b"), 0);
    assert_eq!(resp.status, 200);
    assert_eq!(meta_version(&sql), 5, "replay after re-add bumps nothing");
}

#[test]
fn patch_edge_stale_version_409_and_unknown_404() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a");
    seed_item(&sql, "b");
    post_to(&sql, "/api/blocked_by", &edge_body("a", "b"), 0); // version 3

    let resp = patch_at(
        &sql,
        "/api/blocked_by/a/b",
        r#"{"expected_version": 9, "removed_at": 1}"#,
        0,
    );
    assert_eq!(resp.status, 409);
    let conflict: ConflictResponse<BlockedBy> = body_as(&resp);
    assert_eq!(conflict.current.version, 3);

    let resp = patch_at(&sql, "/api/blocked_by/a/ghost", r#"{"expected_version": 1}"#, 0);
    assert_eq!(resp.status, 404);
}

#[test]
fn patch_setting_removed_at_to_its_current_value_is_a_noop() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a");
    seed_item(&sql, "b");
    post_to(&sql, "/api/blocked_by", &edge_body("a", "b"), 0); // version 3
    patch_at(
        &sql,
        "/api/blocked_by/a/b",
        r#"{"expected_version": 3, "removed_at": 5000}"#,
        0,
    ); // version 4
    let resp = patch_at(
        &sql,
        "/api/blocked_by/a/b",
        r#"{"expected_version": 4, "removed_at": 5000}"#,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged: BlockedBy = body_as(&resp);
    assert_eq!(unchanged.version, 4, "no version bump for a value-identical patch");
    assert_eq!(meta_version(&sql), 4);
}

// The integer/real hazard (#166) — see items.rs for the full explanation.
#[test]
fn patch_value_identical_removed_at_noops_even_when_the_row_reads_back_as_real() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a");
    seed_item(&sql, "b");
    post_to(&sql, "/api/blocked_by", &edge_body("a", "b"), 0); // version 3
    patch_at(
        &sql,
        "/api/blocked_by/a/b",
        r#"{"expected_version": 3, "removed_at": 5000}"#,
        0,
    ); // version 4
    let wrapped = RealCoercingSql::new(&sql);
    let resp = patch_at(
        &wrapped,
        "/api/blocked_by/a/b",
        r#"{"expected_version": 4, "removed_at": 5000}"#,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged: BlockedBy = body_as(&resp);
    assert_eq!(unchanged.version, 4, "removed_at: Integer(5000) vs Real(5000.0) must still compare equal");
    assert_eq!(meta_version(&sql), 4);
}
