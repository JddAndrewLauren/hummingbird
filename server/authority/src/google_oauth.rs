//! The Google OAuth2 access-token half shared by every consumer that talks
//! to `oauth2.googleapis.com`: the token endpoint, the expiry-slack policy,
//! the [`AccessToken`] value, and the response parser.
//!
//! `fcm.rs` is the first consumer (a JWT-bearer assertion grant, minting a
//! `firebase.messaging` token for the send leg) and the calendar-token
//! exchange (a `refresh_token` grant, minting a `calendar.readonly` token
//! for the web host) is the second — see #577/#579. The two grants differ
//! in how they *ask* for a token; what this module owns is what every
//! consumer does with the *answer*, so lifting it here turns the second
//! consumer into a caller instead of a copy.
//!
//! Same split as the rest of the crate: this is the pure, natively-tested
//! half. Each consumer's own runtime shim (`server/worker`'s `fcm` module,
//! and its calendar-token twin) holds only the `fetch` call and zero
//! literals or status arithmetic — `server/worker` has no test harness, so
//! anything expressed there is untested by construction.

use serde::Deserialize;

/// Google's OAuth2 token endpoint — the `aud` of a JWT-bearer assertion,
/// and the URL a `refresh_token` grant POSTs to as well.
pub const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Slack subtracted from an access token's stated lifetime before a
/// consumer considers it stale, so a token can never expire mid-flight
/// between the staleness check and its use.
pub const TOKEN_EXPIRY_SLACK_SECS: i64 = 60;

/// An access token and the wall-clock millisecond after which the caller
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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The `refresh_token` grant's response carries `scope` and
    /// `token_type` alongside the two fields the JWT-bearer grant's
    /// response also carries (#577/#582's second consumer) — unlike
    /// `TokenResponse`'s `#[derive(Deserialize)]`, which has no
    /// `deny_unknown_fields`, both extra fields must be silently ignored
    /// rather than failing the parse.
    #[test]
    fn a_refresh_grant_response_with_scope_and_token_type_still_parses() {
        let body = r#"{
            "access_token": "ya29.refresh-grant",
            "expires_in": 3599,
            "scope": "https://www.googleapis.com/auth/calendar.readonly",
            "token_type": "Bearer"
        }"#;
        let parsed = parse_access_token(body, 0).expect("extra fields are ignored");
        assert_eq!(parsed.token, "ya29.refresh-grant");
    }
}
