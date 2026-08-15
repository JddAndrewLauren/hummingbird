//! The item stage-transition rules (S11/#109, #355/#359/ADR-0023), sunk
//! here from `client/web`'s `item-actions.ts` by ADR-0025/#141 (M1-4).
//!
//! **What this module does NOT reimplement.** [`ItemAction::stage`] already
//! states, once, which stage each act vocabulary word sets — [`crate::Core::act`]
//! calls it directly — so [`applied_stage`] below is a one-line forward to
//! that method rather than a second copy of the same match arms. And
//! [`can_mark_done`] does not hardcode "every stage but Done": it asks
//! [`hummingbird_domain::resulting_stage`] whether a `Resolved` Grill verdict
//! would be accepted, which is `Err` for exactly one stage (`Done`) and
//! nothing else. Both are the same discipline the module doc for
//! [`crate::decisions`] states: a decision two clients could disagree about
//! lives in exactly one place, and that place is never a fork of a machine
//! that already exists elsewhere in this workspace.
//!
//! **What stays a fresh vocabulary here.** [`available_actions`] and
//! [`can_grill`] are UI affordance policy, not domain fact —
//! [`hummingbird_domain::resulting_stage`] happily accepts a `Blocked` item
//! (grill.rs: "Blocked behaves exactly like Ready and In Progress"), but no
//! Grill surface reads a Blocked item today, so `can_grill` excludes it
//! anyway. That is a scope decision this module makes, not one it could
//! derive from the domain's own table — which is exactly why it is pinned
//! by its own test rather than a cross-check against `resulting_stage`.

use hummingbird_domain::{resulting_stage, GrillVerdict, Stage};

use crate::ItemAction;

/// S11/#109's act affordances: which buttons item detail offers for an
/// item's current stage. Triage and Grilling offer nothing (neither is an
/// action yet — `CONTEXT.md`'s **Stage** entry), and Done offers nothing
/// either — a finished item has nothing left to act on.
pub fn available_actions(stage: Stage) -> &'static [ItemAction] {
    match stage {
        Stage::Triage | Stage::Grilling => &[],
        // `Complete` from Ready is the row-checkmark amendment: finishing
        // is one click from any live stage, never gated on having said
        // "start" first.
        Stage::Ready => &[
            ItemAction::Start,
            ItemAction::Complete,
            ItemAction::Block,
            ItemAction::Cancel,
        ],
        // Resuming a stalled `InProgress` item back into `InProgress` is a
        // no-op the UI never offers; only forward (`Complete`) or sideways
        // (`Block`, `Cancel`) actions apply.
        Stage::InProgress => &[ItemAction::Complete, ItemAction::Block, ItemAction::Cancel],
        // `Blocked` means an external wait ended (`CONTEXT.md`) — the way
        // back onto the frontier is `Start`, exactly as if the item were
        // freshly Ready. `Block` is deliberately absent: an already-blocked
        // item offers no "block it again" affordance.
        Stage::Blocked => &[ItemAction::Start, ItemAction::Complete, ItemAction::Cancel],
        Stage::Done => &[],
    }
}

/// The stage an act vocabulary word sets, or `None` for [`ItemAction::Cancel`]
/// (which touches `archived_at` instead of `stage` — the caller decides
/// what to do with `None`, exactly as `item-actions.ts`'s `applyItemAction`
/// did). A one-line forward to [`ItemAction::stage`], never a second copy of
/// its match arms — the "never fork it" half of this module's contract that
/// applies to the act vocabulary rather than the Grill one.
pub fn applied_stage(action: ItemAction) -> Option<Stage> {
    action.stage()
}

/// Whether a row offers the one-click "mark done" checkmark: any live,
/// unarchived stage but Done itself. Deliberately WIDER than
/// [`available_actions`] — Triage and Grilling stay pre-action in the
/// detail panel's vocabulary (no start/block there), but a capture that
/// turned out already finished is still one click.
///
/// **Calls `hummingbird_domain::resulting_stage` for the terminal check
/// rather than forking one.** A `Resolved` Grill verdict is rejected for
/// exactly one stage — Done (`grill.rs`'s own table) — which is the same
/// "nothing left to act on" fact this function needs; asking the domain's
/// own machine for it means a future stage added to either vocabulary
/// cannot make the two silently disagree.
pub fn can_mark_done(stage: Stage, archived: bool) -> bool {
    !archived && resulting_stage(stage, GrillVerdict::Resolved).is_ok()
}

/// Whether a row offers "Grill me" (#355, ADR-0023; widened to Now's
/// frontier by #359): Triage, Grilling, Ready and In Progress. Blocked is
/// excluded even though `hummingbird_domain::resulting_stage` accepts a
/// Blocked item (grill.rs: "Blocked behaves exactly like Ready and In
/// Progress") — no Grill surface reads a Blocked item today (#211 is the
/// open issue for that read surface), so this is a UI scope decision, not
/// one derivable from the domain table. Done is excluded because
/// `resulting_stage` rejects it outright.
pub fn can_grill(stage: Stage) -> bool {
    matches!(
        stage,
        Stage::Triage | Stage::Grilling | Stage::Ready | Stage::InProgress
    )
}

/// The Grill button's own label (#356, ADR-0023): "Resume grill" when this
/// item already carries a draft, "Grill me" otherwise.
pub fn grill_button_label(has_draft: bool) -> &'static str {
    if has_draft {
        "Resume grill"
    } else {
        "Grill me"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triage_and_grilling_offer_no_actions() {
        assert_eq!(available_actions(Stage::Triage), &[] as &[ItemAction]);
        assert_eq!(available_actions(Stage::Grilling), &[] as &[ItemAction]);
    }

    #[test]
    fn ready_offers_start_complete_block_and_cancel() {
        assert_eq!(
            available_actions(Stage::Ready),
            &[
                ItemAction::Start,
                ItemAction::Complete,
                ItemAction::Block,
                ItemAction::Cancel
            ],
        );
    }

    #[test]
    fn in_progress_offers_complete_block_and_cancel_never_start() {
        let actions = available_actions(Stage::InProgress);
        assert_eq!(
            actions,
            &[ItemAction::Complete, ItemAction::Block, ItemAction::Cancel]
        );
        assert!(!actions.contains(&ItemAction::Start));
    }

    #[test]
    fn blocked_offers_start_complete_and_cancel_never_block_again() {
        let actions = available_actions(Stage::Blocked);
        assert_eq!(
            actions,
            &[ItemAction::Start, ItemAction::Complete, ItemAction::Cancel]
        );
        assert!(!actions.contains(&ItemAction::Block));
    }

    #[test]
    fn done_offers_nothing() {
        assert_eq!(available_actions(Stage::Done), &[] as &[ItemAction]);
    }

    #[test]
    fn applied_stage_mirrors_item_action_stage_verbatim() {
        for action in [
            ItemAction::Start,
            ItemAction::Complete,
            ItemAction::Block,
            ItemAction::Cancel,
        ] {
            assert_eq!(applied_stage(action), action.stage());
        }
        assert_eq!(applied_stage(ItemAction::Start), Some(Stage::InProgress));
        assert_eq!(applied_stage(ItemAction::Complete), Some(Stage::Done));
        assert_eq!(applied_stage(ItemAction::Block), Some(Stage::Blocked));
        assert_eq!(applied_stage(ItemAction::Cancel), None);
    }

    /// The pin: `can_mark_done`'s terminal-stage check must agree with
    /// `hummingbird_domain::resulting_stage` for every stage in the
    /// vocabulary, not just the ones this file happens to exercise
    /// elsewhere. A stage this disagrees on is exactly the drift ADR-0025
    /// exists to make impossible.
    #[test]
    fn can_mark_done_agrees_with_resulting_stage_for_every_stage() {
        for stage in Stage::ALL {
            let domain_accepts_a_resolved_verdict =
                resulting_stage(stage, GrillVerdict::Resolved).is_ok();
            assert_eq!(
                can_mark_done(stage, false),
                domain_accepts_a_resolved_verdict,
                "{stage:?} disagreed with hummingbird_domain::resulting_stage",
            );
        }
    }

    #[test]
    fn can_mark_done_allows_every_live_unarchived_stage_triage_and_grilling_included() {
        for stage in [
            Stage::Triage,
            Stage::Grilling,
            Stage::Ready,
            Stage::InProgress,
            Stage::Blocked,
        ] {
            assert!(can_mark_done(stage, false), "{stage:?} should allow mark-done");
        }
    }

    #[test]
    fn can_mark_done_refuses_a_finished_item() {
        assert!(!can_mark_done(Stage::Done, false));
    }

    #[test]
    fn can_mark_done_refuses_an_archived_item() {
        assert!(!can_mark_done(Stage::Ready, true));
    }

    #[test]
    fn can_grill_is_true_for_triage_grilling_ready_and_in_progress() {
        for stage in [Stage::Triage, Stage::Grilling, Stage::Ready, Stage::InProgress] {
            assert!(can_grill(stage), "{stage:?} should allow grill");
        }
    }

    #[test]
    fn can_grill_is_false_for_blocked_and_done() {
        for stage in [Stage::Blocked, Stage::Done] {
            assert!(!can_grill(stage), "{stage:?} should refuse grill");
        }
    }

    #[test]
    fn grill_button_label_reads_grill_me_with_no_draft() {
        assert_eq!(grill_button_label(false), "Grill me");
    }

    #[test]
    fn grill_button_label_reads_resume_grill_with_a_draft() {
        assert_eq!(grill_button_label(true), "Resume grill");
    }
}
