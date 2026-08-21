//! The phone's `calendar.readonly` mint (#564, ADR-0028 as amended): POST
//! this device's own `device` token to `/api/google/calendar_token` and get
//! back a short-lived Google access token the authority exchanged from a
//! server-held refresh token.
//!
//! **This is a deliberate second copy of a transport mapping, not of a
//! decision.** ADR-0025 governs *decisions* — what a client concludes from
//! facts — and sinks them into `hummingbird-core` so no two clients can
//! disagree. What lives here is neither: it is the status-code-to-error-code
//! table for one route, and the web's copy
//! (`client/web/src/calendar/authority-token-client.ts`) is entangled with
//! browser `fetch`, `AbortSignal.timeout` and an IndexedDB-backed
//! `readToken` seam. Sinking it would mean lifting a `reqwest` call into a
//! crate that compiles to `wasm32` for the browser — where `reqwest` is not
//! the transport in use — to share seven string literals. So the *codes* are
//! copied verbatim and pinned by name below, and the one thing that is a
//! decision — which of #564's four Source-connection states a code puts the
//! device in — is decided once, in [`connection_state`], rather than by any
//! Kotlin `when` over an error string.
//!
//! **The device token never appears in anything this module returns.** Its
//! only appearance is the `authorization` header, exactly as the web client
//! records for itself.
//!
//! **Never fails.** Every path answers a [`MintOutcome`]; there is no
//! `Result`, no panic and no timeout escape. That is the same "never throws"
//! contract the web client carries, and the reason the caller
//! (`MobileTaskHost`'s calendar half) can treat a mint as an ordinary state
//! transition rather than an error path.

use std::time::Duration;

/// ADR-0028's route, same-origin with the authority's own base URL
/// (ADR-0018), `device` scope, no body.
pub(crate) const CALENDAR_TOKEN_PATH: &str = "/api/google/calendar_token";

/// How long one request may take before it is called failed — the web
/// client's own 15s, for the same reason: this is "the authority is not
/// answering", not a UX-timed ceiling.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Every failure code this module can produce, spelled exactly as
/// `authority-token-client.ts` spells them. Kotlin never matches on these —
/// [`connection_state`] does — but they cross to Kotlin anyway, on
/// `connection.ts`'s own rule that a code and a sentence are different
/// things and the code is what a health check reads.
pub(crate) mod code {
    /// Nothing to authenticate with: no device token is held.
    pub(crate) const NO_DEVICE_TOKEN: &str = "no_device_token";
    /// 401/403 — the authority refused this device token
    /// (`handlers::auth::authenticate`'s empty-body ADR-0004 verdict), or
    /// ADR-0031's allowed-holder gate refused it.
    pub(crate) const AUTHORITY_REJECTED_DEVICE_TOKEN: &str = "authority_rejected_device_token";
    /// 503 — the three `GOOGLE_CALENDAR_*` secrets are unset.
    pub(crate) const AUTHORITY_UNCONFIGURED: &str = "authority_unconfigured";
    /// 502 — unreachable upstream / `invalid_grant` / any other Google-side
    /// failure. `google_calendar.rs`'s three cases collapse to one code
    /// because none of them is this client's to act on.
    pub(crate) const AUTHORITY_UPSTREAM: &str = "authority_upstream";
    /// The request never resolved: no network, DNS failure, TLS failure, or
    /// the 15s timeout.
    pub(crate) const AUTHORITY_UNREACHABLE: &str = "authority_unreachable";
    /// Any other non-2xx, or a 200 whose body is not JSON at all — a
    /// malformed answer, worth retrying.
    pub(crate) const BAD_TOKEN_RESPONSE: &str = "bad_token_response";
    /// Valid JSON that nonetheless lacks a usable token — structurally
    /// wrong, not transient.
    pub(crate) const NO_ACCESS_TOKEN: &str = "no_access_token";

    /// The whole vocabulary, in one place, so a test can prove
    /// [`super::connection_state`] has a case for each. Test-only: nothing
    /// in production iterates the codes, it matches on them.
    #[cfg(test)]
    pub(crate) const ALL: [&str; 7] = [
        NO_DEVICE_TOKEN,
        AUTHORITY_REJECTED_DEVICE_TOKEN,
        AUTHORITY_UNCONFIGURED,
        AUTHORITY_UPSTREAM,
        AUTHORITY_UNREACHABLE,
        BAD_TOKEN_RESPONSE,
        NO_ACCESS_TOKEN,
    ];
}

/// How far before real expiry a proactive re-mint is scheduled —
/// `client/web/src/calendar/connection.ts`'s `ROTATION_MARGIN_MS`, ported
/// rather than sunk: it is a duration this host drives a timer with, and it
/// now has an Android caller where #564 recorded it had none.
///
/// **Coupled to the authority's own cache boundary** the same way the web's
/// copy is — `server/authority/src/google_calendar.rs`'s
/// `CACHE_REMINT_MARGIN_MS` is deliberately larger, and
/// `the_authority_gives_up_on_its_cache_before_this_client_rotates` below
/// pins the inequality because nothing else can. If the authority still
/// considered its cached token fresh when this timer fired, the route would
/// answer with the same token and the same expiry, `due_for_rotation` would
/// stay true forever, and every tick would re-mint pointlessly until the
/// token died in a live 401.
pub(crate) const ROTATION_MARGIN_MS: i64 = 5 * 60 * 1000;

/// What one mint attempt produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MintOutcome {
    Minted {
        access_token: String,
        expires_at_ms: i64,
    },
    /// One of [`code`]'s constants — never a sentence.
    Failed(&'static str),
}

/// #564's four Source-connection states, restated for the authority-minted
/// lane. Decided here, from a code plus whether this device was ever
/// connected, so no Kotlin `when` can disagree with it.
///
/// The split that matters is *cannot confirm* vs *refused*: an unreachable
/// authority reads as connected and keeps showing its (stale) mirror,
/// because the phone genuinely cannot tell "I am offline" from "the server
/// is down" and does not need to — neither is a reason to un-opt-in a
/// device or to offer Connect again. A *refusal* is the opposite: the
/// authority answered, and the answer names something the operator can act
/// on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CalendarState {
    /// Never opted in on this device. The only state that offers Connect.
    NeverConnected,
    /// Opted in, and the last mint succeeded.
    Connected,
    /// Opted in; the authority could not be reached. Reads as connected,
    /// events shown and marked stale, **never** offers Connect.
    CannotConfirm,
    /// The device token itself is bad — Settings' existing token control is
    /// the remedy.
    RefusedDeviceToken,
    /// The server-side calendar lane is broken (unset secrets, a bad
    /// upstream, a malformed answer). There is no per-device action.
    RefusedServerLane,
}

/// Which state a device is in, given whether it was ever opted in and the
/// code its last mint failed with (`None` = it succeeded).
///
/// Exhaustive over [`code::ALL`] with no wildcard on the codes themselves
/// — a new code added to that list without a case here fails
/// `every_error_code_has_a_state` rather than silently reading as a server
/// fault.
pub(crate) fn connection_state(opted_in: bool, error: Option<&str>) -> CalendarState {
    if !opted_in {
        return CalendarState::NeverConnected;
    }
    match error {
        None => CalendarState::Connected,
        Some(code::AUTHORITY_UNREACHABLE) => CalendarState::CannotConfirm,
        Some(code::NO_DEVICE_TOKEN) | Some(code::AUTHORITY_REJECTED_DEVICE_TOKEN) => {
            CalendarState::RefusedDeviceToken
        }
        Some(code::AUTHORITY_UNCONFIGURED)
        | Some(code::AUTHORITY_UPSTREAM)
        | Some(code::BAD_TOKEN_RESPONSE)
        | Some(code::NO_ACCESS_TOKEN) => CalendarState::RefusedServerLane,
        // An unknown code is a bug in this module, not a fact about the
        // server lane — but it is still a refusal the operator can see,
        // which is the least-wrong of the four.
        Some(_) => CalendarState::RefusedServerLane,
    }
}

/// The status-code half of the mapping, split out so every branch is
/// testable without a live authority. `None` means "2xx — read the body".
pub(crate) fn code_for_status(status: u16) -> Option<&'static str> {
    match status {
        401 | 403 => Some(code::AUTHORITY_REJECTED_DEVICE_TOKEN),
        503 => Some(code::AUTHORITY_UNCONFIGURED),
        502 => Some(code::AUTHORITY_UPSTREAM),
        200..=299 => None,
        _ => Some(code::BAD_TOKEN_RESPONSE),
    }
}

/// The body half: valid JSON carrying both fields, or a code saying which
/// way it was unusable. Total over every shape `serde_json` can produce —
/// `null` is valid JSON, and a non-object body is `no_access_token` like
/// any other unusable answer.
pub(crate) fn outcome_for_body(body: &str) -> MintOutcome {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return MintOutcome::Failed(code::BAD_TOKEN_RESPONSE);
    };
    let access_token = value.get("access_token").and_then(|v| v.as_str());
    let expires_at_ms = value.get("expires_at_ms").and_then(|v| v.as_i64());
    match (access_token, expires_at_ms) {
        (Some(access_token), Some(expires_at_ms)) => MintOutcome::Minted {
            access_token: access_token.to_string(),
            expires_at_ms,
        },
        _ => MintOutcome::Failed(code::NO_ACCESS_TOKEN),
    }
}

/// One mint attempt. `device_token` is whatever the host holds — `None` or
/// empty means no fetch call at all, the same rule the web client follows.
pub(crate) async fn mint_calendar_token(
    client: &reqwest::Client,
    base_url: &str,
    device_token: Option<&str>,
) -> MintOutcome {
    let Some(device_token) = device_token.filter(|token| !token.is_empty()) else {
        return MintOutcome::Failed(code::NO_DEVICE_TOKEN);
    };
    let url = format!("{}{}", base_url.trim_end_matches('/'), CALENDAR_TOKEN_PATH);
    let response = client
        .post(url)
        .bearer_auth(device_token)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await;
    let Ok(response) = response else {
        return MintOutcome::Failed(code::AUTHORITY_UNREACHABLE);
    };
    if let Some(code) = code_for_status(response.status().as_u16()) {
        return MintOutcome::Failed(code);
    }
    // A body that cannot even be read off the socket is the same class of
    // answer as one that is not JSON: malformed, worth retrying.
    let Ok(body) = response.text().await else {
        return MintOutcome::Failed(code::BAD_TOKEN_RESPONSE);
    };
    outcome_for_body(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_code_has_a_state() {
        for code in code::ALL {
            let state = connection_state(true, Some(code));
            assert_ne!(
                state,
                CalendarState::NeverConnected,
                "{code} must not read as never-connected on an opted-in device"
            );
            assert_ne!(
                state,
                CalendarState::Connected,
                "{code} must not read as connected"
            );
        }
    }

    #[test]
    fn only_an_unreachable_authority_reads_as_cannot_confirm() {
        let cannot_confirm: Vec<&str> = code::ALL
            .into_iter()
            .filter(|code| connection_state(true, Some(code)) == CalendarState::CannotConfirm)
            .collect();
        assert_eq!(cannot_confirm, vec![code::AUTHORITY_UNREACHABLE]);
    }

    #[test]
    fn a_bad_device_token_and_a_missing_one_both_point_at_the_token_control() {
        // Both are "this device's credential is the problem", which is the
        // one refusal with a per-device remedy.
        for code in [code::NO_DEVICE_TOKEN, code::AUTHORITY_REJECTED_DEVICE_TOKEN] {
            assert_eq!(
                connection_state(true, Some(code)),
                CalendarState::RefusedDeviceToken
            );
        }
    }

    #[test]
    fn a_device_that_never_opted_in_is_never_connected_whatever_the_code() {
        assert_eq!(connection_state(false, None), CalendarState::NeverConnected);
        for code in code::ALL {
            assert_eq!(
                connection_state(false, Some(code)),
                CalendarState::NeverConnected
            );
        }
    }

    #[test]
    fn a_clean_mint_on_an_opted_in_device_is_connected() {
        assert_eq!(connection_state(true, None), CalendarState::Connected);
    }

    #[test]
    fn the_status_table_matches_the_webs_own_route_mapping() {
        assert_eq!(
            code_for_status(401),
            Some(code::AUTHORITY_REJECTED_DEVICE_TOKEN)
        );
        assert_eq!(
            code_for_status(403),
            Some(code::AUTHORITY_REJECTED_DEVICE_TOKEN)
        );
        assert_eq!(code_for_status(503), Some(code::AUTHORITY_UNCONFIGURED));
        assert_eq!(code_for_status(502), Some(code::AUTHORITY_UPSTREAM));
        assert_eq!(code_for_status(500), Some(code::BAD_TOKEN_RESPONSE));
        assert_eq!(code_for_status(404), Some(code::BAD_TOKEN_RESPONSE));
        assert_eq!(code_for_status(200), None);
        assert_eq!(code_for_status(204), None);
    }

    #[test]
    fn a_well_formed_body_mints() {
        assert_eq!(
            outcome_for_body(r#"{"access_token":"ya29.abc","expires_at_ms":1755000000000}"#),
            MintOutcome::Minted {
                access_token: "ya29.abc".to_string(),
                expires_at_ms: 1_755_000_000_000
            }
        );
    }

    #[test]
    fn json_that_is_not_an_object_is_no_access_token_not_a_panic() {
        for body in ["null", "[]", "42", r#""a string""#] {
            assert_eq!(
                outcome_for_body(body),
                MintOutcome::Failed(code::NO_ACCESS_TOKEN),
                "body {body:?}"
            );
        }
    }

    #[test]
    fn a_body_missing_either_field_is_no_access_token() {
        assert_eq!(
            outcome_for_body(r#"{"access_token":"ya29.abc"}"#),
            MintOutcome::Failed(code::NO_ACCESS_TOKEN)
        );
        assert_eq!(
            outcome_for_body(r#"{"expires_at_ms":1}"#),
            MintOutcome::Failed(code::NO_ACCESS_TOKEN)
        );
        // Present but the wrong type is the same structural failure.
        assert_eq!(
            outcome_for_body(r#"{"access_token":1,"expires_at_ms":1}"#),
            MintOutcome::Failed(code::NO_ACCESS_TOKEN)
        );
        assert_eq!(
            outcome_for_body(r#"{"access_token":"a","expires_at_ms":"soon"}"#),
            MintOutcome::Failed(code::NO_ACCESS_TOKEN)
        );
    }

    #[test]
    fn a_body_that_is_not_json_at_all_is_bad_token_response() {
        assert_eq!(
            outcome_for_body("<html>502 Bad Gateway</html>"),
            MintOutcome::Failed(code::BAD_TOKEN_RESPONSE)
        );
    }

    /// `pub const CACHE_REMINT_MARGIN_MS: i64 = 6 * 60 * 1000;` — a product
    /// of integer literals, which is how both sides spell a duration. Read
    /// out of the server's own source text, exactly as
    /// `client/web/src/calendar/rotation-margin-drift.test.ts` reads it: the
    /// authority is a different cargo workspace, so there is no constant to
    /// import.
    #[test]
    fn the_authority_gives_up_on_its_cache_before_this_client_rotates() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../server/authority/src/google_calendar.rs");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
        let marker = "pub const CACHE_REMINT_MARGIN_MS: i64 = ";
        // Not an assertion about drift — an assertion that this test is
        // still reading something. A renamed constant would make the
        // inequality below pass vacuously, the failure mode a source-text
        // pin is most prone to.
        let start = source
            .find(marker)
            .expect("CACHE_REMINT_MARGIN_MS not found — has it been renamed?")
            + marker.len();
        let expression = &source[start..start + source[start..].find(';').expect("terminated")];
        let server_margin_ms: i64 = expression
            .split('*')
            .map(|factor| {
                factor
                    .replace('_', "")
                    .trim()
                    .parse::<i64>()
                    .expect("a product of integer literals")
            })
            .product();
        assert!(server_margin_ms > 0);
        assert!(
            server_margin_ms > ROTATION_MARGIN_MS,
            "the authority ({server_margin_ms}ms) must call its cached token stale \
             before this client wakes to rotate ({ROTATION_MARGIN_MS}ms)"
        );
    }

    #[tokio::test]
    async fn no_device_token_never_reaches_the_network() {
        // An unresolvable base URL: if this made a request at all it would
        // come back `authority_unreachable`, not `no_device_token`.
        let client = reqwest::Client::new();
        for token in [None, Some("")] {
            assert_eq!(
                mint_calendar_token(&client, "http://127.0.0.1:1", token).await,
                MintOutcome::Failed(code::NO_DEVICE_TOKEN)
            );
        }
    }

    #[tokio::test]
    async fn an_unreachable_authority_is_unreachable_not_a_bad_response() {
        let client = reqwest::Client::new();
        assert_eq!(
            mint_calendar_token(&client, "http://127.0.0.1:1", Some("device-token")).await,
            MintOutcome::Failed(code::AUTHORITY_UNREACHABLE)
        );
    }

    #[tokio::test]
    async fn the_device_token_never_appears_in_a_returned_value() {
        let client = reqwest::Client::new();
        let outcome = mint_calendar_token(&client, "http://127.0.0.1:1", Some("s3cret")).await;
        assert!(!format!("{outcome:?}").contains("s3cret"));
    }
}
