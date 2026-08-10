//! `hummingbird_authority::sweep_tick` (#138): the DO alarm's repeat-tick
//! evaluation. No HTTP surface — the worker's `alarm()` handler calls this
//! directly, on the schedule `ALARM_INTERVAL_MS` names. Every fixture here
//! goes through `sweep_tick` the same way that caller will, and asserts on
//! the `TickMatch`es it returns plus the rows it did (and did not) touch.

use hummingbird_authority::{sweep_tick, DeliveryOutcome, SqlValue, SuppressReason};

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

    let alerts = sql.exec("SELECT source, source_key, subject_key FROM alerts", &[]).unwrap();
    assert_eq!(alerts.len(), 1, "exactly one alert minted");
    assert_eq!(alerts[0].get("source").unwrap().as_text(), Some("item-threshold/v1"));
    assert_eq!(alerts[0].get("source_key").unwrap().as_text(), Some("item:it-1"));
    // ADR-0015 rule 1: an item is not a standing question and has no pane,
    // so the sweep names no subject — the join is left empty deliberately.
    assert_eq!(
        alerts[0].get("subject_key"),
        Some(&SqlValue::Null),
        "sweep_tick leaves subject_key NULL",
    );

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

/// The two-rule ratchet fixture, run once per seed order — the reviewer's
/// repro for the interleaved mint-then-deliver bug: rule order must not
/// change how many deliveries land, and a repeat tick with nothing changed
/// in the world must not manufacture a third one.
fn two_rules_one_item_ratchets_once_and_delivers_twice_per_tick(seed_high_first: bool) {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    if seed_high_first {
        seed_item_threshold_rule(&sql, "r-high", "deadline", "within_next", "2h", "urgent");
        seed_item_threshold_rule(&sql, "r-low", "deadline", "within_next", "2h", "normal");
    } else {
        seed_item_threshold_rule(&sql, "r-low", "deadline", "within_next", "2h", "normal");
        seed_item_threshold_rule(&sql, "r-high", "deadline", "within_next", "2h", "urgent");
    }
    seed_item_with_deadline(&sql, "it-1", "2026-08-15T10:00");

    let first = sweep_tick(&sql, 1786784400000).unwrap();
    assert_eq!(first.len(), 2, "both rules matched, one TickMatch each");
    let logged = first.iter().filter(|m| matches!(m.outcome, DeliveryOutcome::Logged { .. })).count();
    assert_eq!(logged, 2, "one delivery per matching rule, at the ratcheted alert — never a third");

    let alerts = sql.exec("SELECT id, severity FROM alerts", &[]).unwrap();
    assert_eq!(alerts.len(), 1, "one mint for both rules, not two");
    assert_eq!(
        alerts[0].get("severity").unwrap().as_text(),
        Some("urgent"),
        "the alert carries the higher of the two matched severities, regardless of seed order"
    );

    // A later tick, nothing in the world changed: neither rule should ring
    // again — the bug this pins would deliver a third time here.
    let second = sweep_tick(&sql, 1786784400000 + hummingbird_authority::ALARM_INTERVAL_MS).unwrap();
    assert_eq!(second.len(), 2, "both rules still match, so both are still reported");
    assert!(
        second.iter().all(|m| m.outcome == DeliveryOutcome::Suppressed(SuppressReason::AlreadyDelivered)),
        "a repeat tick with nothing changed must ring neither rule again: {second:?}"
    );

    let deliveries = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(deliveries.len(), 2, "exactly one delivery row per rule, ever — no phantom third");
}

#[test]
fn two_rules_one_item_high_severity_seeded_first() {
    two_rules_one_item_ratchets_once_and_delivers_twice_per_tick(true);
}

#[test]
fn two_rules_one_item_low_severity_seeded_first() {
    two_rules_one_item_ratchets_once_and_delivers_twice_per_tick(false);
}

#[test]
fn a_dismissed_alert_whose_item_still_matches_rings_again_on_the_next_tick() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_item_threshold_rule(&sql, "r-1", "deadline", "within_next", "2h", "urgent");
    seed_item_with_deadline(&sql, "it-1", "2026-08-15T10:00");

    let first = sweep_tick(&sql, 1786784400000).unwrap();
    assert!(matches!(first[0].outcome, DeliveryOutcome::Logged { .. }));

    // The human dismisses it (device scope) — the item itself is untouched
    // and still matches the rule.
    let seeded_alert = sql
        .exec("SELECT id, version FROM alerts WHERE source_key = 'item:it-1'", &[])
        .unwrap()
        .remove(0);
    let alert_id = seeded_alert.get("id").unwrap().as_text().unwrap().to_string();
    let alert_version = seeded_alert.get("version").unwrap().as_i64().unwrap();
    // Dismissed strictly after the alert's `raised_at` (1786784400000) —
    // the live formula only counts a dismissal that came after the raise
    // it's dismissing, and the whole point here is that it actually takes.
    let dismissed_at = 1786784400000 + 500;
    let dismiss_resp = patch_at(
        &sql,
        &format!("/api/alerts/{alert_id}"),
        &format!(r#"{{"expected_version": {alert_version}, "dismissed_at": {dismissed_at}}}"#),
        dismissed_at,
    );
    assert_eq!(dismiss_resp.status, 200, "{}", dismiss_resp.body);

    // A later tick, item unchanged, still matching, alert now settled.
    let second = sweep_tick(&sql, 1786784400000 + hummingbird_authority::ALARM_INTERVAL_MS).unwrap();

    assert_eq!(second.len(), 1);
    assert!(
        matches!(second[0].outcome, DeliveryOutcome::Logged { .. }),
        "a dismissed-but-still-matching item must ring again once the alert re-enters live, \
         got {:?}",
        second[0].outcome
    );

    let deliveries = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(deliveries.len(), 2, "the first delivery, plus the re-entry delivery");
}

#[test]
fn a_kindless_rule_still_fires_against_an_item_threshold_event() {
    // `event_kind: NULL` means "any kind" (ADR-0013) — a rule that never
    // names `item_threshold` explicitly still evaluates against this
    // sweep's synthetic event, on its core fields.
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let resp = post_rule(
        &sql,
        r#"{"id": "r-any", "name": "any-kind title match", "conditions": [
            {"field": "title", "op": "eq", "value": "seeded it-1", "negate": false}
        ], "severity": "high", "tier": "normal"}"#,
        0,
    );
    assert!(resp.status == 201 || resp.status == 200, "{}", resp.body);
    seed_item_with_deadline(&sql, "it-1", "2026-08-15T10:00");

    let matches = sweep_tick(&sql, 1786784400000).unwrap();

    assert_eq!(matches.len(), 1, "the kindless rule matched the item's title");
    assert!(matches!(matches[0].outcome, DeliveryOutcome::Logged { .. }));
}
