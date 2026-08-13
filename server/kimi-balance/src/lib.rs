//! `hummingbird-kimi-balance`: the out-of-process poller behind the Kimi
//! balance standing question (#313, ADR-0017).
//!
//! Every six hours it reads Moonshot's own account-balance endpoint and
//! writes the answer as a `context_snapshots` row — one gauge, replaced
//! wholesale each poll, never drained (ADR-0017 decision 5). The pane
//! (`client/web/src/screens/kimi-pane/`) renders the row; this crate is the
//! half that fills it.
//!
//! # Where it runs, and why not somewhere else
//!
//! **Out of process, not in the Durable Object's alarm.** `server/worker`
//! has no test harness of any kind, so anything expressed there is untested
//! by construction — the same reasoning `server/city-waste`'s header
//! states, and ADR-0011's amendment generalised for every evaluated-stream
//! poller. A GitHub Actions cron drives it
//! (`.github/workflows/kimi-balance.yml`).
//!
//! Not `authority` (no HTTP client, no environment, by construction), and
//! not `domain` (`client/core` compiles that to wasm32, and an HTTP client
//! has no business in that budget). It must never become a dependency of
//! `hummingbird-authority-worker`.
//!
//! # The one design fact this crate exists to respect
//!
//! **The host is config, not a constant.** `platform.kimi.ai` and
//! `platform.kimi.com` keys are completely independent, and the endpoint
//! must match the platform the key was minted on — so `main.rs` reads
//! `MOONSHOT_API_BASE_URL` from the environment rather than hardcoding
//! either host. A hardcoded host is a silent 401 the day the key is
//! re-minted on the other one.
//!
//! # The split inside it
//!
//! Everything decidable is here in the lib and natively tested; `main.rs`
//! holds only `std::env`, one GET and one POST.
//!
//! * [`balance`] — reading the endpoint's response, including its own
//!   `code != 0` refusal (`exceeded_current_quota_error`'s own shape);
//! * [`body`] — the snapshot payload, one half of a cross-language contract
//!   with `kimi.ts`.
//!
//! There is no `binding` module, unlike `city-waste`'s: this question reads
//! no per-device `settings` row at all — the credential and the host are
//! both operator-set Actions secrets/variables, never something a device
//! configures (the brief's own "Operator gate (not for the agent)").

pub mod balance;
pub mod body;

#[cfg(test)]
mod tests {
    /// This crate holds an HTTP client, and the wasm32 build must never see
    /// it. `hummingbird-authority-worker` is the only crate that builds for
    /// wasm32, and the only structural protection is that it does not
    /// depend on this one — which is invisible from here, so the guard is
    /// written from the other side, exactly as `city-waste`'s own test is.
    #[test]
    fn the_wasm32_worker_does_not_depend_on_this_crate() {
        let worker = include_str!("../../worker/Cargo.toml");
        assert!(
            !worker.contains("kimi-balance"),
            "server/worker builds for wasm32; this crate's HTTP client has \
             no business in that build"
        );
    }
}
