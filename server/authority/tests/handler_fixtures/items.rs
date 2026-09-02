//! `POST /api/items` and `PATCH /api/items/:id` — the S0 suite, plus the
//! project-referent validation #114 added.

use hummingbird_domain::{ChangesResponse, ConflictResponse};

use crate::rig::*;

// ------------------------------------------------------- create (POST)

#[test]
fn create_returns_201_with_stamped_item() {
    let sql = RusqliteSql::new();
    let resp = post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created = item(&resp);
    assert_eq!(created.id, "a-1");
    assert_eq!(created.seq, Some(1));
    assert_eq!(created.version, 1);
    assert_eq!(created.created_at, 1000);
    assert_eq!(created.updated_at, 1000);
    assert_eq!(created.stage.as_str(), "triage");
    assert_eq!(created.priority, 0);
    assert_eq!(meta_version(&sql), 1);
}

/// #10's delegation axis, both directions. `agent` defaults to false — the
/// human does it — and the loop #10 insists on (set it, claim it, clear it
/// on finish so the next survey does not re-offer the same hand-off) is
/// three ordinary CAS patches with no bespoke route anywhere.
#[test]
fn the_delegation_axis_defaults_to_the_human_and_round_trips_through_create_and_patch() {
    let sql = RusqliteSql::new();
    let plain = item(&post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000));
    assert!(!plain.agent, "unmarked means the human does it");

    let marked = item(&post(
        &sql,
        r#"{"id": "a-2", "title": "compare three insurance quotes", "agent": true}"#,
        1000,
    ));
    assert!(marked.agent, "a create may mint the axis directly");

    let cleared = item(&patch(&sql, "a-2", r#"{"expected_version": 2, "agent": false}"#, 2000));
    assert!(!cleared.agent, "false is how the finish step closes the loop");
    assert_eq!(cleared.version, 3, "clearing it is a real write");

    let noop = patch(&sql, "a-2", r#"{"expected_version": 3, "agent": false}"#, 3000);
    assert_eq!(item(&noop).version, 3, "re-clearing an already-clear axis is a no-op");
}

#[test]
fn create_replay_same_id_returns_200_current_item_without_bump() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let resp = post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 2000);
    assert_eq!(resp.status, 200, "replay is success, not conflict");
    let replayed = item(&resp);
    assert_eq!(replayed.title, "hello");
    assert_eq!(replayed.created_at, 1000, "the original row, untouched");
    assert_eq!(meta_version(&sql), 1, "no version bump on replay");
    let rows = sql.exec("SELECT id FROM items", &[]).unwrap();
    assert_eq!(rows.len(), 1, "no duplicate row");
}

#[test]
fn create_replay_with_divergent_payload_returns_the_original_row() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let resp = post(&sql, r#"{"id": "a-1", "title": "something else"}"#, 2000);
    assert_eq!(resp.status, 200, "already-exists = success (ADR-0008)");
    let replayed = item(&resp);
    assert_eq!(replayed.title, "hello", "the stored row, not the divergent payload");
    assert_eq!(replayed.version, 1);
    assert_eq!(meta_version(&sql), 1, "no version bump");
}

#[test]
fn create_replay_with_invalid_divergent_payload_still_returns_the_stored_row() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    // The replay-select runs before field validation: already-exists is
    // success (ADR-0008), even when the divergent payload would 400 fresh.
    let resp = post(&sql, r#"{"id": "a-1", "title": "hello", "priority": 9}"#, 2000);
    assert_eq!(resp.status, 200, "already-exists wins over validation: {}", resp.body);
    let replayed = item(&resp);
    assert_eq!(replayed.title, "hello");
    assert_eq!(replayed.priority, 0, "the stored row, not the divergent payload");
    assert_eq!(meta_version(&sql), 1, "no version bump");
}

#[test]
fn create_with_server_stamped_fields_400() {
    let sql = RusqliteSql::new();
    for (body, field) in [
        (r#"{"id": "a", "title": "t", "version": 9}"#, "version"),
        (r#"{"id": "a", "title": "t", "seq": 5}"#, "seq"),
    ] {
        let resp = post(&sql, body, 0);
        assert_eq!(resp.status, 400, "server-stamped `{field}`: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 0, "no write happened");
}

#[test]
fn seq_mints_monotonically() {
    let sql = RusqliteSql::new();
    for (i, id) in ["a", "b", "c"].iter().enumerate() {
        let resp = post(&sql, &format!(r#"{{"id": "{id}", "title": "t"}}"#), 0);
        assert_eq!(item(&resp).seq, Some(i as i64 + 1));
    }
}

#[test]
fn create_accepts_the_full_field_set() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    let resp = post(
        &sql,
        r#"{"id": "a-1", "title": "hello", "description": "d", "stage": "ready",
            "size": "quick", "energy": "high", "context": "@computer", "priority": 3,
            "project_id": "p-1", "project_pos": 2, "deadline": "2026-08-15",
            "scheduled_date": "2026-08-10", "source": "google-tasks/v1",
            "source_key": "gt-9", "source_url": "https://example.test/t/9"}"#,
        500,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created = item(&resp);
    assert_eq!(created.stage.as_str(), "ready");
    assert_eq!(created.size.map(|s| s.as_str()), Some("quick"));
    assert_eq!(created.energy.map(|e| e.as_str()), Some("high"));
    assert_eq!(created.priority, 3);
    assert_eq!(created.source.as_deref(), Some("google-tasks/v1"));
}

#[test]
fn create_validation_rejects_bad_input() {
    let sql = RusqliteSql::new();
    for (body, why) in [
        (r#"{"id": "", "title": "t"}"#, "empty id"),
        (r#"{"id": "a", "title": ""}"#, "empty title"),
        (r#"{"id": "a", "title": "t", "priority": 5}"#, "priority out of range"),
        (r#"{"id": "a", "title": "t", "stage": "backlog"}"#, "stage outside the six"),
        (r#"{"id": "a", "title": "t", "project_id": "ghost"}"#, "unknown project"),
        (r#"{"id": "a", "title": "t", "deadline": "2026-08-15T09:30:00"}"#, "deadline has seconds"),
        (r#"{"id": "a", "title": "t", "deadline": "2026-08-15T09:30Z"}"#, "deadline has a Z suffix"),
        (r#"{"id": "a", "title": "t", "deadline": "not-a-date"}"#, "deadline is malformed"),
        (r#"not json"#, "malformed JSON"),
    ] {
        let resp = post(&sql, body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 0, "no write happened");
}

#[test]
fn create_accepts_a_valid_deadline_in_either_form() {
    let sql = RusqliteSql::new();
    let resp = post(&sql, r#"{"id": "a", "title": "t", "deadline": "2026-08-15"}"#, 0);
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert_eq!(item(&resp).deadline.as_deref(), Some("2026-08-15"));

    let resp = post(&sql, r#"{"id": "b", "title": "t", "deadline": "2026-08-15T09:30"}"#, 0);
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert_eq!(item(&resp).deadline.as_deref(), Some("2026-08-15T09:30"));
}

// -------------------------------------------------------- patch (PATCH)

#[test]
fn patch_fresh_version_applies_and_bumps() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let resp = patch(
        &sql,
        "a-1",
        r#"{"expected_version": 1, "title": "renamed", "stage": "in_progress"}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated = item(&resp);
    assert_eq!(updated.title, "renamed");
    assert_eq!(updated.stage.as_str(), "in_progress");
    assert_eq!(updated.version, 2);
    assert_eq!(updated.updated_at, 2000);
    assert_eq!(updated.created_at, 1000, "created_at never restamps");
    assert_eq!(meta_version(&sql), 2);
}

#[test]
fn patch_stale_version_409_carries_current_entity() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let resp = patch(&sql, "a-1", r#"{"expected_version": 99, "title": "x"}"#, 2000);
    assert_eq!(resp.status, 409);
    let conflict: ConflictResponse = body_as(&resp);
    assert_eq!(conflict.error, "version_conflict");
    assert_eq!(conflict.current.title, "hello", "the current entity, unmodified");
    assert_eq!(conflict.current.version, 1);
    assert_eq!(meta_version(&sql), 1, "a stale write bumps nothing");
}

#[test]
fn patch_unknown_id_404() {
    let sql = RusqliteSql::new();
    let resp = patch(&sql, "ghost", r#"{"expected_version": 1}"#, 0);
    assert_eq!(resp.status, 404);
}

#[test]
fn patch_explicit_null_clears_and_absent_leaves() {
    let sql = RusqliteSql::new();
    post(
        &sql,
        r#"{"id": "a-1", "title": "hello", "description": "keep?", "context": "@computer"}"#,
        1000,
    );
    let resp = patch(
        &sql,
        "a-1",
        r#"{"expected_version": 1, "description": null}"#,
        2000,
    );
    let updated = item(&resp);
    assert_eq!(updated.description, None, "explicit null clears");
    assert_eq!(
        updated.context.as_deref(),
        Some("@computer"),
        "absent field is untouched"
    );
}

#[test]
fn patch_with_only_expected_version_is_a_noop() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let resp = patch(&sql, "a-1", r#"{"expected_version": 1}"#, 2000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged = item(&resp);
    assert_eq!(unchanged.version, 1, "no version bump");
    assert_eq!(unchanged.updated_at, 1000, "no updated_at restamp");
    assert_eq!(meta_version(&sql), 1);
}

#[test]
fn patch_setting_every_field_to_its_current_value_is_a_noop() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    post(
        &sql,
        r#"{"id": "a-1", "title": "hello", "priority": 2, "project_id": "p-1",
            "project_pos": 3}"#,
        1000,
    ); // version 2 (p-1 already claimed version 1)
    let resp = patch(
        &sql,
        "a-1",
        r#"{"expected_version": 2, "title": "hello", "priority": 2,
            "project_id": "p-1", "project_pos": 3, "archived_at": null}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged = item(&resp);
    assert_eq!(unchanged.version, 2, "no version bump for a value-identical patch");
    assert_eq!(unchanged.updated_at, 1000, "no updated_at restamp");
    assert_eq!(meta_version(&sql), 2, "no global bump either");
}

#[test]
fn patch_mixing_changed_and_unchanged_fields_writes_only_the_changed_ones() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello", "priority": 2}"#, 1000); // version 1
    let resp = patch(
        &sql,
        "a-1",
        r#"{"expected_version": 1, "title": "hello", "priority": 3}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated = item(&resp);
    assert_eq!(updated.title, "hello", "the unchanged field is left as-is");
    assert_eq!(updated.priority, 3, "the changed field is written");
    assert_eq!(updated.version, 2, "a mixed patch still bumps");
    assert_eq!(updated.updated_at, 2000);
    assert_eq!(meta_version(&sql), 2);
}

// The integer/real representation hazard (#166): a Durable Object cursor
// may surface an INTEGER column as a whole f64, so a comparison that goes
// through the raw `SqlValue` (`Integer(2) != Real(2.0)`) would silently
// never fire the no-op short-circuit in production while passing under
// rusqlite in CI. `RealCoercingSql` stands in for that cursor so these
// fixtures actually exercise the hazard instead of relying on rusqlite
// happening to agree with production.
#[test]
fn patch_value_identical_priority_noops_even_when_the_row_reads_back_as_real() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello", "priority": 2}"#, 1000); // version 1
    let wrapped = RealCoercingSql::new(&sql);
    let resp = patch(&wrapped, "a-1", r#"{"expected_version": 1, "priority": 2}"#, 2000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(item(&resp).version, 1, "priority: Integer(2) vs Real(2.0) must still compare equal");
    assert_eq!(meta_version(&sql), 1);
}

#[test]
fn patch_value_identical_project_pos_noops_even_when_the_row_reads_back_as_real() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1");
    post(
        &sql,
        r#"{"id": "a-1", "title": "hello", "project_id": "p-1", "project_pos": 4}"#,
        1000,
    ); // version 2
    let wrapped = RealCoercingSql::new(&sql);
    let resp = patch(&wrapped, "a-1", r#"{"expected_version": 2, "project_pos": 4}"#, 2000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(item(&resp).version, 2, "project_pos: Integer(4) vs Real(4.0) must still compare equal");
    assert_eq!(meta_version(&sql), 2);
}

#[test]
fn patch_value_identical_archived_at_noops_even_when_the_row_reads_back_as_real() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000); // version 1
    patch(&sql, "a-1", r#"{"expected_version": 1, "archived_at": 5000}"#, 2000); // version 2
    let wrapped = RealCoercingSql::new(&sql);
    let resp = patch(&wrapped, "a-1", r#"{"expected_version": 2, "archived_at": 5000}"#, 3000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(item(&resp).version, 2, "archived_at: Integer(5000) vs Real(5000.0) must still compare equal");
    assert_eq!(meta_version(&sql), 2);
}

#[test]
fn patch_null_on_not_null_field_400() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    for (body, field) in [
        (r#"{"expected_version": 1, "title": null}"#, "title"),
        (r#"{"expected_version": 1, "stage": null}"#, "stage"),
        (r#"{"expected_version": 1, "priority": null}"#, "priority"),
        (r#"{"expected_version": 1, "agent": null}"#, "agent"),
    ] {
        let resp = patch(&sql, "a-1", body, 2000);
        assert_eq!(resp.status, 400, "null `{field}`: {}", resp.body);
        assert!(
            resp.body.contains("may not be null"),
            "the message names the offence: {}",
            resp.body
        );
    }
    assert_eq!(meta_version(&sql), 1, "no write happened");
}

#[test]
fn patch_unknown_field_400() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let resp = patch(&sql, "a-1", r#"{"expected_version": 1, "titel": "x"}"#, 2000);
    assert_eq!(resp.status, 400, "a typo'd field must not silently no-op: {}", resp.body);
    assert_eq!(meta_version(&sql), 1, "no write happened");
}

#[test]
fn patch_clears_enum_and_integer_fields_via_null() {
    let sql = RusqliteSql::new();
    post(
        &sql,
        r#"{"id": "a-1", "title": "hello", "size": "quick", "energy": "high"}"#,
        1000,
    );
    patch(&sql, "a-1", r#"{"expected_version": 1, "archived_at": 5000}"#, 2000);
    let parsed: ChangesResponse = body_as(&changes(&sql, "since=0"));
    assert_eq!(parsed.items.len(), 1, "archived rows are flagged, never deleted");
    assert_eq!(parsed.items[0].archived_at, Some(5000));

    let resp = patch(
        &sql,
        "a-1",
        r#"{"expected_version": 2, "size": null, "energy": null, "archived_at": null}"#,
        3000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let cleared = item(&resp);
    assert_eq!(cleared.size, None);
    assert_eq!(cleared.energy, None);
    assert_eq!(cleared.archived_at, None);
}

#[test]
fn patch_validation_rejects_bad_input() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    for (body, why) in [
        (r#"{"expected_version": 1, "title": ""}"#, "empty title"),
        (r#"{"expected_version": 1, "priority": 9}"#, "priority out of range"),
        (r#"{"expected_version": 1, "project_id": "ghost"}"#, "unknown project"),
        (r#"{"expected_version": 1, "deadline": "2026-08-15T09:30:00"}"#, "deadline has seconds"),
        (r#"{"expected_version": 1, "deadline": "09:30"}"#, "deadline is a bare time"),
        (r#"{"title": "no version"}"#, "missing expected_version"),
        (r#"{"#, "malformed JSON"),
    ] {
        let resp = patch(&sql, "a-1", body, 2000);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 1, "no write happened");
}

#[test]
fn patch_accepts_a_valid_deadline_and_can_clear_it() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);

    let resp = patch(
        &sql,
        "a-1",
        r#"{"expected_version": 1, "deadline": "2026-08-15T09:30"}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(item(&resp).deadline.as_deref(), Some("2026-08-15T09:30"));

    let resp = patch(&sql, "a-1", r#"{"expected_version": 2, "deadline": null}"#, 3000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(item(&resp).deadline, None, "explicit null clears");
}

/// #771: `vault_path` is the one nullable `items` column outside the
/// provenance trio a patch may touch — set at create, re-set, and cleared
/// with an explicit `null`, unlike `source_url` which `ItemPatch` does not
/// carry at all.
#[test]
fn vault_path_is_created_repointed_and_cleared() {
    let sql = RusqliteSql::new();
    let resp = post(
        &sql,
        r#"{"id": "a-1", "title": "hello", "vault_path": "Hummingbird/Knee rehab.md"}"#,
        1000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert_eq!(item(&resp).vault_path.as_deref(), Some("Hummingbird/Knee rehab.md"));

    let resp = patch(
        &sql,
        "a-1",
        r#"{"expected_version": 1, "vault_path": "Reading/Knee rehab"}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(
        item(&resp).vault_path.as_deref(),
        Some("Reading/Knee rehab"),
        "no `.md` is required — the vault also holds .canvas and .base files",
    );

    let resp = patch(&sql, "a-1", r#"{"expected_version": 2, "vault_path": null}"#, 3000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(item(&resp).vault_path, None, "explicit null clears the pointer");
}

/// Non-empty after trim, and nothing more — the same one rule
/// `project_links`' `url` carries. Clearing is `null`, never `""`.
#[test]
fn a_blank_vault_path_is_rejected_on_both_doors() {
    let sql = RusqliteSql::new();
    for body in [
        r#"{"id": "a", "title": "t", "vault_path": ""}"#,
        r#"{"id": "a", "title": "t", "vault_path": "   "}"#,
    ] {
        assert_eq!(post(&sql, body, 0).status, 400, "{body}");
    }
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    for body in [
        r#"{"expected_version": 1, "vault_path": ""}"#,
        r#"{"expected_version": 1, "vault_path": "  "}"#,
    ] {
        assert_eq!(patch(&sql, "a-1", body, 2000).status, 400, "{body}");
    }
}

#[test]
fn patch_can_attach_and_detach_a_real_project() {
    let sql = RusqliteSql::new();
    seed_project(&sql, "p-1"); // version 1
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 0); // version 2
    let resp = patch(&sql, "a-1", r#"{"expected_version": 2, "project_id": "p-1"}"#, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(item(&resp).project_id.as_deref(), Some("p-1"));

    let resp = patch(&sql, "a-1", r#"{"expected_version": 3, "project_id": null}"#, 0);
    assert_eq!(resp.status, 200, "detach via explicit null: {}", resp.body);
    assert_eq!(item(&resp).project_id, None);
}
