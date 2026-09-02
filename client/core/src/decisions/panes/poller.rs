//! **The poller-health question** (#775, part of #773), sunk directly here
//! rather than ported off a web `*.ts` file — the first pane in this family
//! with no browser-only predecessor.
//!
//! # What broke, and why this is not `github.rs` widened
//!
//! `github_subjects` (`github.rs`) used to be the board's only answer to "is
//! a poller running on time?", and it answered a different question than it
//! looked like it did: it derives its tiles from which of this repo's own
//! `.github/workflows/*.yml` declare a `schedule:` trigger, and bands each
//! one off *GitHub's* run history. That measures **the host, not the
//! poller** — the moment a `schedule:` trigger moves (as five of them did,
//! #774/#776, onto the sweeper's own supercronic), the workflow's tile
//! simply stops existing, trading a wrong answer for no answer on exactly
//! the pollers that matter most.
//!
//! This pane answers the question `github_subjects` was only ever a proxy
//! for, straight off the data every poller already writes: every
//! `context_snapshots` row carries `polled_every_ms` in its ADR-0015
//! envelope (surfaced here as [`FreshnessFact::declared_cadence_ms`], **not**
//! re-parsed from any payload body — see the next section), so "is this
//! poller running on time" is answerable as *"how old is the newest row for
//! this source, against the cadence that row itself declares"* — with no
//! poller of its own, no GitHub API call, and no opinion about which
//! platform (GitHub Actions, the sweeper's supercronic, anywhere else)
//! happens to run the writer today.
//!
//! # No body parser, unlike every sibling in this family
//!
//! `waste.rs`/`kimi.rs`/`uptime.rs`/`github.rs` each pin one source's payload
//! *shape* because the fact they band on lives inside the body (a balance, a
//! probe's observed status, a workflow's last run). This pane bands on
//! **freshness alone**, and [`crate::freshness::Freshness::of_snapshot`]
//! already carries that off the envelope, independent of whatever the body
//! holds or whether it parses at all (`freshness.rs`'s own "a broken
//! envelope costs the cadence, not the age"). So [`poller_facts`] never
//! touches [`PaneSnapshotFacts::envelope`] — the one gap here is
//! [`PollerGap::NotFetched`], "no row for this source at all."
//!
//! # Subjects: sources, not keys within one source
//!
//! Every other multi-subject pane in this family (`github.rs`, `uptime.rs`)
//! has ONE fixed [`SOURCE`] and a runtime-discovered set of *keys* under it
//! (a workflow file name, a manifest service id) — genuinely unknowable
//! ahead of time, which is why their subject list comes from observed rows.
//! This pane inverts that: its subject **is** the source string, and the set
//! of sources is exactly what
//! [`hummingbird_domain::REGISTRY`] already declares
//! [`hummingbird_domain::Writes::writes_snapshots`] for — a small,
//! reviewed, compile-time-known set. [`poller_sources`] derives it from that
//! registry rather than re-declaring the list here, which is this pane's own
//! reading of "subjects come from observed keys, not a hand-maintained
//! list": the list is the registry's, not a second copy of it.
//!
//! Every declared source is always a subject, whether or not it has ever
//! written a row — `kimi.rs`'s own "no unbound state, only bound-but-
//! unacquired" shape, applied per source: there is no per-device binding to
//! be unset, so a source this device has not yet read from is a gap, never
//! `unbound`, and it must still be discoverable (ADR-0017's own rule).
//!
//! # `race-alert-poll`, decided explicitly
//!
//! `race-alert-poll` (`server/race-poll`) writes no `context_snapshots` row
//! of its own — it only *reads* `race-schedule/v1` and mints alerts under
//! that same string (`Writes::Both` on the entry, shared with
//! `race-schedule-poll`, ADR-0009's join constraint). It therefore has
//! nothing this pane, or any freshness-only pane, could measure: there is no
//! row whose `fetched_at` would ever be its own. **Decision: leave it
//! uncovered by this pane rather than invent a heartbeat row for it.** Both
//! binaries share one lane and one schedule (`race-alert-poll.yml`'s own
//! `*/15`, tighter than `race-schedule-poll.yml`'s `0 */6`), so
//! `race-schedule/v1`'s freshness — already watched here — is a reasonable
//! proxy for "is this lane's clock still ticking"; it is not a proxy for
//! "did the alert binary itself run cleanly", which stays outside every
//! standing question's reach today, exactly as it was before this pane
//! existed.
//!
//! # The threshold is not GitHub's
//!
//! [`OVERDUE_MULTIPLIER`]/[`MIN_OVERDUE_AFTER_MS`] mirror `github.rs`'s own
//! shape (a multiple of the declared cadence, floored) but **not** its
//! numbers. `github.rs`'s 3h floor exists to absorb GitHub Actions' own
//! queueing delay on a shared runner — measured on this repo's `*/15`
//! workflows at a p95 of 60–73min and a max of 88–106min. A poller on the
//! sweeper's supercronic has no such queue: it is a single always-on
//! process ticking its own crontab, so the only slack between "the job ran"
//! and "the row landed" is cron's own minute granularity plus the poller's
//! own HTTP round trip — a few seconds, not tens of minutes. [`FLOOR_MS`] is
//! sized off *that* clock (ten minutes: comfortably over a minute of cron
//! slop plus run time, and inert for every cadence this registry declares
//! today, since the tightest — the four stream pollers' 15 minutes — already
//! clears it at `3×`). It exists to protect a future very-short-cadence
//! poller from its own execution jitter, never to absorb an external queue.
//!
//! This also means a poller genuinely still queued behind GitHub Actions
//! (today: `city-waste`, `kimi-balance`, `race-schedule-poll`,
//! `uptime-probe`) is judged by the **same** threshold as a supercronic one.
//! Every one of those declares a cadence of an hour or more, so `3×` clears
//! GitHub's own measured queueing with room to spare — except
//! `uptime-probe`, whose Actions delivery this repo has independently
//! measured at ~22% (#773's own note): that poller is *supposed* to read
//! `imminent` here much of the time, which is this pane doing its job, not
//! a false alarm to chase away.

use serde::{Deserialize, Serialize};

use super::contract::{AnswerState, Band, PaneAnswerCore};
use super::inputs::{FreshnessFact, PaneInputs, PaneReadFacts, PaneSnapshotFacts};

/// A scheduled write is judged overdue once its last row is this many
/// multiples of its own declared cadence old.
pub const OVERDUE_MULTIPLIER: i64 = 3;

/// The floor under the overdue threshold — ten minutes, sized for a real
/// clock's own jitter (cron's minute granularity, a poller's HTTP round
/// trip), never for an external queue. See the module header.
pub const FLOOR_MS: i64 = 10 * 60 * 1000;

/// Every source this pane watches: every entry
/// [`hummingbird_domain::REGISTRY`] declares
/// [`hummingbird_domain::Writes::writes_snapshots`] for, live
/// (never retired) — not a second, hand-maintained copy of that list. See
/// the module header for why deriving it here, rather than re-declaring it,
/// is this pane's own reading of "subjects from observed keys, not a
/// hand-maintained list".
///
/// Declaration order in the registry, which is stable and reviewed —
/// `poller_subjects` does not re-sort it.
pub fn poller_sources() -> Vec<&'static str> {
    hummingbird_domain::REGISTRY
        .iter()
        .filter(|entry| !entry.is_retired() && entry.writes_snapshots())
        .map(|entry| entry.source)
        .collect()
}

/// This question's subjects: every source [`poller_sources`] names, always —
/// there is no per-device binding to be unset here, so every declared source
/// is a subject whether or not this device has ever read a row for it
/// (`kimi.rs`'s own "no unbound state" shape, per source).
pub fn poller_subjects(_inputs: &PaneInputs) -> Vec<String> {
    poller_sources().into_iter().map(str::to_string).collect()
}

/// Why this pane has no answer for one source — a **kind**, not a sentence.
/// One arm: this pane touches no body, so the only way to have nothing to
/// say is never having read a row at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "gap", rename_all = "camelCase")]
pub enum PollerGap {
    /// No snapshot row at all for this source: nothing has ever been
    /// fetched, or this device has not read this source yet.
    NotFetched,
}

/// Everything one answered pane needs, decided once — the source's own
/// identity, its freshest row's freshness, and the band that freshness
/// alone decides.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PollerFacts {
    pub freshness: FreshnessFact,
    pub band: Band,
}

/// The whole answered fact set, or the reason there is none.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PollerResolved {
    Facts(PollerFacts),
    Gap { gap: PollerGap },
}

/// The freshest row under one source's read, by measured age — `None` when
/// there are no rows at all. Several sources here carry many keys (a row per
/// GitHub workflow, per uptime service); this pane reduces that fan-out to
/// one reading per source by asking only "when did this source's poller
/// most recently write anything," which is the one fact every key's row
/// agrees about even when their *bodies* do not.
///
/// [`FreshnessFact::Unknown`] sorts as though it were maximally old — never
/// picked over a real age when one exists — on
/// [`crate::freshness::Freshness`]'s own "unknown may never render as
/// fresh" rule; a real snapshot row's freshness is never actually `Unknown`
/// (`freshness.rs`'s own pin), so this only matters defensively.
fn freshest(read: &PaneReadFacts) -> Option<&PaneSnapshotFacts> {
    read.snapshots.iter().min_by_key(|snapshot| match snapshot.freshness {
        FreshnessFact::Unknown => i64::MAX,
        FreshnessFact::Age { age_ms, .. } => age_ms,
    })
}

/// This source's band, from its freshest row's freshness alone.
///
/// **A row declaring no `polled_every_ms` must not read as healthy** —
/// `Band::Distant`, `github_band`'s own "a judgement that could not be made"
/// reading, never `Band::Dormant`. [`FreshnessFact::Unknown`] is worse than
/// that: not merely an unreadable cadence but no age either, so it reads as
/// the most severe band this pane has, `Band::Imminent`, rather than sitting
/// beside the merely-cadence-less case.
pub fn poller_band(freshness: FreshnessFact) -> Band {
    match freshness {
        FreshnessFact::Unknown => Band::Imminent,
        FreshnessFact::Age { declared_cadence_ms: None, .. } => Band::Distant,
        FreshnessFact::Age { age_ms, declared_cadence_ms: Some(declared_cadence_ms) } => {
            let overdue_after_ms = (declared_cadence_ms * OVERDUE_MULTIPLIER).max(FLOOR_MS);
            if age_ms > overdue_after_ms { Band::Imminent } else { Band::Dormant }
        }
    }
}

/// The whole answered fact set for one source, or the reason there is none.
pub fn poller_facts(source: &str, inputs: &PaneInputs) -> PollerResolved {
    let freshest_row = inputs.pane_reads.get(source).and_then(freshest);
    let Some(snapshot) = freshest_row else {
        return PollerResolved::Gap { gap: PollerGap::NotFetched };
    };
    PollerResolved::Facts(PollerFacts {
        freshness: snapshot.freshness,
        band: poller_band(snapshot.freshness),
    })
}

/// This question's answer for the shell — minus its rendering half.
///
/// `within_band` is always `None`: there is no "next relevant moment" a
/// poller's own write history names, on `uptime.rs`'s own reasoning. No
/// separate stale escalation runs here, unlike `github.rs`/`kimi.rs`/
/// `uptime.rs`: on every sibling pane, escalation exists to catch the
/// *poller* going dark behind an otherwise-healthy-looking stored body —
/// but this pane's whole answer already **is** that same freshness check,
/// so there is no second, staler fact left to escalate against.
pub fn poller_answer(source: &str, inputs: &PaneInputs) -> PaneAnswerCore {
    match poller_facts(source, inputs) {
        PollerResolved::Gap { .. } => {
            PaneAnswerCore { answer_state: AnswerState::BoundButUnacquired, band: Band::Dormant, within_band: None }
        }
        PollerResolved::Facts(facts) => {
            PaneAnswerCore { answer_state: AnswerState::Answered, band: facts.band, within_band: None }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_786_636_800_000; // 2026-08-12T16:00:00Z

    fn ok_envelope(source: &str) -> serde_json::Value {
        serde_json::json!({"kind":"ok","schema":source,"body":"{}"})
    }

    fn snapshot_json(key: &str, envelope: serde_json::Value, freshness: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"key": key, "envelope": envelope, "freshness": freshness})
    }

    fn aged(age_ms: i64, declared_cadence_ms: Option<i64>) -> serde_json::Value {
        serde_json::json!({"kind":"age","ageMs": age_ms,"declaredCadenceMs": declared_cadence_ms})
    }

    fn inputs_with(source: &str, snapshots: Vec<serde_json::Value>) -> PaneInputs {
        serde_json::from_value(serde_json::json!({
            "nowMs": NOW,
            "paneReads": { source: { "snapshots": snapshots } },
        }))
        .unwrap()
    }

    fn no_reads() -> PaneInputs {
        serde_json::from_value(serde_json::json!({"nowMs": NOW})).unwrap()
    }

    // ------------------------------------------------------- poller_sources

    #[test]
    fn watches_exactly_the_registrys_live_snapshot_writing_sources() {
        let expected: Vec<&str> = hummingbird_domain::REGISTRY
            .iter()
            .filter(|entry| !entry.is_retired() && entry.writes_snapshots())
            .map(|entry| entry.source)
            .collect();
        assert_eq!(poller_sources(), expected);
        // Pinned by name too, so a registry edit that silently drops one of
        // today's nine sources fails a readable assertion rather than only
        // the structural comparison above.
        assert_eq!(
            poller_sources(),
            vec![
                "gmail/v1",
                "m365-mail/v1",
                "google-calendar/v1",
                "m365-calendar/v1",
                "city-waste/v2",
                "race-schedule/v1",
                "kimi-balance/v1",
                "github-hummingbird/v1",
                "uptime/v1",
            ],
        );
    }

    #[test]
    fn never_watches_a_retired_or_alerts_only_source() {
        let sources = poller_sources();
        // city-waste/v1 is retired in favour of city-waste/v2.
        assert!(!sources.contains(&"city-waste/v1"));
        // item-threshold/v1, healthchecks/v1, home-assistant/v1, github/v1,
        // photo-site/v1 and gmail-alert/v1 are all alerts-only.
        for alerts_only in ["item-threshold/v1", "healthchecks/v1", "home-assistant/v1", "github/v1"] {
            assert!(!sources.contains(&alerts_only), "{alerts_only}");
        }
    }

    // ------------------------------------------------------ poller_subjects

    #[test]
    fn every_declared_source_is_a_subject_even_with_nothing_read_at_all() {
        assert_eq!(poller_subjects(&no_reads()), poller_sources());
    }

    #[test]
    fn every_declared_source_is_still_a_subject_once_some_have_data() {
        let inputs = inputs_with("kimi-balance/v1", vec![snapshot_json("balance", ok_envelope("kimi-balance/v1"), aged(60_000, Some(21_600_000)))]);
        assert_eq!(poller_subjects(&inputs), poller_sources());
    }

    // ---------------------------------------------------------- poller_band

    #[test]
    fn bands_a_fresh_row_dormant() {
        assert_eq!(
            poller_band(FreshnessFact::Age { age_ms: 60_000, declared_cadence_ms: Some(900_000) }),
            Band::Dormant,
        );
    }

    #[test]
    fn bands_a_row_older_than_the_multiplied_cadence_imminent() {
        let cadence = 30 * 60 * 1000; // github-status-poll's own 30min
        assert_eq!(
            poller_band(FreshnessFact::Age { age_ms: cadence * OVERDUE_MULTIPLIER + 1, declared_cadence_ms: Some(cadence) }),
            Band::Imminent,
        );
        assert_eq!(
            poller_band(FreshnessFact::Age { age_ms: cadence * OVERDUE_MULTIPLIER - 1, declared_cadence_ms: Some(cadence) }),
            Band::Dormant,
        );
    }

    #[test]
    fn floors_the_overdue_threshold_for_a_very_tight_cadence() {
        let cadence = 60_000; // one minute — far tighter than anything this registry declares today
        assert!(cadence * OVERDUE_MULTIPLIER < FLOOR_MS, "the floor must bind here");
        assert_eq!(
            poller_band(FreshnessFact::Age { age_ms: FLOOR_MS - 1, declared_cadence_ms: Some(cadence) }),
            Band::Dormant,
        );
        assert_eq!(
            poller_band(FreshnessFact::Age { age_ms: FLOOR_MS + 1, declared_cadence_ms: Some(cadence) }),
            Band::Imminent,
        );
    }

    #[test]
    fn does_not_bind_the_floor_for_the_stream_pollers_own_15_minute_cadence() {
        let cadence = 15 * 60 * 1000;
        assert!(cadence * OVERDUE_MULTIPLIER > FLOOR_MS, "the floor must not bind here");
        assert_eq!(
            poller_band(FreshnessFact::Age { age_ms: cadence * OVERDUE_MULTIPLIER - 1, declared_cadence_ms: Some(cadence) }),
            Band::Dormant,
        );
    }

    #[test]
    fn a_row_declaring_no_cadence_never_reads_as_healthy() {
        let band = poller_band(FreshnessFact::Age { age_ms: 1_000, declared_cadence_ms: None });
        assert_eq!(band, Band::Distant);
        assert_ne!(band, Band::Dormant);
    }

    #[test]
    fn unknown_freshness_is_the_most_severe_band_never_dormant() {
        let band = poller_band(FreshnessFact::Unknown);
        assert_eq!(band, Band::Imminent);
        assert_ne!(band, Band::Dormant);
    }

    // -------------------------------------------------------- poller_facts

    #[test]
    fn is_not_fetched_when_the_source_has_never_been_read() {
        assert_eq!(poller_facts("kimi-balance/v1", &no_reads()), PollerResolved::Gap { gap: PollerGap::NotFetched });
    }

    #[test]
    fn is_not_fetched_when_the_source_was_read_but_holds_no_rows() {
        let inputs = inputs_with("kimi-balance/v1", vec![]);
        assert_eq!(poller_facts("kimi-balance/v1", &inputs), PollerResolved::Gap { gap: PollerGap::NotFetched });
    }

    #[test]
    fn resolves_facts_off_the_freshest_row_when_a_source_has_many_keys() {
        // github-hummingbird/v1 carries one row per scheduled workflow; this
        // pane reduces that fan-out to the single freshest one.
        let inputs = inputs_with(
            "github-hummingbird/v1",
            vec![
                snapshot_json("stale.yml", ok_envelope("github-hummingbird/v1"), aged(3_600_000, Some(1_800_000))),
                snapshot_json("fresh.yml", ok_envelope("github-hummingbird/v1"), aged(60_000, Some(1_800_000))),
            ],
        );
        let PollerResolved::Facts(facts) = poller_facts("github-hummingbird/v1", &inputs) else {
            panic!("expected facts");
        };
        assert_eq!(facts.freshness, FreshnessFact::Age { age_ms: 60_000, declared_cadence_ms: Some(1_800_000) });
        assert_eq!(facts.band, Band::Dormant);
    }

    #[test]
    fn an_unknown_freshness_row_never_beats_a_real_age_when_one_exists() {
        let inputs = inputs_with(
            "uptime/v1",
            vec![
                snapshot_json("authority", ok_envelope("uptime/v1"), serde_json::json!({"kind":"unknown"})),
                snapshot_json("web", ok_envelope("uptime/v1"), aged(120_000, Some(3_600_000))),
            ],
        );
        let PollerResolved::Facts(facts) = poller_facts("uptime/v1", &inputs) else {
            panic!("expected facts");
        };
        assert_eq!(facts.freshness, FreshnessFact::Age { age_ms: 120_000, declared_cadence_ms: Some(3_600_000) });
    }

    // ------------------------------------------------------- poller_answer

    #[test]
    fn is_bound_but_unacquired_never_unbound_when_nothing_has_been_read() {
        let answer = poller_answer("kimi-balance/v1", &no_reads());
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
    }

    #[test]
    fn answers_with_the_sources_own_band_once_a_row_has_landed() {
        let inputs = inputs_with("kimi-balance/v1", vec![snapshot_json("balance", ok_envelope("kimi-balance/v1"), aged(60_000, Some(21_600_000)))]);
        let answer = poller_answer("kimi-balance/v1", &inputs);
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
    }

    #[test]
    fn answers_imminent_for_a_source_that_has_gone_quiet_past_its_own_cadence() {
        let cadence = 21_600_000; // kimi-balance's own 6h
        let inputs = inputs_with(
            "kimi-balance/v1",
            vec![snapshot_json("balance", ok_envelope("kimi-balance/v1"), aged(cadence * OVERDUE_MULTIPLIER + 1, Some(cadence)))],
        );
        assert_eq!(poller_answer("kimi-balance/v1", &inputs).band, Band::Imminent);
    }
}
