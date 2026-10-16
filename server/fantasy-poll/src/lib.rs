//! `hummingbird-fantasy-poll`: the out-of-process poller behind the Fantasy
//! standing question (plan #713's fantasy lane, ADR-0015, ADR-0034).
//!
//! **This is the lib half (#853)**: every decision the poller makes, over
//! typed inputs, before anything talks to Yahoo. The Yahoo wire format,
//! OAuth, the HTTP and the binary are #854's; the pane's bands are #855's;
//! the waivers subject is #719's and the alerts are #720/#721's.
//!
//! # One binary with a conditional fetch, not race-poll's two
//!
//! `race-poll` splits into a six-hourly fetcher and a `*/15` alert binary
//! because "is a race starting inside the lead time" is a pure function of
//! (stored schedule, now). **Lineup validity is not**: the operator fixes a
//! lineup out of band, in Yahoo, where hummingbird cannot see. A `*/15`
//! alert binary over a six-hour-old snapshot would escalate `normal → high`
//! — ringing the phone a second time — for a lineup fixed hours earlier,
//! which teaches the operator the alerts are wrong at exactly the moment
//! they are asked to trust them. So there is one `*/15` binary, and it hits
//! Yahoo **only** when [`gate`] says it matters.
//!
//! The declared cadence still stays **6h** ([`body::POLLED_EVERY_MS`]): a
//! guaranteed floor, since the gate always fetches once the last fetch is
//! over six hours old. ADR-0015's `2 × cadence` staleness rule therefore
//! reads 12h and Actions' cron jitter never trips it — the property
//! race-poll's split was bought to preserve, kept here without the split.
//! ADR-0015 carries the floor reading inline (#853).
//!
//! # The split inside the crate
//!
//! Everything decidable is here and natively tested; the binary will hold
//! only `std::env`, the fetches and the writes. In order of dependence:
//!
//! * [`enabled`] — whether the Fantasy question is asked at all (#715).
//!   Off means nothing else runs and nothing is fetched.
//! * [`binding`] — which leagues the operator bound, and which of them
//!   belong to the current season.
//! * [`roster`] — the typed inputs the live half parses Yahoo into.
//! * [`discovery`] — given the current season's teams, which bound leagues
//!   get a snapshot, and the discovered-league list itself.
//! * [`validity`] — whether a starting slot is decidably invalid.
//! * [`deadline`] — the last moment an invalid slot can still be fixed.
//! * [`body`] — the `yahoo-lineup/v1` snapshot and the discovered-leagues
//!   snapshot, each pinned byte for byte by a golden fixture.
//! * [`gate`] — pure: given what is stored and `now`, is a Yahoo fetch
//!   worth making this run?
//! * [`clamp`] — when a ring may fire: moved back into the waking window,
//!   never forwards, or not at all (ADR-0035, #856). The crate's tzdb.
//!
//! **Explicitly not here: any opinion about football.** A lineup with
//! every slot filled, nobody on bye and nobody Out is valid, whether or not
//! it is a good lineup (#713's "bad but legal").

pub mod binding;
pub mod body;
pub mod clamp;
pub mod deadline;
pub mod discovery;
pub mod enabled;
pub mod gate;
pub mod roster;
pub mod validity;

#[cfg(test)]
mod tests {
    /// This crate will hold an HTTP client (#854) and holds a tzdb (`clamp`,
    /// #856), and the wasm32 build must never see either. `hummingbird-authority-worker` is
    /// the only crate that builds for wasm32, and the only structural
    /// protection is that it does not depend on this one — invisible from
    /// here, so the guard is written from the other side, as `race-poll`'s.
    #[test]
    fn the_wasm32_worker_does_not_depend_on_this_crate() {
        let worker = include_str!("../../worker/Cargo.toml");
        assert!(
            !worker.contains("fantasy-poll"),
            "server/worker builds for wasm32; this crate has no business in that build"
        );
    }
}
