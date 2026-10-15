//! #717: the fantasy lane's two sources, end to end through the real mint
//! route and both ingest routes — the in-process twin of the acceptance
//! box "a hand-posted snapshot and a hand-posted alert under each source are
//! accepted with the new ingest token, and rejected without it".
//!
//! Each token here is minted exactly as the operator's runbook mints one
//! (`POST /api/admin/tokens`, `ingest` scope, bound to one source), which is
//! also what pins the lane's token shape: an ingest token binds **one**
//! source (#145), so the waivers source refuses the lineup's token — the
//! lane needs a token per source, not one shared by both.

use hummingbird_domain::MintedToken;

use crate::rig::*;

const SOURCES: [&str; 2] = ["yahoo-lineup/v1", "yahoo-waivers/v1"];

fn mint_ingest(sql: &dyn Sql, id: &str, source: &str) -> String {
    let body = format!(r#"{{"id": "{id}", "name": "{id}", "scope": "ingest", "source": "{source}"}}"#);
    let resp = req_admin(sql, "POST", "/api/admin/tokens", Some(&body), 0);
    assert_eq!(resp.status, 201, "{source} is enrolled, so it mints: {}", resp.body);
    body_as::<MintedToken>(&resp).token
}

fn snapshot(source: &str) -> String {
    format!(
        r#"{{"source": "{source}", "key": "449.l.123456", "fetched_at": 1000,
            "payload": {{"schema": "{source}", "polled_every_ms": 21600000, "body": {{}}}}}}"#
    )
}

fn alert(source: &str) -> String {
    let key = if source == "yahoo-lineup/v1" { "team:449.l.123456.t.7" } else { "2026-04" };
    format!(r#"{{"source": "{source}", "source_key": "{key}", "title": "hand-posted"}}"#)
}

#[test]
fn each_source_accepts_a_hand_posted_snapshot_and_alert_with_its_own_token() {
    let sql = RusqliteSql::new();
    for (index, source) in SOURCES.iter().enumerate() {
        let token = mint_ingest(&sql, &format!("yahoo-{index}"), source);
        let snap = req_as(&sql, &token, "POST", "/api/snapshots", None, Some(&snapshot(source)), 0);
        assert_eq!(snap.status, 201, "{source} snapshot: {}", snap.body);
        let raised = req_as(&sql, &token, "POST", "/api/alerts", None, Some(&alert(source)), 0);
        assert_eq!(raised.status, 201, "{source} alert: {}", raised.body);
    }
}

#[test]
fn each_source_refuses_the_same_posts_without_its_token() {
    let sql = RusqliteSql::new();
    let lineup = mint_ingest(&sql, "yahoo-lineup", SOURCES[0]);
    let waivers = mint_ingest(&sql, "yahoo-waivers", SOURCES[1]);
    for (source, other_sources_token) in [(SOURCES[0], &waivers), (SOURCES[1], &lineup)] {
        for (path, body) in [("/api/snapshots", snapshot(source)), ("/api/alerts", alert(source))] {
            let anon = req_anon(&sql, "POST", path, None, Some(&body));
            assert_eq!(anon.status, 401, "{source} {path} with no token");
            let device = req(&sql, "POST", path, None, Some(&body), 0);
            assert_eq!(device.status, 403, "{source} {path} with a device token");
            let crossed = req_as(&sql, other_sources_token, "POST", path, None, Some(&body), 0);
            assert_eq!(crossed.status, 403, "{source} {path} with the sibling source's token");
        }
    }
    assert!(sql.exec("SELECT source FROM context_snapshots", &[]).unwrap().is_empty());
    assert!(sql.exec("SELECT source FROM alerts", &[]).unwrap().is_empty());
}
