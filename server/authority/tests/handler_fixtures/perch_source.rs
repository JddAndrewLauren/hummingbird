//! #891: `perch/v1`, the failures of every app built on Perch, end to end
//! through the real mint route, the alert ingest route and the two rules
//! that make one ring — the in-process twin of the operator's after-merge
//! mint, the two rules, and the hand-posted raises.
//!
//! The token is minted exactly as `scripts/mint-ingest-token.sh` mints it
//! with its allowed-origin argument: `ingest` scope, bound to `perch/v1`,
//! carrying Hummingbird's own origin. That origin is public, unlike Auk's
//! (`auk_source.rs`), because it is the one this authority already serves.

use hummingbird_authority::{ApiResponse, DeliveryOutcome};
use hummingbird_domain::{Alert, MintedToken, Tier, TokenInfo};

use crate::rig::*;

const SOURCE: &str = "perch/v1";
const ORIGIN: &str = "https://hb.twinion.net";

fn mint(sql: &dyn Sql, id: &str, source: &str, origin: Option<&str>) -> ApiResponse {
    let origin = origin.map(|o| format!(r#", "allowed_origin": "{o}""#)).unwrap_or_default();
    let body = format!(
        r#"{{"id": "{id}", "name": "{id}", "scope": "ingest", "source": "{source}"{origin}}}"#
    );
    req_admin(sql, "POST", "/api/admin/tokens", Some(&body), 0)
}

fn perch_token(sql: &dyn Sql) -> String {
    let resp = mint(sql, "perch-ingest", SOURCE, Some(ORIGIN));
    assert_eq!(resp.status, 201, "{}", resp.body);
    body_as::<MintedToken>(&resp).token
}

fn seed_perch_rule(sql: &dyn Sql, id: &str, severity: &str, tier: &str) {
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

/// One hand-posted Perch write, as Perch sends it: no link, the app and host
/// in the title, and an explicit raise time. A resolve when `resolved_at` is
/// `Some`.
fn raise(
    sql: &dyn Sql,
    token: &str,
    key: &str,
    severity: &str,
    resolved_at: Option<i64>,
    now_ms: i64,
) -> ApiResponse {
    let resolved = resolved_at.map_or("null".to_string(), |at| at.to_string());
    let body = format!(
        r#"{{"source": "{SOURCE}", "source_key": "{key}",
            "title": "guardian on osprey: api gave up", "severity": "{severity}",
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

/// Box 1: a `perch/v1` token mints with an allowed origin, is refused
/// without one, and an unregistered source is still refused.
#[test]
fn a_perch_token_mints_only_with_an_origin_and_an_unregistered_one_still_does_not() {
    let sql = RusqliteSql::new();

    let refused = mint(&sql, "perch-none", SOURCE, None);
    assert_eq!(refused.status, 400, "{}", refused.body);
    assert!(refused.body.contains(SOURCE), "names the source: {}", refused.body);

    let minted = mint(&sql, "perch-ingest", SOURCE, Some(ORIGIN));
    assert_eq!(minted.status, 201, "{SOURCE} is enrolled, so it mints: {}", minted.body);
    let minted: MintedToken = body_as(&minted);
    assert!(minted.token.starts_with("hb_"));
    assert_eq!(minted.allowed_origin.as_deref(), Some(ORIGIN));
    let listed: Vec<TokenInfo> = body_as(&req_admin(&sql, "GET", "/api/admin/tokens", None, 0));
    let perch = listed.iter().find(|t| t.id == "perch-ingest").expect("listed");
    assert_eq!(perch.allowed_origin.as_deref(), Some(ORIGIN), "the listing reports it");

    let refused = mint(&sql, "perch-typo", "perch/v2", Some(ORIGIN));
    assert_eq!(refused.status, 400, "an unregistered source is still refused: {}", refused.body);
    assert!(refused.body.contains("perch/v2"), "{}", refused.body);
}

/// Box 2: an alert carrying a link off the token's origin is refused,
/// naming the source, and writes nothing; one with no link — what Perch
/// actually sends — is accepted.
#[test]
fn a_perch_alert_linking_off_its_origin_is_refused() {
    let sql = RusqliteSql::new();
    let token = perch_token(&sql);
    let with_url = |url: &str| {
        format!(
            r#"{{"source": "{SOURCE}", "source_key": "osprey/guardian/api", "title": "t",
                "severity": "high", "url": "{url}"}}"#
        )
    };

    for url in ["https://evil.example/", "http://hb.twinion.net/", "http://localhost:8080/"] {
        let resp = req_as(&sql, &token, "POST", "/api/alerts", None, Some(&with_url(url)), 0);
        assert_eq!(resp.status, 400, "{url}: {}", resp.body);
        assert!(resp.body.contains(SOURCE), "names the source: {}", resp.body);
    }
    assert!(sql.exec("SELECT id FROM alerts", &[]).unwrap().is_empty(), "nothing was written");

    let linkless = raise(&sql, &token, "osprey/guardian/api", "high", None, 2000);
    assert_eq!(linkless.status, 201, "{}", linkless.body);
}

/// Box 3: each Perch raise matches the rule for its severity and only that
/// one — high rings urgent, normal rings normal, low rings nothing — and a
/// re-post with a resolve time leaves the live stack and rings nothing.
#[test]
fn a_perch_alert_matches_the_rule_for_its_severity_and_only_that_one() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_perch_rule(&sql, "perch-high-urgent", "high", "urgent");
    seed_perch_rule(&sql, "perch-normal-normal", "normal", "normal");
    let token = perch_token(&sql);

    let high = raise(&sql, &token, "osprey/guardian/api", "high", None, 2000);
    assert_eq!(high.status, 201, "{}", high.body);
    assert_eq!(rang(&high), [Tier::Urgent], "a high raise rings once, at urgent tier");

    let normal = raise(&sql, &token, "osprey/bearing/sync", "normal", None, 2500);
    assert_eq!(normal.status, 201, "{}", normal.body);
    assert_eq!(rang(&normal), [Tier::Normal], "a normal raise rings once, at normal tier");

    let low = raise(&sql, &token, "osprey/guardian/worker/recovered/900", "low", None, 2800);
    assert_eq!(low.status, 201, "{}", low.body);
    assert!(rang(&low).is_empty(), "a low raise rings nothing: {:?}", low.deliveries);

    let resolved = raise(&sql, &token, "osprey/guardian/api", "high", Some(3000), 4000);
    assert_eq!(resolved.status, 200, "{}", resolved.body);
    assert!(rang(&resolved).is_empty(), "a resolve rings nothing");
    assert!(!body_as::<Alert>(&resolved).is_live(4000), "a resolved Service leaves live");

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
            ("perch-high-urgent".to_string(), "urgent".to_string()),
            ("perch-normal-normal".to_string(), "normal".to_string()),
        ]
    );
}
