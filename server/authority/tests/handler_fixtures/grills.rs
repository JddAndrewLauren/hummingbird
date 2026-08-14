//! `POST /api/grills` and `GET /api/grills/:id` (#353, ADR-0023): the
//! immutable per-item Grill attachment, and the two anti-goals — no
//! transcript on the sweep/delta wire, and no new `auth::permitted` arm.

use hummingbird_domain::{ChangesResponse, Grill, Stage};

use crate::rig::*;

fn post_grill(sql: &dyn Sql, body: &str, now_ms: i64) -> hummingbird_authority::ApiResponse {
    post_to(sql, "/api/grills", body, now_ms)
}

fn get_grill(sql: &dyn Sql, id: &str) -> hummingbird_authority::ApiResponse {
    req(sql, "GET", &format!("/api/grills/{id}"), None, None, 0)
}

#[test]
fn create_grill_201_stamps_resulting_stage_and_replays_200() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1"); // version 1, born in triage
    let body = r#"{
        "id": "g-1", "item_id": "a-1", "transcript": "turn 1\nturn 2",
        "summary": "clarified the destination", "verdict": "resolved",
        "model_proposal": "{\"stage\":\"ready\"}", "applied_patch": "{\"stage\":\"ready\"}"
    }"#;
    let resp = post_grill(&sql, body, 5000);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created: Grill = body_as(&resp);
    assert_eq!(created.item_id, "a-1");
    assert_eq!(created.transcript, "turn 1\nturn 2");
    assert_eq!(
        created.resulting_stage,
        Stage::Ready,
        "triage + resolved -> ready, the stored resulting_stage \
         (hummingbird_domain::resulting_stage's own answer, never re-derived)",
    );
    assert_eq!(created.completed_at, 5000, "server-stamped from the clock");
    assert_eq!(created.version, 2);

    let resp = post_grill(&sql, body, 9999);
    assert_eq!(resp.status, 200, "replay is success");
    let replayed: Grill = body_as(&resp);
    assert_eq!(replayed.completed_at, 5000, "the stored row, not a re-stamp");
    assert_eq!(meta_version(&sql), 2, "no bump on replay");
}

#[test]
fn resulting_stage_is_computed_from_the_items_current_stage_never_accepted_from_the_caller() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1"); // v1, triage
    patch(&sql, "a-1", r#"{"expected_version": 1, "stage": "grilling"}"#, 0); // v2

    let resp = post_grill(
        &sql,
        r#"{"id": "g-1", "item_id": "a-1", "transcript": "t", "summary": "s",
            "verdict": "fog_remains", "model_proposal": "p", "applied_patch": "p"}"#,
        0,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created: Grill = body_as(&resp);
    assert_eq!(
        created.resulting_stage,
        Stage::Grilling,
        "grilling + fog_remains -> grilling (resulting_stage's own table)",
    );
}

#[test]
fn create_grill_on_a_done_item_is_a_400() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1"); // v1
    patch(&sql, "a-1", r#"{"expected_version": 1, "stage": "done"}"#, 0); // v2

    let resp = post_grill(
        &sql,
        r#"{"id": "g-1", "item_id": "a-1", "transcript": "t", "summary": "s",
            "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#,
        0,
    );
    assert_eq!(resp.status, 400, "{}", resp.body);
    assert_eq!(meta_version(&sql), 2, "no write happened");
}

#[test]
fn create_grill_validation_400() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    for (body, why) in [
        (
            r#"{"id": "", "item_id": "a-1", "transcript": "t", "summary": "s",
                "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#,
            "empty id",
        ),
        (
            r#"{"id": "g", "item_id": "ghost", "transcript": "t", "summary": "s",
                "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#,
            "unknown item",
        ),
    ] {
        let resp = post_grill(&sql, body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 1, "no write happened");
}

#[test]
fn get_grill_returns_the_full_transcript() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    post_grill(
        &sql,
        r#"{"id": "g-1", "item_id": "a-1", "transcript": "the whole back-and-forth",
            "summary": "s", "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#,
        0,
    );

    let resp = get_grill(&sql, "g-1");
    assert_eq!(resp.status, 200, "{}", resp.body);
    let fetched: Grill = body_as(&resp);
    assert_eq!(fetched.transcript, "the whole back-and-forth");
}

#[test]
fn get_grill_unknown_id_404() {
    let sql = RusqliteSql::new();
    assert_eq!(get_grill(&sql, "ghost").status, 404);
}

/// The whole point of #353: a sweep or delta response for an item with a
/// completed grill carries the record and *not* the transcript.
#[test]
fn sweep_and_delta_carry_grills_without_the_transcript() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1"); // v1
    post_grill(
        &sql,
        r#"{"id": "g-1", "item_id": "a-1", "transcript": "top secret transcript text",
            "summary": "s", "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#,
        0,
    ); // v2

    let sweep_resp = sweep(&sql);
    assert_eq!(sweep_resp.status, 200, "{}", sweep_resp.body);
    assert!(
        !sweep_resp.body.contains("top secret transcript text"),
        "the sweep must never carry the transcript: {}",
        sweep_resp.body
    );
    let parsed: ChangesResponse = body_as(&sweep_resp);
    assert_eq!(parsed.grills.len(), 1);
    assert_eq!(parsed.grills[0].id, "g-1");
    assert_eq!(parsed.grills[0].summary, "s");

    let delta_resp = changes(&sql, "since=0");
    assert!(
        !delta_resp.body.contains("top secret transcript text"),
        "the delta must never carry the transcript either: {}",
        delta_resp.body
    );
    let parsed: ChangesResponse = body_as(&delta_resp);
    assert_eq!(parsed.grills.len(), 1);
}

#[test]
fn delta_and_sweep_ordering_for_grills_is_covered() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1"); // v1
    seed_item(&sql, "a-2"); // v2
    post_grill(
        &sql,
        r#"{"id": "g-1", "item_id": "a-1", "transcript": "t1", "summary": "first",
            "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#,
        0,
    ); // v3
    post_grill(
        &sql,
        r#"{"id": "g-2", "item_id": "a-2", "transcript": "t2", "summary": "second",
            "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#,
        0,
    ); // v4

    let parsed: ChangesResponse = body_as(&changes(&sql, "since=0"));
    assert_eq!(parsed.grills.len(), 2);
    assert_eq!(parsed.grills[0].id, "g-1", "ordered by version, then id");
    assert_eq!(parsed.grills[1].id, "g-2");

    // The delta cursor filters grills independently, same as every other
    // table.
    let parsed: ChangesResponse = body_as(&changes(&sql, "since=3"));
    assert_eq!(parsed.grills.len(), 1);
    assert_eq!(parsed.grills[0].id, "g-2");
}

#[test]
fn wrong_method_on_grills_routes_is_405() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    assert_eq!(req(&sql, "DELETE", "/api/grills", None, None, 0).status, 405);
    assert_eq!(req(&sql, "PATCH", "/api/grills/g-1", None, None, 0).status, 405);
}

/// The anti-goal: no new `auth::permitted` arm. Both routes fall through to
/// the existing default (`_ => matches!(scope, Scope::Device)`), so a
/// sweeper or ingest token — neither named in any explicit arm — is
/// rejected exactly like it would be for any other device-only route.
#[test]
fn grills_routes_are_gated_by_the_existing_default_arm_not_a_new_one() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    let body = r#"{"id": "g-1", "item_id": "a-1", "transcript": "t", "summary": "s",
        "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#;

    for token in [SWEEPER_TOKEN, INGEST_TOKEN] {
        let resp = req_as(&sql, token, "POST", "/api/grills", None, Some(body), 0);
        assert_eq!(resp.status, 403, "{token}: {}", resp.body);
        assert!(resp.body.is_empty());
    }
    assert_eq!(meta_version(&sql), 1, "nothing was written");

    for token in [SWEEPER_TOKEN, INGEST_TOKEN] {
        let resp = req_as(&sql, token, "GET", "/api/grills/g-1", None, None, 0);
        assert_eq!(resp.status, 403, "{token}: {}", resp.body);
        assert!(resp.body.is_empty());
    }
}
