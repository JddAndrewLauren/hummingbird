//! #922: every alert says which app sent it and what it belongs to — the
//! two new optional ingest fields (`context`, `thread_key`) with their caps, and the push data map a real ingest produces. One test per
//! acceptance box that lives on the server.

use hummingbird_authority::{message_json, ApiResponse, DeliveryOutcome};
use hummingbird_domain::{Alert, MintedToken};

use crate::rig::*;

const SOURCE: &str = "auk/v1";
const KEY: &str = "mac:rec-1:awaiting";

fn token(sql: &dyn Sql) -> String {
    let body = r#"{"id": "auk", "name": "auk", "scope": "ingest", "source": "auk/v1",
        "allowed_origin": "https://factory.example"}"#;
    let resp = req_admin(sql, "POST", "/api/admin/tokens", Some(body), 0);
    assert_eq!(resp.status, 201, "{}", resp.body);
    body_as::<MintedToken>(&resp).token
}

/// A write with an arbitrary extra JSON fragment, e.g. `"context": "x"`.
fn body(extra: &str) -> String {
    let extra = if extra.is_empty() { String::new() } else { format!(", {extra}") };
    format!(
        r#"{{"source": "{SOURCE}", "source_key": "{KEY}", "title": "Waiting on you",
            "severity": "normal"{extra}}}"#
    )
}

fn post(sql: &dyn Sql, token: &str, body: &str, now_ms: i64) -> ApiResponse {
    req_as(sql, token, "POST", "/api/alerts", None, Some(body), now_ms)
}

fn stored(sql: &dyn Sql) -> Alert {
    let resp = sweep(sql);
    let changes: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    let alerts: Vec<Alert> = serde_json::from_value(changes["alerts"].clone()).unwrap();
    alerts.into_iter().find(|a| a.source_key == KEY).expect("the alert exists")
}

#[test]
fn context_and_thread_key_are_stored_and_a_later_write_without_them_clears_them() {
    let sql = RusqliteSql::new();
    let t = token(&sql);

    let first = post(&sql, &t, &body(r#""context": "hummingbird · out-412", "thread_key": "out-412""#), 10);
    assert_eq!(first.status, 201, "{}", first.body);
    let sent: Alert = body_as(&first);
    assert_eq!(sent.context.as_deref(), Some("hummingbird · out-412"));
    assert_eq!(sent.thread_key.as_deref(), Some("out-412"));
    let read = stored(&sql);
    assert_eq!(read.context.as_deref(), Some("hummingbird · out-412"), "the read path carries it");
    assert_eq!(read.thread_key.as_deref(), Some("out-412"));
    assert_eq!(read.thread().as_deref(), Some("auk/v1:out-412"), "scoped to the source");

    let changed = post(&sql, &t, &body(r#""context": "hummingbird · out-413", "thread_key": "out-412""#), 20);
    assert_eq!(changed.status, 200, "{}", changed.body);
    assert_eq!(stored(&sql).context.as_deref(), Some("hummingbird · out-413"));

    let cleared = post(&sql, &t, &body(""), 30);
    assert_eq!(cleared.status, 200, "{}", cleared.body);
    let read = stored(&sql);
    assert_eq!((&read.context, &read.thread_key), (&None, &None), "absent clears, like every other field");
    assert_eq!(read.thread(), None);
}

#[test]
fn a_payload_without_the_new_fields_is_accepted_as_before() {
    let sql = RusqliteSql::new();
    let t = token(&sql);
    let resp = post(&sql, &t, &body(""), 10);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let read = stored(&sql);
    assert_eq!((read.context, read.thread_key), (None, None));
}

fn refused_naming(field: &str, extra: &str) {
    let sql = RusqliteSql::new();
    let t = token(&sql);
    let resp = post(&sql, &t, &body(extra), 10);
    assert_eq!(resp.status, 400, "{field}: {}", resp.body);
    assert!(resp.body.contains(field), "the 400 names `{field}`: {}", resp.body);
    let swept: serde_json::Value = serde_json::from_str(&sweep(&sql).body).unwrap();
    assert_eq!(swept["alerts"].as_array().map(Vec::len), Some(0), "nothing was written");
}

#[test]
fn an_over_long_context_is_a_400_naming_the_field() {
    refused_naming("context", &format!(r#""context": "{}""#, "x".repeat(81)));
}

#[test]
fn an_over_long_thread_key_is_a_400_naming_the_field() {
    refused_naming("thread_key", &format!(r#""thread_key": "{}""#, "x".repeat(121)));
}

#[test]
fn an_empty_context_or_thread_key_is_a_400_naming_the_field() {
    refused_naming("context", r#""context": """#);
    refused_naming("thread_key", r#""thread_key": """#);
}

#[test]
fn the_caps_hold_exactly_at_the_limit_and_count_characters_not_bytes() {
    let sql = RusqliteSql::new();
    let t = token(&sql);
    // 80 two-byte characters: 160 bytes, 80 characters.
    let context = "é".repeat(80);
    let payload = format!(
        r#"{{"source": "{SOURCE}", "source_key": "{KEY}", "title": "{}", "body": "{}",
            "context": "{context}", "thread_key": "{}", "severity": "normal"}}"#,
        "t".repeat(500),
        "b".repeat(5000),
        "k".repeat(120)
    );
    let resp = post(&sql, &t, &payload, 10);
    assert_eq!(resp.status, 201, "{}", resp.body);
}

/// The push data a real ingest produces: the registry's name and icon, the
/// context, and the thread scoped to the source — and nothing of the last
/// two when the alert carries neither.
#[test]
fn the_push_a_real_ingest_rings_carries_name_icon_context_and_thread() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel");
    let rule = post_rule(
        &sql,
        r#"{"id": "r", "name": "r", "event_kind": "alert_raised",
            "conditions": [{"field": "source", "op": "eq", "value": "auk/v1", "negate": false}],
            "severity": "normal", "tier": "normal"}"#,
        0,
    );
    assert!(rule.status == 201 || rule.status == 200, "{}", rule.body);
    let t = token(&sql);

    let with = post(&sql, &t, &body(r#""context": "hummingbird · out-412", "thread_key": "out-412""#), 10);
    let DeliveryOutcome::Logged { notification, .. } = with.deliveries[0].clone() else {
        panic!("rang: {:?}", with.deliveries)
    };
    let data: serde_json::Value =
        serde_json::from_str(&message_json(&notification, "tok")).unwrap();
    let data = &data["message"]["data"];
    assert_eq!(data["source_name"], "Auk");
    assert_eq!(data["source_icon"], "auk");
    assert_eq!(data["context"], "hummingbird · out-412");
    assert_eq!(data["thread"], "auk/v1:out-412");

    // A fresh occurrence with neither field: the keys are absent, not empty.
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel");
    post_rule(
        &sql,
        r#"{"id": "r", "name": "r", "event_kind": "alert_raised",
            "conditions": [{"field": "source", "op": "eq", "value": "auk/v1", "negate": false}],
            "severity": "normal", "tier": "normal"}"#,
        0,
    );
    let t = token(&sql);
    let without = post(&sql, &t, &body(""), 10);
    let DeliveryOutcome::Logged { notification, .. } = without.deliveries[0].clone() else {
        panic!("rang: {:?}", without.deliveries)
    };
    let data: serde_json::Value =
        serde_json::from_str(&message_json(&notification, "tok")).unwrap();
    let data = data["message"]["data"].as_object().unwrap();
    assert!(!data.contains_key("context") && !data.contains_key("thread"), "{data:?}");
    assert_eq!(data["source_name"], "Auk");
}
