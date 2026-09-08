//! The nav control's own reading of a surface: **the single most salient
//! band among the panes that surface currently answers**, or nothing at all
//! when every answered pane is quiet.
//!
//! # Why this is a decision and not a rendering
//!
//! "Which colour is the Status button" looks like chrome, and the two
//! clients could each fold their own ranked panes down to a worst case in
//! four lines. That is precisely the shape ADR-0025 forbids: the fold
//! carries two real opinions — *which* answer states count, and *which*
//! bands are worth interrupting a nav bar for — and two copies of an
//! opinion are two answers waiting to disagree. What stays per-client is
//! the last step alone: which colour a [`Band`] paints as
//! (`tile-copy.ts`'s `bandTone`, `BottomNavBar`'s own pair), exactly the
//! split every pane module here already draws between a *kind* and a
//! sentence.
//!
//! # The two opinions, stated once
//!
//! **Only an answered pane can raise the nav.** A pane that has never been
//! polled, or whose payload would not parse, is
//! [`AnswerState::BoundButUnacquired`] — and `uptime.rs`'s own rule ("a
//! probe that cannot tell the truth must never render as healthy") cuts
//! *both* ways here: it must not render as healthy, and it must not render
//! as an outage either. The board already draws that third state as its own
//! `gap` tone, in a place with room to say which gap it is. A nav button
//! has room for one bit, and spending it on "something has not been fetched
//! yet" would cry wolf on every cold start, every first launch after an
//! install, and every question whose poller has simply not run for the
//! first time. So a gap is silent here and legible there.
//!
//! **[`Band::Dormant`] is silence, everything else is not.** No threshold
//! and no second vocabulary: the four remaining bands are already ordered
//! by salience in [`BAND_ORDER`], so this fold is a `min` over that order
//! and the caller paints whichever came back. A pane that escalates itself
//! for going stale ( `uptime.rs`, `github.rs`) therefore reaches the nav by
//! the same route as a genuine outage, which is the point of those
//! escalations.

use super::contract::{AnswerState, Band, PaneAnswerCore, RankedPaneRecord, Surface, BAND_ORDER};
use super::inputs::PaneInputs;
use super::zone::ZoneFacts;

/// Whether one pane's answer is loud enough to reach a nav control.
fn raises(answer: &PaneAnswerCore) -> bool {
    answer.answer_state == AnswerState::Answered && answer.band != Band::Dormant
}

/// The most salient band among `panes`, or `None` when none of them raises
/// the nav at all (see the module header for what "raises" means).
///
/// **Private, and over records rather than inputs, purely so the fold can
/// be tested apart from the ranking.** [`status_alarm`] is the only caller
/// and the only door any host has; a `pub` version would be an API nothing
/// is blocked on, which is the same "a type nobody needed" this family's
/// module header records #534 declining to add.
fn surface_alarm(panes: &[RankedPaneRecord]) -> Option<Band> {
    BAND_ORDER
        .iter()
        .copied()
        .find(|band| panes.iter().any(|pane| raises(&pane.answer) && pane.answer.band == *band))
}

/// The Status surface's alarm, from this device's state alone — the one
/// door a shell needs, since a nav bar holds no ranked panes of its own.
///
/// **No zone facts, and that is not an omission.** None of the status five
/// (kimi/github/uptime/reachability/poller) is civil-date reasoning, so
/// [`super::zone_queries`] answers empty for [`Surface::Status`] and there
/// is nothing for a host to resolve — asserted below rather than assumed,
/// so a fifth status question that *did* need a zone would fail a test here
/// instead of quietly banding against an empty table. There is deliberately
/// no `Surface::Now` counterpart: Now's questions do need the two-phase
/// crossing, and inventing a door that silently skipped it would be the one
/// mistake this note exists to prevent.
pub fn status_alarm(inputs: &PaneInputs) -> Option<Band> {
    debug_assert!(
        super::zone_queries(Surface::Status, inputs).is_empty(),
        "a status question started asking for zone facts; status_alarm resolves none",
    );
    surface_alarm(&super::rank_panes(Surface::Status, inputs, &ZoneFacts::default()))
}

#[cfg(test)]
mod tests {
    use super::super::contract::pane_key;
    use super::*;

    fn pane(question: &str, answer_state: AnswerState, band: Band) -> RankedPaneRecord {
        RankedPaneRecord {
            pane_key: pane_key(question, "s"),
            question: question.to_string(),
            subject_key: "s".to_string(),
            answer: PaneAnswerCore { answer_state, band, within_band: None },
        }
    }

    fn answered(question: &str, band: Band) -> RankedPaneRecord {
        pane(question, AnswerState::Answered, band)
    }

    #[test]
    fn all_quiet_raises_nothing() {
        assert_eq!(surface_alarm(&[]), None);
        assert_eq!(
            surface_alarm(&[answered("kimi", Band::Dormant), answered("uptime", Band::Dormant)]),
            None,
        );
    }

    #[test]
    fn takes_the_most_salient_band_not_the_first_pane() {
        // `order_panes` would already have put `live` first; this asserts
        // the fold does not simply trust that.
        assert_eq!(
            surface_alarm(&[
                answered("kimi", Band::Distant),
                answered("github", Band::Live),
                answered("uptime", Band::Near),
            ]),
            Some(Band::Live),
        );
    }

    #[test]
    fn every_non_dormant_band_reaches_the_nav_on_its_own() {
        for band in [Band::Live, Band::Imminent, Band::Near, Band::Distant] {
            assert_eq!(surface_alarm(&[answered("github", band)]), Some(band));
        }
    }

    #[test]
    fn a_gap_is_silent_rather_than_an_outage() {
        // The whole cold-start case: nothing polled yet, on every question.
        // A nav that shouted here would shout on every fresh install.
        assert_eq!(
            surface_alarm(&[
                pane("kimi", AnswerState::BoundButUnacquired, Band::Live),
                pane("uptime", AnswerState::Unbound, Band::Imminent),
            ]),
            None,
        );
    }

    #[test]
    fn a_gap_never_masks_a_real_band_beside_it() {
        assert_eq!(
            surface_alarm(&[
                pane("kimi", AnswerState::BoundButUnacquired, Band::Live),
                answered("github", Band::Near),
            ]),
            Some(Band::Near),
        );
    }

    #[test]
    fn the_status_surface_asks_for_no_zone_facts_so_the_one_door_needs_none() {
        let inputs: PaneInputs =
            serde_json::from_value(serde_json::json!({"nowMs": 1_786_636_800_000i64})).unwrap();
        assert!(super::super::zone_queries(Surface::Status, &inputs).is_empty());
        // Nothing has ever been polled on a device this bare, so every
        // status pane is a gap — and the nav stays quiet.
        assert_eq!(status_alarm(&inputs), None);
    }

    #[test]
    fn the_one_door_reports_a_real_divergence_off_the_snapshots_alone() {
        // uptime/v1's own wire shape: expected 401, observed 500 — `near`.
        let inputs: PaneInputs = serde_json::from_value(serde_json::json!({
            "nowMs": 1_786_636_800_000i64,
            "paneReads": { super::super::uptime::SOURCE: { "snapshots": [{
                "key": "authority",
                "envelope": {"kind":"ok","schema":super::super::uptime::SOURCE,
                             "body":"{\"expected\":\"on\",\"expect_status\":401,\"observed_status\":500,\"error\":null}"},
                "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs":3_600_000},
            }]}},
        }))
        .unwrap();
        assert_eq!(status_alarm(&inputs), Some(Band::Near));
    }
}
