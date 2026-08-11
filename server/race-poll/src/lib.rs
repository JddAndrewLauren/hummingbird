//! `hummingbird-race-poll`: the out-of-process poller behind the next-race
//! standing question (#266, ADR-0009/ADR-0015).
//!
//! It is **two one-shot processes over one library**:
//!
//! ```text
//! race-schedule-poll  0 */6 * * *   GET api.jolpi.ca → POST /api/snapshots
//! race-alert-poll     */15 * * * *  GET /api/snapshots → POST /api/alerts
//! ```
//!
//! # Why two crons is not a competing clock
//!
//! ADR-0009 says "the same cron that refreshes the snapshot is what may
//! later machine-raise" the alert. This lane splits it, because it has two
//! jobs and only one of them needs the network: refreshing the season needs
//! Jolpica and changes a few times a year, while "is a race starting inside
//! the lead time" is a pure function of (stored schedule, now).
//!
//! What the split buys, in order: `polled_every_ms` stays **6h**, so
//! ADR-0015's `2 × cadence` staleness rule works unchanged (12h) — a single
//! `*/15` cron for both would have needed a 15-minute declared cadence,
//! whose `2 ×` is a 30-minute threshold that GitHub Actions' own cron jitter
//! trips routinely, forcing an ADR-0015 amendment to carve this lane out.
//! **The split deletes a special case instead of adding one.** It also drops
//! Jolpica traffic from 96 to 4 requests a day, and keeps alert precision at
//! ±15 minutes, the first cadence at which a 90-minute lead means what it
//! says. ADR-0009's actual principle — *the question answers, the alert
//! interrupts, never one mechanism* — is about not making the pane do the
//! interrupting, and survives the split intact.
//!
//! CLAUDE.md's competing-clock ban is about **two clocks owning one
//! cadence** (supercronic vs. Actions for the sweeper). Here two clocks own
//! two different jobs, exactly as `graph-poll` already ships two binaries.
//!
//! # The split inside the crate
//!
//! Everything decidable is here in the lib and natively tested; each
//! `main.rs` holds only `std::env`, the fetches and the writes. In order of
//! dependence:
//!
//! * [`binding`] — which series to poll, read out of `settings`, and which
//!   of them this build has an adapter for;
//! * [`schedule`] — the Jolpica response, read against a saved verbatim
//!   sample of the real thing;
//! * [`body`] — the `race-schedule/v1` payload and ADR-0015's envelope
//!   around it, pinned byte-for-byte by `tests/fixtures/golden-body.json`;
//! * [`next`] — pure: given a season and a `now`, which race is inside the
//!   lead time?
//! * [`alert`] — what that race would post, returned rather than performed.
//!
//! # Four outcomes, and only two are failures
//!
//! 1. **Feed unreachable / 5xx / timeout** → exit non-zero, write nothing.
//!    `fetched_at` freezes, the pane bands stale at 12h, Actions emails the
//!    failed run. `city-waste`'s posture verbatim: the lane is
//!    self-monitoring, so a stalled poller is loud within half a day.
//! 2. **200 with a shape the parser does not recognise** → the same: a named
//!    per-field error ([`schedule::ScheduleError`]), exit non-zero, write
//!    nothing. Fail loudly on the first poll rather than writing something
//!    plausible.
//! 3. **Off-season** (fetch fine, every race in the past) → **not a
//!    failure.** Write the snapshot normally; the body carries the season
//!    with no future events and the pane answers "no races scheduled".
//!    Routing it through the failure path would have the pane say "stale,
//!    cannot answer" all winter, when the true answer is "nothing is
//!    scheduled" — different facts, and they must not share a branch.
//! 4. **Binding unset, or holding a non-text value** → write nothing, exit
//!    **0** ([`binding::BindingProblem::is_unconfigured`]). Not configured
//!    is not broken; the same discrimination as 3.
//!
//! There is deliberately no alert raised when the feed dies: that needs new
//! alert semantics for a condition the pane already surfaces.

pub mod alert;
pub mod binding;
pub mod body;
pub mod next;
pub mod schedule;

#[cfg(test)]
mod tests {
    /// This crate holds an HTTP client, and the wasm32 build must never see
    /// one. `hummingbird-authority-worker` is the only crate that builds for
    /// wasm32, and the only structural protection is that it does not depend
    /// on this one — which is invisible from here, so the guard is written
    /// from the other side: assert the *worker's* manifest does not name us.
    #[test]
    fn the_wasm32_worker_does_not_depend_on_this_crate() {
        let worker = include_str!("../../worker/Cargo.toml");
        assert!(
            !worker.contains("race-poll"),
            "server/worker builds for wasm32; this crate's HTTP client has no \
             business in that build"
        );
    }
}
