//! `POST /api/google/calendar_token` and its write-scoped sibling
//! `POST /api/google/calendar_write_token` — the authority's authorization
//! verdicts for the two calendar-token mints (#577/#582, ADR-0031).
//!
//! Same shape as `skills::run_verdict`, for a related but simpler
//! reason. `POST /api/skills/run`'s egress is proxied *above* the Durable
//! Object because of a genuine cycle (`microtask.apply` calls back into
//! this same object). The calendar-token exchange has no such cycle — it
//! only ever calls out to `oauth2.googleapis.com` — but the exchange is
//! still async (a `fetch`) where this crate's `handle()` is sync, and the
//! DO-instance cache the exchange shares with every device belongs to the
//! runtime shim, not this pure crate. So the DO answers the one question it
//! can: **is this caller allowed to mint a calendar token?** A 204 here
//! means "yes, proceed" — the `wasm32` shim then does the actual exchange
//! (or serves its cache) and builds the real 200/503/502 the caller sees,
//! entirely from `hummingbird_authority::google_calendar`'s pure functions.
//! Empty body, no delivery, no write — a tap must not dirty the sync
//! cursor, matching `authenticate`'s no-meta-bump `last_seen` stamp.

use super::ApiResponse;

pub fn verdict() -> ApiResponse {
    ApiResponse {
        status: 204,
        body: String::new(),
        deliveries: Vec::new(),
        principal_id: None,
        cycle_id: None,
        request_id: String::new(),
    }
}

/// The token ids allowed to mint a **write**-scoped calendar token
/// (ADR-0031). Today one: the OpenClaw agent on the operator's gateway.
///
/// A list with a membership test, not an equality against one id, and
/// deliberately a reviewed `const` rather than a secret or a settings row.
/// The agent is expected to move in-app eventually (a runner op, a chat
/// surface in the web app); when it does, **the credential and the route do
/// not change — this list does**, as a one-line diff plus a test, rather
/// than an invisible `wrangler secret put` or a rediscovery of the whole
/// decision.
pub const CALENDAR_WRITE_HOLDERS: &[&str] = &["openclaw-agent"];

/// The write mint's verdict: 204 for a listed holder, **403 for every other
/// `device` token**, which is the whole security claim of ADR-0031 — a
/// browser holds a `device` token and must not be able to edit the
/// operator's calendar. The scope matrix cannot express that (there is no
/// scope meaning "this one host"), so it narrows here, exactly as `#145`'s
/// ingest source-binding and `rules::event_kinds_readable_by` do.
///
/// **No path answers 401** (ADR-0028's rule): a 401 means "your device
/// token is bad" and would make a client discard a working one. A caller
/// that authenticated fine but is not the agent is out of scope — 403,
/// empty body, exactly like the scope matrix's own refusal.
pub fn write_verdict(token_id: &str) -> ApiResponse {
    let allowed = CALENDAR_WRITE_HOLDERS.contains(&token_id);
    ApiResponse {
        status: if allowed { 204 } else { 403 },
        body: String::new(),
        deliveries: Vec::new(),
        principal_id: None,
        cycle_id: None,
        request_id: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_holder_reaches_the_verdict() {
        for id in CALENDAR_WRITE_HOLDERS {
            let resp = write_verdict(id);
            assert_eq!(resp.status, 204, "{id} is a listed holder");
            assert_eq!(resp.body, "");
        }
    }

    /// The membership semantics, not an equality: every other device token
    /// — a browser's, the runner's, a phone's — is refused.
    #[test]
    fn any_other_token_id_is_a_403_with_no_body() {
        for id in ["device-mac", "runner", "openclaw-agent-2", "", "OPENCLAW-AGENT"] {
            let resp = write_verdict(id);
            assert_eq!(resp.status, 403, "{id} must not reach the write mint");
            assert_eq!(resp.body, "", "403 must leak no body");
        }
    }

    /// ADR-0028's rule, pinned on the one route that can refuse a caller
    /// whose device token is perfectly good.
    #[test]
    fn no_verdict_is_ever_a_401() {
        for id in ["openclaw-agent", "device-mac"] {
            assert_ne!(write_verdict(id).status, 401);
        }
    }
}
