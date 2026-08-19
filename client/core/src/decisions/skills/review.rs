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

use serde_json::{Map, Value};

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

/// One row of the review card's "Proposed edit" section (#595): a patch
/// field rendered as words, beside the value it would replace. `current` is
/// `None` when the item holds nothing for that field today.
///
/// This is a **render** decision only. The patch that travels to
/// `Core::complete_grill` is still the opaque JSON object
/// (`GrillProposal.patch` / the mobile seam's `patch_json`) — these rows
/// change what the person *reads* on the confirm screen, never what is
/// recorded. Android is the only consumer: the web keeps its editable
/// JSON textarea because its edit affordance is real, where Android ships
/// no inline edit anywhere and was showing escaped JSON on the one screen
/// that asks for a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedEditRow {
    /// The patch key, verbatim (`"title"`, `"size"`, ...).
    pub field: String,
    /// The label the card prints. Sentence case for the closed set; an
    /// unknown key (a malformed patch — the schema says
    /// `additionalProperties: false`) is printed verbatim rather than
    /// dropped, because a field the person cannot see is a field they are
    /// approving blind.
    pub label: String,
    pub current: Option<String>,
    pub proposed: String,
}

/// The item's current values for the fields a grill patch may touch —
/// `grill-me`'s closed patch set (`.claude/skills/grill-me/schema.json`).
/// A plain data carrier so the mobile seam can fill it from its own record
/// without this module taking a dependency on any one item shape.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CurrentItemFields {
    pub title: String,
    pub description: Option<String>,
    pub size: Option<String>,
    pub energy: Option<String>,
    pub context: Option<String>,
    pub priority: i64,
    pub deadline: Option<String>,
}

/// The closed patch field set, in the order the card prints it — the
/// schema's own field order. One table serving label and order both, so a
/// field added to the schema fails to render until it is answered for here.
const PATCH_FIELDS: [(&str, &str); 7] = [
    ("title", "Title"),
    ("description", "Description"),
    ("size", "Size"),
    ("energy", "Energy"),
    ("context", "Context"),
    ("priority", "Priority"),
    ("deadline", "Deadline"),
];

/// A JSON patch value as the words the card prints: strings verbatim
/// (unquoted), everything else in its JSON spelling. The schema allows
/// strings and one integer, so the fallback arm is for malformed patches —
/// shown, not hidden, same rule as the unknown-key row.
fn display_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// #595: the "Proposed edit" section's rows — every field the patch
/// carries, in [`PATCH_FIELDS`] order, each beside the current value it
/// would replace; unknown keys follow, in the patch's own (alphabetical)
/// order. An empty patch returns no rows, and the card states that as a
/// fact (`fog_remains` commonly carries an empty patch) rather than
/// rendering a blank.
pub fn proposal_rows(patch: &Map<String, Value>, current: &CurrentItemFields) -> Vec<ProposedEditRow> {
    let mut rows = Vec::new();
    for (field, label) in PATCH_FIELDS {
        if let Some(value) = patch.get(field) {
            let current_value = match field {
                "title" => Some(current.title.clone()),
                "description" => current.description.clone(),
                "size" => current.size.clone(),
                "energy" => current.energy.clone(),
                "context" => current.context.clone(),
                "priority" => Some(current.priority.to_string()),
                "deadline" => current.deadline.clone(),
                _ => unreachable!("PATCH_FIELDS is the closed set"),
            };
            rows.push(ProposedEditRow {
                field: field.to_string(),
                label: label.to_string(),
                current: current_value,
                proposed: display_value(value),
            });
        }
    }
    for (key, value) in patch {
        if PATCH_FIELDS.iter().all(|(field, _)| field != key) {
            rows.push(ProposedEditRow {
                field: key.clone(),
                label: key.clone(),
                current: None,
                proposed: display_value(value),
            });
        }
    }
    rows
}

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

    // #595 — proposal_rows.

    fn current() -> CurrentItemFields {
        CurrentItemFields {
            title: "Plan India trip".to_string(),
            description: None,
            size: Some("normal".to_string()),
            energy: None,
            context: Some("@computer".to_string()),
            priority: 2,
            deadline: None,
        }
    }

    fn patch(json: &str) -> Map<String, Value> {
        serde_json::from_str(json).expect("test patch parses")
    }

    #[test]
    fn proposal_rows_render_in_the_schema_field_order_not_the_patch_order() {
        // serde_json's Map is sorted; "size" < "title" alphabetically, so an
        // order-preserving bug would print size first.
        let rows = proposal_rows(&patch(r#"{"size":"deep","title":"Plan India trip — itinerary"}"#), &current());
        let fields: Vec<&str> = rows.iter().map(|r| r.field.as_str()).collect();
        assert_eq!(fields, vec!["title", "size"]);
    }

    #[test]
    fn proposal_rows_pair_each_field_with_its_current_value() {
        let rows = proposal_rows(&patch(r#"{"size":"deep"}"#), &current());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Size");
        assert_eq!(rows[0].current.as_deref(), Some("normal"));
        assert_eq!(rows[0].proposed, "deep");
    }

    #[test]
    fn proposal_rows_report_an_unset_current_as_none_not_empty() {
        let rows = proposal_rows(&patch(r#"{"energy":"low"}"#), &current());
        assert_eq!(rows[0].current, None);
    }

    #[test]
    fn proposal_rows_render_strings_unquoted_and_numbers_in_json_spelling() {
        let rows = proposal_rows(&patch(r#"{"priority":4,"title":"T"}"#), &current());
        assert_eq!(rows[0].proposed, "T");
        assert_eq!(rows[1].proposed, "4");
        assert_eq!(rows[1].current.as_deref(), Some("2"));
    }

    #[test]
    fn proposal_rows_are_empty_for_an_empty_patch() {
        // `fog_remains` commonly carries an empty patch — the card states
        // the fact; this function just returns nothing.
        assert!(proposal_rows(&patch("{}"), &current()).is_empty());
    }

    #[test]
    fn proposal_rows_print_an_unknown_key_verbatim_after_the_known_fields() {
        // additionalProperties: false upstream — but a malformed patch is
        // shown, never silently dropped, because the person is approving it.
        let rows = proposal_rows(&patch(r#"{"stage":"done","title":"T"}"#), &current());
        let fields: Vec<&str> = rows.iter().map(|r| r.field.as_str()).collect();
        assert_eq!(fields, vec!["title", "stage"]);
        assert_eq!(rows[1].label, "stage");
        assert_eq!(rows[1].current, None);
        assert_eq!(rows[1].proposed, "done");
    }
}
