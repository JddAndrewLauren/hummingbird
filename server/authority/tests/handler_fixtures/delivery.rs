//! `hummingbird_authority::deliver` (#139): transitions-only dedupe against
//! `deliveries` (ADR-0012, amended by ADR-0014) plus the FCM send call,
//! exercised through [`crate::rig::FakePusher`] rather than a live project
//! — see the Agent Brief's "verification honesty" note. No HTTP surface:
//! this is the seam #138's periodic sweep calls directly, once per matching
//! rule, so every fixture here calls `deliver` the same way that caller
//! will.

use hummingbird_authority::{deliver, DeliveryOutcome, SuppressReason};

use crate::rig::*;

#[test]
fn entry_into_live_unacked_sends_exactly_once() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let alert = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);
    let pusher = FakePusher::new();

    let outcome = deliver(&sql, 500, &alert, &rule.id, rule.tier, &pusher).unwrap();
    match outcome {
        DeliveryOutcome::Sent { sent_to, failures, .. } => {
            assert_eq!(sent_to, 1);
            assert!(failures.is_empty());
        }
        other => panic!("expected Sent, got {other:?}"),
    }
    assert_eq!(pusher.sent.borrow().len(), 1);
    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 1, "exactly one delivery row");
}

#[test]
fn an_identical_re_raise_sends_nothing() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let alert = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);
    let pusher = FakePusher::new();

    deliver(&sql, 500, &alert, &rule.id, rule.tier, &pusher).unwrap();
    let second = deliver(&sql, 600, &alert, &rule.id, rule.tier, &pusher).unwrap();

    assert_eq!(
        second,
        DeliveryOutcome::Suppressed(SuppressReason::AlreadyDelivered)
    );
    assert_eq!(pusher.sent.borrow().len(), 1, "the second attempt sent nothing");
    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 1, "no duplicate row");
}

#[test]
fn a_severity_escalation_on_an_existing_alert_sends_again() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let first = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);
    let pusher = FakePusher::new();
    deliver(&sql, 500, &first, &rule.id, rule.tier, &pusher).unwrap();

    // Same alert, same generation (raised_at), escalated severity — the
    // caller's own ratcheted view, not a second DB row (#138 will hand
    // `deliver` exactly this: the alert it just ratcheted in memory).
    let escalated = hummingbird_domain::Alert {
        severity: Some("urgent".into()),
        ..first.clone()
    };
    let outcome = deliver(&sql, 700, &escalated, &rule.id, rule.tier, &pusher).unwrap();

    match outcome {
        DeliveryOutcome::Sent { sent_to, .. } => assert_eq!(sent_to, 1),
        other => panic!("expected Sent on escalation, got {other:?}"),
    }
    assert_eq!(pusher.sent.borrow().len(), 2);
    let rows = sql.exec("SELECT id, severity FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 2, "escalation is a distinct delivery row");
}

#[test]
fn a_later_generation_after_re_entering_live_sends_again() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let first = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);
    let pusher = FakePusher::new();
    deliver(&sql, 500, &first, &rule.id, rule.tier, &pusher).unwrap();

    // A re-raise after dismissal: a later `raised_at` is a new generation
    // (ADR-0014), same severity.
    let re_raised = hummingbird_domain::Alert {
        raised_at: 9000,
        dismissed_at: Some(8000),
        ..first.clone()
    };
    let outcome = deliver(&sql, 9500, &re_raised, &rule.id, rule.tier, &pusher).unwrap();
    match outcome {
        DeliveryOutcome::Sent { sent_to, .. } => assert_eq!(sent_to, 1),
        other => panic!("expected Sent on re-entry into live, got {other:?}"),
    }
    assert_eq!(pusher.sent.borrow().len(), 2);
}

#[test]
fn an_alert_that_is_not_live_is_suppressed_without_a_send() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    // dismissed after it was raised, and never re-raised since: not live.
    let alert = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, Some(200), None);
    let pusher = FakePusher::new();

    let outcome = deliver(&sql, 500, &alert, &rule.id, rule.tier, &pusher).unwrap();
    assert_eq!(outcome, DeliveryOutcome::Suppressed(SuppressReason::NotLive));
    assert!(pusher.sent.borrow().is_empty());
    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert!(rows.is_empty());
}

#[test]
fn a_revoked_target_receives_nothing_while_siblings_still_do() {
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
    let pusher = FakePusher::new();

    let outcome = deliver(&sql, 500, &alert, &rule.id, rule.tier, &pusher).unwrap();
    match outcome {
        DeliveryOutcome::Sent { sent_to, .. } => assert_eq!(sent_to, 1, "only the live target"),
        other => panic!("expected Sent, got {other:?}"),
    }
    let sent = pusher.sent.borrow();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "pt-2", "the revoked target never received it");
}

/// The crash-safety acceptance criterion: the delivery row is logged
/// before the send is attempted, so a target that fails to receive the
/// push still leaves the delivery marked handled — a retried/restarted
/// attempt at the same transition is absorbed as a duplicate rather than
/// re-sent, even though the push itself never completed.
#[test]
fn a_send_failure_still_logs_the_delivery_so_a_retry_never_re_sends() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let alert = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);
    let pusher = FakePusher::new();
    pusher.fail_targets.borrow_mut().push("pt-1".to_string());

    let outcome = deliver(&sql, 500, &alert, &rule.id, rule.tier, &pusher).unwrap();
    match outcome {
        DeliveryOutcome::Sent { sent_to, failures, .. } => {
            assert_eq!(sent_to, 0);
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0].target_id, "pt-1");
        }
        other => panic!("expected Sent with a recorded failure, got {other:?}"),
    }
    let rows = sql.exec("SELECT id FROM deliveries", &[]).unwrap();
    assert_eq!(rows.len(), 1, "the attempt is logged despite the send failure");

    // A later retry of the exact same transition — the "restart" — never
    // re-attempts the send.
    let retry = deliver(&sql, 900, &alert, &rule.id, rule.tier, &pusher).unwrap();
    assert_eq!(retry, DeliveryOutcome::Suppressed(SuppressReason::AlreadyDelivered));
    assert_eq!(pusher.sent.borrow().len(), 0, "no successful send ever happened");
}
