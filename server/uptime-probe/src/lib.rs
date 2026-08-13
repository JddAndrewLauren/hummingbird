//! `hummingbird-uptime-probe`: the out-of-process poller behind the
//! `uptime/v1` standing question (#315, ADR-0017 decisions 2/3/4/6) — **the
//! only poller in this design that holds no credential at all** beyond its
//! own `ingest` token.
//!
//! Once an hour it reads the committed `services.json` and, for each
//! declared service, issues one unauthenticated HTTP request and writes one
//! `context_snapshots` row — `key` = the service's own `id` — under one
//! shared source string (`UPTIME_V1`).
//!
//! # The signal is the right refusal, not a 200
//!
//! `runner/src/server.js:43` gates `POST /run` on a bearer token and
//! authenticates *before* dispatch; the authority does the same. An
//! unauthenticated request against either therefore returns **401 with an
//! empty body** — proof that DNS resolved, TLS terminated, the machine
//! booted, the process is listening, `/api/*` still beats the SPA fallback,
//! and auth is switched on, with no secret anywhere (ADR-0017 decision 3).
//! The web origin is the one service this poller expects a plain 200 from:
//! there is no auth gate in front of the SPA shell to refuse against.
//!
//! # Two traps this slice exists to avoid
//!
//! **Fly machine state is not a health signal** (ADR-0017 decision 6). The
//! runner's `min_machines_running = 0` (`runner/fly.toml:21`) makes "no
//! machine running" its healthy idle state; reading machine state instead
//! of probing HTTP would show it down most of the day. The 401 probe cold
//! -boots the runner, which is also the only check that proves it *can*
//! boot — the inversion this ADR names: probing hourly, not every fifteen
//! minutes, because each probe costs the runner a wake-up.
//!
//! **The sweeper has no line here at all** (ADR-0017 decision 4). It is
//! live but never opens a listener — `docs/sweeper.md` forbids
//! `[http_service]`/`[[services]]` so Fly's autostop can't suspend a sweep
//! mid-run — so no truthful `url`/`method`/`expect_status` triple can be
//! written for it and neither `expected` value would describe it honestly.
//! `services.json` carries exactly three services: `authority`, `web`,
//! `runner`.
//!
//! # Where it runs, and why not somewhere else
//!
//! **Out of process, not in the Durable Object's alarm.** `server/worker`
//! has no test harness of any kind, so anything expressed there is untested
//! by construction — every poller here states this. A GitHub Actions cron
//! drives it (`.github/workflows/uptime-probe.yml`).
//!
//! **Reachable ≠ functional** (ADR-0017 decision 3). A half-landed
//! migration still returns a correct 401. This lane answers "is the door
//! open", never "is everything correct behind it" — deep verification stays
//! the hand-run `smoke-prod.sh`.
//!
//! # The split inside it
//!
//! Everything decidable is here in the lib and natively tested; `main.rs`
//! (`src/bin/main.rs`) holds only `std::env`, the requests and the writes.
//!
//! * [`manifest`] — reading the committed `services.json`;
//! * [`verdict`] — the pure decision over (declared expectation, observed
//!   status, transport error) — agreement or divergence, never a band;
//! * [`body`] — the snapshot payload, one half of a cross-language contract
//!   with `uptime.ts`.
//!
//! There is no `binding` module, on `kimi-balance`'s own reasoning: this
//! question reads no per-device `settings` row — which services exist and
//! what they are expected to be doing are facts about the deployment,
//! reviewed in `services.json`, never something a device configures.

pub mod body;
pub mod manifest;
pub mod verdict;

#[cfg(test)]
mod tests {
    /// This crate holds an HTTP client, and the wasm32 build must never see
    /// it. `hummingbird-authority-worker` is the only crate that builds for
    /// wasm32, and the only structural protection is that it does not
    /// depend on this one — which is invisible from here, so the guard is
    /// written from the other side, exactly as every other poller's own
    /// test is (`kimi-balance`, `github-status`).
    #[test]
    fn the_wasm32_worker_does_not_depend_on_this_crate() {
        let worker = include_str!("../../worker/Cargo.toml");
        assert!(
            !worker.contains("uptime-probe"),
            "server/worker builds for wasm32; this crate's HTTP client has \
             no business in that build"
        );
    }
}
