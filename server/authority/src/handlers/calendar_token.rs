//! `POST /api/google/calendar_token` — the authority's authorization
//! verdict for the calendar-token mint (#577/#582).
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
    }
}
