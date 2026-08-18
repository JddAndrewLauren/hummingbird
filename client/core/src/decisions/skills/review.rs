//! The review card's plan-replacement tick and frontier-demotion warning
//! (#355/#359, ADR-0023), sunk here from
//! `client/web/src/screens/grill-review.ts` at #539.
//!
//! `verdict == FogRemains` is enough to know a confirm will strand a live
//! plan or take an item off the frontier, with no second stage-mapping
//! needed here: every live stage demotes to Grilling on `FogRemains`
//! (`hummingbird_domain::resulting_stage`'s own table — Triage, Grilling,
//! Ready, InProgress and Blocked all take that arm), so reading the verdict
//! the model itself proposed is enough to know a demotion is coming,
//! without this module re-deriving what stage results. `Resolved` never
//! strands anything: it either promotes Triage/Grilling to Ready or leaves
//! an already-live stage exactly where it was.

use hummingbird_domain::{GrillVerdict, Stage, Step};

use super::affordance::live_undone_steps;

/// Whether confirming this verdict risks stranding a live plan: the item has
/// at least one live, undone Step, and the verdict is `FogRemains` (the only
/// arm that ever demotes a live stage to Grilling). `false` for `Resolved`,
/// and `false` for an item with no plan to protect either way — the tick has
/// nothing to offer when there is nothing it could delete.
pub fn would_strand_plan(verdict: GrillVerdict, steps: &[Step]) -> bool {
    verdict == GrillVerdict::FogRemains && !live_undone_steps(steps).is_empty()
}

/// The tick's own label, naming the count — never a bare "Delete steps?"
/// that leaves the person guessing what they are agreeing to.
pub fn plan_replacement_label(steps: &[Step]) -> String {
    let count = live_undone_steps(steps).len();
    format!("Also delete {count} unfinished step{}", if count == 1 { "" } else { "s" })
}

/// #359 review round 1: whether confirming this verdict takes a STARTED item
/// off Now's frontier. `FogRemains` demotes every live stage to Grilling,
/// but Ready and InProgress are the only two of those actually visible on
/// the frontier today: Triage and Grilling were never on it, and Blocked is
/// not reachable from a Grill button at all.
pub fn demotes_from_frontier(verdict: GrillVerdict, stage: Stage) -> bool {
    verdict == GrillVerdict::FogRemains && matches!(stage, Stage::Ready | Stage::InProgress)
}

/// The sentence [`demotes_from_frontier`] gates.
pub const FRONTIER_DEMOTION_WARNING: &str =
    "Confirming moves this item to Grilling and takes it off the frontier.";

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: &str, done: bool, deleted_at: Option<i64>) -> Step {
        Step {
            id: id.to_string(),
            item_id: "item-1".to_string(),
            body: "pack".to_string(),
            done,
            position: 0,
            deleted_at,
            version: 1,
        }
    }

    /// Cases ported from `grill-review.test.ts`.
    #[test]
    fn would_strand_plan_is_true_for_fog_remains_with_at_least_one_live_undone_step() {
        assert!(would_strand_plan(GrillVerdict::FogRemains, &[step("s", false, None)]));
    }

    #[test]
    fn would_strand_plan_is_false_for_fog_remains_with_no_steps_at_all() {
        assert!(!would_strand_plan(GrillVerdict::FogRemains, &[]));
    }

    #[test]
    fn would_strand_plan_is_false_when_every_step_is_done_or_deleted() {
        let steps = vec![step("a", true, None), step("b", false, Some(5_000))];
        assert!(!would_strand_plan(GrillVerdict::FogRemains, &steps));
    }

    #[test]
    fn would_strand_plan_is_false_for_resolved_whatever_the_steps() {
        assert!(!would_strand_plan(GrillVerdict::Resolved, &[step("s", false, None)]));
    }

    #[test]
    fn demotes_from_frontier_is_true_for_fog_remains_on_ready_or_in_progress() {
        assert!(demotes_from_frontier(GrillVerdict::FogRemains, Stage::Ready));
        assert!(demotes_from_frontier(GrillVerdict::FogRemains, Stage::InProgress));
    }

    #[test]
    fn demotes_from_frontier_is_false_for_fog_remains_on_triage_or_grilling() {
        assert!(!demotes_from_frontier(GrillVerdict::FogRemains, Stage::Triage));
        assert!(!demotes_from_frontier(GrillVerdict::FogRemains, Stage::Grilling));
    }

    #[test]
    fn demotes_from_frontier_is_false_for_fog_remains_on_blocked_or_done() {
        assert!(!demotes_from_frontier(GrillVerdict::FogRemains, Stage::Blocked));
        assert!(!demotes_from_frontier(GrillVerdict::FogRemains, Stage::Done));
    }

    #[test]
    fn demotes_from_frontier_is_false_for_resolved_whatever_the_stage() {
        assert!(!demotes_from_frontier(GrillVerdict::Resolved, Stage::Ready));
        assert!(!demotes_from_frontier(GrillVerdict::Resolved, Stage::InProgress));
    }

    #[test]
    fn plan_replacement_label_names_the_live_undone_count_singular_and_plural() {
        assert_eq!(plan_replacement_label(&[step("a", false, None)]), "Also delete 1 unfinished step");
        assert_eq!(
            plan_replacement_label(&[step("a", false, None), step("b", false, None)]),
            "Also delete 2 unfinished steps",
        );
    }

    #[test]
    fn plan_replacement_label_excludes_done_and_deleted_steps_from_the_count() {
        let steps = vec![step("a", true, None), step("b", false, Some(5_000)), step("c", false, None)];
        assert_eq!(plan_replacement_label(&steps), "Also delete 1 unfinished step");
    }
}
