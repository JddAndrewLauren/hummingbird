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

// ----------------------------------------------------------------- M4 (#540)
// The rules-editor decision set: the operator table, the duration grammar,
// the kind -> field cascade, the validity read, the `deadline` picker and
// the backtest. Every function below is a door onto
// `hummingbird_core::decisions::rules` — see that family's `mod.rs` for
// what it retires and why. Same boundary discipline as everything above:
// scalars native, structures as camelCase JSON strings, no `Result` and no
// throw (a bad parse answers with an `{"error": ...}` object).
//
// **The registry crosses in full, per call.** It is small (five kinds), the
// caller already holds it from the `kindRegistry` push, and passing it
// keeps the client editing against the catalogue its *authority* exported
// rather than the one this wasm binary happened to compile — see
// `rules::validity`'s header.

use hummingbird_core::decisions::rules::{
    self, BacktestClock, BacktestItem, BacktestOutcome, KindRegistry,
};
use hummingbird_domain::{Condition, FieldType};
use hummingbird_rules_engine::Operator;

fn error_json(message: impl std::fmt::Display) -> String {
    serde_json::json!({ "error": message.to_string() }).to_string()
}

fn parse_registry(registry_json: &str) -> Result<KindRegistry, String> {
    serde_json::from_str(registry_json).map_err(|e| e.to_string())
}

fn parse_conditions(conditions_json: &str) -> Result<Vec<Condition>, String> {
    serde_json::from_str(conditions_json).map_err(|e| e.to_string())
}

/// [`rules::legal_operators`], JSON-encoded as an array of wire operator
/// names. An unrecognised `field_type` answers with an empty array rather
/// than panicking — the TS side is typed to `FieldTypeName`, but the
/// boundary does not trust that from the wasm side.
#[wasm_bindgen]
pub fn rule_legal_operators_json(field_type: &str) -> String {
    let Some(field_type) = FieldType::parse(field_type) else {
        return "[]".to_string();
    };
    let names: Vec<&str> = rules::legal_operators(field_type)
        .into_iter()
        .map(Operator::as_str)
        .collect();
    serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string())
}

/// [`rules::default_operator_for`] — the operator a freshly added
/// condition row opens at.
#[wasm_bindgen]
pub fn rule_default_operator(field_type: &str) -> Option<String> {
    FieldType::parse(field_type)
        .map(|t| rules::default_operator_for(t).as_str().to_string())
}

/// [`rules::parse_duration_ms`]. `f64`, not the core rule's own `i64`:
/// `wasm-bindgen` crosses `i64` as a JS `BigInt`, and every duration a
/// picker can express is exactly representable as a `f64` — the same
/// narrowing reasoning as [`priority_from_select`] above.
#[wasm_bindgen]
pub fn rule_duration_ms(value: &str) -> Option<f64> {
    rules::parse_duration_ms(value).map(|ms| ms as f64)
}

/// [`rules::format_duration`] — [`rule_duration_ms`]'s inverse, for a
/// picker to write the wire literal back. An unrecognised unit answers
/// with an empty string.
#[wasm_bindgen]
pub fn rule_format_duration(amount: f64, unit: &str) -> String {
    match rules::duration::parse_duration_unit(unit) {
        Some(unit) => rules::format_duration(amount as i64, unit),
        None => String::new(),
    }
}

/// [`rules::duration_units_for`], JSON-encoded as an array of unit
/// suffixes.
#[wasm_bindgen]
pub fn rule_duration_units_json(field_type: &str) -> String {
    let Some(field_type) = FieldType::parse(field_type) else {
        return "[]".to_string();
    };
    let units: Vec<&str> = rules::duration_units_for(field_type)
        .into_iter()
        .map(rules::duration::duration_unit_str)
        .collect();
    serde_json::to_string(&units).unwrap_or_else(|_| "[]".to_string())
}

/// [`rules::is_below_alarm_interval`] — the duration warning (#138), never
/// a save gate.
#[wasm_bindgen]
pub fn rule_is_below_alarm_interval(value: &str, alarm_interval_ms: f64) -> bool {
    rules::is_below_alarm_interval(value, alarm_interval_ms as i64)
}

/// [`rules::fields_for_kind`], JSON-encoded as
/// `[{"name":"...","fieldType":"..."}, ...]` — the registry export's own
/// camelCase shape, so the caller re-uses `KindFieldDTO` unchanged.
#[wasm_bindgen]
pub fn rule_fields_for_kind_json(registry_json: &str, event_kind: Option<String>) -> String {
    match parse_registry(registry_json) {
        Ok(registry) => {
            let fields = rules::fields_for_kind(&registry, event_kind.as_deref());
            serde_json::to_string(&fields).unwrap_or_else(error_json)
        }
        Err(error) => error_json(error),
    }
}

/// [`rules::field_type`] — `None` for a field outside the list
/// `event_kind` offers, and for an unreadable registry.
#[wasm_bindgen]
pub fn rule_field_type(
    registry_json: &str,
    event_kind: Option<String>,
    field_name: &str,
) -> Option<String> {
    let registry = parse_registry(registry_json).ok()?;
    rules::field_type(&registry, event_kind.as_deref(), field_name)
        .map(|t| t.as_str().to_string())
}

/// [`rules::invalid_fields`], JSON-encoded as an array of field names.
#[wasm_bindgen]
pub fn rule_invalid_fields_json(
    registry_json: &str,
    event_kind: Option<String>,
    conditions_json: &str,
) -> String {
    match (parse_registry(registry_json), parse_conditions(conditions_json)) {
        (Ok(registry), Ok(conditions)) => {
            let fields = rules::invalid_fields(&conditions, event_kind.as_deref(), &registry);
            serde_json::to_string(&fields).unwrap_or_else(error_json)
        }
        (Err(error), _) | (_, Err(error)) => error_json(error),
    }
}

/// [`rules::is_rule_valid`]. An unreadable registry or condition list
/// answers `true` — this drives a warning badge, and a parse failure at
/// the boundary is not evidence the reader's rule is broken.
#[wasm_bindgen]
pub fn rule_is_valid(
    registry_json: &str,
    event_kind: Option<String>,
    conditions_json: &str,
) -> bool {
    match (parse_registry(registry_json), parse_conditions(conditions_json)) {
        (Ok(registry), Ok(conditions)) => {
            rules::is_rule_valid(&conditions, event_kind.as_deref(), &registry)
        }
        _ => true,
    }
}

/// [`rules::widget_for`], by the widget's wire name. An unrecognised
/// field type or operator answers `"text"` — the widget that can edit
/// anything.
#[wasm_bindgen]
pub fn rule_widget_for(field_name: &str, field_type: &str, operator: &str) -> String {
    let (Some(field_type), Some(operator)) =
        (FieldType::parse(field_type), Operator::parse(operator))
    else {
        return rules::ValueWidget::Text.as_str().to_string();
    };
    rules::widget_for(field_name, field_type, operator)
        .as_str()
        .to_string()
}

/// [`rules::new_condition`], JSON-encoded — `hummingbird_domain::Condition`'s
/// own wire shape, which is already what `ConditionDTO` mirrors.
#[wasm_bindgen]
pub fn rule_new_condition_json(field_name: &str, field_type: &str) -> String {
    match FieldType::parse(field_type) {
        Some(field_type) => serde_json::to_string(&rules::new_condition(field_name, field_type))
            .unwrap_or_else(error_json),
        None => error_json(format!("unrecognised field type {field_type:?}")),
    }
}

/// [`rules::retype_condition`], JSON-encoded — **`"null"` when the
/// condition is already legal for the new type**, so the caller can keep
/// its own object identity rather than swapping in a structurally equal
/// replacement. See that function's doc.
#[wasm_bindgen]
pub fn rule_retype_condition_json(condition_json: &str, new_field_type: &str) -> String {
    let (Ok(condition), Some(field_type)) = (
        serde_json::from_str::<Condition>(condition_json),
        FieldType::parse(new_field_type),
    ) else {
        return "null".to_string();
    };
    match rules::retype_condition(&condition, field_type) {
        Some(retyped) => serde_json::to_string(&retyped).unwrap_or_else(|_| "null".to_string()),
        None => "null".to_string(),
    }
}

/// [`rules::toggle_negate`], JSON-encoded. An unreadable condition is
/// echoed back unchanged rather than replaced with an error object: the
/// caller is mid-edit on a row it already holds.
#[wasm_bindgen]
pub fn rule_toggle_negate_json(condition_json: &str) -> String {
    match serde_json::from_str::<Condition>(condition_json) {
        Ok(condition) => serde_json::to_string(&rules::toggle_negate(&condition))
            .unwrap_or_else(|_| condition_json.to_string()),
        Err(_) => condition_json.to_string(),
    }
}

/// [`rules::datetime_input_value_from_duration`] — the `datetime-local`
/// value that displays a stored duration as a concrete moment. `now` is
/// deadline-shaped local wall clock; the caller resolves it (see
/// `seam.ts`'s `localWallClock`).
#[wasm_bindgen]
pub fn deadline_picker_datetime(duration_value: &str, op: &str, now: &str) -> String {
    match Operator::parse(op) {
        Some(op) => rules::datetime_input_value_from_duration(duration_value, op, now),
        None => String::new(),
    }
}

/// [`rules::duration_from_datetime_input_value`] — the wire duration
/// literal for a picked moment.
#[wasm_bindgen]
pub fn deadline_picker_duration(input_value: &str, op: &str, now: &str) -> Option<String> {
    let op = Operator::parse(op)?;
    rules::duration_from_datetime_input_value(input_value, op, now)
}

/// One mirrored item as [`BacktestItem`] reads it — the exact field set
/// `authority::sweep::item_threshold_event` populates, camelCase, and
/// nothing else `TaskItemDTO` carries. `occurredAt` is already resolved
/// UTC (`now_as_deadline(item.updatedAt)`), and `deadline`/`scheduledDate`
/// are already this device's local civil reading: this crate holds no
/// tzdb, so the two frames are named at the boundary (see
/// `rules::backtest`'s header).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestItemDTO {
    pub id: String,
    pub occurred_at: String,
    pub title: String,
    pub body: Option<String>,
    pub url: Option<String>,
    pub deadline: Option<String>,
    pub scheduled_date: Option<String>,
    pub stage: String,
    pub size: Option<String>,
    pub energy: Option<String>,
    pub context: Option<String>,
    pub priority: i64,
    pub project_id: Option<String>,
    pub source: Option<String>,
    pub source_key: Option<String>,
}

fn to_backtest_item(dto: BacktestItemDTO) -> BacktestItem {
    BacktestItem {
        id: dto.id,
        occurred_at_utc: dto.occurred_at,
        title: dto.title,
        body: dto.body,
        url: dto.url,
        deadline: dto.deadline,
        scheduled_date: dto.scheduled_date,
        stage: dto.stage,
        size: dto.size,
        energy: dto.energy,
        context: dto.context,
        priority: dto.priority,
        project_id: dto.project_id,
        source: dto.source,
        source_key: dto.source_key,
    }
}

/// [`rules::backtest`], JSON-encoded: `{"kind":"unavailable","reason":"..."}`
/// or `{"kind":"ok","ids":[...]}` — the **ids** that matched, never whole
/// items, exactly the frontier functions' pattern above (the caller holds
/// the items already).
#[wasm_bindgen]
pub fn rule_backtest_ids(
    event_kind: Option<String>,
    conditions_json: &str,
    items_json: &str,
    now_local: &str,
    now_utc: &str,
) -> String {
    let (conditions, items) = match (
        parse_conditions(conditions_json),
        serde_json::from_str::<Vec<BacktestItemDTO>>(items_json),
    ) {
        (Ok(conditions), Ok(items)) => (conditions, items),
        (Err(error), _) => return error_json(error),
        (_, Err(error)) => return error_json(error),
    };
    let items: Vec<BacktestItem> = items.into_iter().map(to_backtest_item).collect();
    let clock = BacktestClock {
        now_local: now_local.to_string(),
        now_utc: now_utc.to_string(),
    };
    match rules::backtest(event_kind.as_deref(), &conditions, &items, &clock) {
        BacktestOutcome::Unavailable { reason } => {
            serde_json::json!({ "kind": "unavailable", "reason": reason.as_str() }).to_string()
        }
        BacktestOutcome::Ok { matched_ids } => {
            serde_json::json!({ "kind": "ok", "ids": matched_ids }).to_string()
        }
    }
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

    // ------------------------------------------------------------- M4 (#540)

    const REGISTRY: &str = r#"{
        "coreFields":[{"name":"source","fieldType":"string"},{"name":"occurred_at","fieldType":"timestamp"}],
        "kinds":[{"key":"email","mints":true,"fields":[{"name":"subject","fieldType":"string"},{"name":"to","fieldType":"string_list"}]}],
        "alarmIntervalMs":900000,
        "severities":["low","normal","high","urgent"]
    }"#;

    /// The operator exposure is a pass-through and nothing more — the
    /// table itself is `hummingbird_rules_engine::Operator::is_legal_for`.
    /// This pins that the binding did not grow an opinion of its own on
    /// the way across, for every field type the wire can name.
    #[test]
    fn the_operator_binding_is_the_core_rule_verbatim() {
        for field_type in FieldType::ALL {
            let expected: Vec<&str> = rules::legal_operators(field_type)
                .into_iter()
                .map(Operator::as_str)
                .collect();
            assert_eq!(
                rule_legal_operators_json(field_type.as_str()),
                serde_json::to_string(&expected).unwrap(),
                "{field_type:?} disagreed across the binding",
            );
            assert_eq!(
                rule_default_operator(field_type.as_str()),
                Some(rules::default_operator_for(field_type).as_str().to_string()),
            );
        }
    }

    #[test]
    fn an_unrecognised_field_type_answers_empty_rather_than_panicking() {
        assert_eq!(rule_legal_operators_json("not-a-type"), "[]");
        assert_eq!(rule_duration_units_json("not-a-type"), "[]");
        assert_eq!(rule_default_operator("not-a-type"), None);
    }

    #[test]
    fn the_duration_binding_is_the_core_rule_verbatim() {
        for value in ["10m", "2h", "3d", "0m", "-3d", "3w", "soon", "", "  2h "] {
            assert_eq!(
                rule_duration_ms(value),
                rules::parse_duration_ms(value).map(|ms| ms as f64),
                "{value:?} disagreed across the binding",
            );
        }
        assert_eq!(rule_format_duration(2.0, "h"), "2h");
        assert_eq!(rule_format_duration(2.0, "w"), "");
        assert!(rule_is_below_alarm_interval("5m", 900_000.0));
        assert!(!rule_is_below_alarm_interval("15m", 900_000.0));
    }

    #[test]
    fn duration_units_narrow_to_days_for_a_date_field() {
        assert_eq!(rule_duration_units_json("date"), r#"["d"]"#);
        assert_eq!(rule_duration_units_json("timestamp"), r#"["m","h","d"]"#);
    }

    #[test]
    fn fields_for_kind_crosses_as_the_registrys_own_camel_case_shape() {
        assert_eq!(
            rule_fields_for_kind_json(REGISTRY, None),
            r#"[{"name":"source","fieldType":"string"},{"name":"occurred_at","fieldType":"timestamp"}]"#,
        );
        let named: serde_json::Value =
            serde_json::from_str(&rule_fields_for_kind_json(REGISTRY, Some("email".into()))).unwrap();
        assert_eq!(named.as_array().map(Vec::len), Some(4));
    }

    #[test]
    fn an_unreadable_registry_answers_an_error_object_never_a_throw() {
        let parsed: serde_json::Value =
            serde_json::from_str(&rule_fields_for_kind_json("{oops", None)).unwrap();
        assert!(parsed.get("error").is_some());
    }

    #[test]
    fn field_type_resolves_within_the_offered_list_only() {
        assert_eq!(
            rule_field_type(REGISTRY, Some("email".into()), "subject").as_deref(),
            Some("string"),
        );
        assert_eq!(rule_field_type(REGISTRY, None, "subject"), None);
    }

    #[test]
    fn the_validity_binding_is_the_core_rule_verbatim() {
        let conditions = r#"[{"field":"subject","op":"contains","value":"x","negate":false}]"#;
        assert!(rule_is_valid(REGISTRY, Some("email".into()), conditions));
        assert!(!rule_is_valid(REGISTRY, None, conditions));
        assert_eq!(
            rule_invalid_fields_json(REGISTRY, None, conditions),
            r#"["subject"]"#,
        );
    }

    #[test]
    fn an_unreadable_validity_payload_reads_as_valid_never_as_a_broken_rule() {
        assert!(rule_is_valid("{oops", None, "[]"));
    }

    #[test]
    fn the_widget_cascade_is_the_core_rule_verbatim() {
        assert_eq!(rule_widget_for("to", "string_list", "eq"), "chips");
        assert_eq!(rule_widget_for("deadline", "timestamp", "within_next"), "datetime");
        assert_eq!(rule_widget_for("received_at", "timestamp", "within_last"), "duration");
        assert_eq!(rule_widget_for("priority", "number", "eq"), "number");
        assert_eq!(rule_widget_for("subject", "string", "contains"), "text");
        // An unreadable pair falls back to the widget that can edit anything.
        assert_eq!(rule_widget_for("subject", "not-a-type", "contains"), "text");
    }

    #[test]
    fn a_fresh_condition_crosses_as_the_conditions_own_wire_shape() {
        assert_eq!(
            rule_new_condition_json("subject", "string"),
            r#"{"field":"subject","op":"eq","value":"","negate":false}"#,
        );
        assert_eq!(
            rule_new_condition_json("to", "string_list"),
            r#"{"field":"to","op":"eq","value":[],"negate":false}"#,
        );
    }

    #[test]
    fn retyping_answers_null_when_the_condition_is_already_legal() {
        let condition = r#"{"field":"subject","op":"contains","value":"urgent","negate":true}"#;
        assert_eq!(rule_retype_condition_json(condition, "string"), "null");
        assert_eq!(
            rule_retype_condition_json(condition, "number"),
            r#"{"field":"subject","op":"eq","value":"","negate":true}"#,
        );
        assert_eq!(rule_retype_condition_json("{oops", "number"), "null");
    }

    #[test]
    fn toggling_negate_echoes_an_unreadable_condition_back_unchanged() {
        assert_eq!(
            rule_toggle_negate_json(r#"{"field":"a","op":"eq","value":"x","negate":false}"#),
            r#"{"field":"a","op":"eq","value":"x","negate":true}"#,
        );
        assert_eq!(rule_toggle_negate_json("{oops"), "{oops");
    }

    #[test]
    fn the_deadline_picker_binding_is_the_core_rule_verbatim() {
        let now = "2026-08-15T09:30";
        assert_eq!(deadline_picker_datetime("2h", "within_next", now), "2026-08-15T11:30");
        assert_eq!(deadline_picker_datetime("3d", "within_last", now), "2026-08-12T09:30");
        assert_eq!(deadline_picker_datetime("soon", "within_next", now), "");
        assert_eq!(deadline_picker_datetime("2h", "not-an-op", now), "");
        assert_eq!(
            deadline_picker_duration("2026-08-15T11:30", "within_next", now).as_deref(),
            Some("120m"),
        );
        assert_eq!(deadline_picker_duration("", "within_next", now), None);
        assert_eq!(deadline_picker_duration("2026-08-15T11:30", "not-an-op", now), None);
    }

    fn backtest_item(id: &str, deadline: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "occurredAt": "2026-08-15T11:00",
            "title": "renew passport",
            "body": null,
            "url": null,
            "deadline": deadline,
            "scheduledDate": null,
            "stage": "ready",
            "size": null,
            "energy": null,
            "context": null,
            "priority": 0,
            "projectId": null,
            "source": null,
            "sourceKey": null,
        })
    }

    #[test]
    fn the_backtest_binding_answers_ids_and_an_unavailable_reason() {
        let items = serde_json::json!([backtest_item("i-1", Some("2026-08-15T13:00"))]).to_string();
        let conditions = r#"[{"field":"deadline","op":"within_next","value":"2h","negate":false}]"#;

        assert_eq!(
            rule_backtest_ids(
                None,
                conditions,
                &items,
                "2026-08-15T12:00",
                "2026-08-15T12:00",
            ),
            r#"{"ids":["i-1"],"kind":"ok"}"#,
        );
        assert_eq!(
            rule_backtest_ids(
                Some("email".into()),
                conditions,
                &items,
                "2026-08-15T12:00",
                "2026-08-15T12:00",
            ),
            r#"{"kind":"unavailable","reason":"no_local_history"}"#,
        );
    }

    #[test]
    fn the_backtest_binding_reads_occurred_at_in_utc_and_deadline_locally() {
        // A device eight hours behind UTC: one instant, two readings.
        let items = serde_json::json!([backtest_item("i-1", Some("2026-08-15T05:00"))]).to_string();
        let by_deadline = r#"[{"field":"deadline","op":"within_next","value":"2h","negate":false}]"#;
        let by_occurred = r#"[{"field":"occurred_at","op":"within_last","value":"2h","negate":false}]"#;

        assert_eq!(
            rule_backtest_ids(None, by_deadline, &items, "2026-08-15T04:00", "2026-08-15T12:00"),
            r#"{"ids":["i-1"],"kind":"ok"}"#,
        );
        assert_eq!(
            rule_backtest_ids(None, by_occurred, &items, "2026-08-15T04:00", "2026-08-15T12:00"),
            r#"{"ids":["i-1"],"kind":"ok"}"#,
        );
    }

    #[test]
    fn an_unreadable_backtest_payload_answers_an_error_object_never_a_throw() {
        let parsed: serde_json::Value =
            serde_json::from_str(&rule_backtest_ids(None, "{oops", "[]", "x", "y")).unwrap();
        assert!(parsed.get("error").is_some());
    }
}
