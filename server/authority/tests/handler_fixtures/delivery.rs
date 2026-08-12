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

/// **`deliver` owns severity monotonicity** (ADR-0014's 2026-08-12
/// amendment, #188). It used to dedupe on a *change* of the severity string,
/// deliberately leaving direction to whoever wrote `alert.severity` — and
/// the alerts ingest handler enforced it by ratcheting the stored row, which
/// cost a source the ability to lower severity on its own live occurrence.
/// The row is now a reading; this is the layer that refuses to ring for a
/// fall.
///
/// One occurrence, one rule, walked up and down: only the two genuine
/// escalations ring.
#[test]
fn deliver_rings_on_an_escalation_and_never_on_a_fall_or_a_repeat() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let alert = seed_alert_full_raw(&sql, "al-1", Some("normal"), 100, None, None, None);

    let at = |severity: &str| hummingbird_domain::Alert {
        severity: Some(severity.into()),
        ..alert.clone()
    };
    let rang = |severity: &str, now_ms: i64| {
        matches!(
            deliver(&sql, now_ms, &at(severity), &rule.id, rule.tier).unwrap(),
            DeliveryOutcome::Logged { .. }
        )
    };

    assert!(rang("normal", 500), "the first ring of a generation always lands");
    assert!(!rang("normal", 600), "an unchanged re-raise is absorbed, exactly as before #188");
    assert!(rang("urgent", 700), "a rise above what has rung is an escalation");
    assert!(!rang("high", 800), "a fall must never ring — the reader is being told good news");
    assert!(!rang("urgent", 900), "a climb back to a level already rung says nothing new");
    assert!(!rang("low", 1000), "a fall below where the generation opened is still just a fall");

    let logged = sql.exec("SELECT severity FROM deliveries", &[]).unwrap();
    assert_eq!(logged.len(), 2, "two rings across six calls: `normal`, then `urgent`");
}

/// The comparison is by `hummingbird_domain::severity_rank`, not by string
/// order — which is the whole reason the highest-rung severity is folded in
/// Rust rather than read as a SQL `MAX`. `"high"` sorts *below* `"normal"`
/// lexicographically while ranking above it, so a `MAX(severity)` would have
/// let this ring twice.
#[test]
fn the_escalation_comparison_ranks_severities_it_does_not_sort_them() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let alert = seed_alert_full_raw(&sql, "al-1", Some("high"), 100, None, None, None);
    deliver(&sql, 500, &alert, &rule.id, rule.tier).unwrap();

    let lowered = hummingbird_domain::Alert { severity: Some("normal".into()), ..alert.clone() };
    let outcome = deliver(&sql, 600, &lowered, &rule.id, rule.tier).unwrap();
    assert_eq!(
        outcome,
        DeliveryOutcome::Suppressed(SuppressReason::AlreadyDelivered),
        "`normal` ranks below `high` however the two strings sort"
    );
}

/// **Regression, caught reviewing #188 before it merged.** ADR-0012 warrants
/// a delivery on *entry* into live-unacked as well as on an escalation, and
/// the first rank-based implementation collapsed the two: it asked only
/// `rank > highest_rung`, with the baseline starting at `0`. `severity` is
/// free text with no `CHECK`, so an unranked string also ranks `0` — and a
/// first raise at `"warning"` ranked `0 > 0` and rang **nothing**, silently,
/// for the whole occurrence.
///
/// Reachable, not theoretical: `delivery.rs`'s own `NoSeverity` doc names
/// `healthchecks/v1` and `github/v1` as hand-rolled sources, and `"warning"`
/// / `"critical"` are exactly what such a webhook sends. Silent under-ringing
/// is the direction the module doc calls a bug, so the entry transition is
/// pinned here independently of any rank.
#[test]
fn an_unranked_severity_still_rings_on_entry_into_live() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let alert = seed_alert_full_raw(&sql, "al-1", Some("warning"), 100, None, None, None);

    let outcome = deliver(&sql, 500, &alert, &rule.id, rule.tier).unwrap();
    match outcome {
        DeliveryOutcome::Logged { notification, .. } => {
            assert_eq!(notification.severity, "warning", "sent verbatim, unranked or not");
        }
        other => panic!("an unranked first raise must still ring; got {other:?}"),
    }

    // And the escalation half still holds against it: a second unranked
    // string ties at rank 0, so it is not an escalation.
    let sideways = hummingbird_domain::Alert {
        severity: Some("critical".into()),
        ..alert.clone()
    };
    assert_eq!(
        deliver(&sql, 600, &sideways, &rule.id, rule.tier).unwrap(),
        DeliveryOutcome::Suppressed(SuppressReason::AlreadyDelivered),
        "one unranked string does not outrank another"
    );
    // A known severity does outrank it, and rings.
    let known = hummingbird_domain::Alert {
        severity: Some("urgent".into()),
        ..alert.clone()
    };
    assert!(
        matches!(
            deliver(&sql, 700, &known, &rule.id, rule.tier).unwrap(),
            DeliveryOutcome::Logged { .. }
        ),
        "a ranked severity escalates past an unranked one"
    );
}

/// An unranked severity ranks 0 (`domain::severity_rank`), so it can never
/// win an *escalation* it did not earn — and it does not panic. The mirror of
/// `domain`'s own unranked-challenger test, one layer up. (It may still open
/// an occurrence: see
/// `an_unranked_severity_still_rings_on_entry_into_live`.)
#[test]
fn an_unranked_severity_never_wins_a_ring() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let alert = seed_alert_full_raw(&sql, "al-1", Some("normal"), 100, None, None, None);
    deliver(&sql, 500, &alert, &rule.id, rule.tier).unwrap();

    let bogus = hummingbird_domain::Alert {
        severity: Some("catastrophic".into()),
        ..alert.clone()
    };
    let outcome = deliver(&sql, 600, &bogus, &rule.id, rule.tier).unwrap();
    assert_eq!(
        outcome,
        DeliveryOutcome::Suppressed(SuppressReason::AlreadyDelivered),
        "an unranked string must not ring past a known severity"
    );
}

/// The escalation baseline is scoped to one `generation`, and to one rule. A
/// fresh occurrence rings on its own merits at whatever level it opens at —
/// even one the *previous* occurrence had already rung above. A
/// cross-generation baseline would reinstate the high-water mark #188
/// rejected, one level up: a recovered-then-relapsed check could never ring
/// below its old peak.
#[test]
fn a_new_generation_rings_below_the_previous_generations_peak() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    let rule = seed_rule(&sql, "r-1");
    let urgent = seed_alert_full_raw(&sql, "al-1", Some("urgent"), 100, None, None, None);
    deliver(&sql, 500, &urgent, &rule.id, rule.tier).unwrap();

    // A later raise of the same alert: `raised_at` moves, so `generation`
    // does, and this is a new occurrence.
    let next = hummingbird_domain::Alert {
        severity: Some("normal".into()),
        raised_at: 900,
        ..urgent.clone()
    };
    let outcome = deliver(&sql, 1000, &next, &rule.id, rule.tier).unwrap();
    assert!(
        matches!(outcome, DeliveryOutcome::Logged { .. }),
        "a new occurrence is not held to the last one's peak"
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
