//! #893: `healthchecks/v1`, a Healthchecks.io check that stops checking in,
//! end to end through the real mint route, the Healthchecks receiver
//! (`POST /api/alerts/healthchecks`) and the one rule that makes it ring —
//! the in-process twin of the operator's after-merge mint, integration and
//! rule.
//!
//! Every event below is the documented webhook body
//! (`handlers/healthchecks.rs`'s header) with its placeholders filled the
//! way Healthchecks.io fills them, so the template an operator pastes is
//! the template under test.

use hummingbird_authority::{ApiResponse, DeliveryOutcome};
use hummingbird_domain::{Alert, MintedToken, Tier};

use crate::rig::*;

const SOURCE: &str = "healthchecks/v1";
const PATH: &str = "/api/alerts/healthchecks";
const GUARDIAN: &str = "2a7c1d84-5e0f-4b6a-9c3e-8f1d2b7a6c50";
const BEARING: &str = "9e4b3f21-0c6d-47a8-b5e2-1d8c7f0a3b94";

/// The documented request body, verbatim — the same text on the down and
/// the up event.
const TEMPLATE: &str = r#"{"check": "$CODE", "name": $NAME_JSON, "status": "$STATUS"}"#;

/// Healthchecks.io's own substitution (`hc/integrations/webhook/transport.py`):
/// `$CODE` is the check's UUID, `$NAME_JSON` its name through `json.dumps`,
/// `$STATUS` the flip's new status.
fn render(code: &str, name: &str, status: &str) -> String {
    TEMPLATE
        .replace("$CODE", code)
        .replace("$NAME_JSON", &serde_json::to_string(name).unwrap())
        .replace("$STATUS", status)
}

fn hc_token(sql: &dyn Sql) -> String {
    let body = format!(
        r#"{{"id": "healthchecks-ingest", "name": "healthchecks-ingest", "scope": "ingest",
            "source": "{SOURCE}"}}"#
    );
    let resp = req_admin(sql, "POST", "/api/admin/tokens", Some(&body), 0);
    assert_eq!(resp.status, 201, "{SOURCE} mints without an origin: {}", resp.body);
    body_as::<MintedToken>(&resp).token
}

/// The after-merge rule `healthchecks-high-urgent`, as the brief names it.
fn seed_rule(sql: &dyn Sql) {
    let resp = post_rule(
        sql,
        &format!(
            r#"{{"id": "healthchecks-high-urgent", "name": "healthchecks-high-urgent",
                "event_kind": "alert_raised",
                "conditions": [
                    {{"field": "source", "op": "eq", "value": "{SOURCE}", "negate": false}},
                    {{"field": "severity", "op": "eq", "value": "high", "negate": false}}],
                "severity": "high", "tier": "urgent"}}"#
        ),
        0,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
}

fn event(sql: &dyn Sql, token: &str, code: &str, name: &str, status: &str, now: i64) -> ApiResponse {
    req_as(sql, token, "POST", PATH, None, Some(&render(code, name, status)), now)
}

/// The tier of every delivery an event logged, in rule order.
fn rang(resp: &ApiResponse) -> Vec<Tier> {
    resp.deliveries
        .iter()
        .filter_map(|outcome| match outcome {
            DeliveryOutcome::Logged { notification, .. } => Some(notification.tier),
            DeliveryOutcome::Suppressed(_) => None,
        })
        .collect()
}

fn rig() -> (RusqliteSql, String) {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_rule(&sql);
    let token = hc_token(&sql);
    (sql, token)
}

/// Box 1: a down event raises one `healthchecks/v1` alert keyed by the
/// check, titled with its name, carrying a severity — and it rings.
#[test]
fn a_down_event_raises_one_alert_keyed_by_the_check_and_titled_with_its_name() {
    let (sql, token) = rig();

    let down = event(&sql, &token, GUARDIAN, "Guardian on osprey", "down", 1000);
    assert_eq!(down.status, 201, "{}", down.body);
    let alert: Alert = body_as(&down);
    assert_eq!(alert.source, SOURCE);
    assert_eq!(alert.source_key, GUARDIAN, "keyed by the check's UUID");
    assert_eq!(alert.title, "Guardian on osprey", "titled with the check's name");
    assert_eq!(alert.severity.as_deref(), Some("high"));
    assert_eq!(alert.raised_at, 1000);
    assert!(alert.is_live(1000));
    assert_eq!(rang(&down), [Tier::Urgent], "rings once, at urgent tier");

    // Healthchecks.io retries a failed delivery: the same down again is
    // one alert, and silent.
    let again = event(&sql, &token, GUARDIAN, "Guardian on osprey", "down", 1500);
    assert_eq!(again.status, 200, "{}", again.body);
    assert!(rang(&again).is_empty(), "a repeated down rings nothing");
    assert_eq!(body_as::<Alert>(&again).raised_at, 1000, "and keeps its raise time");
    assert_eq!(sql.exec("SELECT id FROM alerts", &[]).unwrap().len(), 1);
}

/// A check whose name contains quotes still lands, because `$NAME_JSON`
/// escapes it; an unnamed check is titled by its UUID rather than refused,
/// so it still rings.
#[test]
fn an_awkward_or_empty_name_still_raises() {
    let (sql, token) = rig();

    let quoted = event(&sql, &token, GUARDIAN, r#"Guardian "beta" on osprey"#, "down", 1000);
    assert_eq!(quoted.status, 201, "{}", quoted.body);
    assert_eq!(body_as::<Alert>(&quoted).title, r#"Guardian "beta" on osprey"#);

    let unnamed = event(&sql, &token, BEARING, "", "down", 1000);
    assert_eq!(unnamed.status, 201, "{}", unnamed.body);
    assert_eq!(body_as::<Alert>(&unnamed).title, BEARING, "titled by the check's UUID");
    assert_eq!(rang(&unnamed), [Tier::Urgent]);
}

/// Box 2: an up event resolves the alert; a later down re-raises it with a
/// new raise time, and that rings again.
#[test]
fn up_resolves_and_a_later_down_re_raises_with_a_new_raise_time_that_rings_again() {
    let (sql, token) = rig();
    event(&sql, &token, GUARDIAN, "Guardian on osprey", "down", 1000);

    let up = event(&sql, &token, GUARDIAN, "Guardian on osprey", "up", 2000);
    assert_eq!(up.status, 200, "{}", up.body);
    let resolved: Alert = body_as(&up);
    assert_eq!(resolved.resolved_at, Some(2000));
    assert!(!resolved.is_live(2000), "the alert leaves live");
    assert_eq!(resolved.title, "Guardian on osprey", "a resolve touches nothing else");
    assert!(rang(&up).is_empty(), "an up rings nothing");

    // A retried up keeps the first resolution.
    let again = event(&sql, &token, GUARDIAN, "Guardian on osprey", "up", 2500);
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(body_as::<Alert>(&again).resolved_at, Some(2000));

    let down = event(&sql, &token, GUARDIAN, "Guardian on osprey", "down", 3000);
    assert_eq!(down.status, 200, "{}", down.body);
    let reraised: Alert = body_as(&down);
    assert_eq!(reraised.id, resolved.id, "the same alert, not a second one");
    assert_eq!(reraised.raised_at, 3000, "a new raise time");
    assert_eq!(reraised.resolved_at, None);
    assert!(reraised.is_live(3000));
    assert_eq!(rang(&down), [Tier::Urgent], "and it rings again");

    let generations = sql
        .exec("SELECT generation FROM deliveries ORDER BY generation", &[])
        .unwrap()
        .iter()
        .map(|row| row.get("generation").unwrap().as_i64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(generations, [1000, 3000], "one ring per occurrence");
}

/// An alert the operator dismissed while the check was down still rings
/// when the check goes down again after coming back up — the up resolves a
/// dismissed alert too.
#[test]
fn a_dismissed_alert_rings_again_after_an_up_and_a_down() {
    let (sql, token) = rig();
    let down = event(&sql, &token, GUARDIAN, "Guardian on osprey", "down", 1000);
    let alert: Alert = body_as(&down);
    let dismiss = req_as(
        &sql,
        DEVICE_TOKEN,
        "PATCH",
        &format!("/api/alerts/{}", alert.id),
        None,
        Some(&format!(r#"{{"expected_version": {}, "dismissed_at": 1200}}"#, alert.version)),
        1200,
    );
    assert_eq!(dismiss.status, 200, "{}", dismiss.body);

    let up = event(&sql, &token, GUARDIAN, "Guardian on osprey", "up", 2000);
    assert_eq!(body_as::<Alert>(&up).resolved_at, Some(2000));

    let down = event(&sql, &token, GUARDIAN, "Guardian on osprey", "down", 3000);
    assert!(body_as::<Alert>(&down).is_live(3000), "the new raise overtakes the dismissal");
    assert_eq!(rang(&down), [Tier::Urgent]);
}

/// Box 3: a second check's events raise a separate alert, under the same
/// token and the same rule — nothing in Hummingbird changes for it.
#[test]
fn a_second_check_raises_a_separate_alert_with_no_change_here() {
    let (sql, token) = rig();

    let guardian = event(&sql, &token, GUARDIAN, "Guardian on osprey", "down", 1000);
    let bearing = event(&sql, &token, BEARING, "Bearing on kestrel", "down", 1100);
    assert_eq!(bearing.status, 201, "{}", bearing.body);
    assert_eq!(rang(&bearing), [Tier::Urgent], "the same rule rings for it");
    let (guardian, bearing): (Alert, Alert) = (body_as(&guardian), body_as(&bearing));
    assert_ne!(guardian.id, bearing.id);
    assert_eq!(bearing.source_key, BEARING);
    assert_eq!(bearing.title, "Bearing on kestrel");

    // One check's up resolves its own alert only.
    event(&sql, &token, BEARING, "Bearing on kestrel", "up", 2000);
    let rows = sql.exec("SELECT source_key, resolved_at FROM alerts ORDER BY source_key", &[]);
    let rows = rows.unwrap();
    let resolved: Vec<_> = rows
        .iter()
        .map(|row| {
            let key = row.get("source_key").unwrap().as_text().unwrap().to_string();
            (key, row.get("resolved_at").unwrap().as_i64())
        })
        .collect();
    assert_eq!(resolved, [(GUARDIAN.to_string(), None), (BEARING.to_string(), Some(2000))]);
}

/// An up for a check that never went down here (the integration was added
/// while it was already down, or the down was lost) writes nothing.
#[test]
fn an_up_for_an_unknown_check_writes_nothing() {
    let (sql, token) = rig();
    let before = meta_version(&sql);
    let up = event(&sql, &token, GUARDIAN, "Guardian on osprey", "up", 2000);
    assert_eq!(up.status, 204, "{}", up.body);
    assert!(sql.exec("SELECT id FROM alerts", &[]).unwrap().is_empty());
    assert_eq!(meta_version(&sql), before, "no write at all");
}

/// The receiver is the ingest lane's and `healthchecks/v1`'s alone: a token
/// bound to another source, and a device token, are both refused; a status
/// Healthchecks.io never sends, and a body missing the check, are 400s.
#[test]
fn the_receiver_takes_only_a_healthchecks_token_and_its_two_statuses() {
    let (sql, token) = rig();
    let body = render(GUARDIAN, "Guardian on osprey", "down");

    bind_ingest_token(&sql, "home-assistant/v1");
    let other = req_as(&sql, INGEST_TOKEN, "POST", PATH, None, Some(&body), 1000);
    assert_eq!(other.status, 403, "another source's token: {}", other.body);
    let device = req_as(&sql, DEVICE_TOKEN, "POST", PATH, None, Some(&body), 1000);
    assert_eq!(device.status, 403, "a device token: {}", device.body);

    let paused = event(&sql, &token, GUARDIAN, "Guardian on osprey", "paused", 1000);
    assert_eq!(paused.status, 400, "{}", paused.body);
    assert!(paused.body.contains("paused"), "names the status: {}", paused.body);
    let no_check = event(&sql, &token, "", "Guardian on osprey", "down", 1000);
    assert_eq!(no_check.status, 400, "{}", no_check.body);

    assert!(sql.exec("SELECT id FROM alerts", &[]).unwrap().is_empty(), "nothing was written");
    let get = req_as(&sql, &token, "GET", PATH, None, None, 1000);
    assert_eq!(get.status, 403, "only POST is in the ingest scope: {}", get.body);
}
