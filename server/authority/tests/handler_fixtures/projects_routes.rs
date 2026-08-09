//! Projects and their 1:1 Routes: create births both rows under one
//! version stamp; routes are PATCH-only.

use hummingbird_domain::{ConflictResponse, Project, Route};

use crate::rig::*;

#[test]
fn create_project_returns_201_and_births_the_route_row() {
    let sql = RusqliteSql::new();
    let resp = post_to(&sql, "/api/projects", r#"{"id": "p-1", "name": "sell the M3"}"#, 1000);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created: Project = body_as(&resp);
    assert_eq!(created.id, "p-1");
    assert_eq!(created.name, "sell the M3");
    assert_eq!(created.version, 1);
    assert_eq!(created.archived_at, None);
    assert_eq!(meta_version(&sql), 1, "one HTTP write = one bump, route included");

    let routes = sql.exec("SELECT * FROM routes WHERE project_id = 'p-1'", &[]).unwrap();
    assert_eq!(routes.len(), 1, "the Route row is born with its project");
    assert_eq!(
        routes[0].get("version").unwrap().as_i64(),
        Some(1),
        "the route shares the project's version stamp"
    );
    assert_eq!(
        routes[0].get("destination").unwrap().as_text(),
        None,
        "born empty — /to-actions fills it"
    );
}

#[test]
fn create_project_replay_returns_200_without_bump_or_second_route() {
    let sql = RusqliteSql::new();
    post_to(&sql, "/api/projects", r#"{"id": "p-1", "name": "sell the M3"}"#, 1000);
    let resp = post_to(&sql, "/api/projects", r#"{"id": "p-1", "name": "divergent"}"#, 2000);
    assert_eq!(resp.status, 200, "replay is success");
    let replayed: Project = body_as(&resp);
    assert_eq!(replayed.name, "sell the M3", "the stored row, not the divergent payload");
    assert_eq!(meta_version(&sql), 1, "no bump");
    let routes = sql.exec("SELECT project_id FROM routes", &[]).unwrap();
    assert_eq!(routes.len(), 1, "no duplicate route row");
}

#[test]
fn create_project_validation_400() {
    let sql = RusqliteSql::new();
    for (body, why) in [
        (r#"{"id": "", "name": "n"}"#, "empty id"),
        (r#"{"id": "p", "name": ""}"#, "empty name"),
        (r#"{"id": "p", "name": "n", "version": 3}"#, "server-stamped field"),
    ] {
        let resp = post_to(&sql, "/api/projects", body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 0);
}

#[test]
fn patch_project_renames_and_archives_under_cas() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // version 1
    let resp = patch_at(
        &sql,
        "/api/projects/p-1",
        r#"{"expected_version": 1, "name": "renamed"}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated: Project = body_as(&resp);
    assert_eq!(updated.name, "renamed");
    assert_eq!(updated.version, 2);
    assert_eq!(updated.updated_at, 2000);

    let resp = patch_at(
        &sql,
        "/api/projects/p-1",
        r#"{"expected_version": 2, "archived_at": 9000}"#,
        3000,
    );
    let archived: Project = body_as(&resp);
    assert_eq!(archived.archived_at, Some(9000), "archival is a flag, not a delete");
}

#[test]
fn patch_project_stale_version_409_carries_current_project() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    let resp = patch_at(
        &sql,
        "/api/projects/p-1",
        r#"{"expected_version": 42, "name": "x"}"#,
        0,
    );
    assert_eq!(resp.status, 409);
    let conflict: ConflictResponse<Project> = body_as(&resp);
    assert_eq!(conflict.error, "version_conflict");
    assert_eq!(conflict.current.version, 1);
    assert_eq!(meta_version(&sql), 1, "a stale write bumps nothing");
}

#[test]
fn patch_project_unknown_id_404_and_empty_patch_noop() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    let resp = patch_at(&sql, "/api/projects/ghost", r#"{"expected_version": 1}"#, 0);
    assert_eq!(resp.status, 404);

    let resp = patch_at(&sql, "/api/projects/p-1", r#"{"expected_version": 1}"#, 0);
    assert_eq!(resp.status, 200);
    assert_eq!(meta_version(&sql), 1, "empty patch bumps nothing");
}

// ---------------------------------------------------------------- routes

#[test]
fn patch_route_sets_and_clears_content_under_cas() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // project + route at version 1
    let resp = patch_at(
        &sql,
        "/api/routes/p-1",
        r#"{"expected_version": 1, "destination": "car sold", "notes": "list it first"}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let route: Route = body_as(&resp);
    assert_eq!(route.destination.as_deref(), Some("car sold"));
    assert_eq!(route.notes.as_deref(), Some("list it first"));
    assert_eq!(route.version, 2);
    assert_eq!(route.updated_at, 2000);
    assert_eq!(meta_version(&sql), 2);

    let resp = patch_at(
        &sql,
        "/api/routes/p-1",
        r#"{"expected_version": 2, "notes": null}"#,
        3000,
    );
    let route: Route = body_as(&resp);
    assert_eq!(route.notes, None, "explicit null clears");
    assert_eq!(route.destination.as_deref(), Some("car sold"), "absent field untouched");
}

#[test]
fn patch_route_stale_version_409_carries_current_route() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    let resp = patch_at(
        &sql,
        "/api/routes/p-1",
        r#"{"expected_version": 7, "destination": "x"}"#,
        0,
    );
    assert_eq!(resp.status, 409);
    let conflict: ConflictResponse<Route> = body_as(&resp);
    assert_eq!(conflict.current.project_id, "p-1");
    assert_eq!(conflict.current.version, 1);
}

#[test]
fn patch_route_unknown_project_404() {
    let sql = RusqliteSql::new();
    let resp = patch_at(&sql, "/api/routes/ghost", r#"{"expected_version": 1}"#, 0);
    assert_eq!(resp.status, 404);
}
