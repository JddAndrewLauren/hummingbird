//! #830: `starling/v1`, the Starling cockpit's "waiting on you" alerts, end
//! to end through the real mint route, the alert ingest route and the rules
//! that make one ring — the in-process twin of the operator's after-merge
//! mint, the two rules, and the three hand-posted raises.
//!
//! The token is minted exactly as `scripts/mint-ingest-token.sh` mints it
//! (`POST /api/admin/tokens`, `ingest` scope, bound to one source), and the
//! two rules are the two the operator creates: `alert_raised`, conditioned on
//! `source` and `severity`, one per tier.

use hummingbird_authority::{ApiResponse, DeliveryOutcome};
use hummingbird_domain::{Alert, MintedToken, Tier};

use crate::rig::*;

const SOURCE: &str = "starling/v1";

fn mint(sql: &dyn Sql, id: &str, source: &str) -> ApiResponse {
    let body = format!(r#"{{"id": "{id}", "name": "{id}", "scope": "ingest", "source": "{source}"}}"#);
    req_admin(sql, "POST", "/api/admin/tokens", Some(&body), 0)
}

fn seed_starling_rule(sql: &dyn Sql, id: &str, severity: &str, tier: &str) {
    let resp = post_rule(
        sql,
        &format!(
            r#"{{"id": "{id}", "name": "{id}", "event_kind": "alert_raised",
                "conditions": [
                    {{"field": "source", "op": "eq", "value": "{SOURCE}", "negate": false}},
                    {{"field": "severity", "op": "eq", "value": "{severity}", "negate": false}}],
                "severity": "{severity}", "tier": "{tier}"}}"#
        ),
        0,
    );
    assert_eq!(resp.status, 201, "a rule naming {SOURCE} is accepted: {}", resp.body);
}

/// One hand-posted raise of the same occurrence, always with the same
/// `raised_at` — `resolved_at` is the third post's only difference.
fn raise(
    sql: &dyn Sql,
    token: &str,
    severity: &str,
    resolved_at: Option<i64>,
    now_ms: i64,
) -> ApiResponse {
    let resolved = resolved_at.map_or("null".to_string(), |at| at.to_string());
    let body = format!(
        r#"{{"source": "{SOURCE}", "source_key": "question:smoke-2026-10-14",
            "title": "batch-1014 is waiting on you", "severity": "{severity}",
            "raised_at": 1000, "resolved_at": {resolved}}}"#
    );
    req_as(sql, token, "POST", "/api/alerts", None, Some(&body), now_ms)
}

/// The tier of every delivery a raise logged, in rule order.
fn rang(resp: &ApiResponse) -> Vec<Tier> {
    resp.deliveries
        .iter()
        .filter_map(|outcome| match outcome {
            DeliveryOutcome::Logged { notification, .. } => Some(notification.tier),
            DeliveryOutcome::Suppressed(_) => None,
        })
        .collect()
}

#[test]
fn an_ingest_token_bound_to_starling_mints_and_an_unregistered_one_still_does_not() {
    let sql = RusqliteSql::new();
    let minted = mint(&sql, "starling-ingest", SOURCE);
    assert_eq!(minted.status, 201, "{SOURCE} is enrolled, so it mints: {}", minted.body);
    assert!(body_as::<MintedToken>(&minted).token.starts_with("hb_"));

    let refused = mint(&sql, "starling-typo", "starling/v2");
    assert_eq!(refused.status, 400, "an unregistered source is still refused: {}", refused.body);
    assert!(refused.body.contains("starling/v2"), "{}", refused.body);
}

#[test]
fn a_starling_alert_matches_the_rule_for_its_severity_and_only_that_one() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_starling_rule(&sql, "starling-blocked-normal", "normal", "normal");
    seed_starling_rule(&sql, "starling-blocked-urgent", "urgent", "urgent");
    let minted = mint(&sql, "starling-ingest", SOURCE);
    let token = body_as::<MintedToken>(&minted).token;

    let normal = raise(&sql, &token, "normal", None, 2000);
    assert_eq!(normal.status, 201, "{}", normal.body);
    assert_eq!(rang(&normal), [Tier::Normal], "the normal raise rings once, at normal tier");

    // Re-posted at urgent with the same raised_at: one occurrence, escalated.
    let urgent = raise(&sql, &token, "urgent", None, 3000);
    assert_eq!(urgent.status, 200, "same occurrence, updated: {}", urgent.body);
    assert_eq!(rang(&urgent), [Tier::Urgent], "the urgent re-raise rings again, at urgent tier");

    // Re-posted with a resolve time: the occurrence leaves live, and rings nothing.
    let resolved = raise(&sql, &token, "urgent", Some(4000), 5000);
    assert_eq!(resolved.status, 200, "{}", resolved.body);
    assert!(rang(&resolved).is_empty(), "a resolve rings nothing");
    assert!(!body_as::<Alert>(&resolved).is_live(5000), "a resolved occurrence leaves live");

    let rows = sql.exec("SELECT rule_id, tier FROM deliveries ORDER BY sent_at", &[]).unwrap();
    let logged: Vec<_> = rows
        .iter()
        .map(|row| {
            let text = |col| row.get(col).unwrap().as_text().unwrap().to_string();
            (text("rule_id"), text("tier"))
        })
        .collect();
    assert_eq!(
        logged,
        [
            ("starling-blocked-normal".to_string(), "normal".to_string()),
            ("starling-blocked-urgent".to_string(), "urgent".to_string()),
        ]
    );
}
