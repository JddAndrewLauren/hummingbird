//! Project Links (#626, ADR-0030 decision 4): created against a real
//! project, removed by flagging. Mirrors `fog.rs`'s shape exactly.

use hummingbird_domain::{ConflictResponse, ProjectLink};

use crate::rig::*;

#[test]
fn create_project_link_201_and_replay_200_without_bump() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // version 1
    let body = r#"{"id": "l-1", "project_id": "p-1", "url": "https://example.com", "label": "Example", "position": 1}"#;
    let resp = post_to(&sql, "/api/project_links", body, 0);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created: ProjectLink = body_as(&resp);
    assert_eq!(created.url, "https://example.com");
    assert_eq!(created.label, Some("Example".to_string()));
    assert_eq!(created.removed_at, None);
    assert_eq!(created.version, 2);

    let resp = post_to(&sql, "/api/project_links", body, 0);
    assert_eq!(resp.status, 200, "replay is success");
    assert_eq!(meta_version(&sql), 2, "no bump on replay");
}

#[test]
fn create_project_link_defaults_label_to_none() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    let resp = post_to(
        &sql,
        "/api/project_links",
        r#"{"id": "l-1", "project_id": "p-1", "url": "https://example.com", "position": 1}"#,
        0,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created: ProjectLink = body_as(&resp);
    assert_eq!(created.label, None);
}

#[test]
fn create_project_link_validation_400() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    for (body, why) in [
        (r#"{"id": "", "project_id": "p-1", "url": "https://example.com", "position": 1}"#, "empty id"),
        (r#"{"id": "l", "project_id": "p-1", "url": "", "position": 1}"#, "empty url"),
        (r#"{"id": "l", "project_id": "ghost", "url": "https://example.com", "position": 1}"#, "unknown project"),
    ] {
        let resp = post_to(&sql, "/api/project_links", body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 1, "no write happened");
}

#[test]
fn patch_project_link_rewords_moves_and_removes_under_cas() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    post_to(
        &sql,
        "/api/project_links",
        r#"{"id": "l-1", "project_id": "p-1", "url": "https://example.com", "label": "Example", "position": 1}"#,
        0,
    ); // version 2
    let resp = patch_at(
        &sql,
        "/api/project_links/l-1",
        r#"{"expected_version": 2, "url": "https://example.com/docs", "label": "Docs", "position": 3}"#,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated: ProjectLink = body_as(&resp);
    assert_eq!(updated.url, "https://example.com/docs");
    assert_eq!(updated.label, Some("Docs".to_string()));
    assert_eq!(updated.position, 3);
    assert_eq!(updated.version, 3);

    let resp = patch_at(&sql, "/api/project_links/l-1", r#"{"expected_version": 3, "removed_at": 7000}"#, 0);
    let removed: ProjectLink = body_as(&resp);
    assert_eq!(removed.removed_at, Some(7000), "removal is a flag, not a delete");

    // Un-removing: an explicit null clears the flag, same as fog's resolved_at.
    let resp = patch_at(&sql, "/api/project_links/l-1", r#"{"expected_version": 4, "removed_at": null}"#, 0);
    let restored: ProjectLink = body_as(&resp);
    assert_eq!(restored.removed_at, None);

    let resp = patch_at(&sql, "/api/project_links/l-1", r#"{"expected_version": 99}"#, 0);
    assert_eq!(resp.status, 409);
    let conflict: ConflictResponse<ProjectLink> = body_as(&resp);
    assert_eq!(conflict.current.version, 5);
}

#[test]
fn patch_project_link_clears_label_with_explicit_null() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    post_to(
        &sql,
        "/api/project_links",
        r#"{"id": "l-1", "project_id": "p-1", "url": "https://example.com", "label": "Example", "position": 1}"#,
        0,
    ); // version 2
    let resp = patch_at(&sql, "/api/project_links/l-1", r#"{"expected_version": 2, "label": null}"#, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated: ProjectLink = body_as(&resp);
    assert_eq!(updated.label, None, "explicit null clears the label");
    assert_eq!(updated.version, 3);
}

#[test]
fn patch_project_link_setting_every_field_to_its_current_value_is_a_noop() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    post_to(
        &sql,
        "/api/project_links",
        r#"{"id": "l-1", "project_id": "p-1", "url": "https://example.com", "label": "Example", "position": 1}"#,
        0,
    ); // version 2
    let resp = patch_at(
        &sql,
        "/api/project_links/l-1",
        r#"{"expected_version": 2, "url": "https://example.com", "label": "Example", "position": 1, "removed_at": null}"#,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged: ProjectLink = body_as(&resp);
    assert_eq!(unchanged.version, 2, "no version bump for a value-identical patch");
    assert_eq!(meta_version(&sql), 2);
}

#[test]
fn patch_project_link_unknown_id_404_and_null_url_400() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    post_to(
        &sql,
        "/api/project_links",
        r#"{"id": "l-1", "project_id": "p-1", "url": "https://example.com", "position": 1}"#,
        0,
    );
    assert_eq!(
        patch_at(&sql, "/api/project_links/ghost", r#"{"expected_version": 1}"#, 0).status,
        404
    );
    let resp = patch_at(&sql, "/api/project_links/l-1", r#"{"expected_version": 2, "url": null}"#, 0);
    assert_eq!(resp.status, 400, "{}", resp.body);
}
