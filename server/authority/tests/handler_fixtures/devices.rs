//! `GET /api/devices` and `DELETE /api/devices/:id` (ADR-0040 decision 6,
//! #838) — a `device` token lists the device-scope tokens and soft-revokes
//! one. Revoking your own is signing out; `runner` and `openclaw-agent` are
//! listed but refused; `sweeper` and `ingest` tokens are invisible here.

use hummingbird_domain::{DeviceInfo, DeviceListPlatform, MintedToken};

use crate::rig::*;

/// Mint through the admin lane and hand back the plaintext.
fn mint(sql: &dyn Sql, id: &str, scope: &str, now_ms: i64) -> String {
    let body = match scope {
        "ingest" => {
            format!(r#"{{"id": "{id}", "name": "{id} name", "scope": "ingest", "source": "city-waste/v2"}}"#)
        }
        _ => format!(r#"{{"id": "{id}", "name": "{id} name", "scope": "{scope}"}}"#),
    };
    let resp = req_admin(sql, "POST", "/api/admin/tokens", Some(&body), now_ms);
    assert_eq!(resp.status, 201, "{id}: {}", resp.body);
    body_as::<MintedToken>(&resp).token
}

fn list_as(sql: &dyn Sql, token: &str) -> Vec<DeviceInfo> {
    let resp = req_as(sql, token, "GET", "/api/devices", None, None, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    body_as(&resp)
}

fn ids(devices: &[DeviceInfo]) -> Vec<&str> {
    devices.iter().map(|d| d.id.as_str()).collect()
}

#[test]
fn the_list_holds_every_device_token_and_nothing_else() {
    let sql = RusqliteSql::new();
    let phone = mint(&sql, "device-android-pixel", "device", 10);
    mint(&sql, "runner", "device", 20);
    mint(&sql, "openclaw-agent", "device", 30);
    mint(&sql, "sweeper", "sweeper", 40);
    mint(&sql, "city-waste", "ingest", 50);
    // A revoked device is still a device: it lists, with its revoked_at.
    mint(&sql, "device-mac", "device", 60);
    req_admin(&sql, "DELETE", "/api/admin/tokens/device-mac", None, 70);

    let devices = list_as(&sql, &phone);
    let mut listed = ids(&devices);
    listed.sort_unstable();
    assert_eq!(
        listed,
        ["device-android-pixel", "device-mac", "openclaw-agent", "rig-device", "runner"],
        "every device-scope row, no sweeper or ingest token (the rig's own included)"
    );

    for d in &devices {
        assert_eq!(d.is_current, d.id == "device-android-pixel", "{}", d.id);
        assert_eq!(
            d.revocable,
            !matches!(d.id.as_str(), "runner" | "openclaw-agent"),
            "{}",
            d.id
        );
    }
    let mac = devices.iter().find(|d| d.id == "device-mac").unwrap();
    assert_eq!(mac.revoked_at, Some(70));
    assert_eq!(mac.name, "device-mac name");
    assert_eq!(mac.platform, DeviceListPlatform::Other, "a legacy id still reads");
    assert_eq!(mac.created_at, 60);
    let pixel = devices.iter().find(|d| d.id == "device-android-pixel").unwrap();
    assert_eq!(pixel.platform, DeviceListPlatform::Android);
    assert_eq!(pixel.last_seen, Some(0), "the listing request itself stamped it");
}

#[test]
fn the_list_is_device_scope_only() {
    let sql = RusqliteSql::new();
    for token in [SWEEPER_TOKEN, INGEST_TOKEN] {
        let resp = req_as(&sql, token, "GET", "/api/devices", None, None, 0);
        assert_eq!(resp.status, 403, "{}", resp.body);
        assert_eq!(resp.body, "");
    }
    assert_eq!(req_anon(&sql, "GET", "/api/devices", None, None).status, 401);
}

#[test]
fn the_wire_shape_is_the_eight_fields() {
    let sql = RusqliteSql::new();
    let resp = req(&sql, "GET", "/api/devices", None, None, 5);
    let rows: Vec<serde_json::Value> = body_as(&resp);
    let row = rows[0].as_object().unwrap();
    let mut keys: Vec<&str> = row.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "created_at", "id", "is_current", "last_seen", "name", "platform", "revocable",
            "revoked_at"
        ]
    );
    assert!(!resp.body.contains("hash"), "the hash never leaves: {}", resp.body);
    assert_eq!(row["platform"], "other");
}

#[test]
fn revoking_another_device_makes_its_next_request_a_401() {
    let sql = RusqliteSql::new();
    let laptop = mint(&sql, "device-web-laptop", "device", 0);
    assert_eq!(req_as(&sql, &laptop, "GET", "/api/sweep", None, None, 0).status, 200);

    let resp = req(&sql, "DELETE", "/api/devices/device-web-laptop", None, None, 900);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let revoked: DeviceInfo = body_as(&resp);
    assert_eq!(revoked.revoked_at, Some(900));
    assert!(!revoked.is_current);

    assert_eq!(req_as(&sql, &laptop, "GET", "/api/sweep", None, None, 1000).status, 401);
    assert_eq!(req(&sql, "GET", "/api/sweep", None, None, 1000).status, 200, "the caller is untouched");
}

#[test]
fn revoking_yourself_is_signing_out() {
    let sql = RusqliteSql::new();
    let resp = req(&sql, "DELETE", "/api/devices/rig-device", None, None, 400);
    assert_eq!(resp.status, 200, "the sign-out request itself succeeds: {}", resp.body);
    let me: DeviceInfo = body_as(&resp);
    assert!(me.is_current);
    assert_eq!(me.revoked_at, Some(400));

    let next = req(&sql, "GET", "/api/devices", None, None, 500);
    assert_eq!(next.status, 401);
    assert_eq!(next.body, "");
}

#[test]
fn repeating_a_revoke_is_a_no_op_success() {
    let sql = RusqliteSql::new();
    mint(&sql, "device-chrome-mac", "device", 0);
    let first = req(&sql, "DELETE", "/api/devices/device-chrome-mac", None, None, 100);
    assert_eq!(first.status, 200);
    let again = req(&sql, "DELETE", "/api/devices/device-chrome-mac", None, None, 200);
    assert_eq!(again.status, 200, "{}", again.body);
    let row: DeviceInfo = body_as(&again);
    assert_eq!(row.revoked_at, Some(100), "the first revoke's stamp stands");
    assert_eq!(first.body, again.body);
}

#[test]
fn runner_and_openclaw_agent_answer_403_and_stay_live() {
    let sql = RusqliteSql::new();
    let runner = mint(&sql, "runner", "device", 0);
    let agent = mint(&sql, "openclaw-agent", "device", 0);
    for id in ["runner", "openclaw-agent"] {
        let resp = req(&sql, "DELETE", &format!("/api/devices/{id}"), None, None, 100);
        assert_eq!(resp.status, 403, "{id}: {}", resp.body);
        assert_eq!(resp.body, "", "403 leaks no body");
    }
    for token in [&runner, &agent] {
        assert_eq!(req_as(&sql, token, "GET", "/api/sweep", None, None, 200).status, 200);
    }
    // Nor can either one revoke itself through this route.
    let resp = req_as(&sql, &runner, "DELETE", "/api/devices/runner", None, None, 300);
    assert_eq!(resp.status, 403);
}

#[test]
fn a_sweeper_or_ingest_id_or_an_unknown_one_answers_404() {
    let sql = RusqliteSql::new();
    for id in ["rig-sweeper", "rig-ingest", "no-such-token"] {
        let resp = req(&sql, "DELETE", &format!("/api/devices/{id}"), None, None, 100);
        assert_eq!(resp.status, 404, "{id}: {}", resp.body);
    }
    assert_eq!(
        req_as(&sql, SWEEPER_TOKEN, "POST", "/api/items", None, Some(r#"{"id": "a-1", "title": "t"}"#), 200)
            .status,
        201,
        "the sweeper is untouched"
    );
}

#[test]
fn tokens_never_bump_the_workspace_counter() {
    let sql = RusqliteSql::new();
    mint(&sql, "device-web-x", "device", 0);
    let before = meta_version(&sql);
    req(&sql, "GET", "/api/devices", None, None, 1);
    req(&sql, "DELETE", "/api/devices/device-web-x", None, None, 2);
    assert_eq!(meta_version(&sql), before);
}

#[test]
fn wrong_methods_are_405() {
    let sql = RusqliteSql::new();
    for (method, path) in [
        ("POST", "/api/devices"),
        ("DELETE", "/api/devices"),
        ("GET", "/api/devices/rig-device"),
        ("PATCH", "/api/devices/rig-device"),
    ] {
        let resp = req(&sql, method, path, None, Some("{}"), 0);
        assert_eq!(resp.status, 405, "{method} {path}: {}", resp.body);
    }
    assert_eq!(req(&sql, "DELETE", "/api/devices/", None, None, 0).status, 404);
}
