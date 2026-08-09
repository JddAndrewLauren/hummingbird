//! `GET /api/changes?since=N` and `GET /api/sweep`: the S0 delta suite,
//! the all-tables pull, and the byte-for-byte agreement criterion.

use hummingbird_domain::ChangesResponse;

use crate::rig::*;

/// Seed every synced table: six through their handlers, alerts and
/// context_snapshots (no #114 write handler) through the seam. Returns the
/// final workspace version.
fn seed_all_nine_tables(sql: &dyn Sql) -> i64 {
    seed_project(sql, "p-1"); // v1: project + route
    patch_at(
        sql,
        "/api/routes/p-1",
        r#"{"expected_version": 1, "destination": "done"}"#,
        0,
    ); // v2
    post_to(
        sql,
        "/api/fog",
        r#"{"id": "f-1", "project_id": "p-1", "question": "q?", "position": 1}"#,
        0,
    ); // v3
    post(sql, r#"{"id": "a-1", "title": "item one", "project_id": "p-1"}"#, 0); // v4
    seed_item(sql, "a-2"); // v5
    post_to(
        sql,
        "/api/steps",
        r#"{"id": "s-1", "item_id": "a-1", "body": "step one", "position": 1}"#,
        0,
    ); // v6
    post_to(sql, "/api/blocked_by", r#"{"item_id": "a-1", "blocker_id": "a-2"}"#, 0); // v7
    put_setting(sql, "k", r#"{"expected_version": 0, "value": true}"#, 0); // v8
    seed_alert_raw(sql, "al-1", "healthchecks", "sweeper"); // v9
    seed_snapshot_raw(sql, "f1", "schedule") // v10
}

#[test]
fn delta_pull_carries_every_synced_table() {
    let sql = RusqliteSql::new();
    let version = seed_all_nine_tables(&sql);
    assert_eq!(version, 10);

    let parsed: ChangesResponse = body_as(&changes(&sql, "since=0"));
    assert_eq!(parsed.version, 10);
    assert_eq!(parsed.projects.len(), 1);
    assert_eq!(parsed.routes.len(), 1);
    assert_eq!(parsed.fog.len(), 1);
    assert_eq!(parsed.items.len(), 2);
    assert_eq!(parsed.steps.len(), 1);
    assert_eq!(parsed.blocked_by.len(), 1);
    assert_eq!(parsed.alerts.len(), 1);
    assert_eq!(parsed.context_snapshots.len(), 1);
    assert_eq!(parsed.settings.len(), 1);
    assert_eq!(
        parsed.routes[0].destination.as_deref(),
        Some("done"),
        "the route carries its patched content"
    );
}

#[test]
fn delta_cursor_filters_every_table_independently() {
    let sql = RusqliteSql::new();
    seed_all_nine_tables(&sql); // versions 1..=10

    // Above v8: only the alert (v9) and the snapshot (v10).
    let parsed: ChangesResponse = body_as(&changes(&sql, "since=8"));
    assert_eq!(parsed.alerts.len(), 1);
    assert_eq!(parsed.context_snapshots.len(), 1);
    assert!(parsed.projects.is_empty());
    assert!(parsed.items.is_empty());
    assert!(parsed.settings.is_empty());

    // A fresh write moves one row above everything else.
    patch(&sql, "a-2", r#"{"expected_version": 5, "title": "renamed"}"#, 0); // v11
    let parsed: ChangesResponse = body_as(&changes(&sql, "since=10"));
    assert_eq!(parsed.version, 11);
    assert_eq!(parsed.items.len(), 1, "only the re-versioned item");
    assert_eq!(parsed.items[0].id, "a-2");
    assert!(parsed.alerts.is_empty(), "the alert stayed below the cursor");
}

/// #114 acceptance criterion 4, pinned as raw body equality on a workspace
/// seeded across all nine tables. (The sweep *is* the delta with since=0 in
/// code; this test keeps that true.)
#[test]
fn sweep_equals_delta_from_zero_byte_for_byte() {
    let sql = RusqliteSql::new();
    seed_all_nine_tables(&sql);

    let sweep_resp = sweep(&sql);
    let delta_resp = changes(&sql, "since=0");
    assert_eq!(sweep_resp.status, 200);
    assert_eq!(
        sweep_resp.body, delta_resp.body,
        "full sweep and delta-from-zero agree byte-for-byte"
    );
}

#[test]
fn tokens_never_appear_in_changes_or_sweep() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    sql.exec(
        "INSERT INTO tokens (id, name, scope, token_hash, created_at) \
         VALUES ('t-1', 'pixel-9', 'device', 'deadbeef', 0)",
        &[],
    )
    .unwrap();

    for resp in [changes(&sql, "since=0"), sweep(&sql)] {
        assert_eq!(resp.status, 200);
        let value: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        assert!(
            value.get("tokens").is_none(),
            "no tokens key in the response: {}",
            resp.body
        );
        assert!(
            !resp.body.contains("deadbeef") && !resp.body.contains("pixel-9"),
            "no token material leaks: {}",
            resp.body
        );
    }
}

#[test]
fn changes_since_current_reads_only_tokens_and_meta() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let recording = RecordingSql::new(&sql);
    let resp = changes(&recording, "since=1");
    assert_eq!(resp.status, 200);
    let parsed: ChangesResponse = body_as(&resp);
    assert_eq!(parsed.version, 1);
    assert!(parsed.items.is_empty());
    let statements = recording.statements.borrow();
    assert_eq!(
        statements.len(),
        3,
        "an unchanged workspace costs auth (token select + last_seen stamp) \
         plus one meta read: {statements:?}"
    );
    assert!(statements[0].contains("FROM tokens"), "{}", statements[0]);
    assert!(statements[1].contains("UPDATE tokens"), "{}", statements[1]);
    assert!(
        statements[2].contains("FROM meta") && !statements[2].contains("items"),
        "the gate reads meta, never an entity table: {}",
        statements[2]
    );
}

#[test]
fn changes_since_older_returns_only_rows_above_cursor() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a", "title": "first"}"#, 0); // version 1
    post(&sql, r#"{"id": "b", "title": "second"}"#, 0); // version 2
    patch(&sql, "a", r#"{"expected_version": 1, "title": "first-renamed"}"#, 0); // version 3
    let resp = changes(&sql, "since=2");
    let parsed: ChangesResponse = body_as(&resp);
    assert_eq!(parsed.version, 3);
    assert_eq!(parsed.items.len(), 1, "only the re-versioned row");
    assert_eq!(parsed.items[0].id, "a");
    assert_eq!(parsed.items[0].title, "first-renamed");
}

#[test]
fn changes_since_zero_is_the_full_sweep() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a", "title": "first"}"#, 0);
    post(&sql, r#"{"id": "b", "title": "second"}"#, 0);
    let parsed: ChangesResponse = body_as(&changes(&sql, "since=0"));
    assert_eq!(parsed.items.len(), 2);
    assert!(
        parsed.items[0].version < parsed.items[1].version,
        "ordered by version"
    );
}

#[test]
fn changes_since_above_current_version_returns_empty_with_server_version() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a", "title": "first"}"#, 0); // version 1
    let parsed: ChangesResponse = body_as(&changes(&sql, "since=999"));
    assert!(parsed.items.is_empty());
    assert_eq!(parsed.version, 1, "the actual server version, not the cursor");
}

#[test]
fn changes_since_negative_is_the_full_sweep() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a", "title": "first"}"#, 0);
    post(&sql, r#"{"id": "b", "title": "second"}"#, 0);
    // Any cursor below every stamp pulls everything: -1 parses fine and
    // must agree with the sweep, not trip validation.
    let resp = changes(&sql, "since=-1");
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(resp.body, sweep(&sql).body, "a negative cursor is the full sweep");
}

#[test]
fn changes_since_missing_or_non_numeric_400() {
    let sql = RusqliteSql::new();
    // The last case overflows i64 — numeric to the eye, but the parse fails
    // and the 400 answers, never a wrapped cursor.
    for query in ["", "since=abc", "cursor=1", "since=99999999999999999999"] {
        let resp = changes(&sql, query);
        assert_eq!(resp.status, 400, "query {query:?}: {}", resp.body);
    }
    let no_query = req(&sql, "GET", "/api/changes", None, None, 0);
    assert_eq!(no_query.status, 400);
}
