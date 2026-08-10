//! The FCM send leg (#219): everything about talking to FCM that can be
//! decided without a runtime — the OAuth assertion's bytes, the message
//! body, what an FCM response *means*, and the one write a failed send is
//! allowed to make.
//!
//! [`crate::delivery::deliver`] decides *whether* a transition rings and
//! hands back `Logged { targets, notification }`; this module says *what to
//! put on the wire* and *what to do with the answer*. The two things it
//! deliberately does **not** do are the two that need a runtime: signing
//! the RS256 assertion (WebCrypto) and making the HTTP calls (`fetch`).
//! Those are `hummingbird-authority-worker`'s `fcm` module, which is a
//! wasm32-only shim over the pure functions here — the same division the
//! rest of the crate keeps, and the reason the send path is testable at all
//! (`server/worker/src/lib.rs` has no test harness of any kind).
//!
//! **No retry, ever (#219's decided policy).** [`deliver`] commits the
//! claim row *before* the caller can send, so the transition is already
//! burned by the time any of this runs. A retry that re-sent would
//! double-ring — the one failure ADR-0012 says destroys trust in the
//! channel — and a retry that re-entered `deliver` would be absorbed as
//! `AlreadyDelivered` anyway. So a failed send is logged and dropped. The
//! single exception is a *dead token*, which is not a failure of this send
//! but a fact about the device: see [`SendVerdict::TokenDead`].
//!
//! [`deliver`]: crate::delivery::deliver

use hummingbird_domain::Tier;
use serde::Deserialize;

use crate::delivery::PushNotification;
use crate::sql::{Sql, SqlError, SqlValue};

/// The OAuth2 scope an FCM v1 send needs — the only scope this server ever
/// asks the service account for.
pub const FCM_SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

/// Google's OAuth2 token endpoint (the `aud` of the assertion, and where
/// the worker POSTs it).
pub const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// How long an assertion claims validity for. Google caps this at one hour;
/// the access token it buys back carries its own (also ~1h) expiry.
const ASSERTION_TTL_SECS: i64 = 3600;

/// Slack subtracted from an access token's stated lifetime before the
/// worker considers it stale, so a token can never expire mid-flight
/// between the staleness check and the send.
pub const TOKEN_EXPIRY_SLACK_SECS: i64 = 60;

// ---------------------------------------------------------------------------
// The operator credential
// ---------------------------------------------------------------------------

/// The three fields this server needs out of a Google service-account JSON
/// key — the `FCM_SERVICE_ACCOUNT` Worker secret (#219's decided home:
/// `wrangler secret put`, never `wrangler.toml`, never the repo; unset, the
/// send leg fails closed and logs, exactly as `ADMIN_SECRET` does for the
/// admin lane).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ServiceAccount {
    pub project_id: String,
    pub client_email: String,
    /// The PKCS#8 PEM as it appears in the key file. Turn it into the DER
    /// bytes WebCrypto's `importKey` wants with [`pkcs8_der_from_pem`].
    pub private_key: String,
}

impl ServiceAccount {
    /// Parses the service-account JSON. Unknown fields (`type`,
    /// `private_key_id`, `token_uri`, the rest) are ignored rather than
    /// rejected — a Google key file carries a dozen of them and gains more
    /// over time.
    pub fn parse(json: &str) -> Result<ServiceAccount, FcmConfigError> {
        let account: ServiceAccount =
            serde_json::from_str(json).map_err(|_| FcmConfigError::Malformed)?;
        if account.project_id.is_empty()
            || account.client_email.is_empty()
            || account.private_key.is_empty()
        {
            return Err(FcmConfigError::Malformed);
        }
        Ok(account)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FcmConfigError {
    /// The secret is not a service-account key, or is missing one of the
    /// three fields a send needs.
    Malformed,
    /// The `private_key` field is not a PKCS#8 PEM block.
    BadPrivateKey,
}

/// Strips the PEM armour and base64-decodes the body, yielding the DER
/// bytes for WebCrypto's `importKey("pkcs8", …)`. Whitespace and line
/// breaks inside the block are ignored, so it does not matter whether the
/// secret arrived with real newlines or the JSON-escaped `\n` the key file
/// ships.
pub fn pkcs8_der_from_pem(pem: &str) -> Result<Vec<u8>, FcmConfigError> {
    let body = pem
        .split("-----BEGIN PRIVATE KEY-----")
        .nth(1)
        .and_then(|rest| rest.split("-----END PRIVATE KEY-----").next())
        .ok_or(FcmConfigError::BadPrivateKey)?;
    base64_decode(body).ok_or(FcmConfigError::BadPrivateKey)
}

// ---------------------------------------------------------------------------
// The OAuth assertion
// ---------------------------------------------------------------------------

/// The `base64url(header).base64url(claims)` string that gets RS256-signed.
/// Split out from [`assemble_assertion`] because the signature itself is the
/// one step that needs a runtime: the worker imports `private_key` into
/// WebCrypto and signs exactly these bytes.
pub fn assertion_signing_input(client_email: &str, now_secs: i64) -> String {
    let header = base64url_encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = serde_json::json!({
        "iss": client_email,
        "scope": FCM_SCOPE,
        "aud": OAUTH_TOKEN_URL,
        "iat": now_secs,
        "exp": now_secs + ASSERTION_TTL_SECS,
    })
    .to_string();
    format!("{header}.{}", base64url_encode(claims.as_bytes()))
}

/// Appends the RS256 signature to its signing input, giving the complete
/// JWT bearer assertion.
pub fn assemble_assertion(signing_input: &str, signature: &[u8]) -> String {
    format!("{signing_input}.{}", base64url_encode(signature))
}

/// The `application/x-www-form-urlencoded` body of the token request. The
/// assertion is base64url plus `.` — every byte already URL-safe — so only
/// the fixed `grant_type` needs escaping, and it is escaped here as a
/// literal rather than by pulling in a percent-encoding dependency.
pub fn token_request_body(assertion: &str) -> String {
    format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={assertion}"
    )
}

/// An access token and the wall-clock millisecond after which the worker
/// must mint a new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessToken {
    pub token: String,
    pub expires_at_ms: i64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
}

/// Parses Google's token response, converting its relative `expires_in`
/// into an absolute deadline against the caller's clock — minus
/// [`TOKEN_EXPIRY_SLACK_SECS`], so a token that is about to expire is
/// treated as already expired rather than being sent and rejected.
pub fn parse_access_token(body: &str, now_ms: i64) -> Option<AccessToken> {
    let parsed: TokenResponse = serde_json::from_str(body).ok()?;
    if parsed.access_token.is_empty() {
        return None;
    }
    let lifetime = (parsed.expires_in - TOKEN_EXPIRY_SLACK_SECS).max(0);
    Some(AccessToken {
        token: parsed.access_token,
        expires_at_ms: now_ms + lifetime * 1000,
    })
}

// ---------------------------------------------------------------------------
// The message
// ---------------------------------------------------------------------------

/// The FCM v1 send endpoint for one project.
pub fn send_url(project_id: &str) -> String {
    format!("https://fcm.googleapis.com/v1/projects/{project_id}/messages:send")
}

/// The FCM v1 request body for one notification aimed at one device token.
///
/// ADR-0012's two tiers map onto Android's two levers at once: the FCM
/// transport `priority` (whether the message wakes a dozing device at all)
/// and the `channel_id` (which channel's user-controlled behaviour it
/// adopts). The **urgent channel's Do-Not-Disturb bypass is requested on
/// the device**, at channel creation — a channel's behaviour is fixed by
/// whoever created it and cannot be changed by a later message — so all the
/// server can do, and all it should do, is name the right channel.
///
/// `data` carries the identifiers the client needs to act without a round
/// trip: `alert_id` is what the notification's Ack action stamps
/// `dismissed_at` on (ADR-0012's "swipe is not a gesture" — the swipe ends
/// the delivery, the Ack ends the alert). FCM requires every `data` value
/// to be a string.
pub fn message_json(notification: &PushNotification, fcm_token: &str) -> String {
    let (transport_priority, channel_id, notification_priority) = match notification.tier {
        Tier::Urgent => ("high", "urgent", "PRIORITY_MAX"),
        Tier::Normal => ("normal", "normal", "PRIORITY_DEFAULT"),
    };

    let mut notification_block = serde_json::Map::new();
    notification_block.insert("title".into(), notification.title.clone().into());
    if let Some(body) = &notification.body {
        notification_block.insert("body".into(), body.clone().into());
    }

    serde_json::json!({
        "message": {
            "token": fcm_token,
            "notification": notification_block,
            "android": {
                "priority": transport_priority,
                "notification": {
                    "channel_id": channel_id,
                    "notification_priority": notification_priority,
                },
            },
            "data": {
                "alert_id": notification.alert_id,
                "severity": notification.severity,
                "tier": notification.tier.as_str(),
            },
        }
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// The response
// ---------------------------------------------------------------------------

/// What one FCM send response means for the target it was aimed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendVerdict {
    /// FCM accepted the message.
    Delivered,
    /// FCM reports this registration token no longer exists — the app was
    /// uninstalled, its data cleared, or the token otherwise retired. This
    /// is a durable fact about the *device*, not a failure of this send, so
    /// it is the one response that writes: see [`revoke_dead_target`].
    TokenDead,
    /// Anything else — a transport error, a quota rejection, a malformed
    /// payload, an expired credential. Logged and dropped, never retried
    /// (see the module doc).
    Failed { detail: String },
}

#[derive(Deserialize)]
struct FcmErrorEnvelope {
    error: FcmErrorBody,
}

#[derive(Deserialize)]
struct FcmErrorBody {
    #[serde(default)]
    status: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    details: Vec<FcmErrorDetail>,
}

#[derive(Deserialize)]
struct FcmErrorDetail {
    #[serde(rename = "errorCode", default)]
    error_code: String,
}

/// Reads one FCM v1 response into a [`SendVerdict`].
///
/// **`TokenDead` is decided on the `UNREGISTERED` error code alone, never
/// on the HTTP status.** FCM returns `404 NOT_FOUND` both for a retired
/// registration token *and* for a request aimed at a project that does not
/// exist — so a single typo in `project_id`, or a service-account secret
/// swapped for the wrong project's, would 404 every send. Revoking on the
/// status would then silently revoke every push target the operator owns,
/// and each device would only come back by re-registering. The
/// `google.firebase.fcm.v1.FcmError` detail is the narrow signal that
/// actually means "this token is gone", so it is the only one trusted with
/// a write; everything else is [`SendVerdict::Failed`] and merely logged.
pub fn classify_response(status: u16, body: &str) -> SendVerdict {
    if (200..300).contains(&status) {
        return SendVerdict::Delivered;
    }
    if let Ok(envelope) = serde_json::from_str::<FcmErrorEnvelope>(body) {
        if envelope.error.details.iter().any(|d| d.error_code == "UNREGISTERED") {
            return SendVerdict::TokenDead;
        }
        let detail = match (envelope.error.status.as_str(), envelope.error.message.as_str()) {
            ("", "") => format!("HTTP {status}"),
            ("", message) => format!("HTTP {status}: {message}"),
            (status_text, "") => format!("HTTP {status} {status_text}"),
            (status_text, message) => format!("HTTP {status} {status_text}: {message}"),
        };
        return SendVerdict::Failed { detail };
    }
    SendVerdict::Failed { detail: format!("HTTP {status}") }
}

/// Revokes a push target whose token FCM reported as `UNREGISTERED`.
///
/// The same flag `DELETE /api/push_targets/:id` sets, and never a delete —
/// the deliveries that named this target keep their referent. The
/// `revoked_at IS NULL` guard makes it idempotent *and* keeps the original
/// revocation stamp when the operator had already revoked it by hand: this
/// path is telling us the device is gone, not when it went. A device that
/// comes back re-registers, and `push_targets::register` revives the row.
pub fn revoke_dead_target(sql: &dyn Sql, target_id: &str, now_ms: i64) -> Result<(), SqlError> {
    sql.exec(
        "UPDATE push_targets SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL",
        &[SqlValue::Integer(now_ms), SqlValue::Text(target_id.to_string())],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// base64
// ---------------------------------------------------------------------------

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url, unpadded — JWT's encoding (RFC 7515 §2). Hand-rolled rather
/// than pulled in: it is twenty lines, and `authority`'s dependency list is
/// deliberately short enough to audit at a glance (`lib.rs`'s guard test).
fn base64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = |i: usize| *chunk.get(i).unwrap_or(&0) as u32;
        let n = (b(0) << 16) | (b(1) << 8) | b(2);
        let digits = [(n >> 18) & 63, (n >> 12) & 63, (n >> 6) & 63, n & 63];
        // 3 input bytes -> 4 digits, 2 -> 3, 1 -> 2; the rest would be
        // padding, which base64url omits.
        for digit in digits.iter().take(chunk.len() + 1) {
            out.push(URL_SAFE[*digit as usize] as char);
        }
    }
    out
}

/// Standard base64 decode, tolerant of the whitespace and line breaks a PEM
/// block carries and of padding being present or absent.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits = 0;
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for byte in text.bytes() {
        if byte.is_ascii_whitespace() || byte == b'=' {
            continue;
        }
        let value = STANDARD.iter().position(|c| *c == byte)? as u32;
        acc = (acc << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    // Leftover bits are the encoder's zero padding; anything set in them
    // means the input was not valid base64.
    if acc & ((1 << bits) - 1) != 0 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_encodes_without_padding() {
        assert_eq!(base64url_encode(b""), "");
        assert_eq!(base64url_encode(b"f"), "Zg");
        assert_eq!(base64url_encode(b"fo"), "Zm8");
        assert_eq!(base64url_encode(b"foo"), "Zm9v");
        assert_eq!(base64url_encode(b"foob"), "Zm9vYg");
        assert_eq!(base64url_encode(b"fooba"), "Zm9vYmE");
        assert_eq!(base64url_encode(b"foobar"), "Zm9vYmFy");
    }

    /// The two alphabets differ only in the last two digits, which is
    /// exactly where a mix-up would hide: `>>?` encodes to `Pj4/` in
    /// standard base64 and must encode to `Pj4_` here.
    #[test]
    fn base64url_uses_the_url_safe_alphabet() {
        assert_eq!(base64url_encode(&[0x3e, 0x3e, 0x3f]), "Pj4_");
        assert_eq!(base64url_encode(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn base64_decode_round_trips_and_tolerates_pem_whitespace() {
        assert_eq!(base64_decode("Zm9vYmFy").as_deref(), Some(&b"foobar"[..]));
        assert_eq!(base64_decode("Zm9v\nYmFy\n").as_deref(), Some(&b"foobar"[..]));
        assert_eq!(base64_decode("Zg==").as_deref(), Some(&b"f"[..]));
        assert_eq!(base64_decode("Zg").as_deref(), Some(&b"f"[..]));
        assert_eq!(base64_decode("not base64!"), None);
    }

    #[test]
    fn service_account_parses_and_ignores_the_key_file_s_other_fields() {
        let json = r#"{
            "type": "service_account",
            "project_id": "hummingbird-prod",
            "private_key_id": "abc123",
            "private_key": "-----BEGIN PRIVATE KEY-----\nZm9vYmFy\n-----END PRIVATE KEY-----\n",
            "client_email": "push@hummingbird-prod.iam.gserviceaccount.com",
            "client_id": "12345",
            "token_uri": "https://oauth2.googleapis.com/token"
        }"#;
        let account = ServiceAccount::parse(json).expect("a real key file parses");
        assert_eq!(account.project_id, "hummingbird-prod");
        assert_eq!(account.client_email, "push@hummingbird-prod.iam.gserviceaccount.com");
        assert_eq!(pkcs8_der_from_pem(&account.private_key).as_deref(), Ok(&b"foobar"[..]));
    }

    #[test]
    fn service_account_rejects_a_secret_that_is_not_a_key_file() {
        assert_eq!(ServiceAccount::parse("not json"), Err(FcmConfigError::Malformed));
        assert_eq!(
            ServiceAccount::parse(r#"{"project_id":"p","client_email":"e","private_key":""}"#),
            Err(FcmConfigError::Malformed),
        );
    }

    #[test]
    fn pkcs8_der_rejects_a_key_without_pem_armour() {
        assert_eq!(pkcs8_der_from_pem("Zm9vYmFy"), Err(FcmConfigError::BadPrivateKey));
    }

    #[test]
    fn assertion_carries_the_scope_audience_and_a_one_hour_expiry() {
        let input = assertion_signing_input("push@example.iam.gserviceaccount.com", 1_700_000_000);
        let (header, claims) = input.split_once('.').expect("header.claims");
        assert_eq!(
            String::from_utf8(base64_decode_url(header)).unwrap(),
            r#"{"alg":"RS256","typ":"JWT"}"#,
        );
        let claims: serde_json::Value =
            serde_json::from_slice(&base64_decode_url(claims)).expect("claims are JSON");
        assert_eq!(claims["iss"], "push@example.iam.gserviceaccount.com");
        assert_eq!(claims["scope"], FCM_SCOPE);
        assert_eq!(claims["aud"], OAUTH_TOKEN_URL);
        assert_eq!(claims["iat"], 1_700_000_000i64);
        assert_eq!(claims["exp"], 1_700_003_600i64);
        // Unpadded, and no character that would need escaping in the form body.
        assert!(!input.contains('=') && !input.contains('+') && !input.contains('/'));
    }

    #[test]
    fn assembled_assertion_is_three_base64url_segments() {
        let input = assertion_signing_input("e@example.com", 0);
        let jwt = assemble_assertion(&input, &[0xfb, 0xff]);
        assert_eq!(jwt, format!("{input}.-_8"));
        assert_eq!(jwt.split('.').count(), 3);
    }

    #[test]
    fn token_request_body_escapes_only_the_grant_type() {
        let body = token_request_body("aaa.bbb.ccc");
        assert_eq!(
            body,
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion=aaa.bbb.ccc",
        );
    }

    #[test]
    fn access_token_expiry_is_absolute_and_carries_the_slack() {
        let parsed = parse_access_token(r#"{"access_token":"ya29.abc","expires_in":3600}"#, 10_000)
            .expect("a normal token response parses");
        assert_eq!(parsed.token, "ya29.abc");
        // 3600s minus the 60s slack, in ms, from the caller's clock.
        assert_eq!(parsed.expires_at_ms, 10_000 + 3_540_000);
    }

    /// A token that expires sooner than the slack must not produce a
    /// deadline in the past-relative-to-now arithmetic's negative range —
    /// it is simply already stale.
    #[test]
    fn access_token_with_a_lifetime_under_the_slack_is_already_stale() {
        let parsed = parse_access_token(r#"{"access_token":"t","expires_in":30}"#, 10_000).unwrap();
        assert_eq!(parsed.expires_at_ms, 10_000);
    }

    #[test]
    fn access_token_rejects_an_error_or_empty_response() {
        assert_eq!(parse_access_token(r#"{"error":"invalid_grant"}"#, 0), None);
        assert_eq!(parse_access_token(r#"{"access_token":"","expires_in":3600}"#, 0), None);
        assert_eq!(parse_access_token("", 0), None);
    }

    fn notification(tier: Tier) -> PushNotification {
        PushNotification {
            alert_id: "alert-1".into(),
            title: "Ship the thing".into(),
            body: Some("due in 2 days".into()),
            severity: "high".into(),
            tier,
        }
    }

    #[test]
    fn urgent_maps_to_high_transport_priority_and_the_urgent_channel() {
        let json: serde_json::Value =
            serde_json::from_str(&message_json(&notification(Tier::Urgent), "tok-1")).unwrap();
        let message = &json["message"];
        assert_eq!(message["token"], "tok-1");
        assert_eq!(message["notification"]["title"], "Ship the thing");
        assert_eq!(message["notification"]["body"], "due in 2 days");
        assert_eq!(message["android"]["priority"], "high");
        assert_eq!(message["android"]["notification"]["channel_id"], "urgent");
        assert_eq!(
            message["android"]["notification"]["notification_priority"],
            "PRIORITY_MAX",
        );
    }

    #[test]
    fn normal_maps_to_normal_transport_priority_and_the_normal_channel() {
        let json: serde_json::Value =
            serde_json::from_str(&message_json(&notification(Tier::Normal), "tok-1")).unwrap();
        assert_eq!(json["message"]["android"]["priority"], "normal");
        assert_eq!(json["message"]["android"]["notification"]["channel_id"], "normal");
        assert_eq!(
            json["message"]["android"]["notification"]["notification_priority"],
            "PRIORITY_DEFAULT",
        );
    }

    /// The Ack action needs the alert id, and FCM requires every `data`
    /// value to be a string — a number here is a 400 from FCM, at send
    /// time, for every notification.
    #[test]
    fn data_carries_the_alert_id_as_strings() {
        let json: serde_json::Value =
            serde_json::from_str(&message_json(&notification(Tier::Urgent), "tok-1")).unwrap();
        let data = &json["message"]["data"];
        assert_eq!(data["alert_id"], "alert-1");
        assert_eq!(data["severity"], "high");
        assert_eq!(data["tier"], "urgent");
        for (key, value) in data.as_object().expect("data is an object") {
            assert!(value.is_string(), "data.{key} must be a string for FCM");
        }
    }

    #[test]
    fn a_bodyless_notification_omits_the_body_rather_than_sending_null() {
        let mut n = notification(Tier::Normal);
        n.body = None;
        let json: serde_json::Value = serde_json::from_str(&message_json(&n, "tok-1")).unwrap();
        assert_eq!(json["message"]["notification"]["title"], "Ship the thing");
        assert!(json["message"]["notification"].get("body").is_none());
    }

    #[test]
    fn send_url_names_the_project() {
        assert_eq!(
            send_url("hummingbird-prod"),
            "https://fcm.googleapis.com/v1/projects/hummingbird-prod/messages:send",
        );
    }

    #[test]
    fn a_2xx_is_delivered() {
        assert_eq!(classify_response(200, r#"{"name":"projects/p/messages/1"}"#), SendVerdict::Delivered);
    }

    #[test]
    fn the_unregistered_error_code_is_the_only_thing_that_revokes() {
        let unregistered = r#"{"error":{"code":404,"status":"NOT_FOUND",
            "message":"Requested entity was not found.",
            "details":[{"@type":"type.googleapis.com/google.firebase.fcm.v1.FcmError",
                        "errorCode":"UNREGISTERED"}]}}"#;
        assert_eq!(classify_response(404, unregistered), SendVerdict::TokenDead);
    }

    /// The regression this classifier exists to prevent: a wrong
    /// `project_id` 404s with the same status and no `FcmError` detail. If
    /// status alone decided, one bad secret would revoke every device the
    /// operator owns.
    #[test]
    fn a_404_for_a_missing_project_does_not_revoke_the_token() {
        let wrong_project = r#"{"error":{"code":404,"status":"NOT_FOUND",
            "message":"Requested entity was not found."}}"#;
        assert_eq!(
            classify_response(404, wrong_project),
            SendVerdict::Failed { detail: "HTTP 404 NOT_FOUND: Requested entity was not found.".into() },
        );
    }

    #[test]
    fn other_failures_carry_a_loggable_detail_and_never_revoke() {
        assert_eq!(
            classify_response(401, r#"{"error":{"code":401,"status":"UNAUTHENTICATED","message":"bad creds"}}"#),
            SendVerdict::Failed { detail: "HTTP 401 UNAUTHENTICATED: bad creds".into() },
        );
        assert_eq!(
            classify_response(429, r#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED","message":""}}"#),
            SendVerdict::Failed { detail: "HTTP 429 RESOURCE_EXHAUSTED".into() },
        );
        // A gateway error is not JSON at all.
        assert_eq!(
            classify_response(502, "<html>bad gateway</html>"),
            SendVerdict::Failed { detail: "HTTP 502".into() },
        );
    }

    /// base64url decoder, tests only — the production path never decodes
    /// what it encoded, but the assertion tests must read the claims back.
    fn base64_decode_url(text: &str) -> Vec<u8> {
        let standard: String = text
            .chars()
            .map(|c| match c {
                '-' => '+',
                '_' => '/',
                other => other,
            })
            .collect();
        base64_decode(&standard).expect("round-trips its own encoder")
    }
}
