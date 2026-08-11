//! `POST /api/rules` and `PATCH /api/rules/:id` (#131), plus the two
//! sibling tables that land in the same schema slice without an HTTP
//! surface of their own (`push_targets`, `deliveries` — both #139):
//! individual revocation and the delivery dedupe key are exercised
//! straight against the `Sql` seam, the same way #114 seeded `alerts` and
//! `context_snapshots` before they had write handlers.

use hummingbird_domain::{ChangesResponse, ConflictResponse, Rule};

use crate::rig::*;

// ------------------------------------------------------- create (POST)

#[test]
fn create_returns_201_with_stamped_rule() {
    let sql = RusqliteSql::new();
    let resp = post_rule(
        &sql,
        r#"{"id": "r-1", "name": "trash slide", "conditions": [], "severity": "high", "tier": "urgent"}"#,
        1000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created = rule(&resp);
    assert_eq!(created.id, "r-1");
    assert_eq!(created.event_kind, None, "no event_kind supplied = any kind");
    assert!(created.enabled, "enabled defaults to true");
    assert_eq!(created.updated_at, 1000);
    assert_eq!(created.version, 1);
    assert_eq!(meta_version(&sql), 1);
}

#[test]
fn create_replay_same_id_returns_200_current_rule_without_bump() {
    let sql = RusqliteSql::new();
    post_rule(
        &sql,
        r#"{"id": "r-1", "name": "n", "conditions": [], "severity": "high", "tier": "urgent"}"#,
        1000,
    );
    let resp = post_rule(
        &sql,
        r#"{"id": "r-1", "name": "n", "conditions": [], "severity": "high", "tier": "urgent"}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "replay is success, not conflict");
    assert_eq!(rule(&resp).updated_at, 1000, "the original row, untouched");
    assert_eq!(meta_version(&sql), 1, "no version bump on replay");
    let rows = sql.exec("SELECT id FROM rules", &[]).unwrap();
    assert_eq!(rows.len(), 1, "no duplicate row");
}

/// The ADR-0013 amendment this brief is easiest to get wrong by copying
/// ADR-0012 alone: `event_kind` is nullable with no `CHECK` — both `NULL`
/// and a kind the code does not yet know must be accepted. Still true
/// post-#186: `validate_rule`'s `UnknownKind` problem is deliberately
/// excluded from the save-time check (see `rules::save_time_problems`) —
/// `Rule::event_kind`'s own doc says a kind the code does not yet know is
/// legal, and ADR-0013 only requires *field*/*value* problems rejected at
/// save, leaving kind drift to be flagged at load/fire time instead.
#[test]
fn event_kind_accepts_null_and_an_unknown_kind() {
    let sql = RusqliteSql::new();
    let resp = post_rule(
        &sql,
        r#"{"id": "r-null", "name": "n", "conditions": [], "severity": "s", "tier": "normal"}"#,
        0,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert_eq!(rule(&resp).event_kind, None);

    let resp = post_rule(
        &sql,
        r#"{"id": "r-unknown", "name": "n", "event_kind": "weather_alert", "conditions": [],
            "severity": "s", "tier": "normal"}"#,
        0,
    );
    assert_eq!(resp.status, 201, "an unknown registry key is not rejected: {}", resp.body);
    assert_eq!(rule(&resp).event_kind.as_deref(), Some("weather_alert"));
}

/// A `conditions` payload round-trips through the handler with `negate`
/// preserved on every condition. `event_kind: "email"` is required here
/// (#186): `labels`/`subject` are `email`-declared fields, not core ones,
/// and `validate_rule` now rejects an unknown field at save.
#[test]
fn conditions_round_trip_with_negate_through_the_handler() {
    let sql = RusqliteSql::new();
    let resp = post_rule(
        &sql,
        r#"{"id": "r-1", "name": "n", "event_kind": "email",
            "conditions": [
              {"field": "labels", "op": "contains", "value": "alert-high", "negate": true},
              {"field": "subject", "op": "contains", "value": ["urgent", "asap"], "negate": false}
            ],
            "severity": "s", "tier": "normal"}"#,
        0,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created = rule(&resp);
    assert_eq!(created.conditions.len(), 2);
    assert!(created.conditions[0].negate);
    assert!(!created.conditions[1].negate);

    // Re-fetch through the delta pull too, not just the create response —
    // the round trip must survive a write-then-read via storage.
    let pulled: ChangesResponse = body_as(&changes(&sql, "since=0"));
    assert_eq!(pulled.rules.len(), 1);
    assert!(pulled.rules[0].conditions[0].negate);
    assert!(!pulled.rules[0].conditions[1].negate);
}

/// ADR-0013's two save-time duration rules (#186): a malformed duration
/// literal, and an `m`/`h` unit on a `date`-typed field — both rejected at
/// `POST /api/rules`, and neither reaches a write (`meta_version` stays
/// at 0). Each is exercised negated too, so the always-true path #133
/// fixed at fire time (a malformed condition evaluating to `false`, then
/// inverted by `negate: true`) can never be reached through the API.
#[test]
fn create_rejects_a_malformed_duration_literal() {
    let sql = RusqliteSql::new();
    for negate in [false, true] {
        let resp = post_rule(
            &sql,
            &format!(
                r#"{{"id": "r-1", "name": "n", "event_kind": "item_threshold",
                    "conditions": [
                      {{"field": "scheduled_date", "op": "within_next", "value": "1d2h", "negate": {negate}}}
                    ],
                    "severity": "s", "tier": "normal"}}"#
            ),
            0,
        );
        assert_eq!(resp.status, 400, "negate={negate}: {}", resp.body);
        assert!(resp.body.contains("scheduled_date"), "{}", resp.body);
    }
    assert_eq!(meta_version(&sql), 0, "no write happened");
    let rows = sql.exec("SELECT id FROM rules", &[]).unwrap();
    assert!(rows.is_empty(), "no row landed");
}

#[test]
fn create_rejects_an_hour_unit_on_a_date_typed_field() {
    let sql = RusqliteSql::new();
    for negate in [false, true] {
        let resp = post_rule(
            &sql,
            &format!(
                r#"{{"id": "r-1", "name": "n", "event_kind": "item_threshold",
                    "conditions": [
                      {{"field": "scheduled_date", "op": "within_next", "value": "2h", "negate": {negate}}}
                    ],
                    "severity": "s", "tier": "normal"}}"#
            ),
            0,
        );
        assert_eq!(resp.status, 400, "negate={negate}: {}", resp.body);
        assert!(resp.body.contains("scheduled_date"), "{}", resp.body);
    }
    assert_eq!(meta_version(&sql), 0, "no write happened");
    let rows = sql.exec("SELECT id FROM rules", &[]).unwrap();
    assert!(rows.is_empty(), "no row landed");
}

/// `RuleProblem::RetiredSource` (#189) is not `UnknownKind` — it is not
/// excluded from `save_time_problems`, so it is rejected at save exactly
/// like a malformed value, negated or not. `city-waste/v1` is the
/// registry's real retired entry (ADR-0014).
#[test]
fn create_rejects_a_condition_naming_a_retired_source() {
    let sql = RusqliteSql::new();
    for negate in [false, true] {
        let resp = post_rule(
            &sql,
            &format!(
                r#"{{"id": "r-1", "name": "n",
                    "conditions": [
                      {{"field": "source", "op": "eq", "value": "city-waste/v1", "negate": {negate}}}
                    ],
                    "severity": "s", "tier": "normal"}}"#
            ),
            0,
        );
        assert_eq!(resp.status, 400, "negate={negate}: {}", resp.body);
        assert!(resp.body.contains("city-waste/v1"), "{}", resp.body);
        assert!(resp.body.contains("city-waste/v2"), "names the successor: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 0, "no write happened");
    let rows = sql.exec("SELECT id FROM rules", &[]).unwrap();
    assert!(rows.is_empty(), "no row landed");
}

/// A **live** or **unregistered** source is not rejected — only a
/// resolved-and-retired one is a save-time problem.
#[test]
fn create_accepts_a_live_or_unregistered_source_condition() {
    let sql = RusqliteSql::new();
    let resp = post_rule(
        &sql,
        r#"{"id": "r-live", "name": "n",
            "conditions": [{"field": "source", "op": "eq", "value": "gmail/v1", "negate": false}],
            "severity": "s", "tier": "normal"}"#,
        0,
    );
    assert_eq!(resp.status, 201, "live source: {}", resp.body);

    let resp = post_rule(
        &sql,
        r#"{"id": "r-web", "name": "n",
            "conditions": [{"field": "source", "op": "eq", "value": "web/v1", "negate": false}],
            "severity": "s", "tier": "normal"}"#,
        0,
    );
    assert_eq!(resp.status, 201, "unregistered source: {}", resp.body);
}

#[test]
fn create_validation_rejects_bad_input() {
    let sql = RusqliteSql::new();
    for (body, why) in [
        (r#"{"id": "", "name": "n", "conditions": [], "severity": "s", "tier": "normal"}"#, "empty id"),
        (r#"{"id": "r", "name": "", "conditions": [], "severity": "s", "tier": "normal"}"#, "empty name"),
        (r#"{"id": "r", "name": "n", "conditions": [], "severity": "", "tier": "normal"}"#, "empty severity"),
        (r#"{"id": "r", "name": "n", "conditions": [], "severity": "s", "tier": "digest"}"#, "tier outside the two"),
        (r#"not json"#, "malformed JSON"),
    ] {
        let resp = post_rule(&sql, body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 0, "no write happened");
}

// -------------------------------------------------------- patch (PATCH)

#[test]
fn patch_fresh_version_applies_and_bumps() {
    let sql = RusqliteSql::new();
    seed_rule(&sql, "r-1");
    let resp = patch_rule(
        &sql,
        "r-1",
        r#"{"expected_version": 1, "name": "renamed", "enabled": false}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated = rule(&resp);
    assert_eq!(updated.name, "renamed");
    assert!(!updated.enabled, "enable/disable is a one-field toggle");
    assert_eq!(updated.version, 2);
    assert_eq!(updated.updated_at, 2000);
    assert_eq!(meta_version(&sql), 2);
}

/// The acceptance criterion: a stale `expected_version` returns 409
/// carrying the current rule, and a create replayed with the same client
/// id is idempotent (covered above) — both halves of the CAS contract.
#[test]
fn patch_stale_version_409_carries_current_rule() {
    let sql = RusqliteSql::new();
    seed_rule(&sql, "r-1");
    let resp = patch_rule(&sql, "r-1", r#"{"expected_version": 99, "name": "x"}"#, 2000);
    assert_eq!(resp.status, 409);
    let conflict: ConflictResponse<Rule> = body_as(&resp);
    assert_eq!(conflict.error, "version_conflict");
    assert_eq!(conflict.current.name, "seeded r-1", "the current entity, unmodified");
    assert_eq!(conflict.current.version, 1);
    assert_eq!(meta_version(&sql), 1, "a stale write bumps nothing");
}

#[test]
fn patch_unknown_id_404() {
    let sql = RusqliteSql::new();
    let resp = patch_rule(&sql, "ghost", r#"{"expected_version": 1}"#, 0);
    assert_eq!(resp.status, 404);
}

/// `event_kind` is the one nullable column: explicit `null` clears it to
/// "any kind", and an absent field leaves it untouched.
#[test]
fn patch_event_kind_null_clears_to_any_kind() {
    let sql = RusqliteSql::new();
    post_rule(
        &sql,
        r#"{"id": "r-1", "name": "n", "event_kind": "email", "conditions": [], "severity": "s", "tier": "normal"}"#,
        0,
    );
    let resp = patch_rule(&sql, "r-1", r#"{"expected_version": 1, "event_kind": null}"#, 1000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(rule(&resp).event_kind, None);
}

#[test]
fn patch_null_on_not_null_field_400() {
    let sql = RusqliteSql::new();
    seed_rule(&sql, "r-1");
    for (body, field) in [
        (r#"{"expected_version": 1, "name": null}"#, "name"),
        (r#"{"expected_version": 1, "severity": null}"#, "severity"),
        (r#"{"expected_version": 1, "tier": null}"#, "tier"),
        (r#"{"expected_version": 1, "conditions": null}"#, "conditions"),
        (r#"{"expected_version": 1, "enabled": null}"#, "enabled"),
    ] {
        let resp = patch_rule(&sql, "r-1", body, 2000);
        assert_eq!(resp.status, 400, "null `{field}`: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 1, "no write happened");
}

/// The patch counterpart of `create_rejects_a_malformed_duration_literal`
/// (#186): a patch must not be able to smuggle in a condition a create
/// would reject.
#[test]
fn patch_rejects_a_malformed_duration_literal() {
    let sql = RusqliteSql::new();
    post_rule(
        &sql,
        r#"{"id": "r-1", "name": "n", "event_kind": "item_threshold", "conditions": [],
            "severity": "s", "tier": "normal"}"#,
        0,
    );
    for negate in [false, true] {
        let resp = patch_rule(
            &sql,
            "r-1",
            &format!(
                r#"{{"expected_version": 1,
                    "conditions": [
                      {{"field": "scheduled_date", "op": "within_next", "value": "1d2h", "negate": {negate}}}
                    ]}}"#
            ),
            2000,
        );
        assert_eq!(resp.status, 400, "negate={negate}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 1, "no write happened beyond the create");
}

/// The patch counterpart of `create_rejects_an_hour_unit_on_a_date_typed_field`.
#[test]
fn patch_rejects_an_hour_unit_on_a_date_typed_field() {
    let sql = RusqliteSql::new();
    post_rule(
        &sql,
        r#"{"id": "r-1", "name": "n", "event_kind": "item_threshold", "conditions": [],
            "severity": "s", "tier": "normal"}"#,
        0,
    );
    for negate in [false, true] {
        let resp = patch_rule(
            &sql,
            "r-1",
            &format!(
                r#"{{"expected_version": 1,
                    "conditions": [
                      {{"field": "scheduled_date", "op": "within_next", "value": "2h", "negate": {negate}}}
                    ]}}"#
            ),
            2000,
        );
        assert_eq!(resp.status, 400, "negate={negate}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 1, "no write happened beyond the create");
}

/// A patch that leaves `conditions` untouched must still be validated
/// against its *post-patch* `event_kind` (#186): the seeded rule's
/// `scheduled_date` condition is legal under `item_threshold`, but
/// re-kinding to `email` (which declares no `scheduled_date`) must reject
/// even though the patch body never mentions `conditions` itself.
#[test]
fn patch_validates_untouched_conditions_against_a_new_event_kind() {
    let sql = RusqliteSql::new();
    post_rule(
        &sql,
        r#"{"id": "r-1", "name": "n", "event_kind": "item_threshold",
            "conditions": [
              {"field": "scheduled_date", "op": "within_next", "value": "3d", "negate": false}
            ],
            "severity": "s", "tier": "normal"}"#,
        0,
    );
    let resp = patch_rule(&sql, "r-1", r#"{"expected_version": 1, "event_kind": "email"}"#, 2000);
    assert_eq!(resp.status, 400, "{}", resp.body);
    assert!(resp.body.contains("scheduled_date"), "{}", resp.body);
    assert_eq!(meta_version(&sql), 1, "no write happened beyond the create");
}

/// #221: the compare-typed no-op #166 gave the other seven patch handlers,
/// applied to `rules`. A PATCH that sets every settable field to the value
/// already stored performs no write at all.
#[test]
fn patch_setting_every_field_to_its_current_value_is_a_noop() {
    let sql = RusqliteSql::new();
    post_rule(
        &sql,
        r#"{"id": "r-1", "name": "n", "event_kind": "email",
            "conditions": [{"field": "labels", "op": "contains", "value": "x", "negate": false}],
            "severity": "high", "tier": "urgent", "enabled": true}"#,
        0,
    ); // version 1
    let resp = patch_rule(
        &sql,
        "r-1",
        r#"{"expected_version": 1, "name": "n", "event_kind": "email",
            "conditions": [{"field": "labels", "op": "contains", "value": "x", "negate": false}],
            "severity": "high", "tier": "urgent", "enabled": true}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged = rule(&resp);
    assert_eq!(unchanged.version, 1, "no version bump for a value-identical patch");
    assert_eq!(unchanged.updated_at, 0, "no updated_at restamp either");
    assert_eq!(meta_version(&sql), 1);
}

/// A patch mixing a changed field with unchanged ones writes only the
/// changed column, and still bumps once.
#[test]
fn patch_mixing_changed_and_unchanged_fields_writes_only_the_changed_ones() {
    let sql = RusqliteSql::new();
    post_rule(
        &sql,
        r#"{"id": "r-1", "name": "n", "conditions": [], "severity": "high", "tier": "urgent", "enabled": true}"#,
        0,
    ); // version 1
    let resp = patch_rule(
        &sql,
        "r-1",
        r#"{"expected_version": 1, "name": "n", "severity": "high", "enabled": false}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated = rule(&resp);
    assert_eq!(updated.name, "n", "the unchanged field is left as-is");
    assert!(!updated.enabled, "the changed field is written");
    assert_eq!(updated.version, 2, "a mixed patch still bumps");
    assert_eq!(updated.updated_at, 2000);
    assert_eq!(meta_version(&sql), 2);
}

// The integer/real hazard (#166) — `enabled` is the one integer column
// this handler patches. See items.rs for the full explanation: a Durable
// Object cursor may surface an INTEGER column as a whole f64, so a naive
// raw-SqlValue comparison must never be used for the no-op check.
#[test]
fn patch_value_identical_enabled_noops_even_when_the_row_reads_back_as_real() {
    let sql = RusqliteSql::new();
    post_rule(
        &sql,
        r#"{"id": "r-1", "name": "n", "conditions": [], "severity": "high", "tier": "urgent", "enabled": false}"#,
        0,
    ); // version 1, enabled: false
    let wrapped = RealCoercingSql::new(&sql);
    let resp = patch_rule(&wrapped, "r-1", r#"{"expected_version": 1, "enabled": false}"#, 2000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let unchanged = rule(&resp);
    assert_eq!(unchanged.version, 1, "enabled: Integer(0) vs Real(0.0) must still compare equal");
    assert_eq!(meta_version(&sql), 1);
}

#[test]
fn patch_with_only_expected_version_is_a_noop() {
    let sql = RusqliteSql::new();
    seed_rule(&sql, "r-1");
    let resp = patch_rule(&sql, "r-1", r#"{"expected_version": 1}"#, 2000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(rule(&resp).version, 1, "no version bump");
    assert_eq!(meta_version(&sql), 1);
}

// -------------------------------------------------- delta pull / sweep

#[test]
fn rules_appear_in_the_delta_pull_and_sweep_stays_byte_identical() {
    let sql = RusqliteSql::new();
    seed_rule(&sql, "r-1");
    seed_rule(&sql, "r-2");

    let full = changes(&sql, "since=0");
    let swept = sweep(&sql);
    assert_eq!(full.body, swept.body, "sweep is a delta pull that has seen everything");

    let parsed: ChangesResponse = body_as(&full);
    assert_eq!(parsed.rules.len(), 2, "both rules are present");
    let ids: Vec<&str> = parsed.rules.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"r-1") && ids.contains(&"r-2"));
}

#[test]
fn delta_pull_only_returns_rules_above_the_cursor() {
    let sql = RusqliteSql::new();
    let first = seed_rule(&sql, "r-1").version;
    seed_rule(&sql, "r-2");

    let parsed: ChangesResponse = body_as(&changes(&sql, &format!("since={first}")));
    assert_eq!(parsed.rules.len(), 1, "only the rule created after the cursor");
    assert_eq!(parsed.rules[0].id, "r-2");
}

// ---------------------------------------------------- list (GET, #135-137)

/// The evaluated-stream pollers' own read: every rule, including a disabled
/// one — the poller (via `hummingbird_rules_engine::evaluate_rules`) is the
/// one deciding what fires, and a filtered list here would be a second,
/// driftable copy of `evaluate_rule`'s own `enabled` check.
#[test]
fn list_returns_every_rule_enabled_or_not() {
    let sql = RusqliteSql::new();
    seed_rule(&sql, "r-1");
    post_rule(
        &sql,
        r#"{"id": "r-2", "name": "n", "conditions": [], "severity": "s", "tier": "normal", "enabled": false}"#,
        0,
    );
    let resp = get_rules_as(&sql, INGEST_TOKEN);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let rules: Vec<Rule> = body_as(&resp);
    assert_eq!(rules.len(), 2);
    assert!(rules.iter().any(|r| r.id == "r-1" && r.enabled));
    assert!(rules.iter().any(|r| r.id == "r-2" && !r.enabled));
}

#[test]
fn list_is_readable_by_device_and_ingest_but_not_sweeper() {
    let sql = RusqliteSql::new();
    seed_rule(&sql, "r-1");

    assert_eq!(get_rules_as(&sql, DEVICE_TOKEN).status, 200);
    assert_eq!(get_rules_as(&sql, INGEST_TOKEN).status, 200);

    let sweeper = get_rules_as(&sql, SWEEPER_TOKEN);
    assert_eq!(sweeper.status, 403, "{}", sweeper.body);
    assert!(sweeper.body.is_empty(), "403 bodies are empty");

    let anon = req_anon(&sql, "GET", "/api/rules", None, None);
    assert_eq!(anon.status, 401);
    assert!(anon.body.is_empty(), "401 bodies are empty");
}

// ------------------------------------------------- push_targets / deliveries

/// `push_targets` gets no HTTP surface in #131 (that's #139); the
/// acceptance criterion — an individual revoke leaves siblings intact — is
/// exercised straight against the seam.
#[test]
fn push_target_can_be_revoked_individually() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_push_target_raw(&sql, "pt-2", "pixel-watch");

    sql.exec(
        "UPDATE push_targets SET revoked_at = 5000 WHERE id = 'pt-1'",
        &[],
    )
    .unwrap();

    let rows = sql
        .exec("SELECT id, revoked_at FROM push_targets ORDER BY id", &[])
        .unwrap();
    assert_eq!(rows.len(), 2, "revoking never deletes the row");
    assert!(rows[0].get("revoked_at").unwrap().as_i64().is_some(), "pt-1 revoked");
    assert!(
        rows[1].get("revoked_at").unwrap().as_i64().is_none(),
        "pt-2 untouched by pt-1's revoke"
    );
}

/// `deliveries` enforces its `(alert, rule, generation, severity)` dedupe
/// key (ADR-0014) — #139's transitions-only dedupe reads this constraint;
/// here it is pinned as a schema-level guarantee.
#[test]
fn deliveries_enforces_the_dedupe_key() {
    let sql = RusqliteSql::new();
    seed_alert_raw(&sql, "al-1", "healthchecks/v1", "sweeper");
    let rule = seed_rule(&sql, "r-1");

    seed_delivery_raw(&sql, "d-1", "al-1", &rule.id, 100, "high").expect("first delivery inserts");
    let dup = seed_delivery_raw(&sql, "d-2", "al-1", &rule.id, 100, "high");
    assert!(
        dup.is_err(),
        "an identical (alert, rule, generation, severity) must be rejected"
    );

    // A different generation (a later re-raise) or a different severity
    // (an escalation) is a distinct, permitted delivery.
    seed_delivery_raw(&sql, "d-3", "al-1", &rule.id, 200, "high")
        .expect("a later generation is a new delivery");
    seed_delivery_raw(&sql, "d-4", "al-1", &rule.id, 100, "urgent")
        .expect("an escalated severity is a new delivery");

    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 3, "the duplicate never landed");
}

/// ADR-0014's rejected alternative, pinned directly: a delivery dedupe key
/// without `rule_id` "collapses two rules that happen to agree on
/// severity." Two different rules matching the same event on the same
/// generation and severity must both persist as distinct delivery rows —
/// the "N matching rules, N deliveries" contract holds regardless of
/// whether the rules' severities happen to coincide.
#[test]
fn deliveries_key_does_not_collapse_two_rules_agreeing_on_severity() {
    let sql = RusqliteSql::new();
    seed_alert_raw(&sql, "al-1", "healthchecks/v1", "sweeper");
    let rule_a = seed_rule(&sql, "r-a");
    let rule_b = seed_rule(&sql, "r-b");

    seed_delivery_raw(&sql, "d-1", "al-1", &rule_a.id, 100, "high")
        .expect("rule a's delivery inserts");
    seed_delivery_raw(&sql, "d-2", "al-1", &rule_b.id, 100, "high")
        .expect("rule b's delivery, same generation and severity, is a distinct row");

    let rows = sql.exec("SELECT id, rule_id FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 2, "both rules' rings persist");
}
