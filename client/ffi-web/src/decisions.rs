//! The free `#[wasm_bindgen]` door onto `hummingbird_core::decisions`
//! (ADR-0025, #141/M1-1).
//!
//! Everything here is a **free function over plain scalars and JSON**, in
//! deliberate contrast to [`crate::task_host`]/[`crate::calendar_host`],
//! which hand JS a stateful handle. That difference is the whole reason the
//! web can instantiate this module a second time on the main thread without
//! touching ADR-0010: a second *instance* of a stateless module holds no
//! core, no storage handle and no queue, so there is no second sync engine
//! and nothing to keep coherent between the two instantiations. Add a
//! constructor, a `static mut`, or anything that reads storage here, and
//! that argument stops being true — the ADR-0025 amendment's scope note
//! says so in the ADR, and this is the same sentence at the point of use.
//!
//! Unlike the host shims, these compile and are unit-tested on the native
//! target: `wasm_bindgen` on a free function over `&str`/`bool`/`String`
//! needs no JS to run against, exactly like `core_api_version`.

use hummingbird_core::ItemAction;
use hummingbird_domain::Stage;
use wasm_bindgen::prelude::*;

/// Whether a capture draft is worth submitting — `hummingbird_core`'s
/// [`hummingbird_core::decisions::can_submit_capture`] verbatim, exposed to
/// the web's `decisions/seam.ts` wrapper. Called per keystroke on the main
/// thread, which is why it takes the draft and returns a `bool` rather than
/// posting anything anywhere.
#[wasm_bindgen]
pub fn can_submit_capture(draft: &str) -> bool {
    hummingbird_core::decisions::can_submit_capture(draft)
}

// -------------------------------------------------------------- M1-2 (#500)
// The capture decision set: urgency, the deadline-field grammar, and the
// capture/triage field problems. Every function below is a thin
// `#[wasm_bindgen]` door onto `hummingbird_core::decisions::{urgency,
// capture}` — see those modules for the rules themselves. Structured
// values cross as JSON: `wasm_bindgen` has no derive for an arbitrary
// struct without pulling in `serde-wasm-bindgen`, and JSON is what
// `seam.ts` already parses on the way back.

use hummingbird_core::decisions::{capture, urgency};

/// [`urgency::compute_urgency`], with the band returned by its wire name
/// (`urgency::UrgencyBand::as_str`) rather than a JS enum binding neither
/// wasm-bindgen exposes cheaply nor a free function needs. `now` is
/// deadline-shaped — see `urgency.rs`'s module header for why this takes a
/// string rather than an epoch millisecond count.
#[wasm_bindgen]
pub fn compute_urgency(deadline: Option<String>, now: &str) -> String {
    urgency::compute_urgency(deadline.as_deref(), now).as_str().to_string()
}

#[wasm_bindgen]
pub fn is_valid_deadline(deadline: &str) -> bool {
    urgency::is_valid_deadline_field(deadline)
}

#[wasm_bindgen]
pub fn is_valid_scheduled_date(scheduled_date: &str) -> bool {
    urgency::is_valid_scheduled_date(scheduled_date)
}

#[wasm_bindgen]
pub fn deadline_sort_key(deadline: &str) -> String {
    urgency::deadline_sort_key_field(deadline)
}

/// [`urgency::split_deadline`], JSON-encoded: `{"date":"...","time":"..."|null}`.
#[wasm_bindgen]
pub fn split_deadline(value: &str) -> String {
    let parts = urgency::split_deadline(value);
    serde_json::json!({ "date": parts.date, "time": parts.time }).to_string()
}

#[wasm_bindgen]
pub fn join_deadline(date: &str, time: Option<String>) -> String {
    urgency::join_deadline(date, time.as_deref())
}

/// [`capture::capture_meta_problems`], JSON-encoded:
/// `{"deadline":"..."|absent,"scheduledDate":"..."|absent}` — camelCase to
/// match `CaptureMetaProblems` (`capture-meta.ts`) and `TriageDraftProblems`
/// (`triage-form.ts`), both of which read this without a remapping step.
#[wasm_bindgen]
pub fn capture_meta_problems(deadline: &str, scheduled_date: &str) -> String {
    let problems = capture::capture_meta_problems(deadline, scheduled_date);
    serde_json::json!({
        "deadline": problems.deadline,
        "scheduledDate": problems.scheduled_date,
    })
    .to_string()
}

/// [`capture::priority_from_select`] — the capture box's `"0"` -> "not
/// sent" priority rule. `i32`, not the core function's own `i64`:
/// wasm-bindgen crosses `i64` as a JS `BigInt`, and `CaptureFields.priority`
/// (`store/worker-client.ts`) is a plain `number` — the wire's `priority`
/// column is `0..=4`, nowhere near `i32`'s range, so narrowing here loses
/// nothing and keeps the boundary type the caller already expects.
#[wasm_bindgen]
pub fn priority_from_select(raw: &str) -> Option<i32> {
    capture::priority_from_select(raw).map(|value| value as i32)
}

use hummingbird_core::decisions::vocabulary;

/// [`vocabulary::size_options`]/[`vocabulary::energy_options`], JSON-encoded
/// as `[{"value":"...","label":"..."}, ...]` — the vocabulary's real values
/// only, no leading "Not set" entry (that is the TS form-adapter's own
/// resting-state concern, prepended client-side).
#[wasm_bindgen]
pub fn size_options_json() -> String {
    serde_json::to_string(&vocab_json(vocabulary::size_options())).unwrap()
}

#[wasm_bindgen]
pub fn energy_options_json() -> String {
    serde_json::to_string(&vocab_json(vocabulary::energy_options())).unwrap()
}

fn vocab_json(options: Vec<vocabulary::VocabOption>) -> serde_json::Value {
    serde_json::Value::Array(
        options
            .into_iter()
            .map(|o| serde_json::json!({ "value": o.value, "label": o.label }))
            .collect(),
    )
}

/// [`vocabulary::CONTEXTS`], JSON-encoded as a plain string array — see
/// that constant's doc comment for why the web's own `field-vocabulary.ts`
/// export stays a literal array pinned against this rather than a live
/// call through this function in M1-2 (a module-evaluation-order
/// constraint, recorded in #500's PR). M1-5's Android capture surface is
/// this function's first production caller.
#[wasm_bindgen]
pub fn contexts_json() -> String {
    serde_json::to_string(&vocabulary::CONTEXTS).unwrap()
}

/// [`vocabulary::FRONTIER_AXES`], JSON-encoded — M1-3's (#501) first
/// consumer; nothing in M1-2 calls this in production.
#[wasm_bindgen]
pub fn frontier_axes_json() -> String {
    serde_json::to_string(&vocabulary::FRONTIER_AXES).unwrap()
}

// -------------------------------------------------------------- M1-4 (#502)

/// S11/#109's act affordances for `stage` — `hummingbird_core::decisions::available_actions`
/// verbatim, wire-vocabulary strings in, a JSON array of the same strings
/// out. An unrecognised stage answers with an empty array rather than
/// panicking; the TS side is typed to `TaskStageName` so this never fires
/// in practice, but the boundary does not trust that from the wasm side.
/// The `Stage`/`ItemAction` string vocabularies themselves are spelled once
/// each, in `Stage::parse`/`Stage::as_str` and `ItemAction::parse`/
/// `ItemAction::as_str` (`client/core/src/lib.rs`) — this file, and
/// `client/ffi-web/src/task_host.rs`'s own boundary, both call those rather
/// than carrying a second copy of either match.
#[wasm_bindgen]
pub fn item_available_actions(stage: &str) -> String {
    let actions: Vec<&'static str> = match Stage::parse(stage) {
        Some(stage) => hummingbird_core::decisions::available_actions(stage)
            .iter()
            .map(|action| action.as_str())
            .collect(),
        None => Vec::new(),
    };
    serde_json::to_string(&actions).unwrap_or_else(|_| "[]".to_string())
}

/// The stage an act vocabulary word sets — `hummingbird_core::decisions::applied_stage`
/// verbatim. `null` covers both `ItemAction::Cancel` (which sets no stage)
/// and an unrecognised `action` string.
#[wasm_bindgen]
pub fn item_applied_stage(action: &str) -> Option<String> {
    ItemAction::parse(action)
        .and_then(hummingbird_core::decisions::applied_stage)
        .map(|stage| stage.as_str().to_string())
}

/// Whether `stage` offers the one-click mark-done checkmark —
/// `hummingbird_core::decisions::can_mark_done` verbatim. An unrecognised
/// stage answers `false`.
#[wasm_bindgen]
pub fn item_can_mark_done(stage: &str, archived: bool) -> bool {
    match Stage::parse(stage) {
        Some(stage) => hummingbird_core::decisions::can_mark_done(stage, archived),
        None => false,
    }
}

/// Whether `stage` offers "Grill me" — `hummingbird_core::decisions::can_grill`
/// verbatim. An unrecognised stage answers `false`.
#[wasm_bindgen]
pub fn item_can_grill(stage: &str) -> bool {
    match Stage::parse(stage) {
        Some(stage) => hummingbird_core::decisions::can_grill(stage),
        None => false,
    }
}

/// The Grill button's own label — `hummingbird_core::decisions::grill_button_label`
/// verbatim.
#[wasm_bindgen]
pub fn item_grill_button_label(has_draft: bool) -> String {
    hummingbird_core::decisions::grill_button_label(has_draft).to_string()
}

// ----------------------------------------------------------------- M1-3 (#501)
// The frontier's ordering, grouping and faceting, plus the combined
// Now/Triage queue. Every function below crosses [`FrontierItemDTO`]s — the
// handful of fields [`FrontierItem`] actually reads, JSON's own strict
// subset of `TaskItemDTO` (`client/web/src/store/protocol.ts`), exactly
// [`QueueItemDTO`] below's pattern — and returns the *ordered/filtered ids*
// rather than whole items: `seam.ts`'s wrappers hold the full `TaskItemDTO`s
// already and map ids back onto them, so nothing crosses the boundary
// twice, and the crossing itself never carries a field no rule reads.
//
// This replaces `decisions_probe_item_payload`, M1-1's measuring instrument
// for exactly this crossing cost — the real calls below are its answer,
// not a second, still-hypothetical one next to it.

use hummingbird_core::decisions::frontier::{
    self, FacetSelection, Facet, FrontierAxis, FrontierItem, ProjectName,
};
use hummingbird_core::decisions::queue::{self, QueueItem};

/// One item as [`FrontierItem`] reads it — `id`, `priority`, `deadline`,
/// `context`, `size`, `energy` and `projectId`, camelCase, and nothing
/// else: the frontier rules never read a title, a stage or a timestamp, so
/// `seam.ts`'s `frontierPayload` never serializes one outward. Deliberately
/// a distinct shape from [`crate::task_host::FrontierItemDTO`]: that one is
/// what the *worker* serializes on its way out of the core (a whole item,
/// `#[serde(flatten)]`ed), while this is the main-thread seam's own
/// strict-subset input, exactly [`QueueItemDTO`]'s pattern below.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontierItemDTO {
    pub id: String,
    pub priority: i64,
    pub deadline: Option<String>,
    pub context: Option<String>,
    pub size: Option<String>,
    pub energy: Option<String>,
    pub project_id: Option<String>,
}

fn to_frontier_item(dto: &FrontierItemDTO) -> FrontierItem {
    FrontierItem {
        id: dto.id.clone(),
        priority: dto.priority,
        deadline: dto.deadline.clone(),
        context: dto.context.clone(),
        size: dto.size.clone(),
        energy: dto.energy.clone(),
        project_id: dto.project_id.clone(),
    }
}

fn parse_items(items_json: &str) -> Result<Vec<FrontierItemDTO>, String> {
    serde_json::from_str(items_json).map_err(|e| e.to_string())
}

/// [`frontier::priority_rank`] — pinned from the web side by
/// `seam.test.ts` against `priority.ts`'s own `priorityRank`, the one
/// vocabulary the M1-3 review found still duplicated rather than sunk
/// (`priority.ts`'s header records why the *labels* stay client-side; the
/// rank was never meant to be part of that carve-out). `i32` in and out,
/// not the core rule's own `i64`: same reasoning as
/// [`priority_from_select`] above — `wasm-bindgen` crosses `i64` as a JS
/// `BigInt`, and both the raw wire priority and its rank live nowhere near
/// `i32`'s range, so narrowing at the boundary loses nothing and matches
/// the plain `number` `priorityRank`/`priorityRankFromCore` already use.
#[wasm_bindgen]
pub fn priority_rank(raw: i32) -> i32 {
    frontier::priority_rank(raw as i64) as i32
}

/// The frontier's stable priority/deadline display order
/// ([`frontier::by_priority_then_due`] — ADR-0021 decision 1's one
/// spelling), JSON-encoded as an ordered array of ids.
#[wasm_bindgen]
pub fn order_frontier_ids(items_json: &str) -> String {
    match parse_items(items_json) {
        Ok(items) => {
            let entries: Vec<FrontierItem> = items.iter().map(to_frontier_item).collect();
            serde_json::to_string(&frontier::by_priority_then_due(&entries)).unwrap()
        }
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    }
}

/// One project's id and name — [`frontier::ProjectName`], JSON-encoded.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectNameDTO {
    id: String,
    name: String,
}

/// One [`frontier::FrontierColumn`], JSON-encoded:
/// `{"value":"..."|null,"label":"..."|null,"ids":["..."]}`.
///
/// `axis` is one of `frontier::FrontierAxis`'s wire names
/// (`"context"|"project"|"size"|"energy"`); an unrecognised axis answers no
/// columns rather than panicking.
#[wasm_bindgen]
pub fn group_frontier_json(items_json: &str, axis: &str, projects_json: &str) -> String {
    let Some(axis) = FrontierAxis::parse(axis) else {
        return "[]".to_string();
    };
    let items = match parse_items(items_json) {
        Ok(items) => items,
        Err(error) => return serde_json::json!({ "error": error }).to_string(),
    };
    let projects: Vec<ProjectNameDTO> = match serde_json::from_str(projects_json) {
        Ok(projects) => projects,
        Err(error) => return serde_json::json!({ "error": error.to_string() }).to_string(),
    };
    let entries: Vec<FrontierItem> = items.iter().map(to_frontier_item).collect();
    let project_names: Vec<ProjectName> = projects
        .into_iter()
        .map(|p| ProjectName { id: p.id, name: p.name })
        .collect();

    let columns = frontier::group_frontier(&entries, axis, &project_names);
    let json: Vec<serde_json::Value> = columns
        .into_iter()
        .map(|c| serde_json::json!({ "value": c.value, "label": c.label, "ids": c.ids }))
        .collect();
    serde_json::to_string(&json).unwrap()
}

/// One `FacetSelection`, JSON-encoded:
/// `{"context":["..."],"size":["..."],"energy":["..."],"urgency":["..."]}`.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
struct FacetSelectionDTO {
    context: Vec<String>,
    size: Vec<String>,
    energy: Vec<String>,
    urgency: Vec<String>,
}

fn to_facet_selection(dto: &FacetSelectionDTO) -> FacetSelection {
    FacetSelection {
        context: dto.context.iter().cloned().collect(),
        size: dto.size.iter().cloned().collect(),
        energy: dto.energy.iter().cloned().collect(),
        urgency: dto.urgency.iter().cloned().collect(),
    }
}

fn from_facet_selection(sel: &FacetSelection) -> FacetSelectionDTO {
    FacetSelectionDTO {
        context: sel.context.iter().cloned().collect(),
        size: sel.size.iter().cloned().collect(),
        energy: sel.energy.iter().cloned().collect(),
        urgency: sel.urgency.iter().cloned().collect(),
    }
}

/// [`frontier::facet_count`].
#[wasm_bindgen]
pub fn facet_count_json(selection_json: &str) -> u32 {
    let Ok(dto) = serde_json::from_str::<FacetSelectionDTO>(selection_json) else {
        return 0;
    };
    frontier::facet_count(&to_facet_selection(&dto)) as u32
}

/// [`frontier::toggle_facet`], JSON in and out. `facet` is one of
/// `"context"|"size"|"energy"|"urgency"`; an unrecognised facet returns the
/// selection unchanged.
#[wasm_bindgen]
pub fn toggle_facet_json(selection_json: &str, facet: &str, value: &str) -> String {
    let Ok(dto) = serde_json::from_str::<FacetSelectionDTO>(selection_json) else {
        return selection_json.to_string();
    };
    let Some(facet) = Facet::parse(facet) else {
        return selection_json.to_string();
    };
    let next = frontier::toggle_facet(&to_facet_selection(&dto), facet, value);
    serde_json::to_string(&from_facet_selection(&next)).unwrap()
}

/// [`frontier::apply_facets`], JSON-encoded as an array of ids. `now` is
/// deadline-shaped (see `urgency.rs`'s module header).
#[wasm_bindgen]
pub fn apply_facets_ids(items_json: &str, selection_json: &str, now: &str) -> String {
    let items = match parse_items(items_json) {
        Ok(items) => items,
        Err(error) => return serde_json::json!({ "error": error }).to_string(),
    };
    let Ok(dto) = serde_json::from_str::<FacetSelectionDTO>(selection_json) else {
        return "[]".to_string();
    };
    let entries: Vec<FrontierItem> = items.iter().map(to_frontier_item).collect();
    let picked = to_facet_selection(&dto);
    serde_json::to_string(&frontier::apply_facets(&entries, &picked, now)).unwrap()
}

/// [`frontier::contexts_of`], JSON-encoded as an array of strings.
#[wasm_bindgen]
pub fn contexts_of_json(items_json: &str) -> String {
    let items = match parse_items(items_json) {
        Ok(items) => items,
        Err(error) => return serde_json::json!({ "error": error }).to_string(),
    };
    let entries: Vec<FrontierItem> = items.iter().map(to_frontier_item).collect();
    serde_json::to_string(&frontier::contexts_of(&entries)).unwrap()
}

/// One item as [`queue::order_triage`]/[`queue::triage_process_queue`] read
/// it: an id and its capture time — the JSON shape `seam.ts` sends, a strict
/// subset of `TaskItemDTO` exactly like [`FrontierItemDTO`] above.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueueItemDTO {
    id: String,
    created_at: i64,
}

fn to_queue_item(dto: &QueueItemDTO) -> QueueItem {
    QueueItem { id: dto.id.clone(), created_at: dto.created_at }
}

/// [`queue::order_triage`], JSON-encoded as an ordered array of ids.
#[wasm_bindgen]
pub fn order_triage_ids(items_json: &str) -> String {
    match serde_json::from_str::<Vec<QueueItemDTO>>(items_json) {
        Ok(items) => {
            let entries: Vec<QueueItem> = items.iter().map(to_queue_item).collect();
            serde_json::to_string(&queue::order_triage(&entries)).unwrap()
        }
        Err(error) => serde_json::json!({ "error": error.to_string() }).to_string(),
    }
}

/// [`queue::triage_process_queue`], JSON-encoded:
/// `{"ids":["..."],"capturedCount":N,"grillingCount":N}`.
#[wasm_bindgen]
pub fn triage_process_queue_json(
    triage_json: &str,
    grilling_json: &str,
    draft_ids_json: &str,
) -> String {
    let parse = |s: &str| serde_json::from_str::<Vec<QueueItemDTO>>(s);
    let (triage, grilling, draft_ids) = match (
        parse(triage_json),
        parse(grilling_json),
        serde_json::from_str::<Vec<String>>(draft_ids_json),
    ) {
        (Ok(t), Ok(g), Ok(d)) => (t, g, d),
        _ => return serde_json::json!({ "error": "unreadable queue payload" }).to_string(),
    };
    let triage: Vec<QueueItem> = triage.iter().map(to_queue_item).collect();
    let grilling: Vec<QueueItem> = grilling.iter().map(to_queue_item).collect();

    let result = queue::triage_process_queue(&triage, &grilling, &draft_ids);
    serde_json::json!({
        "ids": result.ids,
        "capturedCount": result.captured_count,
        "grillingCount": result.grilling_count,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exposure is a pass-through and nothing more — the rule itself is
    /// tested in `hummingbird_core::decisions::capture`. This pins that the
    /// binding did not grow an opinion of its own on the way across.
    #[test]
    fn the_capture_binding_is_the_core_rule_verbatim() {
        for draft in [
            "",
            "   ",
            "\t\n",
            "\u{feff}",
            "buy milk",
            "  buy milk  ",
            "\u{feff}buy milk",
        ] {
            assert_eq!(
                can_submit_capture(draft),
                hummingbird_core::decisions::can_submit_capture(draft),
                "{draft:?} disagreed across the binding",
            );
        }
    }

    #[test]
    fn item_available_actions_is_the_core_rule_verbatim_as_json() {
        assert_eq!(item_available_actions("triage"), "[]");
        assert_eq!(
            item_available_actions("ready"),
            r#"["start","complete","block","cancel"]"#,
        );
        assert_eq!(item_available_actions("in_progress"), r#"["complete","block","cancel"]"#);
        assert_eq!(item_available_actions("blocked"), r#"["start","complete","cancel"]"#);
        assert_eq!(item_available_actions("done"), "[]");
    }

    #[test]
    fn item_available_actions_answers_empty_for_an_unrecognised_stage() {
        assert_eq!(item_available_actions("not-a-stage"), "[]");
    }

    #[test]
    fn item_applied_stage_is_the_core_rule_verbatim() {
        assert_eq!(item_applied_stage("start").as_deref(), Some("in_progress"));
        assert_eq!(item_applied_stage("complete").as_deref(), Some("done"));
        assert_eq!(item_applied_stage("block").as_deref(), Some("blocked"));
        assert_eq!(item_applied_stage("cancel"), None);
    }

    #[test]
    fn item_applied_stage_answers_none_for_an_unrecognised_action() {
        assert_eq!(item_applied_stage("not-an-action"), None);
    }

    #[test]
    fn item_can_mark_done_is_the_core_rule_verbatim() {
        for stage in ["triage", "grilling", "ready", "in_progress", "blocked"] {
            assert!(item_can_mark_done(stage, false), "{stage} should allow mark-done");
        }
        assert!(!item_can_mark_done("done", false));
        assert!(!item_can_mark_done("ready", true));
    }

    #[test]
    fn item_can_mark_done_answers_false_for_an_unrecognised_stage() {
        assert!(!item_can_mark_done("not-a-stage", false));
    }

    #[test]
    fn item_can_grill_is_the_core_rule_verbatim() {
        for stage in ["triage", "grilling", "ready", "in_progress"] {
            assert!(item_can_grill(stage), "{stage} should allow grill");
        }
        for stage in ["blocked", "done"] {
            assert!(!item_can_grill(stage), "{stage} should refuse grill");
        }
    }

    #[test]
    fn item_can_grill_answers_false_for_an_unrecognised_stage() {
        assert!(!item_can_grill("not-a-stage"));
    }

    #[test]
    fn item_grill_button_label_is_the_core_rule_verbatim() {
        assert_eq!(item_grill_button_label(false), "Grill me");
        assert_eq!(item_grill_button_label(true), "Resume grill");
    }

    /// A `FrontierItemDTO` JSON literal — `stage` is accepted for callers'
    /// readability (`one_item("a", "ready")` names what the id stands for)
    /// but is not part of the payload: `FrontierItem` never reads a stage,
    /// so `seam.ts`'s real `frontierPayload` never sends one either.
    fn one_item(id: &str, _stage: &str) -> String {
        serde_json::json!({
            "id": id,
            "size": "quick",
            "energy": "low",
            "context": "@errands",
            "priority": 2,
            "projectId": null,
            "deadline": "2026-08-20",
        })
        .to_string()
    }

    // ---------------------------------------------------------- M1-3 (#501)
    // Every binding below is a pass-through and nothing more — the rule
    // itself is tested in `hummingbird_core::decisions::{frontier,queue}`.
    // These pin that the JSON crossing did not grow an opinion of its own.

    #[test]
    fn order_frontier_ids_answers_urgent_before_low_never_the_raw_wire_number() {
        let payload = format!(
            "[{}, {}]",
            json_item("none", 0, None),
            json_item("urgent", 1, None),
        );
        assert_eq!(order_frontier_ids(&payload), r#"["urgent","none"]"#);
    }

    #[test]
    fn order_frontier_ids_answers_rather_than_panicking_on_junk() {
        let answer = order_frontier_ids("not json at all");
        assert!(answer.contains("error"), "got {answer}");
    }

    #[test]
    fn group_frontier_json_buckets_by_the_named_axis() {
        let payload = format!(
            "[{}, {}]",
            one_item("a", "ready"),
            json_item("b", 0, None),
        );
        let json = group_frontier_json(&payload, "context", "[]");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed[0]["value"], serde_json::json!("@errands"));
        assert_eq!(parsed[0]["ids"], serde_json::json!(["a"]));
    }

    #[test]
    fn group_frontier_json_answers_no_columns_for_an_unrecognised_axis() {
        assert_eq!(group_frontier_json("[]", "not-an-axis", "[]"), "[]");
    }

    #[test]
    fn toggle_facet_json_round_trips_a_selection() {
        let empty = r#"{"context":[],"size":[],"energy":[],"urgency":[]}"#;
        let once = toggle_facet_json(empty, "size", "deep");
        assert_eq!(facet_count_json(&once), 1);
        let twice = toggle_facet_json(&once, "size", "deep");
        assert_eq!(facet_count_json(&twice), 0);
    }

    #[test]
    fn apply_facets_ids_filters_and_preserves_order() {
        let payload = format!(
            "[{}, {}]",
            one_item("a", "ready"),
            json_item("b", 0, None),
        );
        let picked = toggle_facet_json(
            r#"{"context":[],"size":[],"energy":[],"urgency":[]}"#,
            "context",
            "@errands",
        );
        assert_eq!(
            apply_facets_ids(&payload, &picked, "2026-08-13T12:00"),
            r#"["a"]"#,
        );
    }

    #[test]
    fn contexts_of_json_lists_the_contexts_actually_present() {
        let payload = one_item("a", "ready");
        assert_eq!(contexts_of_json(&format!("[{payload}]")), r#"["@errands"]"#);
    }

    #[test]
    fn order_triage_ids_orders_oldest_capture_first() {
        let payload = r#"[{"id":"b","createdAt":2},{"id":"a","createdAt":1}]"#;
        assert_eq!(order_triage_ids(payload), r#"["a","b"]"#);
    }

    #[test]
    fn triage_process_queue_json_orders_drafts_then_grilling_then_captured() {
        let triage = r#"[{"id":"c","createdAt":1000},{"id":"d","createdAt":2000}]"#;
        let grilling = r#"[{"id":"g","createdAt":3000}]"#;
        let drafts = r#"["d"]"#;

        let json = triage_process_queue_json(triage, grilling, drafts);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["ids"], serde_json::json!(["d", "g", "c"]));
        assert_eq!(parsed["capturedCount"], serde_json::json!(2));
        assert_eq!(parsed["grillingCount"], serde_json::json!(1));
    }

    /// A minimal [`FrontierItemDTO`] JSON literal, only the fields the
    /// frontier functions read varying by parameter — everything else
    /// fixed at a value none of these tests inspects.
    fn json_item(id: &str, priority: i64, context: Option<&str>) -> String {
        serde_json::json!({
            "id": id,
            "size": null,
            "energy": null,
            "context": context,
            "priority": priority,
            "projectId": null,
            "deadline": null,
        })
        .to_string()
    }

    /// [`priority_rank`] pass-through, pinned against the core rule the same
    /// way `the_capture_binding_is_the_core_rule_verbatim` pins
    /// `can_submit_capture` above.
    #[test]
    fn priority_rank_binding_is_the_core_rule_verbatim() {
        for raw in [0, 1, 2, 3, 4, 5, -1] {
            assert_eq!(
                priority_rank(raw) as i64,
                hummingbird_core::decisions::frontier::priority_rank(raw as i64),
                "{raw} disagreed across the binding",
            );
        }
    }
}
