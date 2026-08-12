//! `POST /api/skills/run` — the Durable Object's half of the skill-runner
//! proxy (#273, ADR-0018).
//!
//! The DO answers exactly one question here: **is this caller allowed to
//! run a skill?** It never sees the body, never talks to the runner, and
//! writes nothing. The egress — and the streaming response — happens in the
//! `wasm32` shim, *above* the DO dispatch, because `microtask.apply` calls
//! back into `/api/steps` on this same object: routing the run through the
//! DO would make it await a subrequest only it can answer.
//!
//! So the shim sends a **bodiless preflight** to this route and reads the
//! verdict off the status: a 204 means proceed, anything else is the
//! answer to return to the caller unchanged. The authorization work itself
//! is already done by the time this function runs — `route` authenticates
//! and applies `auth::permitted`, whose default arm is device-only, which
//! is exactly this route's rule.

use super::ApiResponse;

/// The verdict: a 204 reached only by an authenticated, in-scope caller.
/// Empty body, no delivery, no write — a tap must not dirty the sync
/// cursor, and `authenticate` stamps `last_seen` without bumping `meta`.
pub fn run_verdict() -> ApiResponse {
    ApiResponse {
        status: 204,
        body: String::new(),
        deliveries: Vec::new(),
    }
}
