//! ADR-0023's verdict→stage mapping: written once here so the review UI, the
//! authority route that applies a completed grill, and the stored
//! `resulting_stage` (#353) never each infer their own answer to "what does
//! this verdict do to this item's stage" — the same "one spelling" precedent
//! [`crate::is_live`] set for ADR-0016 Q5 ("predicate expressed where? Rust
//! filter... one spelling").

use crate::item::Stage;
use serde::{Deserialize, Serialize};

/// A Grill's reading of whether fog remains — never a stage in itself
/// (`CONTEXT.md`'s **Grill** entry draws the same cut **Severity** draws
/// against **Tier**). The two wire spellings (`resolved`, `fog_remains`)
/// already exist as the runner's grill-me schema enum; this is the same
/// vocabulary, not a second one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrillVerdict {
    Resolved,
    FogRemains,
}

/// A Grill verdict was asked of a `Done` item. Done is out of scope for the
/// whole Grill plan; this makes that fact a rejection rather than a guess —
/// there is deliberately no `Stage` this function will return for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrillOnDoneItem;

/// The decided verdict→stage table (issue #352), taking only the item's
/// current stage and the verdict — no clock, no item, no ambient state:
///
/// | Current stage | `resolved`  | `fog_remains` |
/// |----------------|------------|----------------|
/// | Triage         | Ready      | Grilling       |
/// | Grilling       | Ready      | Grilling       |
/// | Ready          | unchanged  | Grilling       |
/// | In Progress    | unchanged  | Grilling       |
/// | Blocked        | unchanged  | Grilling       |
/// | Done           | unreachable — rejected |
///
/// **Blocked behaves exactly like Ready and In Progress.** Blocked means
/// the world is making the work wait (`CONTEXT.md`'s **External wait**), a
/// fact a fog verdict has no business ending — so `resolved` leaves a
/// Blocked item blocked, and `fog_remains` demotes it to Grilling like any
/// other live stage. No Grill surface reads a Blocked item today (#211 is
/// the open issue for that read surface), but the arm is decided now so
/// landing #211 needs no second decision here.
pub fn resulting_stage(current: Stage, verdict: GrillVerdict) -> Result<Stage, GrillOnDoneItem> {
    match (current, verdict) {
        (Stage::Done, _) => Err(GrillOnDoneItem),
        (Stage::Triage | Stage::Grilling, GrillVerdict::Resolved) => Ok(Stage::Ready),
        (_, GrillVerdict::FogRemains) => Ok(Stage::Grilling),
        (unchanged, GrillVerdict::Resolved) => Ok(unchanged),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triage_or_grilling_resolved_lands_on_ready() {
        assert_eq!(resulting_stage(Stage::Triage, GrillVerdict::Resolved), Ok(Stage::Ready));
        assert_eq!(resulting_stage(Stage::Grilling, GrillVerdict::Resolved), Ok(Stage::Ready));
    }

    #[test]
    fn every_live_stage_with_fog_remains_demotes_to_grilling() {
        for stage in [Stage::Triage, Stage::Grilling, Stage::Ready, Stage::InProgress, Stage::Blocked] {
            assert_eq!(
                resulting_stage(stage, GrillVerdict::FogRemains),
                Ok(Stage::Grilling),
                "{stage:?} + fog_remains should demote to Grilling",
            );
        }
    }

    #[test]
    fn ready_in_progress_and_blocked_resolved_are_unchanged() {
        for stage in [Stage::Ready, Stage::InProgress, Stage::Blocked] {
            assert_eq!(
                resulting_stage(stage, GrillVerdict::Resolved),
                Ok(stage),
                "{stage:?} + resolved should leave the stage unchanged",
            );
        }
    }

    /// Blocked means the world is making the work wait — a fog verdict has
    /// no business ending that, so it takes exactly the same two arms as
    /// Ready and In Progress rather than some Blocked-specific rule.
    #[test]
    fn blocked_behaves_exactly_like_ready_and_in_progress() {
        // Same shape for all three: `resolved` leaves the stage unchanged,
        // `fog_remains` demotes to Grilling — no Blocked-specific rule.
        assert_eq!(resulting_stage(Stage::Blocked, GrillVerdict::Resolved), Ok(Stage::Blocked));
        assert_eq!(resulting_stage(Stage::Blocked, GrillVerdict::FogRemains), Ok(Stage::Grilling));
        assert_eq!(
            resulting_stage(Stage::Blocked, GrillVerdict::FogRemains),
            resulting_stage(Stage::InProgress, GrillVerdict::FogRemains),
        );
    }

    #[test]
    fn done_is_rejected_for_both_verdicts_not_silently_mapped() {
        assert_eq!(resulting_stage(Stage::Done, GrillVerdict::Resolved), Err(GrillOnDoneItem));
        assert_eq!(resulting_stage(Stage::Done, GrillVerdict::FogRemains), Err(GrillOnDoneItem));
    }

    /// `GrillVerdict`'s wire spelling must match the runner's existing
    /// grill-me schema enum (`runner/test/grill-me.test.js`) — this is the
    /// same vocabulary, not a second one.
    #[test]
    fn verdict_serde_matches_the_runner_s_existing_wire_spelling() {
        assert_eq!(serde_json::to_string(&GrillVerdict::Resolved).unwrap(), "\"resolved\"");
        assert_eq!(serde_json::to_string(&GrillVerdict::FogRemains).unwrap(), "\"fog_remains\"");
    }
}
