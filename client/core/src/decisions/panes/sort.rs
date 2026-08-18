//! ADR-0015's cross-pane sort — the shell's whole opinion about which
//! standing question the eye should reach first, sunk from
//! `screens/questions/sort.ts` (#533/M4).
//!
//! Mirrors [`super::super::frontier::by_priority_then_due`]'s discipline
//! exactly: pure, clock-free, never mutating its input, and **total** (no
//! two distinct panes ever compare equal), so ranking the same snapshot
//! twice is byte-identical.
//!
//! Five axes, in this order:
//!
//!   1. answer state — answered first, then a gap, then unbound. An answer
//!      beats a promise, and a question nobody has bound yet is setup
//!      rather than context, so it settles at the bottom.
//!   2. band — ADR-0015's salience vocabulary, live -> dormant.
//!   3. `within_band` — soonest first inside one band; `None` after every
//!      `Some`.
//!   4. declared question order — the registry's, not alphabetical.
//!   5. subject key — the last resort, so the order is total.

use std::cmp::Ordering;

use super::contract::{AnswerState, RankedPaneRecord, BAND_ORDER};

const ANSWER_STATE_ORDER: [AnswerState; 3] =
    [AnswerState::Answered, AnswerState::BoundButUnacquired, AnswerState::Unbound];

/// An unrecognised value sorts **last** rather than first. It cannot happen
/// through the type system for the two closed vocabularies, but the
/// alternative default (index `-1`, i.e. first) would promote a broken pane
/// to the top of the region — and `question_order` really is open, since a
/// caller declares it.
fn rank_in<T: PartialEq>(order: &[T], value: &T) -> usize {
    order.iter().position(|candidate| candidate == value).unwrap_or(order.len())
}

/// `None` sorts after every real instant — "nothing to order by" is not
/// "infinitely soon".
fn compare_within_band(a: Option<i64>, b: Option<i64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Orders the ranked panes for display. Never mutates `panes`.
///
/// `question_order` is a list of question *names* rather than
/// [`super::contract::StandingQuestion`]s — see [`RankedPaneRecord`]'s own
/// doc for why the sort stays total over names it has never heard of.
pub fn order_panes(panes: &[RankedPaneRecord], question_order: &[String]) -> Vec<RankedPaneRecord> {
    let mut ordered: Vec<RankedPaneRecord> = panes.to_vec();
    ordered.sort_by(|a, b| {
        rank_in(&ANSWER_STATE_ORDER, &a.answer.answer_state)
            .cmp(&rank_in(&ANSWER_STATE_ORDER, &b.answer.answer_state))
            .then_with(|| rank_in(&BAND_ORDER, &a.answer.band).cmp(&rank_in(&BAND_ORDER, &b.answer.band)))
            .then_with(|| compare_within_band(a.answer.within_band, b.answer.within_band))
            .then_with(|| rank_in(question_order, &a.question).cmp(&rank_in(question_order, &b.question)))
            .then_with(|| a.subject_key.cmp(&b.subject_key))
    });
    ordered
}

/// Whether two ranked lists describe the same set of panes in the same
/// answer states — the one comparison `RankedRegion` re-samples its
/// captured order on, beyond a sync cycle.
///
/// Deliberately **not** a full equality: band and `within_band` move on
/// their own with the clock, and re-sorting on those would be exactly the
/// under-the-cursor reordering the captured order exists to prevent. A pane
/// appearing, disappearing, or flipping between an answer and a gap is a
/// different kind of change — the region is showing something that is no
/// longer true — and is worth a re-sort at once.
pub fn same_pane_identity(a: &[RankedPaneRecord], b: &[RankedPaneRecord]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    b.iter().all(|pane| {
        a.iter()
            .find(|candidate| candidate.pane_key == pane.pane_key)
            .is_some_and(|found| found.answer.answer_state == pane.answer.answer_state)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decisions::panes::contract::{pane_key, Band, PaneAnswerCore};

    /// Ported from `sort.test.ts`, synthetic question names and all — the
    /// suite's own proof that the sort is total over an order it is handed
    /// rather than over the registry's.
    fn pane(
        question: &str,
        subject_key: &str,
        answer_state: AnswerState,
        band: Band,
        within_band: Option<i64>,
    ) -> RankedPaneRecord {
        RankedPaneRecord {
            question: question.to_string(),
            subject_key: subject_key.to_string(),
            pane_key: pane_key(question, subject_key),
            answer: PaneAnswerCore { answer_state, band, within_band },
        }
    }

    fn answered(subject_key: &str) -> RankedPaneRecord {
        pane("alpha", subject_key, AnswerState::Answered, Band::Near, Some(0))
    }

    fn order() -> Vec<String> {
        vec!["alpha".to_string(), "beta".to_string()]
    }

    fn keys(panes: &[RankedPaneRecord]) -> Vec<String> {
        panes.iter().map(|p| p.pane_key.clone()).collect()
    }

    #[test]
    fn puts_an_answer_ahead_of_a_gap_and_a_gap_ahead_of_an_unbound_question() {
        let ordered = order_panes(
            &[
                pane("alpha", "unbound", AnswerState::Unbound, Band::Near, Some(0)),
                pane("alpha", "gap", AnswerState::BoundButUnacquired, Band::Near, Some(0)),
                answered("answered"),
            ],
            &order(),
        );
        assert_eq!(keys(&ordered), ["alpha:answered", "alpha:gap", "alpha:unbound"]);
    }

    #[test]
    fn orders_by_band_before_anything_the_panes_say_about_themselves() {
        let ordered = order_panes(
            &[
                pane("alpha", "dormant", AnswerState::Answered, Band::Dormant, Some(0)),
                pane("alpha", "distant", AnswerState::Answered, Band::Distant, Some(0)),
                pane("alpha", "live", AnswerState::Answered, Band::Live, Some(0)),
                pane("alpha", "near", AnswerState::Answered, Band::Near, Some(0)),
                pane("alpha", "imminent", AnswerState::Answered, Band::Imminent, Some(0)),
            ],
            &order(),
        );
        assert_eq!(
            keys(&ordered),
            ["alpha:live", "alpha:imminent", "alpha:near", "alpha:distant", "alpha:dormant"],
        );
    }

    #[test]
    fn breaks_a_band_tie_by_within_band_soonest_first() {
        let ordered = order_panes(
            &[
                pane("alpha", "later", AnswerState::Answered, Band::Near, Some(900)),
                pane("alpha", "sooner", AnswerState::Answered, Band::Near, Some(-50)),
                pane("alpha", "middle", AnswerState::Answered, Band::Near, Some(10)),
            ],
            &order(),
        );
        assert_eq!(keys(&ordered), ["alpha:sooner", "alpha:middle", "alpha:later"]);
    }

    #[test]
    fn sorts_a_missing_within_band_after_every_real_one() {
        let ordered = order_panes(
            &[
                pane("alpha", "none", AnswerState::Answered, Band::Near, None),
                pane("alpha", "huge", AnswerState::Answered, Band::Near, Some(i64::MAX)),
            ],
            &order(),
        );
        assert_eq!(keys(&ordered), ["alpha:huge", "alpha:none"]);
    }

    #[test]
    fn falls_back_to_the_declared_question_order_then_the_subject_key() {
        let ordered = order_panes(
            &[answered("b_unused"), pane("beta", "a", AnswerState::Answered, Band::Near, Some(0))],
            &order(),
        );
        // Declared order beats alphabetical: "beta:a" would win on subject
        // key alone.
        assert_eq!(keys(&ordered), ["alpha:b_unused", "beta:a"]);

        let by_subject = order_panes(&[answered("b"), answered("a")], &order());
        assert_eq!(keys(&by_subject), ["alpha:a", "alpha:b"]);
    }

    #[test]
    fn never_mutates_its_input_and_is_byte_identical_on_a_repeat_call() {
        let input = vec![
            pane("alpha", "b", AnswerState::Answered, Band::Dormant, Some(0)),
            pane("alpha", "a", AnswerState::Answered, Band::Live, Some(0)),
        ];
        let before = keys(&input);
        let once = order_panes(&input, &order());
        let twice = order_panes(&input, &order());
        assert_eq!(keys(&input), before);
        assert_eq!(keys(&once), keys(&twice));
    }

    #[test]
    fn distinguishes_an_answered_pane_with_nothing_to_say_from_a_gap() {
        // Both carry a `None` within-band and a dormant band, so the only
        // thing separating them is the answer state.
        let ordered = order_panes(
            &[
                pane("alpha", "gap", AnswerState::BoundButUnacquired, Band::Dormant, None),
                pane("alpha", "quiet", AnswerState::Answered, Band::Dormant, None),
            ],
            &order(),
        );
        assert_eq!(keys(&ordered), ["alpha:quiet", "alpha:gap"]);
    }

    #[test]
    fn a_question_outside_the_declared_order_sorts_last_rather_than_first() {
        let ordered = order_panes(
            &[pane("omega", "a", AnswerState::Answered, Band::Near, Some(0)), answered("a")],
            &order(),
        );
        assert_eq!(keys(&ordered), ["alpha:a", "omega:a"]);
    }

    #[test]
    fn same_pane_identity_is_true_when_only_band_and_within_band_moved() {
        assert!(same_pane_identity(
            &[pane("alpha", "a", AnswerState::Answered, Band::Dormant, Some(900))],
            &[pane("alpha", "a", AnswerState::Answered, Band::Imminent, Some(5))],
        ));
    }

    #[test]
    fn same_pane_identity_is_false_when_a_pane_appears_vanishes_or_changes_state() {
        let base = [answered("a")];
        assert!(!same_pane_identity(&base, &[answered("a"), answered("b")]));
        assert!(!same_pane_identity(&base, &[]));
        assert!(!same_pane_identity(
            &base,
            &[pane("alpha", "a", AnswerState::BoundButUnacquired, Band::Near, Some(0))],
        ));
    }

    #[test]
    fn same_pane_identity_compares_by_pane_key_not_by_position() {
        assert!(same_pane_identity(
            &[answered("a"), answered("b")],
            &[answered("b"), answered("a")],
        ));
    }
}
