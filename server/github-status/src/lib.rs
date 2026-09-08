//! `hummingbird-github-status`: the out-of-process poller behind the
//! `github-hummingbird/v1` standing question (#314, ADR-0017 decision 2).
//!
//! Every half hour it reads this repo's own committed `.github/workflows/*.yml`
//! for every workflow carrying a `schedule:` trigger, asks GitHub's own
//! Actions API for each one's recent run history, and writes one
//! `context_snapshots` row per workflow — `key` = the workflow's file name
//! — under one shared source string (`GITHUB_HUMMINGBIRD_V1`).
//!
//! # Why this exists
//!
//! GitHub disables a scheduled workflow after 60 days of repo inactivity —
//! one silent event that stops every scheduled workflow at once, invisible
//! to any probe of the *services* those workflows write to. `city-waste`'s
//! own lane is self-monitoring (its pane bands stale and refuses to answer
//! within a day of a missed poll); the four evaluated-stream pollers
//! (`gmail-poll`, `calendar-poll`, `graph-mail-poll`, `graph-calendar-poll`)
//! have no equivalent tell — a missed poll for any of them produces no
//! visible gap, since the alerts it would have raised simply never exist.
//! This crate makes the cron layer itself observable, for every scheduled
//! workflow in the repo, not just those four.
//!
//! # Where it runs, and why not somewhere else
//!
//! **Out of process, not in the Durable Object's alarm.** `server/worker`
//! has no test harness of any kind, so anything expressed there is untested
//! by construction — `city-waste`'s own header states this, and it
//! generalises to every poller here. **Moved off Actions `schedule:` at
//! #774**, onto the sweeper's own `crontab`; `.github/workflows/
//! github-status.yml` keeps a `workflow_dispatch:` trigger for a manual run
//! against a fresh checkout, but no longer drives the standing cadence.
//!
//! **Needs no credential of its own to read this repo's run history when
//! run through that workflow** — the automatic per-run `GITHUB_TOKEN` with
//! `permissions: actions: read` covers same-repo run history there. **The
//! standing run on `hummingbird-sweeper` has no such automatic token**, so
//! it authenticates with `GH_STATUS_PAT` instead (a fine-grained PAT, Fly
//! secret, mapped onto `GITHUB_TOKEN` by `crontab`) — `HB_INGEST_TOKEN`
//! (this poller's own `ingest`-scope credential for `POST /api/snapshots`)
//! is the only secret either path shares.
//!
//! **This poller cannot detect its own death** — a dead run writes nothing,
//! and nothing here can make it write something about itself. What detects
//! that is the pane's own staleness: once it stops running, every row it
//! owns simply stops moving, and `Freshness` bands the whole pane stale
//! within ~6h, exactly the tell `city-waste`'s own pane already relies on
//! for itself. Before #774 that death could only be GitHub's 60-day
//! auto-disable of an inactive repo's `schedule:` triggers; running on
//! `hummingbird-sweeper` instead trades that specific failure mode for
//! whatever can take the Fly machine itself down, which the pane's
//! staleness read is blind to the cause of either way.
//!
//! # The split inside it
//!
//! Everything decidable is here in the lib and natively tested; `main.rs`
//! (`src/bin/main.rs`) holds only `std::env`, the workflow-directory scan,
//! the fetches and the writes.
//!
//! * [`manifest`] — reading a workflow file for its `name:` and its
//!   `schedule:` cron strings, so a new scheduled workflow enrolls itself
//!   the moment its file lands, with no second list to maintain;
//! * [`cron`] — the declared cadence a cron string names, for the three
//!   shapes this repo's own files use;
//! * [`instant`] — turning a GitHub API timestamp into epoch ms, dependency-
//!   free, on `race-poll`'s own reasoning for its own stamps;
//! * [`runs`] — reading the runs-list API response and deciding one
//!   workflow's verdict from it: its last run (any event) and its last
//!   *scheduled* success, kept apart so a green manual run can never mask a
//!   dead cron;
//! * [`body`] — the snapshot payload, one half of a cross-language contract
//!   with `github.ts`.
//!
//! There is no `binding` module, on `kimi-balance`'s own reasoning: this
//! question reads no per-device `settings` row — which workflows exist and
//! how often they run are facts about the repo, not something a device
//! configures.

pub mod body;
pub mod cron;
pub mod instant;
pub mod manifest;
pub mod runs;

#[cfg(test)]
mod tests {
    /// This crate holds an HTTP client, and the wasm32 build must never see
    /// it. `hummingbird-authority-worker` is the only crate that builds for
    /// wasm32, and the only structural protection is that it does not
    /// depend on this one — which is invisible from here, so the guard is
    /// written from the other side, exactly as every other poller's own
    /// test is (`kimi-balance`, `city-waste`).
    #[test]
    fn the_wasm32_worker_does_not_depend_on_this_crate() {
        let worker = include_str!("../../worker/Cargo.toml");
        assert!(
            !worker.contains("github-status"),
            "server/worker builds for wasm32; this crate's HTTP client has \
             no business in that build"
        );
    }
}
