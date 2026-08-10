//! Fog segments: created against a real project, resolved by flagging.

use hummingbird_domain::{ConflictResponse, Fog};

use crate::rig::*;

#[test]
fn create_fog_201_and_replay_200_without_bump() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // version 1
    let body = r#"{"id": "f-1", "project_id": "p-1", "question": "which movers?", "position": 1}"#;
    let resp = post_to(&sql, "/api/fog", body, 0);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created: Fog = body_as(&resp);
    assert_eq!(created.question, "which movers?");
    assert_eq!(created.resolved_at, None);
    assert_eq!(created.version, 2);

    let resp = post_to(&sql, "/api/fog", body, 0);
    assert_eq!(resp.status, 200, "replay is success");
    assert_eq!(meta_version(&sql), 2, "no bump on replay");
}

#[test]
fn create_fog_validation_400() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    for (body, why) in [
        (r#"{"id": "", "project_id": "p-1", "question": "q", "position": 1}"#, "empty id"),
        (r#"{"id": "f", "project_id": "p-1", "question": "", "position": 1}"#, "empty question"),
        (r#"{"id": "f", "project_id": "ghost", "question": "q", "position": 1}"#, "unknown project"),
    ] {
        let resp = post_to(&sql, "/api/fog", body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 1, "no write happened");
}

#[test]
fn patch_fog_rewords_moves_and_resolves_under_cas() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    post_to(
        &sql,
        "/api/fog",
        r#"{"id": "f-1", "project_id": "p-1", "question": "which movers?", "position": 1}"#,
        0,
    ); // version 2
    let resp = patch_at(
        &sql,
        "/api/fog/f-1",
        r#"{"expected_version": 2, "question": "which movers, and when?", "position": 3}"#,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated: Fog = body_as(&resp);
    assert_eq!(updated.question, "which movers, and when?");
    assert_eq!(updated.position, 3);
    assert_eq!(updated.version, 3);

    let resp = patch_at(&sql, "/api/fog/f-1", r#"{"expected_version": 3, "resolved_at": 7000}"#, 0);
    let resolved: Fog = body_as(&resp);
    assert_eq!(resolved.resolved_at, Some(7000), "resolution is a flag, not a delete");

    let resp = patch_at(&sql, "/api/fog/f-1", r#"{"expected_version": 99}"#, 0);
    assert_eq!(resp.status, 409);
    let conflict: ConflictResponse<Fog> = body_as(&resp);
    assert_eq!(conflict.current.version, 4);
}

#[test]
fn patch_fog_setting_every_field_to_its_current_value_is_a_noop() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    post_to(
        &sql,
        "/api/fog",
        r#"{"id": "f-1", "project_id": "p-1", "question": "which movers?", "position": 1}"#,
        0,
    ); // version 2
    let resp = patch_at(
        &sql,
        "/api/fog/f-1",
        r#"{"expected_version": 2, "question": "which movers?", "position": 1, "resolved_at": null}"#,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged: Fog = body_as(&resp);
    assert_eq!(unchanged.version, 2, "no version bump for a value-identical patch");
    assert_eq!(meta_version(&sql), 2);
}

#[test]
fn patch_fog_mixing_changed_and_unchanged_fields_bumps_once() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    post_to(
        &sql,
        "/api/fog",
        r#"{"id": "f-1", "project_id": "p-1", "question": "which movers?", "position": 1}"#,
        0,
    ); // version 2
    let resp = patch_at(
        &sql,
        "/api/fog/f-1",
        r#"{"expected_version": 2, "question": "which movers?", "position": 3}"#,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated: Fog = body_as(&resp);
    assert_eq!(updated.question, "which movers?", "unchanged field kept");
    assert_eq!(updated.position, 3, "changed field written");
    assert_eq!(updated.version, 3, "a mixed patch still bumps");
}

// The integer/real hazard (#166) — see items.rs for the full explanation.
#[test]
fn patch_fog_value_identical_position_noops_even_when_the_row_reads_back_as_real() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    post_to(
        &sql,
        "/api/fog",
        r#"{"id": "f-1", "project_id": "p-1", "question": "q", "position": 1}"#,
        0,
    ); // version 2
    let wrapped = RealCoercingSql::new(&sql);
    let resp = patch_at(&wrapped, "/api/fog/f-1", r#"{"expected_version": 2, "position": 1}"#, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged: Fog = body_as(&resp);
    assert_eq!(unchanged.version, 2, "position: Integer(1) vs Real(1.0) must still compare equal");
    assert_eq!(meta_version(&sql), 2);
}

#[test]
fn patch_fog_unknown_id_404_and_null_question_400() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    post_to(
        &sql,
        "/api/fog",
        r#"{"id": "f-1", "project_id": "p-1", "question": "q", "position": 1}"#,
        0,
    );
    assert_eq!(
        patch_at(&sql, "/api/fog/ghost", r#"{"expected_version": 1}"#, 0).status,
        404
    );
    let resp = patch_at(&sql, "/api/fog/f-1", r#"{"expected_version": 2, "question": null}"#, 0);
    assert_eq!(resp.status, 400, "{}", resp.body);
}
