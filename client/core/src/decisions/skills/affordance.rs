//! Which microtask gesture an item's own steps make legal (#273, against
//! #307; sunk here from `client/web/src/skills/microtask-affordance.ts` at
//! #539).
//!
//! The seam declines a bare run against an item carrying a live *undone*
//! plan, and rewriting takes an explicit `replace: true`. The client reads
//! the item's steps through the normal path anyway, so it knows which
//! gesture is legal before the tap and offers only that one — the seam's
//! decline stays a backstop for races and non-client callers, and #307's
//! body says not to string-match its prose to pick an affordance.
//!
//! **`item_detail::ItemDetail::microtask_affordance` is the applied
//! result** (#539): every client reads the field, none re-derives it —
//! Kotlin is told whether to offer the affordance, not how to decide.

use hummingbird_domain::Step;
use serde::{Deserialize, Serialize};

/// The steps that still have a plan left in them — `undoneSteps` over
/// `liveSteps` in `runner/src/skills/microtask.js`, the same two predicates
/// in the same order.
///
/// Soft-deleted rows are history, and a `done` step is *record*: neither is
/// something a continuation would clobber.
pub fn live_undone_steps(steps: &[Step]) -> Vec<&Step> {
    steps.iter().filter(|step| step.deleted_at.is_none() && !step.done).collect()
}

/// The web's own `{ kind: "break" }` / `{ kind: "rewrite", undoneCount }`
/// shape — the wire encoding this type crosses `ffi-web` as, matching
/// `skills/microtask-affordance.ts`'s `MicrotaskAffordance` byte for byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MicrotaskAffordance {
    Break,
    Rewrite {
        #[serde(rename = "undoneCount")]
        undone_count: u32,
    },
}

/// **All-done is [`MicrotaskAffordance::Break`], not `Rewrite`** (#307 point
/// 1). Ticked steps are record rather than plan, so an item whose live
/// steps are all done has nothing left to protect and an append after them
/// is the normal case — the seam agrees, and offering "Rewrite 0 steps"
/// there would both read as nonsense and send a `replace: true` that has
/// nothing to replace.
pub fn microtask_affordance(steps: &[Step]) -> MicrotaskAffordance {
    let undone = live_undone_steps(steps);
    if undone.is_empty() {
        MicrotaskAffordance::Break
    } else {
        MicrotaskAffordance::Rewrite { undone_count: undone.len() as u32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: &str, done: bool, deleted_at: Option<i64>) -> Step {
        Step {
            id: id.to_string(),
            item_id: "item-1".to_string(),
            body: "put on music".to_string(),
            done,
            position: 1,
            deleted_at,
            version: 1,
        }
    }

    /// Cases ported from `microtask-affordance.test.ts`.
    #[test]
    fn live_undone_steps_is_live_and_not_done_both_predicates_in_that_order() {
        let steps = vec![
            step("a", false, None),
            step("b", true, None),
            step("c", false, Some(1_000)),
            step("d", true, Some(1_000)),
            step("e", false, None),
        ];
        let ids: Vec<&str> = live_undone_steps(&steps).iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "e"]);
    }

    #[test]
    fn live_undone_steps_is_empty_for_no_steps_at_all() {
        assert!(live_undone_steps(&[]).is_empty());
    }

    #[test]
    fn an_item_with_no_steps_offers_break() {
        assert_eq!(microtask_affordance(&[]), MicrotaskAffordance::Break);
    }

    #[test]
    fn an_item_whose_live_steps_are_all_done_offers_break_not_rewrite() {
        let steps = vec![step("a", true, None), step("b", true, None)];
        assert_eq!(microtask_affordance(&steps), MicrotaskAffordance::Break);
    }

    #[test]
    fn an_item_with_only_soft_deleted_steps_offers_break() {
        let steps = vec![step("a", false, Some(1_000))];
        assert_eq!(microtask_affordance(&steps), MicrotaskAffordance::Break);
    }

    #[test]
    fn a_live_undone_plan_offers_rewrite_counting_only_the_undone() {
        let steps = vec![
            step("a", true, None),
            step("b", false, None),
            step("c", false, None),
            step("d", false, Some(1_000)),
        ];
        assert_eq!(microtask_affordance(&steps), MicrotaskAffordance::Rewrite { undone_count: 2 });
    }

    /// The wire shape `skills/microtask-affordance.ts` expects, byte for
    /// byte — what makes the web's sink a rewire rather than a rewrite.
    #[test]
    fn the_wire_shape_matches_the_webs_own_type() {
        assert_eq!(serde_json::to_string(&MicrotaskAffordance::Break).unwrap(), r#"{"kind":"break"}"#);
        assert_eq!(
            serde_json::to_string(&MicrotaskAffordance::Rewrite { undone_count: 2 }).unwrap(),
            r#"{"kind":"rewrite","undoneCount":2}"#,
        );
    }
}
