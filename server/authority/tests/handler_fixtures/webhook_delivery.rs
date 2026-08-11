//! #255: `POST /api/alerts`'s inline evaluate-then-deliver hook — the
//! second caller of `hummingbird_authority::deliver`, alongside #138's
//! `sweep_tick`. No HTTP-only surface exists for `deliver` itself
//! (`delivery.rs`'s fixtures already cover that seam directly); these
//! fixtures instead go through the real `POST /api/alerts` route, the way
//! a webhook source actually reaches it, and assert on `ApiResponse`'s new
//! `deliveries` field plus the rows it did (and did not) write — the same
//! shape `sweep.rs`'s fixtures already use for the sweep path.

use hummingbird_authority::{DeliveryOutcome, SuppressReason};

use crate::rig::*;

fn seed_any_kind_rule(sql: &dyn Sql, id: &str, severity: &str) {
    let resp = post_rule(
        sql,
        &format!(
            r#"{{"id": "{id}", "name": "seeded {id}", "conditions": [], "severity": "{severity}", "tier": "normal"}}"#
        ),
        0,
    );
    assert!(resp.status == 201 || resp.status == 200, "rule seed failed: {}", resp.body);
}

fn seed_severity_gated_rule(sql: &dyn Sql, id: &str, event_kind: &str, wants: &str) {
    let resp = post_rule(
        sql,
        &format!(
            r#"{{"id": "{id}", "name": "seeded {id}", "event_kind": "{event_kind}",
                "conditions": [{{"field": "severity", "op": "eq", "value": "{wants}", "negate": false}}],
                "severity": "urgent", "tier": "urgent"}}"#
        ),
        0,
    );
    assert!(resp.status == 201 || resp.status == 200, "rule seed failed: {}", resp.body);
}

/// AC1: an alert raised through `POST /api/alerts` that matches an enabled
/// rule produces a delivery row and a send, through `deliver`'s own dedupe
/// — never a second implementation. The response itself carries the
/// `Logged` outcome (#255's new `ApiResponse::deliveries`), which is what
/// the worker shim's `wait_until` hook would drain and send from.
#[test]
fn a_webhook_alert_matching_an_enabled_rule_delivers() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_any_kind_rule(&sql, "r-1", "high");

    let resp = ingest_alert(
        &sql,
        r#"{"source": "healthchecks/v1", "source_key": "sweeper", "title": "sweeper is down",
            "severity": "high"}"#,
        1000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert_eq!(resp.deliveries.len(), 1, "one matching rule, one outcome");
    match &resp.deliveries[0] {
        DeliveryOutcome::Logged { targets, notification, .. } => {
            assert_eq!(targets.len(), 1);
            assert_eq!(targets[0].id, "pt-1");
            assert_eq!(notification.severity, "high");
        }
        other => panic!("expected Logged, got {other:?}"),
    }

    let rows = sql.exec("SELECT id, rule_id FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 1, "exactly one delivery row");
    assert_eq!(rows[0].get("rule_id").unwrap().as_text(), Some("r-1"));
}

/// AC2: the no-retry policy holds on this path too — the delivery row
/// commits (inside `deliver`, before this handler could possibly attempt a
/// send) on the very first ingest, so a byte-identical replay of the same
/// webhook payload lands on the same dedupe key and is suppressed rather
/// than re-logged.
#[test]
fn a_replayed_identical_webhook_payload_is_never_re_delivered() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_any_kind_rule(&sql, "r-1", "high");
    let body = r#"{"source": "healthchecks/v1", "source_key": "sweeper", "title": "sweeper is down",
                    "severity": "high"}"#;

    let first = ingest_alert(&sql, body, 1000);
    assert_eq!(first.deliveries.len(), 1);
    assert!(matches!(first.deliveries[0], DeliveryOutcome::Logged { .. }));

    // An identical replay: `upsert` itself is a no-op (AC1 of #114), and
    // `deliver` is called regardless (unconditionally, per the 2026-08-10
    // grilling decision) — its own dedupe key is what absorbs the replay.
    let second = ingest_alert(&sql, body, 2000);
    assert_eq!(second.status, 200, "re-raise is success, not conflict");
    assert_eq!(second.deliveries.len(), 1);
    assert_eq!(
        second.deliveries[0],
        DeliveryOutcome::Suppressed(SuppressReason::AlreadyDelivered)
    );

    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 1, "no duplicate delivery row");
}

/// AC3: zero live targets suppresses without logging, exactly as it does
/// for the sweep path — the transition is never burned, so it still rings
/// once a device finally registers.
#[test]
fn zero_live_targets_suppresses_without_logging() {
    let sql = RusqliteSql::new();
    seed_any_kind_rule(&sql, "r-1", "high");

    let resp = ingest_alert(
        &sql,
        r#"{"source": "healthchecks/v1", "source_key": "sweeper", "title": "sweeper is down",
            "severity": "high"}"#,
        1000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert_eq!(resp.deliveries.len(), 1);
    assert_eq!(resp.deliveries[0], DeliveryOutcome::Suppressed(SuppressReason::NoTargets));

    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert!(rows.is_empty(), "nothing was attempted, so nothing is logged");
}

/// A raise matching no enabled rule delivers nothing and logs nothing —
/// default-deny stands, and the row still reaches devices via the delta
/// pull regardless (unasserted here; `alerts.rs`'s own fixtures already
/// cover the row itself).
#[test]
fn a_webhook_alert_matching_no_rule_delivers_nothing() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_severity_gated_rule(&sql, "r-1", "alert_raised", "urgent");

    let resp = ingest_alert(
        &sql,
        r#"{"source": "healthchecks/v1", "source_key": "sweeper", "title": "sweeper is down",
            "severity": "high"}"#,
        1000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert!(resp.deliveries.is_empty(), "no rule matched, nothing to deliver");
    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert!(rows.is_empty());
}

/// A rule scoped to a different `event_kind` never fires for a webhook
/// alert — `alert_raised` events only match a rule naming `alert_raised`
/// (or naming no kind at all).
#[test]
fn a_rule_scoped_to_a_different_event_kind_does_not_match() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_severity_gated_rule(&sql, "r-1", "email", "high");

    let resp = ingest_alert(
        &sql,
        r#"{"source": "healthchecks/v1", "source_key": "sweeper", "title": "sweeper is down",
            "severity": "high"}"#,
        1000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert!(resp.deliveries.is_empty(), "an `email`-kind rule must not see an alert_raised event");
    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert!(rows.is_empty());
}

/// A rule explicitly scoped to `event_kind: "alert_raised"` matches, and
/// so does one naming no kind at all (`None` = "any kind") — the second
/// case is `a_webhook_alert_matching_an_enabled_rule_delivers` above; this
/// pins the first.
#[test]
fn a_rule_scoped_to_alert_raised_matches_on_the_gated_field() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_severity_gated_rule(&sql, "r-1", "alert_raised", "high");

    let resp = ingest_alert(
        &sql,
        r#"{"source": "healthchecks/v1", "source_key": "sweeper", "title": "sweeper is down",
            "severity": "high"}"#,
        1000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert_eq!(resp.deliveries.len(), 1);
    assert!(matches!(resp.deliveries[0], DeliveryOutcome::Logged { .. }));
}

/// A disabled rule is never evaluated — the poller-facing `load_enabled`
/// query this hook shares with `sweep_tick` already filters it out, so
/// this is really a regression pin on the shared query rather than new
/// logic.
#[test]
fn a_disabled_rule_never_delivers() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let disable = patch_rule(
        &sql,
        &rule.id,
        &format!(r#"{{"expected_version": {}, "enabled": false}}"#, rule.version),
        0,
    );
    assert_eq!(disable.status, 200, "{}", disable.body);

    let resp = ingest_alert(
        &sql,
        r#"{"source": "healthchecks/v1", "source_key": "sweeper", "title": "sweeper is down",
            "severity": "high"}"#,
        1000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert!(resp.deliveries.is_empty(), "a disabled rule must not deliver");
}
