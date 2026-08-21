//! Everything `POST /api/google/calendar_token` (#577/#582) can decide
//! without a runtime: the refresh-grant request body, the cached-token
//! freshness boundary, the 200 response, and every failure's status and
//! prose.
//!
//! Same split as `fcm.rs` and `skills.rs`: the runtime half — the actual
//! `fetch` to `oauth2.googleapis.com`, and the one-`RefCell`-per-DO-instance
//! cache the whole "N devices collapse to one exchange per hour" story
//! depends on — is `hummingbird-authority-worker`'s `calendar` module. That
//! cache is not just an optimisation: while a token is cached, a caller
//! holding a stolen `device` token gets that cached token rather than a
//! fresh exchange, so steady-state abuse of Google's token endpoint stays at
//! one exchange per hour. **It is a steady-state cap, not a hard throttle**
//! — nothing here caches an in-flight or *failed* exchange, so requests that
//! overlap a cache miss can each start one, and a credential Google is
//! refusing (`invalid_grant`) is re-attempted on every request. Bounding
//! those would mean an in-flight lock and negative caching in the untested
//! shim; the exposure is a personal workspace's own dead credential, so the
//! claim is narrowed rather than the mechanism grown. The cache is never
//! persisted (a plaintext Google bearer at rest would be a new class of
//! stored credential — the `tokens` table holds only sha256 digests — and
//! eviction costs exactly one extra exchange).
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

/// The one scope this credential is allowed to yield, asserted on every
/// exchange. See [`calendar_refresh_grant_body`] for why it is sent.
pub const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";

/// The write lane's scope (ADR-0031): read plus create/edit/delete of
/// **events**, on a third dedicated credential of its own.
///
/// Deliberately not `.../auth/calendar`, which would additionally grant
/// creating calendars and changing who they are shared with — powers no
/// verb of the OpenClaw calendar skill has any use for, and ones whose
/// abuse is not confined to the operator's own data.
pub const CALENDAR_WRITE_SCOPE: &str = "https://www.googleapis.com/auth/calendar.events";

/// The `refresh_token` grant's `POST oauth2.googleapis.com/token` body.
///
/// **`scope` is sent, and Google honours it (#581).** This module used to
/// omit it, on the stated ground that a `refresh_token` grant returns a token
/// bearing the whole grant and ignores the parameter. That was measured
/// during #581's provisioning and is false: asking the *shared* three-scope
/// credential for `calendar.readonly` alone yields a token that Gmail and
/// Tasks both refuse with 403 `insufficient authentication scopes`, while
/// Calendar answers 200 — a real narrowing, not an echo in the response body.
///
/// Sending it is not what keeps this lane safe — `GOOGLE_CALENDAR_REFRESH_TOKEN`
/// is a dedicated one-scope credential (ADR-0028), so there is nothing to
/// narrow when provisioning is correct. It is what makes *incorrect*
/// provisioning fail closed. The dedicated credential and the shared one sit
/// in the same 1Password vault under titles one word apart, and pasting the
/// wrong one here would otherwise hand every browser holding a `device` token
/// a live `gmail.modify` bearer, silently, with nothing on any surface
/// looking wrong. With `scope` sent, that mistake is a 403 from Gmail instead
/// of a granted one.
pub fn calendar_refresh_grant_body(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> String {
    refresh_grant_body(CALENDAR_SCOPE, client_id, client_secret, refresh_token)
}

/// The write lane's grant body (ADR-0031): [`calendar_refresh_grant_body`]
/// with [`CALENDAR_WRITE_SCOPE`], and the same mis-provisioning argument
/// with one more credential to confuse — the vault holds three
/// near-identical Google OAuth clients and this lane's own makes a
/// **fourth** once the operator mints it, so an exchange that names its
/// scope is what makes pasting the wrong refresh token here fail closed.
pub fn calendar_write_refresh_grant_body(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> String {
    refresh_grant_body(CALENDAR_WRITE_SCOPE, client_id, client_secret, refresh_token)
}

fn refresh_grant_body(
    scope: &str,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> String {
    format!(
        "grant_type=refresh_token&client_id={}&client_secret={}&refresh_token={}&scope={}",
        percent_encode(client_id),
        percent_encode(client_secret),
        percent_encode(refresh_token),
        percent_encode(scope),
    )
}

/// How long before a cached token's deadline this module starts minting a
/// new one instead of handing the old one back.
///
/// **This must exceed the web client's rotation margin** —
/// `client/web/src/calendar/connection.ts`'s `msUntilRotation`, 5 minutes —
/// and that coupling is the whole point of the constant. The client wakes a
/// timer at `expires_at_ms - 5min` and re-mints; if this module still called
/// the cached token fresh at that moment it would answer with the *same*
/// token and the *same* `expires_at_ms`, the client's rotation effect would
/// see unchanged inputs, no new timer would be armed, and proactive rotation
/// would be a permanent no-op after its first cache hit — leaving every
/// session to discover expiry through a live 401 instead.
pub const CACHE_REMINT_MARGIN_MS: i64 = 6 * 60 * 1000;

/// Whether a cached token is still safe to hand out. `expires_at_ms` already
/// carries [`crate::google_oauth::TOKEN_EXPIRY_SLACK_SECS`], baked in by
/// [`crate::google_oauth::parse_access_token`]; the extra margin subtracted
/// here is not about the token's own validity but about staying ahead of the
/// client's rotation timer — see [`CACHE_REMINT_MARGIN_MS`].
pub fn token_is_fresh(token: &AccessToken, now_ms: i64) -> bool {
    now_ms < token.expires_at_ms - CACHE_REMINT_MARGIN_MS
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

/// `GOOGLE_CALENDAR_WRITE_CLIENT_ID`/`_SECRET`/`_REFRESH_TOKEN` unset
/// (ADR-0031). Same 503 and same `calendar_unconfigured` code as the
/// readonly lane — a caller distinguishes the two lanes by which route it
/// called, not by the error code — but its own prose, because the two lanes
/// are provisioned independently and one can be live while the other is
/// not.
pub fn calendar_write_secrets_unset() -> (u16, String) {
    failure(
        503,
        "calendar_unconfigured",
        "The Google calendar write credential is not configured on this server.",
    )
}

/// The write lane's `invalid_grant` (ADR-0031). Its whole value is naming
/// the secret to re-mint, so it must name the **write** one: sending the
/// operator to `GOOGLE_CALENDAR_REFRESH_TOKEN` would have them rotate a
/// healthy credential and leave the dead one in place.
pub fn calendar_write_invalid_grant() -> (u16, String) {
    failure(
        502,
        "calendar_invalid_grant",
        "Google rejected the calendar write refresh token (invalid_grant) — re-mint \
         GOOGLE_CALENDAR_WRITE_REFRESH_TOKEN.",
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
             &refresh_token=1%2F%2Frefresh%20token\
             &scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fcalendar.readonly",
        );
    }

    /// #581: the scope is the mis-provisioning guard, so it is pinned on its
    /// own rather than only inside the whole-body assertion above. Dropping
    /// it would widen a stolen `device` token's reach to whatever the
    /// configured refresh token happens to carry — the exact failure
    /// ADR-0028 exists to prevent — and no test that only checked encoding
    /// would notice.
    #[test]
    fn refresh_grant_body_asks_for_calendar_readonly_only() {
        let body = calendar_refresh_grant_body("id", "secret", "token");
        assert!(
            body.contains("&scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fcalendar.readonly"),
            "the exchange must down-scope: {body}",
        );
        assert!(!body.contains("gmail"), "no Gmail scope may ever be requested here: {body}");
        assert!(!body.contains("tasks"), "no Tasks scope may ever be requested here: {body}");
    }

    /// The twin of the test above, on the lane where a mis-provisioned
    /// secret would be strictly worse: this credential can *write*. The
    /// events scope is asserted whole — a truncation to `calendar` would
    /// grant calendar creation and ACL edits, and a substring check for
    /// "calendar" would not notice.
    #[test]
    fn write_grant_body_asks_for_calendar_events_only() {
        let body = calendar_write_refresh_grant_body("id", "secret", "token");
        assert!(
            body.contains("&scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fcalendar.events"),
            "the write exchange must ask for calendar.events: {body}",
        );
        assert!(!body.contains("gmail"), "no Gmail scope may ever be requested here: {body}");
        assert!(!body.contains("tasks"), "no Tasks scope may ever be requested here: {body}");
        assert!(
            !body.contains("calendar.readonly"),
            "the write lane is not the readonly lane: {body}",
        );
    }

    /// The two lanes must not collapse onto one credential or one scope —
    /// the whole of ADR-0031's blast-radius argument is that the readonly
    /// credential every browser can reach is a different secret from the
    /// write one only the agent can.
    #[test]
    fn the_two_lanes_ask_for_different_scopes() {
        assert_ne!(CALENDAR_SCOPE, CALENDAR_WRITE_SCOPE);
        assert_ne!(
            calendar_refresh_grant_body("id", "secret", "token"),
            calendar_write_refresh_grant_body("id", "secret", "token"),
        );
    }

    fn token(expires_at_ms: i64) -> AccessToken {
        AccessToken { token: "ya29.abc".into(), expires_at_ms }
    }

    /// Both sides of the boundary: "fresh" is a strict `<` against the
    /// caller's clock, one re-mint margin ahead of the deadline.
    #[test]
    fn token_freshness_is_a_strict_boundary() {
        let deadline = 10_000 + CACHE_REMINT_MARGIN_MS;
        assert!(token_is_fresh(&token(deadline + 1), 10_000), "before the margin is fresh");
        assert!(!token_is_fresh(&token(deadline), 10_000), "exactly at the margin is stale");
        assert!(!token_is_fresh(&token(10_000), 10_000), "at the deadline is stale");
        assert!(!token_is_fresh(&token(9_999), 10_000), "past the deadline is stale");
    }

    /// The coupling [`CACHE_REMINT_MARGIN_MS`] exists for: the web client
    /// wakes its rotation timer 5 minutes before `expires_at_ms`, and that
    /// call must get a *new* token, or the client's effect deps never change
    /// and it never arms another timer.
    #[test]
    fn a_token_the_client_wakes_to_rotate_is_no_longer_fresh() {
        const CLIENT_ROTATION_MARGIN_MS: i64 = 5 * 60 * 1000;
        let expires_at_ms = 3_600_000;
        let client_wakes_at = expires_at_ms - CLIENT_ROTATION_MARGIN_MS;
        assert!(
            !token_is_fresh(&token(expires_at_ms), client_wakes_at),
            "the server must consider a token stale before the client wakes to rotate it",
        );
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

    /// The write lane's own provisioning failures: the same codes (a caller
    /// tells the lanes apart by the route it called), different prose, and
    /// each naming its own secret — an operator sent to the wrong
    /// `_REFRESH_TOKEN` rotates a healthy credential.
    #[test]
    fn the_write_lane_names_its_own_secrets() {
        let (status, body) = calendar_write_secrets_unset();
        assert_eq!(status, 503);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["error"], "calendar_unconfigured");
        assert_eq!(
            parsed["message"],
            "The Google calendar write credential is not configured on this server."
        );

        let (status, body) = calendar_write_invalid_grant();
        assert_eq!(status, 502);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["error"], "calendar_invalid_grant");
        let message = parsed["message"].as_str().unwrap();
        assert!(
            message.contains("GOOGLE_CALENDAR_WRITE_REFRESH_TOKEN"),
            "the write lane must name the write secret: {message}",
        );
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
            calendar_write_secrets_unset().0,
            calendar_write_invalid_grant().0,
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
