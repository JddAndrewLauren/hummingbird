//! `hummingbird_authority::deliver` (#139): transitions-only dedupe against
//! `deliveries` (ADR-0012, amended by ADR-0014). No HTTP surface: this is
//! the seam #138's periodic sweep calls directly, once per matching rule,
//! so every fixture here calls `deliver` the same way that caller will.
//! `deliver` is sync and never sends (see its module doc) — it decides,
//! logs, and hands back exactly what an async caller must send and to
//! whom, so these fixtures assert on the returned plan, not on a fake
//! network call.

use hummingbird_authority::{deliver, DeliveryOutcome, SuppressReason};
use hummingbird_domain::Tier;

use crate::rig::*;

#[test]
fn entry_into_live_unacked_sends_exactly_once() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let alert = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);

    let outcome = deliver(&sql, 500, &alert, &rule.id, rule.tier).unwrap();
    match outcome {
        DeliveryOutcome::Logged { targets, notification, .. } => {
            assert_eq!(targets.len(), 1);
            assert_eq!(targets[0].id, "pt-1");
            assert_eq!(notification.alert_id, "al-1");
            assert_eq!(notification.title, "seeded alert");
            assert_eq!(notification.severity, "high");
            assert_eq!(notification.tier, rule.tier);
        }
        other => panic!("expected Logged, got {other:?}"),
    }
    let rows = sql.exec("SELECT id, tier FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 1, "exactly one delivery row");
    assert_eq!(rows[0].get("tier").unwrap().as_text(), Some(rule.tier.as_str()));
}

#[test]
fn an_identical_re_raise_sends_nothing() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let alert = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);

    deliver(&sql, 500, &alert, &rule.id, rule.tier).unwrap();
    let second = deliver(&sql, 600, &alert, &rule.id, rule.tier).unwrap();

    assert_eq!(second, DeliveryOutcome::Suppressed(SuppressReason::AlreadyDelivered));
    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 1, "no duplicate row");
}

#[test]
fn a_severity_escalation_on_an_existing_alert_sends_again() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let first = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);
    deliver(&sql, 500, &first, &rule.id, rule.tier).unwrap();

    // Same alert, same generation (raised_at), escalated severity — the
    // caller's own ratcheted view, not a second DB row (#138 will hand
    // `deliver` exactly this: the alert it just ratcheted in memory).
    let escalated = hummingbird_domain::Alert {
        severity: Some("urgent".into()),
        ..first.clone()
    };
    let outcome = deliver(&sql, 700, &escalated, &rule.id, rule.tier).unwrap();

    match outcome {
        DeliveryOutcome::Logged { targets, notification, .. } => {
            assert_eq!(targets.len(), 1);
            assert_eq!(notification.severity, "urgent");
        }
        other => panic!("expected Logged on escalation, got {other:?}"),
    }
    let rows = sql.exec("SELECT id, severity FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 2, "escalation is a distinct delivery row");
}

#[test]
fn a_later_generation_after_re_entering_live_sends_again() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let first = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);
    deliver(&sql, 500, &first, &rule.id, rule.tier).unwrap();

    // A re-raise after dismissal: a later `raised_at` is a new generation
    // (ADR-0014), same severity.
    let re_raised = hummingbird_domain::Alert {
        raised_at: 9000,
        dismissed_at: Some(8000),
        ..first.clone()
    };
    let outcome = deliver(&sql, 9500, &re_raised, &rule.id, rule.tier).unwrap();
    match outcome {
        DeliveryOutcome::Logged { targets, .. } => assert_eq!(targets.len(), 1),
        other => panic!("expected Logged on re-entry into live, got {other:?}"),
    }
    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn an_alert_that_is_not_live_is_suppressed_without_logging() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    // dismissed after it was raised, and never re-raised since: not live.
    let alert = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, Some(200), None);

    let outcome = deliver(&sql, 500, &alert, &rule.id, rule.tier).unwrap();
    assert_eq!(outcome, DeliveryOutcome::Suppressed(SuppressReason::NotLive));
    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert!(rows.is_empty());
}

#[test]
fn a_revoked_target_is_excluded_while_a_sibling_still_receives_it() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_push_target_raw(&sql, "pt-2", "pixel-watch");
    sql.exec(
        "UPDATE push_targets SET revoked_at = 100 WHERE id = 'pt-1'",
        &[],
    )
    .unwrap();
    let rule = seed_rule(&sql, "r-1");
    let alert = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);

    let outcome = deliver(&sql, 500, &alert, &rule.id, rule.tier).unwrap();
    match outcome {
        DeliveryOutcome::Logged { targets, .. } => {
            assert_eq!(targets.len(), 1, "only the live target");
            assert_eq!(targets[0].id, "pt-2", "the revoked target is excluded");
        }
        other => panic!("expected Logged, got {other:?}"),
    }
}

/// Blocking review finding on the first round: zero live targets must not
/// permanently dedupe a transition that was never attempted — otherwise
/// every alert #138 raises before the first device ever registers (#141)
/// rings for no one, forever, once one finally does.
#[test]
fn no_live_targets_suppresses_without_logging_and_never_burns_the_transition() {
    let sql = RusqliteSql::new();
    let rule = seed_rule(&sql, "r-1");
    let alert = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);

    let outcome = deliver(&sql, 500, &alert, &rule.id, rule.tier).unwrap();
    assert_eq!(outcome, DeliveryOutcome::Suppressed(SuppressReason::NoTargets));
    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert!(rows.is_empty(), "nothing was attempted, so nothing is logged");

    // Once a target registers, the *same* transition (same alert, same
    // generation, same severity) is not burned — it still rings.
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let retry = deliver(&sql, 600, &alert, &rule.id, rule.tier).unwrap();
    match retry {
        DeliveryOutcome::Logged { targets, .. } => assert_eq!(targets.len(), 1),
        other => panic!("expected Logged once a target exists, got {other:?}"),
    }
}

/// The crash-safety acceptance criterion, restated for the sync/async
/// split: `deliver` logs and returns before the caller can possibly begin
/// sending, so a retried or restarted attempt at the exact same transition
/// — the "crash between send and log" scenario — is suppressed as already
/// handled rather than logged (and so sent) a second time.
#[test]
fn a_retry_of_the_same_transition_after_deliver_returns_is_never_re_logged() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let alert = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);

    let first = deliver(&sql, 500, &alert, &rule.id, rule.tier).unwrap();
    let delivery_id = match first {
        DeliveryOutcome::Logged { delivery_id, .. } => delivery_id,
        other => panic!("expected Logged, got {other:?}"),
    };

    // Simulates the caller crashing (or its send failing) before actually
    // reaching FCM, then retrying the whole transition from scratch.
    let retry = deliver(&sql, 900, &alert, &rule.id, rule.tier).unwrap();
    assert_eq!(retry, DeliveryOutcome::Suppressed(SuppressReason::AlreadyDelivered));

    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("id").unwrap().as_text(), Some(delivery_id.as_str()));
}

/// Non-blocking review note: `deliver` reads whatever severity string
/// `alert.severity` carries and dedupes on a *change* of it, not on a
/// validated escalation — enforcing the "never down" ratchet is the
/// responsibility of whoever mutates `alert.severity` (the alerts ingest
/// handler already does this via `domain::higher_severity`; #138's minting
/// will too), not `deliver`'s. Pinned here so that stays a deliberate
/// choice, not an oversight: a caller bug that let severity regress would
/// still (correctly, from `deliver`'s own contract) be treated as a new
/// transition and ring again.
#[test]
fn deliver_treats_any_severity_change_as_a_new_transition_ratchet_or_not() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let first = seed_alert_full_raw(&sql, "al-1", Some("urgent"), 100, None, None, None);
    deliver(&sql, 500, &first, &rule.id, rule.tier).unwrap();

    let downgraded = hummingbird_domain::Alert {
        severity: Some("low".into()),
        ..first.clone()
    };
    let outcome = deliver(&sql, 700, &downgraded, &rule.id, rule.tier).unwrap();
    assert!(
        matches!(outcome, DeliveryOutcome::Logged { .. }),
        "deliver does not itself enforce the ratchet direction"
    );
}

/// Sanity check that `Tier` rides the delivery, independent of the alert.
#[test]
fn tier_comes_from_the_caller_not_the_alert() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1"); // seeded with tier: normal
    assert_eq!(rule.tier, Tier::Normal);
    let alert = seed_alert_full_raw(&sql, "al-1", Some("urgent"), 100, None, None, None);

    let outcome = deliver(&sql, 500, &alert, &rule.id, Tier::Urgent).unwrap();
    match outcome {
        DeliveryOutcome::Logged { notification, .. } => {
            assert_eq!(
                notification.tier,
                Tier::Urgent,
                "the caller's verdict tier, not the rule's stored one"
            );
        }
        other => panic!("expected Logged, got {other:?}"),
    }
}
