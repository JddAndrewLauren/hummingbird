//! `POST /api/auth/google` (ADR-0040, #837) — a Google sign-in mints a
//! `device` token. Driven exactly as the worker shim drives it: `begin`,
//! the JWKS key for the token's `kid`, the signature verdict, `complete`.
//! The two runtime steps are stood in for — the JWKS by the fixture set,
//! `crypto.subtle.verify` by a `bool` — so every token here except the
//! fixture's own is unsigned; the fixture's real RS256 signature is
//! verified through WebCrypto by `server/worker/tests/rs256-import.test.mjs`.

use hummingbird_authority::auth_google::{
    begin, complete, jwks_from_response, jwks_unavailable, Refusal,
};
use hummingbird_authority::{ApiResponse, HandleContext, JwksCache, SigninConfig, SqlValue};
use hummingbird_domain::{DeviceListPlatform, DevicePlatform, SignedInDevice, TokenInfo};

use crate::rig::*;

const FIXTURE: &str = include_str!("../../fixtures/google-signin-rs256.json");
const CLIENT_ID: &str = "fixture-client.apps.googleusercontent.com";
const OPERATOR_SUB: &str = "110000000000000000001";
/// One minute after the fixture token's `iat`.
const NOW_MS: i64 = 1_760_000_060_000;

fn fixture() -> serde_json::Value {
    serde_json::from_str(FIXTURE).unwrap()
}

fn config() -> SigninConfig {
    SigninConfig::from_env(Some(CLIENT_ID), Some(OPERATOR_SUB)).unwrap()
}

fn jwks() -> JwksCache {
    JwksCache::parse(&fixture()["jwks"].to_string(), Some("max-age=3600"), NOW_MS).unwrap()
}

fn b64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = |i: usize| *chunk.get(i).unwrap_or(&0) as u32;
        let n = (b(0) << 16) | (b(1) << 8) | b(2);
        for k in 0..=chunk.len() {
            out.push(ALPHABET[((n >> (18 - 6 * k)) & 63) as usize] as char);
        }
    }
    out
}

fn good_claims() -> serde_json::Value {
    serde_json::json!({
        "iss": "https://accounts.google.com",
        "aud": CLIENT_ID,
        "sub": OPERATOR_SUB,
        "email": "operator@example.com",
        "email_verified": true,
        "iat": 1_760_000_000,
        "exp": 1_760_003_600,
    })
}

/// An unsigned token: claims are checked before the signature, and the
/// signature verdict is the driver's `bool`.
fn token(header: serde_json::Value, claims: serde_json::Value) -> String {
    format!(
        "{}.{}.{}",
        b64url(header.to_string().as_bytes()),
        b64url(claims.to_string().as_bytes()),
        b64url(b"not-a-real-signature"),
    )
}

fn good_header() -> serde_json::Value {
    serde_json::json!({"alg": "RS256", "kid": "fixture-kid-1", "typ": "JWT"})
}

fn body(id_token: &str, name: &str, platform: &str) -> String {
    serde_json::json!({"id_token": id_token, "name": name, "platform": platform}).to_string()
}

/// What the route answers, and the line the shim would log.
struct Outcome {
    response: ApiResponse,
    log: Option<String>,
}

/// The shim's sequence, with its two runtime steps stood in for.
fn sign_in_with(
    sql: &dyn Sql,
    config: Option<&SigninConfig>,
    method: &str,
    body: Option<&str>,
    signature_ok: bool,
    now_ms: i64,
) -> Outcome {
    let refused = |r: Refusal| Outcome {
        response: r.response,
        log: r.log,
    };
    let pending = match begin(method, body, now_ms, config) {
        Ok(pending) => pending,
        Err(r) => return refused(r),
    };
    let jwks = jwks();
    if let Err(r) = pending.key(&jwks) {
        return refused(r);
    }
    let signed_in = match pending.signature_checked(signature_ok) {
        Ok(signed_in) => signed_in,
        Err(r) => return refused(r),
    };
    let ctx = HandleContext {
        now_ms,
        admin_secret: None,
        entropy: &TestEntropy,
    };
    Outcome {
        response: complete(signed_in, &ctx, sql),
        log: None,
    }
}

/// A good sign-in at `now_ms`: a fresh token, as a device would hold one.
fn sign_in(sql: &dyn Sql, name: &str, platform: &str, now_ms: i64) -> ApiResponse {
    let mut claims = good_claims();
    claims["iat"] = (now_ms / 1000 - 60).into();
    claims["exp"] = (now_ms / 1000 + 3540).into();
    let body = body(&token(good_header(), claims), name, platform);
    sign_in_with(sql, Some(&config()), "POST", Some(&body), true, now_ms).response
}

fn token_count(sql: &dyn Sql) -> usize {
    sql.exec("SELECT id FROM tokens", &[]).unwrap().len()
}

// ------------------------------------------------------------ rejections

/// Acceptance: each claim rejection (wrong iss, wrong aud, expired, stale
/// iat, unverified email, other sub, alg ≠ RS256, unknown kid) yields 403
/// and mints nothing — plus a bad signature and a malformed token.
#[test]
fn every_rejection_is_an_empty_403_that_mints_nothing_and_logs_the_sub() {
    let claims_with = |key: &str, value: serde_json::Value| {
        let mut claims = good_claims();
        claims[key] = value;
        claims
    };
    let header_with = |key: &str, value: serde_json::Value| {
        let mut header = good_header();
        header[key] = value;
        header
    };
    let cases: Vec<(&str, String, bool, &str)> = vec![
        (
            "wrong iss",
            token(
                good_header(),
                claims_with("iss", "https://evil.example".into()),
            ),
            true,
            "iss is not Google",
        ),
        (
            "wrong aud",
            token(good_header(), claims_with("aud", "someone-else".into())),
            true,
            "aud is not",
        ),
        (
            "expired",
            token(good_header(), claims_with("exp", 1_760_000_000.into())),
            true,
            "exp has passed",
        ),
        (
            "stale iat",
            token(good_header(), claims_with("iat", 1_759_999_000.into())),
            true,
            "iat is over",
        ),
        (
            "unverified email",
            token(good_header(), claims_with("email_verified", false.into())),
            true,
            "email_verified",
        ),
        (
            "other sub",
            token(good_header(), claims_with("sub", "999".into())),
            true,
            "sub is not",
        ),
        (
            "alg HS256",
            token(header_with("alg", "HS256".into()), good_claims()),
            true,
            "alg is not RS256",
        ),
        (
            "alg none",
            token(header_with("alg", "none".into()), good_claims()),
            true,
            "alg is not RS256",
        ),
        (
            "unknown kid",
            token(header_with("kid", "rotated-away".into()), good_claims()),
            true,
            "kid is not in",
        ),
        (
            "bad signature",
            token(good_header(), good_claims()),
            false,
            "signature does not verify",
        ),
    ];
    for (why, id_token, signature_ok, reason) in cases {
        let sql = RusqliteSql::new();
        let before = token_count(&sql);
        let out = sign_in_with(
            &sql,
            Some(&config()),
            "POST",
            Some(&body(&id_token, "Pixel 9", "android")),
            signature_ok,
            NOW_MS,
        );
        assert_eq!(out.response.status, 403, "{why}: {}", out.response.body);
        assert_eq!(out.response.body, "", "{why}: a 403 leaks no body");
        assert_eq!(token_count(&sql), before, "{why}: nothing minted");
        let log = out
            .log
            .unwrap_or_else(|| panic!("{why}: a rejection is logged"));
        assert!(log.contains(reason), "{why}: {log}");
        let logged_sub = if why == "other sub" {
            "999"
        } else {
            OPERATOR_SUB
        };
        assert!(log.ends_with(&format!("sub={logged_sub}")), "{why}: {log}");
    }
}

/// Acceptance: the rejected `sub` comes from a token whose signature was
/// never checked, so the caller controls it. It reaches the log escaped —
/// one line, whatever it holds — and cut to a bounded length with a marker.
#[test]
fn a_rejected_sub_is_logged_escaped_and_capped() {
    let sql = RusqliteSql::new();
    let refused_sub = |sub: String| {
        let mut claims = good_claims();
        claims["sub"] = sub.into();
        let body = body(&token(good_header(), claims), "Pixel 9", "android");
        let out = sign_in_with(&sql, Some(&config()), "POST", Some(&body), true, NOW_MS);
        assert_eq!(out.response.status, 403);
        out.log.unwrap()
    };

    let log = refused_sub(format!(
        "evil\nsub={OPERATOR_SUB}\u{1b}[2J{}",
        "x".repeat(500)
    ));
    assert!(!log.contains('\n') && !log.contains('\u{1b}'), "{log:?}");
    assert!(log.contains("sub is not the allowed subject"), "{log}");
    assert!(
        log.contains(r"sub=evil\nsub=110000000000000000001\u{1b}[2J"),
        "{log}"
    );
    assert!(log.ends_with("...[cut]"), "{log}");
    assert!(log.len() < 160, "bounded: {} bytes", log.len());

    // A `sub` inside the cap is logged whole, with no marker.
    assert!(refused_sub("999\r".into()).ends_with(r"sub=999\r"));
}

/// Acceptance: an unknown `kid` inside the re-fetch interval is refused
/// from the cache the shim holds — the same 403 and log line as a `kid`
/// missing from a fresh fetch — and only after the interval asks for a
/// fetch. A known `kid` is answered from the cache; no cache asks for one.
#[test]
fn an_unknown_kid_inside_the_interval_is_refused_without_a_fetch() {
    let pending = |kid: &str| {
        let mut header = good_header();
        header["kid"] = kid.into();
        let body = body(&token(header, good_claims()), "Pixel 9", "android");
        begin("POST", Some(&body), NOW_MS, Some(&config())).unwrap()
    };
    let jwks = jwks(); // fetched at NOW_MS
    let refusal = pending("made-up")
        .cached_key(Some(&jwks), NOW_MS + 59_999)
        .unwrap_err();
    assert_eq!(refusal.response.status, 403);
    assert_eq!(refusal.response.body, "");
    let log = refusal.log.unwrap();
    assert!(log.contains("kid is not in"), "{log}");
    assert!(log.ends_with(&format!("sub={OPERATOR_SUB}")), "{log}");

    assert_eq!(
        pending("made-up")
            .cached_key(Some(&jwks), NOW_MS + 60_000)
            .unwrap(),
        None,
        "after the interval: fetch"
    );
    assert!(pending("fixture-kid-1")
        .cached_key(Some(&jwks), NOW_MS + 60_000)
        .unwrap()
        .is_some());
    assert_eq!(
        pending("fixture-kid-1").cached_key(None, NOW_MS).unwrap(),
        None,
        "nothing cached yet: fetch"
    );
}

#[test]
fn a_malformed_token_is_a_403_that_logs_no_sub() {
    let sql = RusqliteSql::new();
    for id_token in ["", "not-a-jwt", "a.b.c"] {
        let out = sign_in_with(
            &sql,
            Some(&config()),
            "POST",
            Some(&body(id_token, "Pixel 9", "android")),
            true,
            NOW_MS,
        );
        assert_eq!(out.response.status, 403, "{id_token:?}");
        assert!(out.log.unwrap().ends_with("sub=-"));
    }
    assert_eq!(token_count(&sql), 3, "only the rig's seeded tokens");
}

/// The fixture token is a real RS256 JWT: it passes every claim check (at
/// its own `iat`), and its `kid` resolves in the fixture JWKS to exactly
/// the SPKI DER openssl produced for the key that signed it — the bytes
/// the shim hands `crypto.subtle.importKey("spki", …)`.
#[test]
fn the_fixture_token_reaches_the_signature_check_with_openssls_spki() {
    let fixture = fixture();
    let body = body(fixture["id_token"].as_str().unwrap(), "Pixel 9", "android");
    let pending = begin("POST", Some(&body), NOW_MS, Some(&config())).expect("claims pass");
    assert_eq!(pending.kid, "fixture-kid-1");
    let jwks = jwks();
    let spki = pending.key(&jwks).expect("the kid is in the fixture JWKS");
    let expected = fixture["spki_der_base64"].as_str().unwrap();
    assert_eq!(b64_standard(spki), expected);
    assert_eq!(
        format!("{}.{}", pending.signing_input, b64url(&pending.signature)),
        fixture["id_token"].as_str().unwrap(),
    );
}

fn b64_standard(bytes: &[u8]) -> String {
    let mut s = b64url(bytes).replace('-', "+").replace('_', "/");
    while !s.len().is_multiple_of(4) {
        s.push('=');
    }
    s
}

// ------------------------------------------------------------- minting

/// Acceptance: a valid identity mints a `device` token that then
/// authenticates `GET /api/changes`.
#[test]
fn a_valid_identity_mints_a_device_token_that_authenticates() {
    let sql = RusqliteSql::new();
    let resp = sign_in(&sql, "Pixel 9 Pro Fold", "android", NOW_MS);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let minted: SignedInDevice = body_as(&resp);
    assert_eq!(minted.id, "device-android-pixel-9-pro-fold");
    assert!(
        minted.token.starts_with("hb_") && minted.token.len() == 67,
        "{}",
        minted.token
    );
    let fields: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    assert_eq!(
        fields.as_object().unwrap().len(),
        2,
        "exactly {{id, token}}: {}",
        resp.body
    );

    let row = &sql
        .exec(
            "SELECT * FROM tokens WHERE id = ?",
            &[SqlValue::Text(minted.id.clone())],
        )
        .unwrap()[0];
    assert_eq!(row.get("scope").unwrap().as_text(), Some("device"));
    assert_eq!(row.get("name").unwrap().as_text(), Some("Pixel 9 Pro Fold"));
    assert_eq!(row.get("source"), Some(&SqlValue::Null));
    assert_eq!(
        row.get("token_hash").unwrap().as_text(),
        Some(sha256_hex(&minted.token).as_str())
    );

    let resp = req_as(
        &sql,
        &minted.token,
        "GET",
        "/api/changes",
        Some("since=0"),
        None,
        NOW_MS,
    );
    assert_eq!(
        resp.status, 200,
        "the signed-in device token works end to end"
    );
    assert_eq!(
        resp.principal_id.as_deref(),
        Some("device-android-pixel-9-pro-fold")
    );
}

/// Acceptance: a replayed name gets a `-2` id; an id held by a revoked row
/// is never reused.
#[test]
fn a_replayed_name_gets_the_next_suffix_and_a_revoked_id_is_never_reused() {
    let sql = RusqliteSql::new();
    let id = |resp: ApiResponse| {
        assert_eq!(resp.status, 201, "{}", resp.body);
        body_as::<SignedInDevice>(&resp).id
    };
    assert_eq!(
        id(sign_in(&sql, "MacBook", "web", NOW_MS)),
        "device-web-macbook"
    );
    assert_eq!(
        id(sign_in(&sql, "MacBook", "web", NOW_MS)),
        "device-web-macbook-2"
    );

    let resp = req_admin(
        &sql,
        "DELETE",
        "/api/admin/tokens/device-web-macbook-2",
        None,
        NOW_MS,
    );
    assert!(body_as::<TokenInfo>(&resp).revoked_at.is_some());
    assert_eq!(
        id(sign_in(&sql, "MacBook", "web", NOW_MS)),
        "device-web-macbook-3"
    );

    // The same name on another platform is another id altogether.
    assert_eq!(
        id(sign_in(&sql, "MacBook", "chrome", NOW_MS)),
        "device-chrome-macbook"
    );
}

/// Any existing row blocks an id, not only this route's: an admin-minted
/// token that happens to hold the derived id is skipped too.
#[test]
fn an_admin_minted_id_is_never_taken_over() {
    let sql = RusqliteSql::new();
    req_admin(
        &sql,
        "POST",
        "/api/admin/tokens",
        Some(r#"{"id": "device-chrome-studio", "name": "studio", "scope": "device"}"#),
        0,
    );
    let resp = sign_in(&sql, "Studio", "chrome", NOW_MS);
    assert_eq!(
        body_as::<SignedInDevice>(&resp).id,
        "device-chrome-studio-2"
    );
}

/// Acceptance: the eleventh mint inside an hour answers 429.
#[test]
fn the_eleventh_mint_inside_an_hour_is_a_429() {
    let sql = RusqliteSql::new();
    for i in 0..10 {
        let resp = sign_in(&sql, "Pixel", "android", NOW_MS + i * 60_000);
        assert_eq!(resp.status, 201, "mint {}: {}", i + 1, resp.body);
    }
    let before = token_count(&sql);
    let resp = sign_in(&sql, "Pixel", "android", NOW_MS + 30 * 60_000);
    assert_eq!(resp.status, 429, "{}", resp.body);
    assert_eq!(token_count(&sql), before, "a capped mint writes nothing");

    // Rolling: an hour after the first mint, one slot has opened.
    let first_mint_plus_an_hour = NOW_MS + 3_600_000;
    let resp = sign_in(&sql, "Pixel", "android", first_mint_plus_an_hour);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let resp = sign_in(&sql, "Pixel", "android", first_mint_plus_an_hour);
    assert_eq!(
        resp.status, 429,
        "the second mint of the hour is still capped"
    );
}

/// The cap counts this route's id shape, revoked rows included, and
/// nothing else in the tokens table.
#[test]
fn the_cap_counts_revoked_mints_but_not_other_tokens() {
    let sql = RusqliteSql::new();
    for i in 0..9 {
        req_admin(
            &sql,
            "POST",
            "/api/admin/tokens",
            Some(&format!(
                r#"{{"id": "runner-{i}", "name": "n", "scope": "device"}}"#
            )),
            NOW_MS,
        );
    }
    for _ in 0..10 {
        let resp = sign_in(&sql, "Pixel", "android", NOW_MS);
        let id = body_as::<SignedInDevice>(&resp).id;
        req_admin(
            &sql,
            "DELETE",
            &format!("/api/admin/tokens/{id}"),
            None,
            NOW_MS,
        );
    }
    assert_eq!(sign_in(&sql, "Pixel", "android", NOW_MS).status, 429);
}

/// Acceptance: the id shape has three readers — the mint that builds it,
/// the cap that counts it and the device list that reads its platform back
/// — and they agree. For every platform, a named and a bare (nothing
/// sluggable) id, each with its `-2` replay, reads back as that platform
/// and counts toward the cap: those four mints and six more leave the
/// eleventh capped, which would mint if any of the four went uncounted.
#[test]
fn every_minted_id_shape_is_capped_and_listed_as_its_platform() {
    for platform in DevicePlatform::ALL {
        let want = match platform {
            DevicePlatform::Web => DeviceListPlatform::Web,
            DevicePlatform::Android => DeviceListPlatform::Android,
            DevicePlatform::Chrome => DeviceListPlatform::Chrome,
        };
        let p = platform.as_str();
        let sql = RusqliteSql::new();
        let mint = |name: &str| {
            let resp = sign_in(&sql, name, p, NOW_MS);
            assert_eq!(resp.status, 201, "{p} {name}: {}", resp.body);
            body_as::<SignedInDevice>(&resp).id
        };
        let ids = ["Box", "Box", "!!!", "!!!"].map(mint);
        assert_eq!(
            ids,
            [
                format!("device-{p}-box"),
                format!("device-{p}-box-2"),
                format!("device-{p}"),
                format!("device-{p}-2"),
            ],
        );
        for id in &ids {
            assert_eq!(DeviceListPlatform::of_token_id(id), want, "{id}");
        }
        for i in 0..6 {
            mint(&format!("Filler {i}"));
        }
        assert_eq!(sign_in(&sql, "Last", p, NOW_MS).status, 429, "{p}");
    }
}

// ------------------------------------------------- config, body, method

/// Acceptance: with either config value unset the route answers 503 —
/// before reading the body, and without minting.
#[test]
fn either_config_value_unset_is_a_503() {
    let good = body(&token(good_header(), good_claims()), "Pixel", "android");
    for (client_ids, allowed_sub) in [
        (None, Some(OPERATOR_SUB)),
        (Some(CLIENT_ID), None),
        (None, None),
        (Some(""), Some(OPERATOR_SUB)),
        (Some(CLIENT_ID), Some("")),
    ] {
        let sql = RusqliteSql::new();
        let config = SigninConfig::from_env(client_ids, allowed_sub);
        assert_eq!(config, None, "{client_ids:?} {allowed_sub:?}");
        for body in [Some(good.as_str()), None] {
            let out = sign_in_with(&sql, config.as_ref(), "POST", body, true, NOW_MS);
            assert_eq!(out.response.status, 503, "{client_ids:?} {allowed_sub:?}");
            assert!(
                out.response.body.contains("signin_unconfigured"),
                "{}",
                out.response.body
            );
        }
        assert_eq!(token_count(&sql), 3);
    }
}

#[test]
fn a_bad_body_is_a_400() {
    let sql = RusqliteSql::new();
    let jwt = token(good_header(), good_claims());
    for (body, why) in [
        (None, "no body".to_string()),
        (Some("not json".to_string()), "not JSON".to_string()),
        (
            Some(r#"{"name": "n", "platform": "web"}"#.to_string()),
            "no id_token".to_string(),
        ),
        (
            Some(format!(r#"{{"id_token": "{jwt}", "platform": "web"}}"#)),
            "no name".to_string(),
        ),
        (
            Some(format!(r#"{{"id_token": "{jwt}", "name": "n"}}"#)),
            "no platform".to_string(),
        ),
        (
            Some(body(&jwt, "n", "ios")),
            "platform outside the closed set".to_string(),
        ),
        (
            Some(body(&jwt, "n", "Web")),
            "platform is case-sensitive".to_string(),
        ),
        (Some(body(&jwt, "   ", "web")), "blank name".to_string()),
        (
            Some(body(&jwt, &"x".repeat(65), "web")),
            "name too long".to_string(),
        ),
        (
            Some(format!(
                r#"{{"id_token": "{jwt}", "name": "n", "platform": "web", "id": "mine"}}"#
            )),
            "the caller never chooses the id".to_string(),
        ),
    ] {
        let out = sign_in_with(&sql, Some(&config()), "POST", body.as_deref(), true, NOW_MS);
        assert_eq!(out.response.status, 400, "{why}: {}", out.response.body);
    }
    assert_eq!(token_count(&sql), 3);
}

#[test]
fn any_other_method_is_a_405() {
    let sql = RusqliteSql::new();
    for method in ["GET", "PUT", "DELETE", "PATCH"] {
        let out = sign_in_with(&sql, Some(&config()), method, None, true, NOW_MS);
        assert_eq!(out.response.status, 405, "{method}");
    }
}

/// The route is the shim's, ahead of `handle()`: `handle()` itself still
/// treats the path like any other — no bearer, 401 — so nothing about the
/// sign-in lane leaks into the authenticated router.
#[test]
fn handle_never_mints_from_this_path() {
    let sql = RusqliteSql::new();
    let body = body(&token(good_header(), good_claims()), "Pixel", "android");
    let resp = req_anon(&sql, "POST", "/api/auth/google", None, Some(&body));
    assert_eq!(resp.status, 401);
    assert_eq!(token_count(&sql), 3);
}

/// Google's keys cannot be had: nothing is verified and nothing refused — a
/// 503, never the 403 that would read as "this account is not allowed".
#[test]
fn an_unusable_jwks_response_is_a_503() {
    let jwks = fixture()["jwks"].to_string();
    for (status, body) in [(500, jwks.as_str()), (404, jwks.as_str()), (200, "<html>")] {
        let refusal = jwks_from_response(status, None, body, NOW_MS).unwrap_err();
        assert_eq!(refusal.response.status, 503, "{status} {body}");
        assert!(refusal.response.body.contains("signin_unavailable"));
        assert!(refusal.log.unwrap().contains("JWKS fetch failed"));
    }
    assert_eq!(jwks_unavailable("timeout").response.status, 503);
    let cache = jwks_from_response(200, Some("max-age=60"), &jwks, NOW_MS).unwrap();
    assert!(cache.key("fixture-kid-1").is_some());
}
