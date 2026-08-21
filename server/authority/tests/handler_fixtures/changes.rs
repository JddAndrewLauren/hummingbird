//! `GET /api/changes?since=N` and `GET /api/sweep`: the S0 delta suite,
//! the all-tables pull, and the byte-for-byte agreement criterion.

use hummingbird_authority::ApiResponse;
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
    seed_alert_raw(sql, "al-1", "healthchecks/v1", "sweeper"); // v9
    seed_snapshot_raw(sql, "f1/v1", "schedule"); // v10
    post_to(
        sql,
        "/api/project_links",
        r#"{"id": "l-1", "project_id": "p-1", "url": "https://example.com", "position": 1}"#,
        0,
    ); // v11
    meta_version(sql)
}

#[test]
fn delta_pull_carries_every_synced_table() {
    let sql = RusqliteSql::new();
    let version = seed_all_nine_tables(&sql);
    assert_eq!(version, 11);

    let parsed: ChangesResponse = body_as(&changes(&sql, "since=0"));
    assert_eq!(parsed.version, 11);
    assert_eq!(parsed.projects.len(), 1);
    assert_eq!(parsed.routes.len(), 1);
    assert_eq!(parsed.fog.len(), 1);
    assert_eq!(parsed.project_links.len(), 1);
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
    patch(&sql, "a-2", r#"{"expected_version": 5, "title": "renamed"}"#, 0); // v12
    let parsed: ChangesResponse = body_as(&changes(&sql, "since=11"));
    assert_eq!(parsed.version, 12);
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

// ADR-0016's wire horizon. Every test here names its own clock, because
// these two reads had no clock at all before this: the rig's `changes()` /
// `sweep()` pass `now_ms: 0`, and `seed_alert_raw` stamps `raised_at: 100`
// with no settling stamps — a live alert, which rides at any age, which is
// why every suite above is untouched by the horizon.

const NOW: i64 = 1_800_000_000_000;
const DAY: i64 = 24 * 60 * 60 * 1_000;

/// The alert ids a read carries, in wire order.
fn alert_ids(resp: &ApiResponse) -> Vec<String> {
    assert_eq!(resp.status, 200, "{}", resp.body);
    let parsed: ChangesResponse = body_as(resp);
    parsed.alerts.into_iter().map(|alert| alert.id).collect()
}

/// ADR-0012's ack contract: an alert nobody has acked stays on the wire
/// forever, because the wire is how the device learns it is still standing.
/// The horizon is settled-*and*-old, never old — this is the test that says
/// so.
#[test]
fn a_live_alert_of_any_age_rides_the_sweep() {
    let sql = RusqliteSql::new();
    seed_alert_full_raw(&sql, "ancient", None, NOW - 5 * 365 * DAY, None, None, None);

    assert_eq!(alert_ids(&sweep_at(&sql, NOW)), ["ancient"]);
}

#[test]
fn a_settled_alert_inside_the_horizon_still_rides() {
    let sql = RusqliteSql::new();
    seed_alert_full_raw(&sql, "recent", None, NOW - 100 * DAY, None, Some(NOW - 10 * DAY), None);

    assert_eq!(alert_ids(&sweep_at(&sql, NOW)), ["recent"]);
}

#[test]
fn a_settled_alert_drops_from_the_sweep_past_the_horizon() {
    let sql = RusqliteSql::new();
    seed_alert_full_raw(&sql, "old", None, NOW - 400 * DAY, None, Some(NOW - 200 * DAY), None);
    seed_alert_full_raw(&sql, "kept", None, NOW - 400 * DAY, None, Some(NOW - 89 * DAY), None);

    assert_eq!(alert_ids(&sweep_at(&sql, NOW)), ["kept"]);
}

/// Q2: the age is measured from the settling stamp, never from the raise.
#[test]
fn the_horizon_measures_from_the_settling_stamp_not_the_raise() {
    let sql = RusqliteSql::new();
    seed_alert_full_raw(&sql, "long-lived", None, NOW - 730 * DAY, None, Some(NOW - DAY), None);

    assert_eq!(alert_ids(&sweep_at(&sql, NOW)), ["long-lived"]);
}

/// ADR-0014's re-raise semantics reach the horizon intact: a raise stamped
/// after an old dismissal returns the row to live, and a live row rides.
#[test]
fn a_re_raised_alert_rides_again_regardless_of_age() {
    let sql = RusqliteSql::new();
    seed_alert_full_raw(&sql, "re-raised", None, NOW - 300 * DAY, None, Some(NOW - 730 * DAY), None);

    assert_eq!(alert_ids(&sweep_at(&sql, NOW)), ["re-raised"]);
}

/// Q4's `max`: dismissed two years ago but only expiring yesterday, so one
/// applicable stamp is still inside the horizon and the row is carried.
#[test]
fn every_applicable_stamp_must_be_old_for_a_row_to_drop() {
    let sql = RusqliteSql::new();
    seed_alert_full_raw(
        &sql,
        "dismissed-then-expired",
        None,
        NOW - 800 * DAY,
        None,
        Some(NOW - 730 * DAY),
        Some(NOW - DAY),
    );

    assert_eq!(alert_ids(&sweep_at(&sql, NOW)), ["dismissed-then-expired"]);
}

/// #114's agreement criterion, re-asserted at a non-zero clock with a row
/// the horizon excludes: the filter lives inside the shared code path, so
/// the two reads cannot diverge on it.
#[test]
fn sweep_equals_delta_from_zero_byte_for_byte_under_a_horizon() {
    let sql = RusqliteSql::new();
    seed_all_nine_tables(&sql);
    seed_alert_full_raw(&sql, "old", None, NOW - 400 * DAY, None, Some(NOW - 200 * DAY), None);
    seed_alert_full_raw(&sql, "kept", None, NOW - 400 * DAY, None, Some(NOW - DAY), None);

    let sweep_resp = sweep_at(&sql, NOW);
    let delta_resp = changes_at(&sql, "since=0", NOW);
    assert_eq!(sweep_resp.status, 200);
    assert_eq!(
        sweep_resp.body, delta_resp.body,
        "full sweep and delta-from-zero agree byte-for-byte under the horizon"
    );
    assert!(!sweep_resp.body.contains("\"old\""), "{}", sweep_resp.body);
}

/// Uniform application: a settled-and-old alert whose version is *above*
/// the cursor is excluded from the delta too, not only from the sweep.
#[test]
fn the_horizon_applies_to_the_delta_and_the_sweep_alike() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "a-1");
    let version = seed_alert_full_raw(
        &sql,
        "old",
        None,
        NOW - 400 * DAY,
        None,
        Some(NOW - 200 * DAY),
        None,
    )
    .version;

    let delta = changes_at(&sql, &format!("since={}", version - 1), NOW);
    assert!(alert_ids(&delta).is_empty(), "{}", delta.body);
    assert!(alert_ids(&sweep_at(&sql, NOW)).is_empty());
}
