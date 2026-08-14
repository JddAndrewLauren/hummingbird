//! ADR-0023's verdict→stage mapping: written once here so the review UI, the
//! authority route that applies a completed grill, and the stored
//! `resulting_stage` (#353) never each infer their own answer to "what does
//! this verdict do to this item's stage" — the same "one spelling" precedent
//! [`crate::is_live`] set for ADR-0016 Q5 ("predicate expressed where? Rust
//! filter... one spelling"). Also the wire shapes of the `grills` table
//! (#353): the immutable per-item attachment ADR-0023 decision 2 names.

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

impl GrillVerdict {
    pub const ALL: [GrillVerdict; 2] = [GrillVerdict::Resolved, GrillVerdict::FogRemains];

    /// Byte-for-byte the `grills.verdict` DDL `CHECK` literals (`Tier`'s own
    /// `as_str`/`parse` shape) — the string this crate's serde already
    /// produces, spelled out for the storage layer's round trip.
    pub fn as_str(self) -> &'static str {
        match self {
            GrillVerdict::Resolved => "resolved",
            GrillVerdict::FogRemains => "fog_remains",
        }
    }

    pub fn parse(s: &str) -> Option<GrillVerdict> {
        GrillVerdict::ALL.into_iter().find(|v| v.as_str() == s)
    }
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

/// One completed Grill, exactly the `grills` columns, transcript included.
/// Immutable once written (ADR-0023 decision 2: "never edited after it
/// ends") — there is no patch route and no `expected_version` write for
/// this table, only the create in [`crate::api::CreateGrill`]. Served in
/// full by `GET /api/grills/:id` only, never by the sweep (decision 4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grill {
    pub id: String,
    pub item_id: String,
    pub transcript: String,
    pub summary: String,
    pub verdict: GrillVerdict,
    pub model_proposal: String,
    pub applied_patch: String,
    pub resulting_stage: Stage,
    pub completed_at: i64,
    pub version: i64,
}

/// Every `grills` column [`Grill`] carries **except `transcript`** — the
/// shape `ChangesResponse.grills` actually carries (ADR-0023 decision 4;
/// #353's whole point). A struct without the field, not `Grill` with
/// `transcript` skipped by an attribute: a later refactor cannot
/// accidentally widen a `#[serde(skip)]` back into a leak, because there is
/// no field here to un-skip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrillWithoutTranscript {
    pub id: String,
    pub item_id: String,
    pub summary: String,
    pub verdict: GrillVerdict,
    pub model_proposal: String,
    pub applied_patch: String,
    pub resulting_stage: Stage,
    pub completed_at: i64,
    pub version: i64,
}

impl From<&Grill> for GrillWithoutTranscript {
    fn from(grill: &Grill) -> GrillWithoutTranscript {
        GrillWithoutTranscript {
            id: grill.id.clone(),
            item_id: grill.item_id.clone(),
            summary: grill.summary.clone(),
            verdict: grill.verdict,
            model_proposal: grill.model_proposal.clone(),
            applied_patch: grill.applied_patch.clone(),
            resulting_stage: grill.resulting_stage,
            completed_at: grill.completed_at,
            version: grill.version,
        }
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

    #[test]
    fn verdict_as_str_matches_its_own_serde_spelling_and_round_trips() {
        for verdict in GrillVerdict::ALL {
            let wire = serde_json::to_string(&verdict).unwrap();
            assert_eq!(format!("\"{}\"", verdict.as_str()), wire);
            assert_eq!(GrillVerdict::parse(verdict.as_str()), Some(verdict));
        }
        assert_eq!(GrillVerdict::parse("nope"), None);
    }

    fn sample_grill() -> Grill {
        Grill {
            id: "g-1".into(),
            item_id: "a-1".into(),
            transcript: "turn 1\nturn 2".into(),
            summary: "clarified the destination".into(),
            verdict: GrillVerdict::Resolved,
            model_proposal: "{\"stage\":\"ready\"}".into(),
            applied_patch: "{\"stage\":\"ready\"}".into(),
            resulting_stage: Stage::Ready,
            completed_at: 1000,
            version: 1,
        }
    }

    /// The whole point of the split type (#353): converting a `Grill` to
    /// its wire-without-transcript shape carries every other field
    /// unchanged and the transcript is nowhere in the result — not present
    /// as an empty string, not present at all, because the field does not
    /// exist on the target type.
    #[test]
    fn without_transcript_keeps_every_other_field() {
        let full = sample_grill();
        let stripped = GrillWithoutTranscript::from(&full);
        assert_eq!(stripped.id, full.id);
        assert_eq!(stripped.item_id, full.item_id);
        assert_eq!(stripped.summary, full.summary);
        assert_eq!(stripped.verdict, full.verdict);
        assert_eq!(stripped.model_proposal, full.model_proposal);
        assert_eq!(stripped.applied_patch, full.applied_patch);
        assert_eq!(stripped.resulting_stage, full.resulting_stage);
        assert_eq!(stripped.completed_at, full.completed_at);
        assert_eq!(stripped.version, full.version);

        let json = serde_json::to_value(&stripped).unwrap();
        assert!(
            json.as_object().unwrap().get("transcript").is_none(),
            "no `transcript` key at all: {json}",
        );
    }
}
