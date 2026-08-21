//! Projects and their 1:1 Routes: create births both rows under one
//! version stamp; routes are PATCH-only.

use hummingbird_domain::{ChangesResponse, ConflictResponse, Item, Project, Route};

use crate::rig::*;

// ---------------------------------------------- archive cascade (#630)

/// Reads one item's current row via a full sweep — the cascade bumps a
/// row's own `version` on every write, so guessing at a CAS-friendly
/// `expected_version` to read it back through PATCH is the wrong tool;
/// `GET /api/changes` is a plain, side-effect-free read.
fn item_now(sql: &dyn Sql, id: &str) -> Item {
    let parsed: ChangesResponse = body_as(&changes(sql, "since=0"));
    parsed
        .items
        .into_iter()
        .find(|item| item.id == id)
        .unwrap_or_else(|| panic!("no such item: {id}"))
}

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
        (r#"{"id": "p", "name": "n", "github_repo": "not-a-slug"}"#, "malformed github_repo"),
        (
            r#"{"id": "p", "name": "n", "github_repo": "https://github.com/o/r"}"#,
            "a URL, not owner/repo",
        ),
    ] {
        let resp = post_to(&sql, "/api/projects", body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 0);
}

#[test]
fn create_project_carries_github_repo_and_default_context() {
    let sql = RusqliteSql::new();
    let resp = post_to(
        &sql,
        "/api/projects",
        r#"{"id": "p-1", "name": "sell the M3", "github_repo": "JddAndrewLauren/hummingbird", "default_context": "@computer"}"#,
        1000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created: Project = body_as(&resp);
    assert_eq!(created.github_repo.as_deref(), Some("JddAndrewLauren/hummingbird"));
    assert_eq!(created.default_context.as_deref(), Some("@computer"));
}

#[test]
fn create_project_without_the_new_fields_leaves_them_null() {
    let sql = RusqliteSql::new();
    let resp = post_to(&sql, "/api/projects", r#"{"id": "p-1", "name": "sell the M3"}"#, 1000);
    let created: Project = body_as(&resp);
    assert_eq!(created.github_repo, None);
    assert_eq!(created.default_context, None);
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
fn patch_project_sets_and_clears_github_repo_and_default_context() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // version 1
    let resp = patch_at(
        &sql,
        "/api/projects/p-1",
        r#"{"expected_version": 1, "github_repo": "JddAndrewLauren/hummingbird", "default_context": "@computer"}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated: Project = body_as(&resp);
    assert_eq!(updated.github_repo.as_deref(), Some("JddAndrewLauren/hummingbird"));
    assert_eq!(updated.default_context.as_deref(), Some("@computer"));
    assert_eq!(updated.version, 2);

    let resp = patch_at(
        &sql,
        "/api/projects/p-1",
        r#"{"expected_version": 2, "github_repo": null, "default_context": null}"#,
        3000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let cleared: Project = body_as(&resp);
    assert_eq!(cleared.github_repo, None, "explicit null clears");
    assert_eq!(cleared.default_context, None, "explicit null clears");
    assert_eq!(cleared.version, 3);
}

#[test]
fn patch_project_rejects_a_malformed_github_repo() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    let resp = patch_at(
        &sql,
        "/api/projects/p-1",
        r#"{"expected_version": 1, "github_repo": "not-a-slug"}"#,
        0,
    );
    assert_eq!(resp.status, 400, "{}", resp.body);
    assert_eq!(meta_version(&sql), 1, "the rejected patch bumps nothing");
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

#[test]
fn patch_project_setting_every_field_to_its_current_value_is_a_noop() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // version 1
    let resp = patch_at(
        &sql,
        "/api/projects/p-1",
        r#"{"expected_version": 1, "name": "seeded p-1", "archived_at": null}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged: Project = body_as(&resp);
    assert_eq!(unchanged.version, 1, "no version bump for a value-identical patch");
    assert_eq!(unchanged.updated_at, 0, "no updated_at restamp");
    assert_eq!(meta_version(&sql), 1);
}

#[test]
fn patch_project_mixing_changed_and_unchanged_fields_bumps_once() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // version 1
    let resp = patch_at(
        &sql,
        "/api/projects/p-1",
        r#"{"expected_version": 1, "name": "seeded p-1", "archived_at": 9000}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated: Project = body_as(&resp);
    assert_eq!(updated.name, "seeded p-1", "the unchanged field is left as-is");
    assert_eq!(updated.archived_at, Some(9000), "the changed field is written");
    assert_eq!(updated.version, 2, "a mixed patch still bumps");
}

// The integer/real hazard (#166) — see items.rs for the full explanation.
#[test]
fn patch_project_value_identical_archived_at_noops_even_when_the_row_reads_back_as_real() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // version 1
    patch_at(&sql, "/api/projects/p-1", r#"{"expected_version": 1, "archived_at": 9000}"#, 0); // version 2
    let wrapped = RealCoercingSql::new(&sql);
    let resp = patch_at(&wrapped, "/api/projects/p-1", r#"{"expected_version": 2, "archived_at": 9000}"#, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged: Project = body_as(&resp);
    assert_eq!(unchanged.version, 2, "archived_at: Integer(9000) vs Real(9000.0) must still compare equal");
    assert_eq!(meta_version(&sql), 2);
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

#[test]
fn patch_route_setting_every_field_to_its_current_value_is_a_noop() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // project + route at version 1
    patch_at(
        &sql,
        "/api/routes/p-1",
        r#"{"expected_version": 1, "destination": "car sold", "notes": "list it first"}"#,
        2000,
    ); // version 2
    let resp = patch_at(
        &sql,
        "/api/routes/p-1",
        r#"{"expected_version": 2, "destination": "car sold", "notes": "list it first"}"#,
        3000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged: Route = body_as(&resp);
    assert_eq!(unchanged.version, 2, "no version bump for a value-identical patch");
    assert_eq!(unchanged.updated_at, 2000, "no updated_at restamp");
    assert_eq!(meta_version(&sql), 2);
}

#[test]
fn patch_route_mixing_changed_and_unchanged_fields_bumps_once() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    patch_at(
        &sql,
        "/api/routes/p-1",
        r#"{"expected_version": 1, "destination": "car sold", "notes": "list it first"}"#,
        2000,
    ); // version 2
    let resp = patch_at(
        &sql,
        "/api/routes/p-1",
        r#"{"expected_version": 2, "destination": "car sold", "notes": "list it, then sell"}"#,
        3000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated: Route = body_as(&resp);
    assert_eq!(updated.destination.as_deref(), Some("car sold"), "unchanged field kept");
    assert_eq!(updated.notes.as_deref(), Some("list it, then sell"), "changed field written");
    assert_eq!(updated.version, 3, "a mixed patch still bumps");
}

// -------------------------------------------------------- archive cascade

/// ADR-0030 decision 5's whole mechanism: archiving stamps every live item
/// with the project's *exact* `archived_at`, not a generic "now".
#[test]
fn archiving_a_project_stamps_every_live_item_with_its_exact_timestamp() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // version 1
    post(&sql, r#"{"id": "a-1", "title": "one", "project_id": "p-1"}"#, 0);
    post(&sql, r#"{"id": "a-2", "title": "two", "project_id": "p-1"}"#, 0);
    // A different project's item must never be touched.
    seed_project(&sql, "p-2");
    post(&sql, r#"{"id": "b-1", "title": "elsewhere", "project_id": "p-2"}"#, 0);

    let resp = patch_at(
        &sql,
        "/api/projects/p-1",
        r#"{"expected_version": 1, "archived_at": 9000}"#,
        9000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let archived: Project = body_as(&resp);
    assert_eq!(archived.archived_at, Some(9000));

    assert_eq!(item_now(&sql, "a-1").archived_at, Some(9000), "cascaded to the project's exact stamp");
    assert_eq!(item_now(&sql, "a-2").archived_at, Some(9000));
    assert_eq!(item_now(&sql, "b-1").archived_at, None, "a different project's item is untouched");
}

/// The whole point of the matched timestamp: an item archived individually
/// *before* the project was survives the project's own archive/unarchive
/// round trip, while an item the cascade itself archived comes back.
#[test]
fn patch_project_archive_cascade_covers_the_mixed_case_end_to_end() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // version 1

    // An item archived individually before the project's own archive.
    post(&sql, r#"{"id": "solo", "title": "archived on its own", "project_id": "p-1"}"#, 0);
    let solo_v1 = item_now(&sql, "solo").version;
    patch(&sql, "solo", &format!(r#"{{"expected_version": {solo_v1}, "archived_at": 500}}"#), 0);

    // Two items still live when the project archives.
    post(&sql, r#"{"id": "cascaded", "title": "goes down with the project", "project_id": "p-1"}"#, 0);
    post(&sql, r#"{"id": "reopened", "title": "comes back up", "project_id": "p-1"}"#, 0);

    let resp = patch_at(
        &sql,
        "/api/projects/p-1",
        r#"{"expected_version": 1, "archived_at": 9000}"#,
        9000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let archived: Project = body_as(&resp);

    assert_eq!(item_now(&sql, "cascaded").archived_at, Some(9000), "archived by the cascade");
    assert_eq!(
        item_now(&sql, "solo").archived_at,
        Some(500),
        "the pre-existing individual archive is untouched by the cascade"
    );

    // Unarchive the project. The global version counter has moved on past
    // the small numbers the seed steps above might suggest — every write to
    // any table shares it — so the CAS `expected_version` here is the
    // archive response's own `version`, not a guessed literal.
    let resp = patch_at(
        &sql,
        "/api/projects/p-1",
        &format!(r#"{{"expected_version": {}, "archived_at": null}}"#, archived.version),
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unarchived: Project = body_as(&resp);
    assert_eq!(unarchived.archived_at, None);

    assert_eq!(item_now(&sql, "cascaded").archived_at, None, "the cascade-archived item comes back up");
    assert_eq!(
        item_now(&sql, "solo").archived_at,
        Some(500),
        "the individually-archived item stays archived through the round trip \
         (a blanket clear-all would silently resurrect it)"
    );
}

/// Steps cascade implicitly through their item — the cascade must never
/// stamp `steps.deleted_at`.
#[test]
fn patch_project_archive_cascade_never_writes_a_steps_delete_flag() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // version 1
    post(&sql, r#"{"id": "a-1", "title": "has steps", "project_id": "p-1"}"#, 0);
    post_to(&sql, "/api/steps", r#"{"id": "s-1", "item_id": "a-1", "body": "do it", "position": 0}"#, 0);

    patch_at(&sql, "/api/projects/p-1", r#"{"expected_version": 1, "archived_at": 9000}"#, 9000);

    let rows = sql.exec("SELECT deleted_at FROM steps WHERE id = 's-1'", &[]).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("deleted_at").unwrap().as_i64(), None, "the cascade never flags a step deleted");
}

/// A project archive/unarchive with no items in it cascades nothing and
/// bumps only the project's own version.
#[test]
fn patch_project_archive_cascade_is_a_noop_with_no_items() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // version 1
    let resp = patch_at(&sql, "/api/projects/p-1", r#"{"expected_version": 1, "archived_at": 9000}"#, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(meta_version(&sql), 2, "only the project's own bump");
}
