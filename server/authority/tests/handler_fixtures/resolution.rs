//! ADR-0014's resolution pass (#217): the second phase of every
//! `sweep_tick`, which says "the condition ended" for `item-threshold/v1` —
//! the one thing #138's sweep could never express, and without which an
//! alert nobody hand-acked stayed live and top-of-stack forever.
//!
//! Two properties carry the whole design, and both are pinned here:
//!
//! 1. **The phases partition the alert set** — phase one mints, phase two
//!    resolves exactly what phase one did not touch, so no alert is written
//!    twice in a tick.
//! 2. **The sweep never clears `resolved_at`** — the hazard the issue
//!    named, where `upsert_alert`'s absolute sets erase the pass's stamp on
//!    the very next tick.

use hummingbird_authority::{sweep_tick, DeliveryOutcome, ALARM_INTERVAL_MS};

use crate::rig::*;

/// 2026-08-15T09:00Z. The item deadlines below sit an hour later, inside
/// the rules' 2h `within_next` window.
const TICK: i64 = 1786784400000;

fn seed_deadline_rule(sql: &dyn Sql, id: &str) {
    let body = format!(
        r#"{{"id": "{id}", "name": "seeded {id}", "event_kind": "item_threshold", "conditions": [
            {{"field": "deadline", "op": "within_next", "value": "2h", "negate": false}}
        ], "severity": "urgent", "tier": "normal"}}"#
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

/// Seeds a rule, a matching item and a push target, then runs the tick that
/// mints and rings — the starting state for every "…and then it ends" case.
fn seed_a_ringing_alert(sql: &dyn Sql) {
    seed_push_target_raw(sql, "pt-1", "pixel-9");
    seed_deadline_rule(sql, "r-1");
    seed_item_with_deadline(sql, "it-1", "2026-08-15T10:00");
    let first = sweep_tick(sql, TICK).unwrap();
    assert!(
        matches!(first[0].outcome, DeliveryOutcome::Logged { .. }),
        "the fixture's premise is an alert that actually rang",
    );
    assert_eq!(alert_field(sql, "resolved_at"), None, "not resolved yet");
}

fn alert_field(sql: &dyn Sql, column: &str) -> Option<i64> {
    let rows = sql
        .exec(
            &format!("SELECT {column} FROM alerts WHERE source_key = 'item:it-1'"),
            &[],
        )
        .unwrap();
    rows.first().and_then(|row| row.get(column).unwrap().as_i64())
}

// ---------------------------------------------------------------------------
// ADR-0014's four triggers
// ---------------------------------------------------------------------------

/// The plain case the issue opens with: nobody dismissed it, the item was
/// simply edited past the threshold, and before #217 that alert had no path
/// back to quiet — ever.
#[test]
fn an_item_edited_past_the_threshold_resolves_its_alert() {
    let sql = RusqliteSql::new();
    seed_a_ringing_alert(&sql);

    // The deadline moves well outside the 2h window: the rule stops
    // producing a verdict for this item.
    sql.exec("UPDATE items SET deadline = '2030-01-01T00:00' WHERE id = 'it-1'", &[])
        .unwrap();
    let second = sweep_tick(&sql, TICK + ALARM_INTERVAL_MS).unwrap();

    assert!(second.is_empty(), "no rule matches any more");
    assert_eq!(
        alert_field(&sql, "resolved_at"),
        Some(TICK + ALARM_INTERVAL_MS),
        "the condition ended, so the pass stamped it",
    );
}

/// ADR-0014 lists "done" first among the things that end the condition.
/// `load_live_items` does *not* exclude a done-but-unarchived item — #138
/// deliberately left `stage` out of the evaluation boundary — so this only
/// works because the sweep skips done items for minting, which drops them
/// out of the matched set and hands them to the pass.
#[test]
fn a_done_item_resolves_its_alert() {
    let sql = RusqliteSql::new();
    seed_a_ringing_alert(&sql);

    sql.exec("UPDATE items SET stage = 'done' WHERE id = 'it-1'", &[]).unwrap();
    let second = sweep_tick(&sql, TICK + ALARM_INTERVAL_MS).unwrap();

    assert!(second.is_empty(), "a done item mints nothing, even while it still matches");
    assert_eq!(alert_field(&sql, "resolved_at"), Some(TICK + ALARM_INTERVAL_MS));
}

/// Archiving removes the item from the scan entirely, which is exactly why
/// an item-side pass could never have closed this alert.
#[test]
fn an_archived_item_resolves_its_alert() {
    let sql = RusqliteSql::new();
    seed_a_ringing_alert(&sql);

    sql.exec("UPDATE items SET archived_at = 1 WHERE id = 'it-1'", &[]).unwrap();
    sweep_tick(&sql, TICK + ALARM_INTERVAL_MS).unwrap();

    assert_eq!(alert_field(&sql, "resolved_at"), Some(TICK + ALARM_INTERVAL_MS));
}

/// "A deleted or unknown `item:<id>` is the condition ending in its most
/// total form" — it resolves rather than erroring.
#[test]
fn a_deleted_item_resolves_its_alert() {
    let sql = RusqliteSql::new();
    seed_a_ringing_alert(&sql);

    sql.exec("DELETE FROM items WHERE id = 'it-1'", &[]).unwrap();
    sweep_tick(&sql, TICK + ALARM_INTERVAL_MS).unwrap();

    assert_eq!(alert_field(&sql, "resolved_at"), Some(TICK + ALARM_INTERVAL_MS));
}

// ---------------------------------------------------------------------------
// The partition
// ---------------------------------------------------------------------------

/// The other half of the partition, and the one that would break loudly if
/// the pass re-derived "still matching" instead of reading phase one's set.
#[test]
fn a_still_matching_item_is_never_resolved() {
    let sql = RusqliteSql::new();
    seed_a_ringing_alert(&sql);

    sweep_tick(&sql, TICK + ALARM_INTERVAL_MS).unwrap();

    assert_eq!(alert_field(&sql, "resolved_at"), None, "the condition has not ended");
}

/// Disabling the rule that raised an alert is the operator saying they no
/// longer want to hear about it. Before #217 the tick returned early on an
/// empty rule set, which would have stranded every outstanding alert live.
#[test]
fn disabling_every_rule_still_resolves_the_alerts_they_raised() {
    let sql = RusqliteSql::new();
    seed_a_ringing_alert(&sql);

    let version = sql
        .exec("SELECT version FROM rules WHERE id = 'r-1'", &[])
        .unwrap()[0]
        .get("version")
        .unwrap()
        .as_i64()
        .unwrap();
    let resp = patch_rule(
        &sql,
        "r-1",
        &format!(r#"{{"expected_version": {version}, "enabled": false}}"#),
        TICK,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    sweep_tick(&sql, TICK + ALARM_INTERVAL_MS).unwrap();

    assert_eq!(alert_field(&sql, "resolved_at"), Some(TICK + ALARM_INTERVAL_MS));
}

/// The pass is scoped to `item-threshold/v1`. An event source's alert never
/// ends on its own (ADR-0014) and must not be closed by this scan.
#[test]
fn an_alert_from_another_source_is_never_touched() {
    let sql = RusqliteSql::new();
    seed_deadline_rule(&sql, "r-1");
    let foreign = seed_alert_full_raw(&sql, "al-mail", Some("high"), TICK, None, None, None);

    sweep_tick(&sql, TICK + ALARM_INTERVAL_MS).unwrap();

    let rows = sql
        .exec("SELECT resolved_at, version FROM alerts WHERE id = 'al-mail'", &[])
        .unwrap();
    assert_eq!(rows[0].get("resolved_at").unwrap().as_i64(), None);
    assert_eq!(
        rows[0].get("version").unwrap().as_i64(),
        Some(foreign.version),
        "not even a version bump — the pass never saw it",
    );
}

// ---------------------------------------------------------------------------
// Composition with the mint path — the hazard the issue named
// ---------------------------------------------------------------------------

/// **The test #217 asked for by name.** Resolve, then a tick on which the
/// item matches again: `resolved_at` must survive rather than being erased
/// by `upsert_alert`'s absolute sets — and the alert must nonetheless be
/// live again and ring, because a later `raised_at` overtakes the stamp
/// (ADR-0014's "Live: how a settled alert rings again"). Both halves matter:
/// keeping the stamp without re-raising would leave it permanently quiet,
/// and re-raising while erasing the stamp is the silent data loss the issue
/// predicted.
#[test]
fn a_re_matching_item_re_raises_without_erasing_the_resolution_stamp() {
    let sql = RusqliteSql::new();
    seed_a_ringing_alert(&sql);

    // Tick two: pushed out of the window, so the pass resolves it.
    let resolved_tick = TICK + ALARM_INTERVAL_MS;
    sql.exec("UPDATE items SET deadline = '2030-01-01T00:00' WHERE id = 'it-1'", &[])
        .unwrap();
    sweep_tick(&sql, resolved_tick).unwrap();
    assert_eq!(alert_field(&sql, "resolved_at"), Some(resolved_tick));

    // Tick three: the deadline is re-committed and matches again.
    let re_raised_tick = TICK + 2 * ALARM_INTERVAL_MS;
    sql.exec("UPDATE items SET deadline = '2026-08-15T10:00' WHERE id = 'it-1'", &[])
        .unwrap();
    let third = sweep_tick(&sql, re_raised_tick).unwrap();

    assert_eq!(
        alert_field(&sql, "resolved_at"),
        Some(resolved_tick),
        "the mint path must carry the stamp through, never clear it",
    );
    assert_eq!(
        alert_field(&sql, "raised_at"),
        Some(re_raised_tick),
        "a fresh occurrence starts now, overtaking the resolution",
    );
    assert!(
        matches!(third[0].outcome, DeliveryOutcome::Logged { .. }),
        "a resolved alert whose condition returns must ring again, got {:?}",
        third[0].outcome,
    );
    let deliveries = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(deliveries.len(), 2, "the original ring, plus the re-entry ring");
}

/// The pass considers only *live* alerts, which is what stops the stamp
/// creeping forward one tick at a time — and stops a resolved alert being
/// re-resolved (and re-versioned) on every tick forever, which would make
/// every delta pull carry a row that never actually changed.
#[test]
fn resolving_is_idempotent_across_repeat_ticks() {
    let sql = RusqliteSql::new();
    seed_a_ringing_alert(&sql);
    sql.exec("UPDATE items SET deadline = '2030-01-01T00:00' WHERE id = 'it-1'", &[])
        .unwrap();

    let resolved_tick = TICK + ALARM_INTERVAL_MS;
    sweep_tick(&sql, resolved_tick).unwrap();
    let version_after_resolve = alert_field(&sql, "version");

    sweep_tick(&sql, TICK + 2 * ALARM_INTERVAL_MS).unwrap();
    sweep_tick(&sql, TICK + 3 * ALARM_INTERVAL_MS).unwrap();

    assert_eq!(alert_field(&sql, "resolved_at"), Some(resolved_tick), "the stamp never moves");
    assert_eq!(alert_field(&sql, "version"), version_after_resolve, "and nothing re-writes it");
}

/// Resolution is not dismissal, and not a re-report: it writes one column.
/// Everything the last raise established — title, severity, `raised_at`, and
/// the human-owned `dismissed_at` — must survive it untouched.
#[test]
fn resolving_writes_only_resolved_at_and_the_version() {
    let sql = RusqliteSql::new();
    seed_a_ringing_alert(&sql);
    let before = sql
        .exec("SELECT * FROM alerts WHERE source_key = 'item:it-1'", &[])
        .unwrap()
        .remove(0);

    sql.exec("UPDATE items SET deadline = '2030-01-01T00:00' WHERE id = 'it-1'", &[])
        .unwrap();
    sweep_tick(&sql, TICK + ALARM_INTERVAL_MS).unwrap();

    let after = sql
        .exec("SELECT * FROM alerts WHERE source_key = 'item:it-1'", &[])
        .unwrap()
        .remove(0);
    for (column, value) in &before {
        if column == "resolved_at" || column == "version" {
            continue;
        }
        assert_eq!(after.get(column), Some(value), "the pass must not touch `{column}`");
    }
    assert_eq!(after.get("dismissed_at").unwrap().as_i64(), None);
}

/// A resolution has to reach devices on the ordinary delta pull, not wait
/// for the daily `GET /api/sweep` backstop — so it bumps the shared `meta`
/// version like every other write.
#[test]
fn resolving_bumps_the_delta_version() {
    let sql = RusqliteSql::new();
    seed_a_ringing_alert(&sql);
    let before = meta_version(&sql);

    sql.exec("UPDATE items SET deadline = '2030-01-01T00:00' WHERE id = 'it-1'", &[])
        .unwrap();
    sweep_tick(&sql, TICK + ALARM_INTERVAL_MS).unwrap();

    let after = meta_version(&sql);
    assert!(after > before, "the resolved alert must appear in the next delta pull");
    assert_eq!(alert_field(&sql, "version"), Some(after));
}

/// Two items, one ending and one continuing, on the same tick — the
/// partition has to hold per-alert, not just when the whole set agrees.
#[test]
fn one_tick_resolves_the_ended_item_and_leaves_the_continuing_one_alone() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_deadline_rule(&sql, "r-1");
    seed_item_with_deadline(&sql, "it-1", "2026-08-15T10:00");
    seed_item_with_deadline(&sql, "it-2", "2026-08-15T10:30");
    assert_eq!(sweep_tick(&sql, TICK).unwrap().len(), 2, "both matched");

    sql.exec("UPDATE items SET deadline = '2030-01-01T00:00' WHERE id = 'it-1'", &[])
        .unwrap();
    sweep_tick(&sql, TICK + ALARM_INTERVAL_MS).unwrap();

    assert_eq!(alert_field(&sql, "resolved_at"), Some(TICK + ALARM_INTERVAL_MS));
    let other = sql
        .exec("SELECT resolved_at FROM alerts WHERE source_key = 'item:it-2'", &[])
        .unwrap();
    assert_eq!(
        other[0].get("resolved_at").unwrap().as_i64(),
        None,
        "the still-matching item's alert is untouched by the same pass",
    );
}
