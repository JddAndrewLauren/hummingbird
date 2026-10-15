//! #845: the factory (`auk/v1`) as an alert source, end to end through the
//! real mint route and `POST /api/alerts` — one test per acceptance box.
//! Three things are pinned here: enrollment with a required allowed link
//! origin, ADR-0014's 2026-10-15 forward-only lifecycle (a late raise or a
//! late resolve changes nothing and rings nothing), and ADR-0035's
//! amendment that a pushed ring is never held for the waking window.
//!
//! Each token is minted the way the operator mints one: `POST
//! /api/admin/tokens`, `ingest` scope, bound to one source, carrying the
//! factory's origin. `factory.example` stands in for the real one, which
//! is set at mint from the operator's terminal and never checked in.

use hummingbird_authority::{ApiResponse, DeliveryOutcome};
use hummingbird_domain::{Alert, MintedToken, Tier, TokenInfo};

use crate::rig::*;

const ORIGIN: &str = "https://factory.example";
const KEY: &str = "mac-mini:rec-42:awaiting-review";
const T1: i64 = 1_000_000;
const T2: i64 = 2_000_000;

fn mint(sql: &dyn Sql, id: &str, source: &str, origin: Option<&str>) -> ApiResponse {
    let origin = origin.map(|o| format!(r#", "allowed_origin": "{o}""#)).unwrap_or_default();
    let body = format!(
        r#"{{"id": "{id}", "name": "{id}", "scope": "ingest", "source": "{source}"{origin}}}"#
    );
    req_admin(sql, "POST", "/api/admin/tokens", Some(&body), 0)
}

fn auk_token(sql: &dyn Sql) -> String {
    let resp = mint(sql, "auk", "auk/v1", Some(ORIGIN));
    assert_eq!(resp.status, 201, "{}", resp.body);
    body_as::<MintedToken>(&resp).token
}

/// One Auk write: a raise when `resolved_at` is `None`, a resolve when it
/// is `Some`, always carrying the explicit raise time Auk sends.
fn write(raised_at: i64, resolved_at: Option<i64>) -> String {
    let resolved = resolved_at.map(|r| format!(r#", "resolved_at": {r}"#)).unwrap_or_default();
    format!(
        r#"{{"source": "auk/v1", "source_key": "{KEY}", "title": "Waiting on your review",
            "severity": "normal", "url": "{ORIGIN}/map", "raised_at": {raised_at}{resolved}}}"#
    )
}

fn post(sql: &dyn Sql, token: &str, body: &str, now_ms: i64) -> ApiResponse {
    req_as(sql, token, "POST", "/api/alerts", None, Some(body), now_ms)
}

/// A `normal`-tier rule on `auk/v1` alone, and one live push target, so a
/// ring is observable as a `Logged` outcome on the ingest response.
fn seed_auk_rule_and_target(sql: &dyn Sql) {
    seed_push_target_raw(sql, "pt-1", "pixel-9");
    let resp = post_rule(
        sql,
        r#"{"id": "r-auk", "name": "factory waits", "event_kind": "alert_raised",
            "conditions": [{"field": "source", "op": "eq", "value": "auk/v1", "negate": false}],
            "severity": "normal", "tier": "normal"}"#,
        0,
    );
    assert!(resp.status == 201 || resp.status == 200, "rule seed failed: {}", resp.body);
}

fn logged_count(resp: &ApiResponse) -> usize {
    resp.deliveries.iter().filter(|d| matches!(d, DeliveryOutcome::Logged { .. })).count()
}

fn stored(sql: &dyn Sql) -> Alert {
    let resp = sweep(sql);
    let changes: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    let alerts: Vec<Alert> = serde_json::from_value(changes["alerts"].clone()).unwrap();
    alerts.into_iter().find(|a| a.source_key == KEY).expect("the auk alert exists")
}

/// Box 1: an `auk/v1` token mints with an `https` origin, its alerts are
/// accepted as a state source's, and a mint with no origin or a
/// non-`https` one is refused.
#[test]
fn an_auk_token_mints_only_with_an_https_origin_and_its_alerts_are_accepted() {
    let sql = RusqliteSql::new();

    let refused = mint(&sql, "auk-none", "auk/v1", None);
    assert_eq!(refused.status, 400, "{}", refused.body);
    assert!(refused.body.contains("auk/v1"), "names the source: {}", refused.body);
    let refused = mint(&sql, "auk-http", "auk/v1", Some("http://factory.example"));
    assert_eq!(refused.status, 400, "{}", refused.body);

    let resp = mint(&sql, "auk", "auk/v1", Some("HTTPS://Factory.Example:443"));
    assert_eq!(resp.status, 201, "{}", resp.body);
    let minted: MintedToken = body_as(&resp);
    assert_eq!(minted.allowed_origin.as_deref(), Some(ORIGIN), "stored canonical");
    let listed: Vec<TokenInfo> = body_as(&req_admin(&sql, "GET", "/api/admin/tokens", None, 0));
    let auk = listed.iter().find(|t| t.id == "auk").expect("listed");
    assert_eq!(auk.allowed_origin.as_deref(), Some(ORIGIN), "the listing reports it");

    let raised = post(&sql, &minted.token, &write(T1, None), T1);
    assert_eq!(raised.status, 201, "{}", raised.body);
    let resolved = post(&sql, &minted.token, &write(T1, Some(T1 + 500)), T1 + 500);
    assert_eq!(resolved.status, 200, "{}", resolved.body);
    let alert = stored(&sql);
    assert_eq!(alert.resolved_at, Some(T1 + 500), "the factory resolves its own alert");
    assert!(!alert.is_live(T1 + 500));
}

/// Box 2: a raise that lands after the resolution it preceded leaves the
/// alert resolved and rings nothing; a later genuine re-raise rings.
#[test]
fn a_raise_landing_after_its_resolution_changes_nothing_and_rings_nothing() {
    let sql = RusqliteSql::new();
    seed_auk_rule_and_target(&sql);
    let token = auk_token(&sql);

    assert_eq!(logged_count(&post(&sql, &token, &write(T1, None), T1)), 1, "the raise rings");
    post(&sql, &token, &write(T1, Some(T1 + 500)), T1 + 500);
    let before = stored(&sql);

    let late = post(&sql, &token, &write(T1, None), T1 + 900);
    assert_eq!(late.status, 200, "a stale write is answered as success: {}", late.body);
    assert!(late.deliveries.is_empty(), "and rings nothing: {:?}", late.deliveries);
    assert_eq!(stored(&sql), before, "no field changed, no version bump");

    let again = post(&sql, &token, &write(T2, None), T2);
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(logged_count(&again), 1, "a genuine re-raise rings as before");
    let alert = stored(&sql);
    assert_eq!((alert.raised_at, alert.resolved_at), (T2, None));
}

/// Box 3: a resolve that lands after the re-raise it preceded leaves the
/// re-raised alert live, with its later raise time, and rings nothing more.
#[test]
fn a_resolve_landing_after_the_re_raise_leaves_it_live_and_rings_nothing() {
    let sql = RusqliteSql::new();
    seed_auk_rule_and_target(&sql);
    let token = auk_token(&sql);

    post(&sql, &token, &write(T1, None), T1);
    post(&sql, &token, &write(T1, Some(T1 + 500)), T1 + 500);
    assert_eq!(logged_count(&post(&sql, &token, &write(T2, None), T2)), 1, "the re-raise rings");
    let before = stored(&sql);

    // The resolve Auk sent for the first wait, timed out on its side, lands
    // now — carrying the first wait's raise time.
    let late = post(&sql, &token, &write(T1, Some(T1 + 500)), T2 + 100);
    assert_eq!(late.status, 200, "{}", late.body);
    assert!(late.deliveries.is_empty(), "rings nothing more: {:?}", late.deliveries);
    let alert = stored(&sql);
    assert_eq!(alert, before, "no field changed");
    assert_eq!(alert.raised_at, T2, "raised_at never moves backwards");
    assert!(alert.is_live(T2 + 100), "the returned wait is still live");
}

/// Box 4: a write with no explicit raise time behaves as before — it keeps
/// the stored stamp and its resolution is written absolutely, on a source
/// with no origin either. (Every enrolled source's existing traffic is the
/// rest of this suite, unchanged.)
#[test]
fn writes_without_an_explicit_raised_at_behave_as_before() {
    let sql = RusqliteSql::new();
    let raise = r#"{"source": "healthchecks/v1", "source_key": "c", "title": "down"}"#;
    let first: Alert = body_as(&ingest_alert(&sql, raise, 1000));
    assert_eq!(first.raised_at, 1000);

    let resolve =
        r#"{"source": "healthchecks/v1", "source_key": "c", "title": "down", "resolved_at": 900}"#;
    let resolved: Alert = body_as(&ingest_alert(&sql, resolve, 2000));
    assert_eq!((resolved.raised_at, resolved.resolved_at), (1000, Some(900)));

    let again: Alert = body_as(&ingest_alert(&sql, raise, 3000));
    assert_eq!((again.raised_at, again.resolved_at), (1000, None), "the absent resolution clears");
}

/// Box 5: a link outside the token's allowed origin is refused with an
/// error naming the source, one on it is accepted, and a token with no
/// origin accepts any link as before.
#[test]
fn a_link_is_held_to_its_tokens_origin() {
    let sql = RusqliteSql::new();
    let token = auk_token(&sql);
    let with_url = |url: &str| {
        format!(r#"{{"source": "auk/v1", "source_key": "{KEY}", "title": "t", "url": "{url}"}}"#)
    };

    for url in ["https://evil.example/map", "https://factory.example@evil.example/", "http://factory.example/map"] {
        let resp = post(&sql, &token, &with_url(url), 0);
        assert_eq!(resp.status, 400, "{url}: {}", resp.body);
        assert!(resp.body.contains("auk/v1"), "names the source: {}", resp.body);
    }
    assert!(sql.exec("SELECT id FROM alerts", &[]).unwrap().is_empty(), "nothing was written");

    let on = post(&sql, &token, &with_url("https://factory.example/map?record=42"), 0);
    assert_eq!(on.status, 201, "{}", on.body);

    let hc = mint(&sql, "hc", "healthchecks/v1", None);
    let hc: MintedToken = body_as(&hc);
    assert_eq!(hc.allowed_origin, None);
    let anywhere = r#"{"source": "healthchecks/v1", "source_key": "c", "title": "t",
                      "url": "https://evil.example/anything"}"#;
    let resp = post(&sql, &hc.token, anywhere, 0);
    assert_eq!(resp.status, 201, "a token with no origin links anywhere: {}", resp.body);
}

/// Box 6 (ADR-0035's 2026-10-15 amendment): a pushed `auk/v1` alert raised
/// at 03:00 Pacific, outside the 08:00–22:00 waking window, is delivered at
/// once on the tier its Rule names — never held for the morning.
#[test]
fn a_pushed_ring_outside_the_waking_window_is_delivered_at_once() {
    // 2026-10-15T10:00:00Z — 03:00 PDT.
    const THREE_AM_PACIFIC: i64 = 1_792_058_400_000;
    let sql = RusqliteSql::new();
    seed_auk_rule_and_target(&sql);
    let token = auk_token(&sql);

    let resp = post(&sql, &token, &write(THREE_AM_PACIFIC, None), THREE_AM_PACIFIC);
    assert_eq!(resp.status, 201, "{}", resp.body);
    assert_eq!(resp.deliveries.len(), 1, "{:?}", resp.deliveries);
    match &resp.deliveries[0] {
        DeliveryOutcome::Logged { notification, .. } => {
            assert_eq!(notification.tier, Tier::Normal, "the Rule's tier, unclamped");
            assert_eq!(notification.source, "auk/v1");
        }
        other => panic!("expected an immediate Logged ring, got {other:?}"),
    }
}
