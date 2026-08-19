//! The runtime half of the calendar-token mint (#577/#582): the token-
//! endpoint `fetch`, and the one-`RefCell`-per-DO-instance cache the whole
//! "N devices collapse to one exchange per hour" cost story — and its
//! steady-state cap on a stolen `device` token — depends on. What that cap
//! does and does not cover (overlapping misses, failed exchanges) is stated
//! where the boundary lives, in [`hummingbird_authority::google_calendar`].
//!
//! Everything else — the grant body, the freshness boundary, the response
//! DTO, and every failure's status and prose — lives in that pure surface,
//! where a native test can execute it. This file holds zero *policy*: no
//! wording, no status arithmetic, no freshness boundary. The only literals
//! it may carry are the names of its own Worker bindings and the request's
//! content type, exactly as `fcm.rs` carries `FCM_SERVICE_ACCOUNT` and the
//! same form encoding — a binding name is what this file *is*, and both
//! shims spell the content type at their `post` call so the two token legs
//! stay one shape. The discipline matters because `server/worker` has no test
//! harness of any kind, so anything expressed here is untested by
//! construction.
//!
//! # Credential
//!
//! `GOOGLE_CALENDAR_CLIENT_ID` / `_SECRET` / `_REFRESH_TOKEN` — three
//! Worker secrets (#577's credential decision: a *second*, dedicated
//! `calendar.readonly`-only refresh token, never the shared
//! Gmail-modify-carrying one #486 minted — Google's `refresh_token` grant
//! does not honour down-scoping, so reusing the shared credential would
//! hand a stolen `device` token a bearer that can modify the operator's
//! Gmail). Set with `wrangler secret put`, never `wrangler.toml`, never the
//! repo, and never GitHub Actions — beside `ADMIN_SECRET` and
//! `FCM_SERVICE_ACCOUNT`. Any one missing means [`CalendarMinter::from_env`]
//! returns `None` and the route fails closed with a 503, never a 401.

use std::cell::RefCell;

use hummingbird_authority::{
    calendar_invalid_grant, calendar_refresh_grant_body, calendar_token_success,
    calendar_unreachable, calendar_upstream_status, is_invalid_grant, parse_access_token,
    token_is_fresh, AccessToken, OAUTH_TOKEN_URL,
};
use worker::*;

use crate::http::post;

/// Holds the three-part refresh-token credential and caches the access
/// token it buys.
pub struct CalendarMinter {
    client_id: String,
    client_secret: String,
    refresh_token: String,
    /// Not persisted (see the module doc's cost/security argument, the same
    /// call `fcm.rs`'s `FcmSender` already made): an evicted DO instance
    /// simply mints a fresh token on its next request.
    cached: RefCell<Option<AccessToken>>,
}

impl CalendarMinter {
    /// Reads the three secrets. `None` — any one unset — means the lane is
    /// not configured; the caller answers the 503 the pure crate names for
    /// that case.
    pub fn from_env(env: &Env) -> Option<CalendarMinter> {
        let client_id = env.secret("GOOGLE_CALENDAR_CLIENT_ID").ok()?.to_string();
        let client_secret = env.secret("GOOGLE_CALENDAR_CLIENT_SECRET").ok()?.to_string();
        let refresh_token = env.secret("GOOGLE_CALENDAR_REFRESH_TOKEN").ok()?.to_string();
        Some(CalendarMinter {
            client_id,
            client_secret,
            refresh_token,
            cached: RefCell::new(None),
        })
    }

    /// The status and body to answer the caller with: the cache, a fresh
    /// exchange, or whatever that exchange's failure means. Every branch
    /// defers its wording and status entirely to
    /// `hummingbird_authority::google_calendar`.
    pub async fn token_response(&self, now_ms: i64) -> (u16, String) {
        // Cloned out of the RefCell rather than borrowed across the await
        // below — a borrow held over a suspension point would panic the
        // moment two requests overlapped (the same note as `fcm.rs`'s
        // `access_token`).
        let cached = self.cached.borrow().clone();
        if let Some(token) = cached {
            if token_is_fresh(&token, now_ms) {
                return calendar_token_success(&token);
            }
        }

        let body = calendar_refresh_grant_body(
            &self.client_id,
            &self.client_secret,
            &self.refresh_token,
        );
        let (status, response_body) = match post(
            OAUTH_TOKEN_URL,
            &body,
            None,
            "application/x-www-form-urlencoded",
        )
        .await
        {
            Ok(pair) => pair,
            Err(_) => return calendar_unreachable(),
        };

        if let Some(token) = parse_access_token(&response_body, now_ms) {
            *self.cached.borrow_mut() = Some(token.clone());
            return calendar_token_success(&token);
        }
        if is_invalid_grant(&response_body) {
            return calendar_invalid_grant();
        }
        calendar_upstream_status(status)
    }
}
