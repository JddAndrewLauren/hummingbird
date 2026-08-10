//! `hummingbird_authority::sweep_tick` (#138): the DO alarm's repeat-tick
//! evaluation. No HTTP surface — the worker's `alarm()` handler calls this
//! directly, on the schedule `ALARM_INTERVAL_MS` names. Every fixture here
//! goes through `sweep_tick` the same way that caller will, and asserts on
//! the `TickMatch`es it returns plus the rows it did (and did not) touch.

use hummingbird_authority::{sweep_tick, DeliveryOutcome, SuppressReason};

use crate::rig::*;

fn seed_item_threshold_rule(sql: &dyn Sql, id: &str, field: &str, op: &str, value: &str, severity: &str) {
    let condition = format!(
        r#"{{"field": "{field}", "op": "{op}", "value": "{value}", "negate": false}}"#
    );
    let body = format!(
        r#"{{"id": "{id}", "name": "seeded {id}", "event_kind": "item_threshold", "conditions": [{condition}], "severity": "{severity}", "tier": "normal"}}"#
    );
    let resp = post_rule(sql, &body, 0);
    assert!(resp.status == 201 || resp.status == 200, "rule seed failed: {}", resp.body);
}

fn seed_item_with_deadline(sql: &dyn Sql, id: &str, deadline: &str) {
    let resp = post(
        sql,
        &format!(r#"{{"id": "{id}", "title": "seeded {id}", "deadline": "{deadline}"}}"#),
        0,
    );
    assert!(resp.status == 201 || resp.status == 200, "item seed failed: {}", resp.body);
}

#[test]
fn an_item_matching_a_time_predicate_mints_and_delivers_on_the_tick_it_becomes_true() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_item_threshold_rule(&sql, "r-1", "deadline", "within_next", "2h", "urgent");
    seed_item_with_deadline(&sql, "it-1", "2026-08-15T10:00");

    // "now" = 2026-08-15T09:00: the deadline is within the 2h window.
    let matches = sweep_tick(&sql, 1786784400000).unwrap();

    assert_eq!(matches.len(), 1, "exactly one (item, rule) match");
    assert_eq!(matches[0].item_id, "it-1");
    assert_eq!(matches[0].rule_id, "r-1");
    match &matches[0].outcome {
        DeliveryOutcome::Logged { targets, notification, .. } => {
            assert_eq!(targets.len(), 1);
            assert_eq!(notification.severity, "urgent");
        }
        other => panic!("expected Logged, got {other:?}"),
    }

    let alerts = sql.exec("SELECT source, source_key FROM alerts", &[]).unwrap();
    assert_eq!(alerts.len(), 1, "exactly one alert minted");
    assert_eq!(alerts[0].get("source").unwrap().as_text(), Some("item-threshold/v1"));
    assert_eq!(alerts[0].get("source_key").unwrap().as_text(), Some("item:it-1"));

    let deliveries = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(deliveries.len(), 1, "exactly one delivery row logged");
}

#[test]
fn a_repeat_tick_on_the_same_still_matching_item_does_not_ring_again() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_item_threshold_rule(&sql, "r-1", "deadline", "within_next", "2h", "urgent");
    seed_item_with_deadline(&sql, "it-1", "2026-08-15T10:00");

    let first = sweep_tick(&sql, 1786784400000).unwrap();
    assert!(matches!(first[0].outcome, DeliveryOutcome::Logged { .. }));

    // A later tick, still inside the window, item unchanged.
    let second = sweep_tick(&sql, 1786784400000 + hummingbird_authority::ALARM_INTERVAL_MS).unwrap();

    assert_eq!(second.len(), 1, "the item still matches, so it's still reported");
    assert_eq!(
        second[0].outcome,
        DeliveryOutcome::Suppressed(SuppressReason::AlreadyDelivered),
        "the repeat tick lands on the same dedupe generation and is absorbed"
    );

    let alerts = sql.exec("SELECT id FROM alerts", &[]).unwrap();
    assert_eq!(alerts.len(), 1, "still exactly one alert row — no duplicate mint");
    let deliveries = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(deliveries.len(), 1, "still exactly one delivery row — no re-ring");
}

#[test]
fn an_overdue_item_still_matches_within_next_unbounded_on_the_past_side() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_item_threshold_rule(&sql, "r-1", "deadline", "within_next", "2h", "urgent");
    // Long overdue: `within_next` is unbounded on the past side (ADR-0013)
    // — an overdue item must still match, not fall out of the window.
    seed_item_with_deadline(&sql, "it-1", "2020-01-01T00:00");

    let matches = sweep_tick(&sql, 1786784400000).unwrap();

    assert_eq!(matches.len(), 1);
    assert!(matches!(matches[0].outcome, DeliveryOutcome::Logged { .. }));
}

#[test]
fn a_far_future_deadline_does_not_match_a_short_within_next_window() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_item_threshold_rule(&sql, "r-1", "deadline", "within_next", "2h", "urgent");
    seed_item_with_deadline(&sql, "it-1", "2030-01-01T00:00");

    let matches = sweep_tick(&sql, 1786784400000).unwrap();

    assert!(matches.is_empty(), "well outside the window: no match, no mint");
    let alerts = sql.exec("SELECT id FROM alerts", &[]).unwrap();
    assert!(alerts.is_empty(), "zero live targets or not: nothing minted for a non-match");
}

#[test]
fn the_sweep_never_writes_to_items_or_rules() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_item_threshold_rule(&sql, "r-1", "deadline", "within_next", "2h", "urgent");
    seed_item_with_deadline(&sql, "it-1", "2026-08-15T10:00");

    let item_before = sql.exec("SELECT * FROM items WHERE id = 'it-1'", &[]).unwrap();
    let rule_before = sql.exec("SELECT * FROM rules WHERE id = 'r-1'", &[]).unwrap();

    sweep_tick(&sql, 1786784400000).unwrap();

    let item_after = sql.exec("SELECT * FROM items WHERE id = 'it-1'", &[]).unwrap();
    let rule_after = sql.exec("SELECT * FROM rules WHERE id = 'r-1'", &[]).unwrap();
    assert_eq!(item_before, item_after, "the sweep must never mutate or re-class an item");
    assert_eq!(rule_before, rule_after, "the sweep must never mutate a rule");
}

#[test]
fn an_archived_item_is_excluded_from_evaluation_entirely() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_item_threshold_rule(&sql, "r-1", "deadline", "within_next", "2h", "urgent");
    seed_item_with_deadline(&sql, "it-1", "2026-08-15T10:00");
    sql.exec("UPDATE items SET archived_at = 1 WHERE id = 'it-1'", &[]).unwrap();

    let matches = sweep_tick(&sql, 1786784400000).unwrap();

    assert!(matches.is_empty(), "an archived item must never be scanned, even a still-matching one");
}

#[test]
fn a_disabled_rule_is_never_evaluated() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_item_threshold_rule(&sql, "r-1", "deadline", "within_next", "2h", "urgent");
    patch_rule(&sql, "r-1", r#"{"expected_version": 1, "enabled": false}"#, 0);
    seed_item_with_deadline(&sql, "it-1", "2026-08-15T10:00");

    let matches = sweep_tick(&sql, 1786784400000).unwrap();

    assert!(matches.is_empty());
}
