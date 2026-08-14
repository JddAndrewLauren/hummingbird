//! `POST /api/grills` and `GET /api/grills/:id` (#353, ADR-0023): the
//! immutable per-item Grill attachment, and the two anti-goals — no
//! transcript on the sweep/delta wire, and no new `auth::permitted` arm.
//!
//! `#354` fixtures live in this same file: the atomic completion mutation
//! (record + item stage + optional plan soft-delete, one transaction), its
//! CAS conflict, its plan-deletion flag, and the anti-goal tests the issue
//! names directly — no partial write on a mid-transaction failure, and
//! Record (ticked steps) never touched either way.

use hummingbird_authority::{Row, SqlValue};
use hummingbird_domain::{ChangesResponse, Grill, Stage};

use crate::rig::*;

fn post_grill(sql: &dyn Sql, body: &str, now_ms: i64) -> hummingbird_authority::ApiResponse {
    post_to(sql, "/api/grills", body, now_ms)
}

fn get_grill(sql: &dyn Sql, id: &str) -> hummingbird_authority::ApiResponse {
    req(sql, "GET", &format!("/api/grills/{id}"), None, None, 0)
}

/// Seeds a Step through the handler and returns its version — mirrors
/// `rig::seed_item`.
fn seed_step(sql: &dyn Sql, id: &str, item_id: &str, position: i64) {
    let resp = post_to(
        sql,
        "/api/steps",
        &format!(r#"{{"id": "{id}", "item_id": "{item_id}", "body": "step {id}", "position": {position}}}"#),
        0,
    );
    assert!(resp.status == 201 || resp.status == 200, "step seed failed: {}", resp.body);
}

fn step_row(sql: &dyn Sql, id: &str) -> Row {
    sql.exec("SELECT * FROM steps WHERE id = ?", &[SqlValue::Text(id.to_string())])
        .unwrap()
        .into_iter()
        .next()
        .expect("step exists")
}

#[test]
fn create_grill_201_stamps_resulting_stage_and_replays_200() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1"); // version 1, born in triage
    let body = r#"{
        "id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "turn 1\nturn 2",
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
        r#"{"id": "g-1", "item_id": "a-1", "expected_version": 2, "transcript": "t", "summary": "s",
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
        r#"{"id": "g-1", "item_id": "a-1", "expected_version": 2, "transcript": "t", "summary": "s",
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
            r#"{"id": "", "item_id": "a-1", "expected_version": 1, "transcript": "t", "summary": "s",
                "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#,
            "empty id",
        ),
        (
            r#"{"id": "g", "item_id": "ghost", "expected_version": 1, "transcript": "t", "summary": "s",
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
        r#"{"id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "the whole back-and-forth",
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
        r#"{"id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "top secret transcript text",
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
        r#"{"id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "t1", "summary": "first",
            "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#,
        0,
    ); // v3
    post_grill(
        &sql,
        r#"{"id": "g-2", "item_id": "a-2", "expected_version": 2, "transcript": "t2", "summary": "second",
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
    let body = r#"{"id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "t", "summary": "s",
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

// -------------------------------------------------- #354: atomic completion

/// The heart of #354: one `POST /api/grills` moves the item's own `stage`
/// column, not just the grill row's stored `resulting_stage`.
#[test]
fn completing_a_grill_moves_the_items_own_stage_column() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1"); // v1, triage

    let resp = post_grill(
        &sql,
        r#"{"id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "t", "summary": "s",
            "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#,
        0,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);

    let item_resp = req(&sql, "GET", "/api/items/a-1", None, None, 0);
    // No direct GET /api/items/:id route exists; read through changes.
    let _ = item_resp;
    let parsed: ChangesResponse = body_as(&changes(&sql, "since=0"));
    let item = parsed.items.iter().find(|i| i.id == "a-1").expect("item present");
    assert_eq!(item.stage, Stage::Ready, "triage + resolved moves the item to ready");
}

/// A stale `expected_version` is a 409 naming the item's current row — the
/// same CAS contract every other patch DTO carries — and nothing is
/// written: no grill row, no item change.
#[test]
fn a_stale_expected_version_is_a_409_with_no_write() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1"); // v1
    patch(&sql, "a-1", r#"{"expected_version": 1, "priority": 2}"#, 0); // v2, now version 2

    let resp = post_grill(
        &sql,
        r#"{"id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "t", "summary": "s",
            "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#,
        0,
    );
    assert_eq!(resp.status, 409, "{}", resp.body);
    assert_eq!(meta_version(&sql), 2, "no write happened on a stale version");
    assert_eq!(get_grill(&sql, "g-1").status, 404, "no grill row was created");
}

/// `delete_unticked_plan: true` soft-deletes every currently unticked,
/// undeleted Step on the item — and only those.
#[test]
fn delete_unticked_plan_true_soft_deletes_the_unticked_steps() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    seed_step(&sql, "s-1", "a-1", 1); // unticked
    seed_step(&sql, "s-2", "a-1", 2); // unticked

    let resp = post_grill(
        &sql,
        r#"{"id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "t", "summary": "s",
            "verdict": "resolved", "model_proposal": "p", "applied_patch": "p",
            "delete_unticked_plan": true}"#,
        7000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);

    for id in ["s-1", "s-2"] {
        let row = step_row(&sql, id);
        assert_eq!(
            RowReaderTest(&row).opt_int("deleted_at"),
            Some(7000),
            "step {id} must be soft-deleted"
        );
    }
}

/// The anti-goal, asserted directly rather than inferred from the
/// soft-delete predicate: a **ticked** Step is never touched, in either
/// value of `delete_unticked_plan`.
#[test]
fn ticked_record_is_never_touched_in_either_branch_of_the_plan_flag() {
    for delete_unticked_plan in [false, true] {
        let sql = RusqliteSql::new();
        seed_item(&sql, "a-1");
        seed_step(&sql, "s-1", "a-1", 1);
        // Tick it through the real handler — the step's own version is
        // whatever the shared meta counter stood at when it was seeded,
        // not necessarily 1 (the item seed already claimed 1).
        let step_version = RowReaderTest(&step_row(&sql, "s-1")).opt_int("version").unwrap();
        let tick_resp = patch_at(
            &sql,
            "/api/steps/s-1",
            &format!(r#"{{"expected_version": {step_version}, "done": true}}"#),
            0,
        );
        assert_eq!(tick_resp.status, 200, "{}", tick_resp.body);
        let ticked_before = step_row(&sql, "s-1");

        let body = format!(
            r#"{{"id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "t", "summary": "s",
                "verdict": "resolved", "model_proposal": "p", "applied_patch": "p",
                "delete_unticked_plan": {delete_unticked_plan}}}"#
        );
        let resp = post_grill(&sql, &body, 0);
        assert_eq!(resp.status, 201, "{}", resp.body);

        let ticked_after = step_row(&sql, "s-1");
        assert_eq!(
            ticked_after, ticked_before,
            "delete_unticked_plan={delete_unticked_plan}: a ticked step must be byte-for-byte untouched"
        );
    }
}

/// `delete_unticked_plan` absent (the default, `false`) leaves every step
/// untouched, even though there is a live unticked Plan to strand — the
/// **Replace** glossary rule: absent the explicit gesture, nothing is
/// re-cut or deleted.
#[test]
fn delete_unticked_plan_defaults_to_false_and_touches_nothing() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    seed_step(&sql, "s-1", "a-1", 1);
    let before = step_row(&sql, "s-1");

    let resp = post_grill(
        &sql,
        r#"{"id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "t", "summary": "s",
            "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#,
        0,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert_eq!(step_row(&sql, "s-1"), before, "no delete_unticked_plan field at all: nothing touched");
}

/// Replaying an already-completed Grill is a no-op: the item's stage and
/// every step stay exactly as the first call left them, and no second
/// write happens — pinned directly rather than inferred from the 200.
#[test]
fn replaying_the_completion_touches_nothing_a_second_time() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    seed_step(&sql, "s-1", "a-1", 1);
    let body = r#"{"id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "t", "summary": "s",
        "verdict": "resolved", "model_proposal": "p", "applied_patch": "p", "delete_unticked_plan": true}"#;

    let first = post_grill(&sql, body, 0);
    assert_eq!(first.status, 201, "{}", first.body);
    let version_after_first = meta_version(&sql);
    let step_after_first = step_row(&sql, "s-1");

    let second = post_grill(&sql, body, 12345);
    assert_eq!(second.status, 200, "replay is success");
    assert_eq!(meta_version(&sql), version_after_first, "no second bump");
    assert_eq!(step_row(&sql, "s-1"), step_after_first, "no second touch on the step");
}

/// The atomicity anti-goal: a failure partway through the write burst
/// leaves nothing behind — no grill row, no item stage change, no step
/// soft-delete, no version bump.
#[test]
fn create_grill_no_partial_write_on_mid_transaction_failure() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1"); // v1, triage
    seed_step(&sql, "s-1", "a-1", 1);
    let item_before = sql
        .exec("SELECT * FROM items WHERE id = 'a-1'", &[])
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let step_before = step_row(&sql, "s-1");
    let version_before = meta_version(&sql);

    // Fail exactly the item stage UPDATE — after the grill INSERT already
    // ran, before the step soft-delete and the meta bump.
    let failing = FailingSql {
        inner: &sql,
        fail_marker: "UPDATE items",
    };
    let resp = post_grill(
        &failing,
        r#"{"id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "t", "summary": "s",
            "verdict": "resolved", "model_proposal": "p", "applied_patch": "p",
            "delete_unticked_plan": true}"#,
        0,
    );
    assert_eq!(resp.status, 500, "the injected fault surfaces as a 500, not a swallowed error");

    assert_eq!(get_grill(&sql, "g-1").status, 404, "no grill record was left behind");
    let item_after = sql
        .exec("SELECT * FROM items WHERE id = 'a-1'", &[])
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(item_after, item_before, "the item row must be exactly as it was");
    assert_eq!(step_row(&sql, "s-1"), step_before, "the step row must be exactly as it was");
    assert_eq!(meta_version(&sql), version_before, "no version bump from a rolled-back write");
}

/// Small local shim so this file doesn't need to import `RowReader` from
/// the crate's private `codec` module — it re-derives just the one
/// accessor these fixtures need, directly off the raw `Row`.
struct RowReaderTest<'a>(&'a Row);
impl RowReaderTest<'_> {
    fn opt_int(&self, col: &str) -> Option<i64> {
        self.0.get(col).and_then(SqlValue::as_i64)
    }
}
