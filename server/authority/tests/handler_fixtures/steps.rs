//! Steps: the below-item checklist whose one-scalar tick is the write
//! ADR-0008 exists for.

use hummingbird_domain::{ConflictResponse, Step};

use crate::rig::*;

#[test]
fn create_step_201_defaults_undone_and_replays_200() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1"); // version 1
    let body = r#"{"id": "s-1", "item_id": "a-1", "body": "empty the shelf", "position": 1}"#;
    let resp = post_to(&sql, "/api/steps", body, 0);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created: Step = body_as(&resp);
    assert!(!created.done, "born unticked");
    assert_eq!(created.deleted_at, None);
    assert_eq!(created.version, 2);

    let resp = post_to(&sql, "/api/steps", body, 0);
    assert_eq!(resp.status, 200, "replay is success");
    assert_eq!(meta_version(&sql), 2, "no bump on replay");
}

#[test]
fn create_step_validation_400() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    for (body, why) in [
        (r#"{"id": "", "item_id": "a-1", "body": "b", "position": 1}"#, "empty id"),
        (r#"{"id": "s", "item_id": "a-1", "body": "", "position": 1}"#, "empty body"),
        (r#"{"id": "s", "item_id": "ghost", "body": "b", "position": 1}"#, "unknown item"),
        (r#"{"id": "s", "item_id": "a-1", "body": "b", "position": 1, "done": true}"#, "server-defaulted field"),
    ] {
        let resp = post_to(&sql, "/api/steps", body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 1, "no write happened");
}

#[test]
fn ticking_a_step_is_one_scalar_cas_write() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    post_to(
        &sql,
        "/api/steps",
        r#"{"id": "s-1", "item_id": "a-1", "body": "empty the shelf", "position": 1}"#,
        0,
    ); // version 2

    let resp = patch_at(&sql, "/api/steps/s-1", r#"{"expected_version": 2, "done": true}"#, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let ticked: Step = body_as(&resp);
    assert!(ticked.done);
    assert_eq!(ticked.version, 3);

    let resp = patch_at(&sql, "/api/steps/s-1", r#"{"expected_version": 3, "done": false}"#, 0);
    let unticked: Step = body_as(&resp);
    assert!(!unticked.done, "untick is the same scalar write");
}

#[test]
fn patch_step_edits_moves_deletes_and_conflicts() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    post_to(
        &sql,
        "/api/steps",
        r#"{"id": "s-1", "item_id": "a-1", "body": "empty the shelf", "position": 1}"#,
        0,
    ); // version 2

    let resp = patch_at(
        &sql,
        "/api/steps/s-1",
        r#"{"expected_version": 2, "body": "empty both shelves", "position": 4}"#,
        0,
    );
    let updated: Step = body_as(&resp);
    assert_eq!(updated.body, "empty both shelves");
    assert_eq!(updated.position, 4);

    let resp = patch_at(&sql, "/api/steps/s-1", r#"{"expected_version": 3, "deleted_at": 5000}"#, 0);
    let deleted: Step = body_as(&resp);
    assert_eq!(deleted.deleted_at, Some(5000), "deletion is a flag, never erased");

    let resp = patch_at(&sql, "/api/steps/s-1", r#"{"expected_version": 1, "done": true}"#, 0);
    assert_eq!(resp.status, 409);
    let conflict: ConflictResponse<Step> = body_as(&resp);
    assert_eq!(conflict.current.version, 4, "the 409 carries the current step");
    assert_eq!(meta_version(&sql), 4, "a stale write bumps nothing");
}

#[test]
fn patch_setting_every_field_to_its_current_value_is_a_noop() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    post_to(
        &sql,
        "/api/steps",
        r#"{"id": "s-1", "item_id": "a-1", "body": "empty the shelf", "position": 1}"#,
        0,
    ); // version 2
    let resp = patch_at(
        &sql,
        "/api/steps/s-1",
        r#"{"expected_version": 2, "body": "empty the shelf", "done": false, "position": 1}"#,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged: Step = body_as(&resp);
    assert_eq!(unchanged.version, 2, "no version bump for a value-identical patch");
    assert_eq!(meta_version(&sql), 2);
}

#[test]
fn patch_mixing_changed_and_unchanged_fields_writes_only_the_changed_ones() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    post_to(
        &sql,
        "/api/steps",
        r#"{"id": "s-1", "item_id": "a-1", "body": "empty the shelf", "position": 1}"#,
        0,
    ); // version 2
    let resp = patch_at(
        &sql,
        "/api/steps/s-1",
        r#"{"expected_version": 2, "body": "empty the shelf", "done": true}"#,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated: Step = body_as(&resp);
    assert_eq!(updated.body, "empty the shelf", "the unchanged field is left as-is");
    assert!(updated.done, "the changed field is written");
    assert_eq!(updated.version, 3, "a mixed patch still bumps");
}

// The integer/real hazard (#166) — see items.rs for the full explanation.
#[test]
fn patch_value_identical_done_noops_even_when_the_row_reads_back_as_real() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    post_to(
        &sql,
        "/api/steps",
        r#"{"id": "s-1", "item_id": "a-1", "body": "b", "position": 1}"#,
        0,
    ); // version 2
    patch_at(&sql, "/api/steps/s-1", r#"{"expected_version": 2, "done": true}"#, 0); // version 3
    let wrapped = RealCoercingSql::new(&sql);
    let resp = patch_at(&wrapped, "/api/steps/s-1", r#"{"expected_version": 3, "done": true}"#, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged: Step = body_as(&resp);
    assert_eq!(unchanged.version, 3, "done: Integer(1) vs Real(1.0) must still compare equal");
    assert_eq!(meta_version(&sql), 3);
}

#[test]
fn patch_step_unknown_id_404_and_null_done_400() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    post_to(
        &sql,
        "/api/steps",
        r#"{"id": "s-1", "item_id": "a-1", "body": "b", "position": 1}"#,
        0,
    );
    assert_eq!(
        patch_at(&sql, "/api/steps/ghost", r#"{"expected_version": 1}"#, 0).status,
        404
    );
    let resp = patch_at(&sql, "/api/steps/s-1", r#"{"expected_version": 2, "done": null}"#, 0);
    assert_eq!(resp.status, 400, "{}", resp.body);
    assert!(resp.body.contains("may not be null"));
}
