//! `POST|GET /api/admin/tokens`, `DELETE /api/admin/tokens/:id` — minting,
//! listing, revoking. The plaintext appears exactly once; the hash never
//! leaves the server; tokens never bump the workspace counter.

use hummingbird_domain::{MintedToken, TokenInfo};

use crate::rig::*;

#[test]
fn mint_201_returns_a_working_plaintext_and_stores_only_its_hash() {
    let sql = RusqliteSql::new();
    let resp = req_admin(
        &sql,
        "POST",
        "/api/admin/tokens",
        Some(r#"{"id": "t-pixel", "name": "pixel-9", "scope": "device"}"#),
        7000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let minted: MintedToken = body_as(&resp);
    assert!(minted.token.starts_with("hb_"), "{}", minted.token);
    assert_eq!(minted.created_at, 7000);

    let stored_hash = sql
        .exec("SELECT token_hash FROM tokens WHERE id = 't-pixel'", &[])
        .unwrap()[0]
        .get("token_hash")
        .unwrap()
        .as_text()
        .unwrap()
        .to_string();
    assert_eq!(stored_hash, sha256_hex(&minted.token), "at rest = sha256 of the plaintext");
    assert_ne!(stored_hash, minted.token, "never the plaintext itself");

    // And the minted token actually authenticates.
    let resp = req_as(&sql, &minted.token, "GET", "/api/changes", Some("since=0"), None, 0);
    assert_eq!(resp.status, 200, "the minted device token works end-to-end");
}

#[test]
fn mint_replay_returns_metadata_without_the_plaintext() {
    let sql = RusqliteSql::new();
    let body = r#"{"id": "t-1", "name": "pixel-9", "scope": "device"}"#;
    req_admin(&sql, "POST", "/api/admin/tokens", Some(body), 0);
    let resp = req_admin(&sql, "POST", "/api/admin/tokens", Some(body), 0);
    assert_eq!(resp.status, 200, "replay is success");
    assert!(
        !resp.body.contains("\"token\""),
        "the plaintext is unrecoverable by design: {}",
        resp.body
    );
    let info: TokenInfo = body_as(&resp);
    assert_eq!(info.name, "pixel-9");
    let rows = sql.exec("SELECT id FROM tokens WHERE id = 't-1'", &[]).unwrap();
    assert_eq!(rows.len(), 1, "no duplicate");
}

#[test]
fn mint_validation_400() {
    let sql = RusqliteSql::new();
    for (body, why) in [
        (r#"{"id": "", "name": "n", "scope": "device"}"#, "empty id"),
        (r#"{"id": "t", "name": "", "scope": "device"}"#, "empty name"),
        (r#"{"id": "t", "name": "n", "scope": "admin"}"#, "admin is not a scope"),
        (r#"{"id": "t", "name": "n"}"#, "missing scope"),
        (
            r#"{"id": "t", "name": "n", "scope": "ingest"}"#,
            "ingest without a source",
        ),
        (
            r#"{"id": "t", "name": "n", "scope": "device", "source": "hc"}"#,
            "device with a source",
        ),
        (
            r#"{"id": "t", "name": "n", "scope": "sweeper", "source": "hc"}"#,
            "sweeper with a source",
        ),
    ] {
        let resp = req_admin(&sql, "POST", "/api/admin/tokens", Some(body), 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
}

#[test]
fn mint_ingest_token_requires_and_stores_a_source() {
    let sql = RusqliteSql::new();
    let resp = req_admin(
        &sql,
        "POST",
        "/api/admin/tokens",
        Some(r#"{"id": "t-hc", "name": "healthchecks", "scope": "ingest", "source": "hc"}"#),
        0,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let minted: MintedToken = body_as(&resp);
    assert_eq!(minted.source, Some("hc".to_string()));

    let row_source = sql
        .exec("SELECT source FROM tokens WHERE id = 't-hc'", &[])
        .unwrap()[0]
        .get("source")
        .unwrap()
        .as_text()
        .map(str::to_string);
    assert_eq!(row_source, Some("hc".to_string()));
}

#[test]
fn a_null_source_token_remains_valid_for_non_ingest_scopes() {
    let sql = RusqliteSql::new();
    // The rig's device/sweeper tokens are seeded with a null source and
    // must keep authenticating and working exactly as before #145.
    let resp = req_as(&sql, DEVICE_TOKEN, "GET", "/api/changes", Some("since=0"), None, 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let resp = req_as(
        &sql,
        SWEEPER_TOKEN,
        "POST",
        "/api/items",
        None,
        Some(r#"{"id": "x", "title": "t"}"#),
        0,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
}

#[test]
fn list_returns_metadata_and_never_the_hashes() {
    let sql = RusqliteSql::new();
    req_admin(
        &sql,
        "POST",
        "/api/admin/tokens",
        Some(r#"{"id": "t-1", "name": "pixel-9", "scope": "device"}"#),
        0,
    );
    let resp = req_admin(&sql, "GET", "/api/admin/tokens", None, 0);
    assert_eq!(resp.status, 200);
    let tokens: Vec<TokenInfo> = body_as(&resp);
    assert_eq!(tokens.len(), 4, "three rig seeds + one minted");
    assert!(
        !resp.body.contains("token_hash") && !resp.body.contains(&sha256_hex(DEVICE_TOKEN)),
        "no hash material leaks: {}",
        resp.body
    );
}

#[test]
fn revoke_is_idempotent_and_unknown_404() {
    let sql = RusqliteSql::new();
    let resp = req_admin(&sql, "DELETE", "/api/admin/tokens/rig-sweeper", None, 4000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let revoked: TokenInfo = body_as(&resp);
    assert_eq!(revoked.revoked_at, Some(4000));

    let resp = req_admin(&sql, "DELETE", "/api/admin/tokens/rig-sweeper", None, 9000);
    assert_eq!(resp.status, 200, "re-revoke is a no-op success");
    let still: TokenInfo = body_as(&resp);
    assert_eq!(still.revoked_at, Some(4000), "the original stamp survives");

    let resp = req_admin(&sql, "DELETE", "/api/admin/tokens/ghost", None, 0);
    assert_eq!(resp.status, 404);
}

#[test]
fn token_writes_never_bump_the_workspace_counter() {
    let sql = RusqliteSql::new();
    let minted = req_admin(
        &sql,
        "POST",
        "/api/admin/tokens",
        Some(r#"{"id": "t-1", "name": "n", "scope": "ingest", "source": "hc"}"#),
        0,
    );
    assert_eq!(minted.status, 201, "{}", minted.body);
    req_admin(&sql, "DELETE", "/api/admin/tokens/t-1", None, 0);
    assert_eq!(
        meta_version(&sql),
        0,
        "tokens are machinery outside the delta contract"
    );
}

#[test]
fn admin_routes_wrong_method_405_and_unknown_404() {
    let sql = RusqliteSql::new();
    let resp = req_admin(&sql, "PATCH", "/api/admin/tokens", Some("{}"), 0);
    assert_eq!(resp.status, 405);
    let resp = req_admin(&sql, "POST", "/api/admin/tokens/t-1", Some("{}"), 0);
    assert_eq!(resp.status, 405);
    let resp = req_admin(&sql, "GET", "/api/admin/nope", None, 0);
    assert_eq!(resp.status, 404);
}
