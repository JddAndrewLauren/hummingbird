//! `hummingbird-city-waste`: the out-of-process poller behind the which-cans
//! standing question (#120, ADR-0015).
//!
//! Once a day it reads the council's collection page for one address, writes
//! the answer as a `context_snapshots` row, and — only on a week where the
//! collection has moved — raises one alert about it. The pane
//! (`client/web/src/screens/waste-pane/`) renders the row; this crate is the
//! half that fills it.
//!
//! # Where it runs, and why not somewhere else
//!
//! **Out of process, not in the Durable Object's alarm.** `server/worker` has
//! no test harness of any kind, so anything expressed there is untested by
//! construction — the same reasoning that split `authority/src/fcm.rs` from
//! `worker/src/fcm.rs`. A GitHub Actions cron drives it
//! (`.github/workflows/city-waste.yml`).
//!
//! Not `authority` (no HTTP client, no clock, no environment, by
//! construction), and not `domain` (`client/core` compiles that to wasm32,
//! and an HTML parser has no business in that budget). It must never become
//! a dependency of `hummingbird-authority-worker`.
//!
//! # The split inside it
//!
//! Everything decidable is here in the lib and natively tested; `main.rs`
//! holds only `std::env`, one GET and two POSTs. In order of dependence:
//!
//! * [`date`] — civil days as integers, plus the one zone-aware question
//!   (when does a collection day end *at the address*);
//! * [`cadence`] — the collection rhythm, one for the address, not one per
//!   bin;
//! * [`judge`] — materiality as **deviation from cadence**, never a diff
//!   against the previous poll. This is the design's load-bearing choice:
//!   see that module's header;
//! * [`body`] — the snapshot payload, one half of a cross-language contract
//!   with `waste.ts`;
//! * [`alert`] — what a deviation would post, returned rather than performed;
//! * [`binding`] — reading the address's page URL out of `settings`;
//! * [`page`] — the council's HTML, and the one module still unbuilt.
//!
//! # Two orderings that matter
//!
//! **Snapshot first, alert second.** A death between the two leaves the pane
//! showing the corrected day — which is the answer — with the alert catching
//! up on the next poll. The inverse leaves an alert about a slide the
//! snapshot does not show.
//!
//! **An unset or unusable binding is a clean exit with no writes at all**,
//! never a snapshot built from a guessed address. `GET /api/settings/:key`
//! answers 404 for an unset binding precisely so that state is distinguishable.

pub mod alert;
pub mod binding;
pub mod body;
pub mod cadence;
pub mod date;
pub mod judge;
pub mod page;

#[cfg(test)]
mod tests {
    /// This crate holds an HTTP client and a tzdb, and the wasm32 build must
    /// never see either. `hummingbird-authority-worker` is the only crate
    /// that builds for wasm32, and the only structural protection is that it
    /// does not depend on this one — which is invisible from here, so the
    /// guard is written from the other side: assert the *worker's* manifest
    /// does not name us.
    #[test]
    fn the_wasm32_worker_does_not_depend_on_this_crate() {
        let worker = include_str!("../../worker/Cargo.toml");
        assert!(
            !worker.contains("city-waste"),
            "server/worker builds for wasm32; this crate's HTTP client, tzdb and \
             (eventually) HTML parser have no business in that build"
        );
    }
}
