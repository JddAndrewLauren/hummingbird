//! Everything `POST /api/google/calendar_token` (#577/#582) can decide
//! without a runtime: the refresh-grant request body, the cached-token
//! freshness boundary, the 200 response, and every failure's status and
//! prose.
//!
//! Same split as `fcm.rs` and `skills.rs`: the runtime half — the actual
//! `fetch` to `oauth2.googleapis.com`, and the one-`RefCell`-per-DO-instance
//! cache the whole "N devices collapse to one exchange per hour" story
//! depends on — is `hummingbird-authority-worker`'s `calendar` module. That
//! cache is not just an optimisation: it caps what a stolen `device` token
//! can do to Google's token endpoint at one exchange per hour, and it is
//! never persisted (a plaintext Google bearer at rest would be a new class
//! of stored credential — the `tokens` table holds only sha256 digests —
//! and eviction costs exactly one extra exchange).
//!
//! **No path here answers 401.** Per ADR-0004 that would make the client
//! re-prompt a device token that is perfectly fine. Unset secrets are a
//! 503 ([`calendar_secrets_unset`]); a transport failure, an `invalid_grant`
//! response, or anything else upstream is a 502.
//!
//! The refresh-grant body percent-encodes every field. Unlike `fcm.rs`'s
//! JWT-bearer assertion — base64url plus `.`, every byte already URL-safe —
//! a `GOCSPX-` client secret is arbitrary bytes.

use serde::Deserialize;

use hummingbird_domain::{ApiError, CalendarTokenResponse};

use crate::google_oauth::AccessToken;

/// `application/x-www-form-urlencoded` percent-encoding: every byte outside
/// the unreserved set (`A-Za-z0-9-._~`, RFC 3986 §2.3) becomes `%XX`,
/// uppercase hex. Hand-rolled for the same reason `fcm.rs`'s base64url is:
/// `authority`'s dependency list is deliberately short enough to audit at a
/// glance (`lib.rs`'s guard test).
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The `refresh_token` grant's `POST oauth2.googleapis.com/token` body.
/// Passing `scope` here would be ignored — Google's `refresh_token` grant
/// does not honour down-scoping, which is exactly why #577 mints a second,
/// dedicated `calendar.readonly`-only refresh token rather than reusing the
/// shared one.
pub fn calendar_refresh_grant_body(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> String {
    format!(
        "grant_type=refresh_token&client_id={}&client_secret={}&refresh_token={}",
        percent_encode(client_id),
        percent_encode(client_secret),
        percent_encode(refresh_token),
    )
}

/// Whether a cached token is still safe to hand out. The boundary itself —
/// subtracting the expiry slack — is already baked into `expires_at_ms` by
/// [`crate::google_oauth::parse_access_token`], so this is a plain
/// comparison against the caller's clock.
pub fn token_is_fresh(token: &AccessToken, now_ms: i64) -> bool {
    now_ms < token.expires_at_ms
}

/// The 200 body.
pub fn calendar_token_success(token: &AccessToken) -> (u16, String) {
    let body = CalendarTokenResponse {
        access_token: token.token.clone(),
        expires_at_ms: token.expires_at_ms,
    };
    (200, serde_json::to_string(&body).expect("DTOs serialize"))
}

fn failure(status: u16, code: &str, message: &str) -> (u16, String) {
    let body = ApiError { error: code.to_string(), message: message.to_string() };
    (status, serde_json::to_string(&body).expect("DTOs serialize"))
}

/// `GOOGLE_CALENDAR_CLIENT_ID`/`_SECRET`/`_REFRESH_TOKEN` unset. **503,
/// never 401** — see the module doc.
pub fn calendar_secrets_unset() -> (u16, String) {
    failure(
        503,
        "calendar_unconfigured",
        "The Google calendar credential is not configured on this server.",
    )
}

/// The token-endpoint subrequest itself errored — DNS, TLS, connection.
pub fn calendar_unreachable() -> (u16, String) {
    failure(502, "calendar_unreachable", "Google's token endpoint is unreachable.")
}

/// Google answered `{"error":"invalid_grant"}`: the refresh token itself is
/// dead (revoked, expired, or minted for the wrong client). Names the
/// secret to re-mint so the operator knows exactly what to do.
pub fn calendar_invalid_grant() -> (u16, String) {
    failure(
        502,
        "calendar_invalid_grant",
        "Google rejected the calendar refresh token (invalid_grant) — re-mint \
         GOOGLE_CALENDAR_REFRESH_TOKEN.",
    )
}

/// Any other non-2xx from the token endpoint.
pub fn calendar_upstream_status(status: u16) -> (u16, String) {
    failure(502, "calendar_upstream", &format!("Google's token endpoint answered {status}."))
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: String,
}

/// Whether a token-endpoint response body is specifically `invalid_grant` —
/// the one upstream failure worth distinguishing from a generic 502,
/// because its prose is the operator's hint of exactly which secret is
/// dead.
pub fn is_invalid_grant(body: &str) -> bool {
    serde_json::from_str::<ErrorEnvelope>(body)
        .map(|e| e.error == "invalid_grant")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encoding_leaves_unreserved_bytes_alone() {
        assert_eq!(percent_encode("abcXYZ019-._~"), "abcXYZ019-._~");
    }

    /// The awkward case the module doc calls out: a `GOCSPX-` client secret
    /// (or a refresh token) is arbitrary bytes, unlike the FCM leg's
    /// already-URL-safe base64url assertion.
    #[test]
    fn percent_encoding_escapes_everything_else() {
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("a+b/c=d"), "a%2Bb%2Fc%3Dd");
        assert_eq!(percent_encode("GOCSPX-a!b&c"), "GOCSPX-a%21b%26c");
    }

    #[test]
    fn refresh_grant_body_percent_encodes_every_field() {
        let body = calendar_refresh_grant_body("client id", "GOCSPX-a+b", "1//refresh token");
        assert_eq!(
            body,
            "grant_type=refresh_token&client_id=client%20id&client_secret=GOCSPX-a%2Bb\
             &refresh_token=1%2F%2Frefresh%20token",
        );
    }

    fn token(expires_at_ms: i64) -> AccessToken {
        AccessToken { token: "ya29.abc".into(), expires_at_ms }
    }

    /// Both sides of the boundary: `expires_at_ms` is already the deadline
    /// (slack baked in by `parse_access_token`), so "fresh" is a strict `<`
    /// against the caller's clock.
    #[test]
    fn token_freshness_is_a_strict_boundary() {
        assert!(token_is_fresh(&token(10_001), 10_000), "not yet expired is fresh");
        assert!(!token_is_fresh(&token(10_000), 10_000), "exactly at the deadline is stale");
        assert!(!token_is_fresh(&token(9_999), 10_000), "past the deadline is stale");
    }

    #[test]
    fn calendar_token_success_body_shape() {
        let (status, body) = calendar_token_success(&token(123_456));
        assert_eq!(status, 200);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["access_token"], "ya29.abc");
        assert_eq!(parsed["expires_at_ms"], 123_456);
    }

    #[test]
    fn unconfigured_is_a_503_never_a_401() {
        let (status, body) = calendar_secrets_unset();
        assert_eq!(status, 503);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["error"], "calendar_unconfigured");
    }

    #[test]
    fn every_upstream_failure_is_a_502_with_distinguishable_prose() {
        let cases: Vec<((u16, String), &str, &str)> = vec![
            (
                calendar_unreachable(),
                "calendar_unreachable",
                "Google's token endpoint is unreachable.",
            ),
            (
                calendar_invalid_grant(),
                "calendar_invalid_grant",
                "Google rejected the calendar refresh token (invalid_grant) — re-mint \
                 GOOGLE_CALENDAR_REFRESH_TOKEN.",
            ),
            (
                calendar_upstream_status(500),
                "calendar_upstream",
                "Google's token endpoint answered 500.",
            ),
        ];
        for ((status, body), code, message) in cases {
            assert_eq!(status, 502, "{code}");
            let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(parsed["error"], code);
            assert_eq!(parsed["message"], message);
        }
    }

    #[test]
    fn upstream_status_prose_names_the_status() {
        let (_, body) = calendar_upstream_status(429);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["message"], "Google's token endpoint answered 429.");
    }

    /// 401 means "the device token itself is bad" (ADR-0004) and is decided
    /// entirely by `handlers::auth::authenticate`, before this module is
    /// ever reached (`handler_fixtures/calendar_token.rs`'s own test pins
    /// that 401 is the *only* thing an unauthenticated caller ever sees).
    /// Every status this module can produce is a provisioning or upstream
    /// problem — a 401 here would be indistinguishable from a bad device
    /// token and make the client wrongly re-prompt.
    #[test]
    fn no_failure_this_module_can_produce_is_ever_a_401() {
        let statuses = [
            calendar_secrets_unset().0,
            calendar_unreachable().0,
            calendar_invalid_grant().0,
            calendar_upstream_status(500).0,
            calendar_upstream_status(401).0,
        ];
        for status in statuses {
            assert_ne!(status, 401, "no calendar-token failure may answer 401");
        }
    }

    #[test]
    fn is_invalid_grant_detects_only_that_error() {
        assert!(is_invalid_grant(r#"{"error":"invalid_grant"}"#));
        assert!(!is_invalid_grant(r#"{"error":"invalid_client"}"#));
        assert!(!is_invalid_grant(r#"{"access_token":"t","expires_in":3600}"#));
        assert!(!is_invalid_grant("not json"));
        assert!(!is_invalid_grant(""));
    }
}
