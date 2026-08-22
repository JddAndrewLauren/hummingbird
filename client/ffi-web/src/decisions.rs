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

// ------------------------------------------------------------------ M3 (#532)
// Done's ordering and the Ledger's ordering + row-state read, both sunk from
// `done-order.ts`/`ledger-order.ts` into `hummingbird_core::decisions::roster`.

use hummingbird_core::decisions::roster::{self, LedgerRosterItem, RosterItem};

/// One item as [`roster::order_done`] reads it: an id and when it was last
/// touched — the JSON shape `seam.ts` sends, a strict subset of
/// `TaskItemDTO` exactly like [`QueueItemDTO`] above.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RosterItemDTO {
    id: String,
    updated_at: i64,
}

fn to_roster_item(dto: &RosterItemDTO) -> RosterItem {
    RosterItem { id: dto.id.clone(), updated_at: dto.updated_at }
}

/// [`roster::order_done`], JSON-encoded as an ordered array of ids.
#[wasm_bindgen]
pub fn order_done_ids(items_json: &str) -> String {
    match serde_json::from_str::<Vec<RosterItemDTO>>(items_json) {
        Ok(items) => {
            let entries: Vec<RosterItem> = items.iter().map(to_roster_item).collect();
            serde_json::to_string(&roster::order_done(&entries)).unwrap()
        }
        Err(error) => serde_json::json!({ "error": error.to_string() }).to_string(),
    }
}

/// One Ledger row as [`roster::ledger_row_state`]/[`roster::last_touched_ms`]/
/// [`roster::order_ledger`] read it — the JSON shape `seam.ts` sends, a
/// strict subset of `LedgerRowDTO`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LedgerRosterItemDTO {
    id: String,
    updated_at: i64,
    archived_at: Option<i64>,
    absent_since_ms: Option<i64>,
}

fn to_ledger_roster_item(dto: &LedgerRosterItemDTO) -> LedgerRosterItem {
    LedgerRosterItem {
        id: dto.id.clone(),
        updated_at: dto.updated_at,
        archived_at: dto.archived_at,
        absent_since_ms: dto.absent_since_ms,
    }
}

/// [`roster::ledger_row_state`], JSON-encoded as `{"kind":"live"}` or
/// `{"kind":"archived","sinceMs":N}` — `ledger-order.ts`'s own
/// `LedgerRowState` union, verbatim.
#[wasm_bindgen]
pub fn ledger_row_state_json(row_json: &str) -> String {
    match serde_json::from_str::<LedgerRosterItemDTO>(row_json) {
        Ok(dto) => serde_json::to_string(&roster::ledger_row_state(&to_ledger_roster_item(&dto)))
            .unwrap(),
        Err(error) => serde_json::json!({ "error": error.to_string() }).to_string(),
    }
}

/// [`roster::last_touched_ms`]. An unreadable payload answers `0` — the same
/// "parse failure degrades to the safe default" convention
/// `facet_count_json` uses, since this crosses as a raw number with no room
/// for an `{"error": ...}` object.
#[wasm_bindgen]
pub fn ledger_last_touched_ms(row_json: &str) -> f64 {
    match serde_json::from_str::<LedgerRosterItemDTO>(row_json) {
        Ok(dto) => roster::last_touched_ms(&to_ledger_roster_item(&dto)) as f64,
        Err(_) => 0.0,
    }
}

/// [`roster::order_ledger`], JSON-encoded as an ordered array of ids.
#[wasm_bindgen]
pub fn order_ledger_ids(rows_json: &str) -> String {
    match serde_json::from_str::<Vec<LedgerRosterItemDTO>>(rows_json) {
        Ok(rows) => {
            let entries: Vec<LedgerRosterItem> = rows.iter().map(to_ledger_roster_item).collect();
            serde_json::to_string(&roster::order_ledger(&entries)).unwrap()
        }
        Err(error) => serde_json::json!({ "error": error.to_string() }).to_string(),
    }
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

/// [`rules::DEFAULT_SEVERITY`] — the severity a fresh draft opens on. A
/// function rather than a re-stated literal in the form, so the phone and
/// this client cannot disagree about where a rule is born (ADR-0025).
#[wasm_bindgen]
pub fn rule_default_severity() -> String {
    rules::DEFAULT_SEVERITY.to_string()
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

// -------------------------------------------------------------- M4 (#538)
// The skills lane's decision half (`hummingbird_core::decisions::skills`).
// Same shape as everything above — free functions over scalars and JSON —
// and the same reason: `useMicrotaskWiring.ts`/`useGrillWiring.ts` reduce
// synchronously as each NDJSON line lands, on the main thread, where a
// worker round trip cannot be spliced in.
//
// **The states round-trip as JSON.** `reduce_skill_run`/`reduce_grill_turn`
// take the previous state as text and answer with the next one, so the web
// holds the state in React and the *rule* lives in the core. A state or
// event this side cannot parse is answered with the state text verbatim —
// a strict no-op, never an invented phase — because a malformed argument is
// a caller bug and swallowing it into `idle` would wipe a live run's
// narration.

use hummingbird_core::decisions::skills;

/// [`skills::classify_line`], JSON-encoded — the same object shape
/// `envelope.ts`'s `classifyLine` has always returned, so its callers parse
/// it and are otherwise unchanged.
#[wasm_bindgen]
pub fn classify_skill_line(text: &str) -> String {
    serde_json::to_string(&skills::classify_line(text)).unwrap()
}

/// [`skills::microtask_result`] / [`skills::grill_result`] over the terminal
/// line's `result` value, JSON-encoded, `"null"` when the result is not
/// that schema's shape.
#[wasm_bindgen]
pub fn microtask_result_json(result_json: &str) -> String {
    let value = serde_json::from_str(result_json).unwrap_or(serde_json::Value::Null);
    serde_json::to_string(&skills::microtask_result(&value)).unwrap()
}

#[wasm_bindgen]
pub fn grill_result_json(result_json: &str) -> String {
    let value = serde_json::from_str(result_json).unwrap_or(serde_json::Value::Null);
    serde_json::to_string(&skills::grill_result(&value)).unwrap()
}

/// [`skills::reduce_run`], state and event in as JSON text, the next state
/// out as JSON text.
#[wasm_bindgen]
pub fn reduce_skill_run(state_json: &str, event_json: &str) -> String {
    let (Ok(state), Ok(event)) = (
        serde_json::from_str::<skills::SkillRunState>(state_json),
        serde_json::from_str::<skills::SkillEvent>(event_json),
    ) else {
        return state_json.to_string();
    };
    serde_json::to_string(&skills::reduce_run(&state, &event)).unwrap()
}

/// [`skills::reduce_grill_turn`], the same way.
#[wasm_bindgen]
pub fn reduce_grill_turn(state_json: &str, event_json: &str) -> String {
    let (Ok(state), Ok(event)) = (
        serde_json::from_str::<skills::GrillTurnState>(state_json),
        serde_json::from_str::<skills::SkillEvent>(event_json),
    ) else {
        return state_json.to_string();
    };
    serde_json::to_string(&skills::reduce_grill_turn(&state, &event)).unwrap()
}

/// [`skills::stamp_label`] — `None` (and so JS `undefined`) whenever the
/// envelope named no backend. There is no default name to fall back to,
/// here or anywhere in this lane.
#[wasm_bindgen]
pub fn skill_stamp_label(state_json: &str) -> Option<String> {
    let state = serde_json::from_str::<skills::SkillRunState>(state_json).ok()?;
    skills::stamp_label(&state)
}

/// [`skills::microtask_run_body`] / [`skills::grill_run_body`] — the exact
/// request text, byte-pinned across the three languages by
/// `client/core/tests/fixtures/skills-run-bodies.json`. The empty string on
/// an unreadable input, so a caller cannot post a half-built body: an empty
/// body fails at the transport, loudly, rather than reaching the runner as
/// something plausible.
#[wasm_bindgen]
pub fn microtask_run_body_json(input_json: &str) -> String {
    match serde_json::from_str::<skills::MicrotaskRunInput>(input_json) {
        Ok(input) => skills::microtask_run_body(&input),
        Err(_) => String::new(),
    }
}

#[wasm_bindgen]
pub fn grill_run_body_json(reference: &str, turns_json: &str) -> String {
    match serde_json::from_str::<Vec<skills::GrillTurn>>(turns_json) {
        Ok(turns) => skills::grill_run_body(reference, &turns),
        Err(_) => String::new(),
    }
}

/// [`skills::format_grill_transcript`] — the plain-text record
/// `Core::complete_grill` carries (ADR-0023 decision 2).
#[wasm_bindgen]
pub fn format_grill_transcript(turns_json: &str) -> String {
    match serde_json::from_str::<Vec<skills::GrillTurn>>(turns_json) {
        Ok(turns) => skills::format_grill_transcript(&turns),
        Err(_) => String::new(),
    }
}

/// The four decline sentences, from
/// [`hummingbird_core::decisions::skills::decline`] and
/// [`hummingbird_core::decisions::skills::grill::OUTSIDE_SCHEMA`]. The two
/// constants are exposed as functions because a `#[wasm_bindgen]` const is
/// not a thing — and, on the TS side, because the web's own copies stay
/// literal strings for module-evaluation order and are *pinned* against
/// these by `seam.test.ts` (ADR-0025's #538 amendment records that row).
#[wasm_bindgen]
pub fn decline_for_transport(detail: &str) -> String {
    skills::decline_for_transport(detail)
}

/// `u32` rather than `u16`: wasm-bindgen's numeric boundary is a JS
/// `number` either way, and every caller already holds `response.status` as
/// one. The core takes the `u16` an HTTP status actually is.
#[wasm_bindgen]
pub fn decline_for_response(status: u32) -> String {
    skills::decline_for_response(status.min(u16::MAX as u32) as u16)
}

#[wasm_bindgen]
pub fn no_token_decline() -> String {
    skills::NO_TOKEN.to_string()
}

#[wasm_bindgen]
pub fn no_terminal_line_decline() -> String {
    skills::NO_TERMINAL_LINE.to_string()
}

#[wasm_bindgen]
pub fn outside_schema_decline() -> String {
    skills::OUTSIDE_SCHEMA.to_string()
}

// ---- #539: the microtask affordance, the backend picker's tier fallback,
// and the Grill review card's predicates, sunk out of
// `client/web/src/skills/microtask-affordance.ts`,
// `client/web/src/skills/backend-registry.ts`/`backend-selection.ts`, and
// `client/web/src/screens/grill-review.ts`.

/// [`StepDTO`]'s own wire shape — the seven fields `microtask-affordance.ts`
/// and `grill-review.ts` both already read a `StepDTO[]` as. Defined here
/// rather than shared from anywhere else in this crate: it is the one Step
/// shape this seam ever needs, and every field but `deleted_at`/`done` is
/// unread by either predicate family.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StepDTO {
    id: String,
    item_id: String,
    body: String,
    done: bool,
    position: i64,
    deleted_at: Option<i64>,
    version: i64,
}

fn to_domain_step(dto: StepDTO) -> hummingbird_domain::Step {
    hummingbird_domain::Step {
        id: dto.id,
        item_id: dto.item_id,
        body: dto.body,
        done: dto.done,
        position: dto.position,
        deleted_at: dto.deleted_at,
        version: dto.version,
    }
}

fn to_domain_steps(steps_json: &str) -> Vec<hummingbird_domain::Step> {
    serde_json::from_str::<Vec<StepDTO>>(steps_json)
        .unwrap_or_default()
        .into_iter()
        .map(to_domain_step)
        .collect()
}

/// [`skills::microtask_affordance`], JSON-encoded — the same
/// `{ kind: "break" }`/`{ kind: "rewrite", undoneCount }` shape
/// `microtask-affordance.ts`'s own `MicrotaskAffordance` already is.
#[wasm_bindgen]
pub fn microtask_affordance_json(steps_json: &str) -> String {
    serde_json::to_string(&skills::microtask_affordance(&to_domain_steps(steps_json))).unwrap()
}

/// [`skills::fallback_backend_id`] — the one-tap fallback offered when a
/// pin declines. `registry_ids_json` is a bare `string[]` of ids, the only
/// part of a `BackendEntry` this rule reads.
#[wasm_bindgen]
pub fn fallback_backend_id(registry_ids_json: &str, dead_id: &str) -> Option<String> {
    let ids: Vec<String> = serde_json::from_str(registry_ids_json).unwrap_or_default();
    skills::fallback_backend_id(&ids, dead_id)
}

/// [`skills::resolve_backend_selection`] — Auto when nothing is stored, or
/// when the stored id no longer names a registered entry.
#[wasm_bindgen]
pub fn resolve_backend_selection(stored: Option<String>, registry_ids_json: &str) -> String {
    let ids: Vec<String> = serde_json::from_str(registry_ids_json).unwrap_or_default();
    skills::resolve_backend_selection(stored.as_deref(), &ids)
}

#[wasm_bindgen]
pub fn backend_auto_selection() -> String {
    skills::AUTO_SELECTION.to_string()
}

/// [`skills::declined_backend_fallback`] — #274's one-tap fallback offer,
/// decided whole (#539's round-2 review moved the predicate here from
/// `ffi-mobile`, which had been deciding it itself). `state_json` is a
/// [`skills::SkillRunState`] in its own wire shape — the same text
/// `reduce_skill_run` already round-trips — so a caller already holding
/// `run` as that JSON sends it unchanged.
#[wasm_bindgen]
pub fn declined_backend_fallback(state_json: &str, selection: &str, registry_ids_json: &str) -> Option<String> {
    let state: skills::SkillRunState = serde_json::from_str(state_json).ok()?;
    let ids: Vec<String> = serde_json::from_str(registry_ids_json).unwrap_or_default();
    skills::declined_backend_fallback(&state, selection, &ids)
}

fn parse_verdict(verdict: &str) -> Option<hummingbird_domain::GrillVerdict> {
    serde_json::from_value(serde_json::Value::String(verdict.to_string())).ok()
}

/// [`skills::would_strand_plan`] — `false` for a `verdict` this build
/// cannot parse, the safe reading for a value only ever produced by
/// [`hummingbird_domain::GrillVerdict`]'s own wire spelling.
#[wasm_bindgen]
pub fn grill_would_strand_plan(verdict: &str, steps_json: &str) -> bool {
    match parse_verdict(verdict) {
        Some(verdict) => skills::would_strand_plan(verdict, &to_domain_steps(steps_json)),
        None => false,
    }
}

#[wasm_bindgen]
pub fn grill_plan_replacement_label(steps_json: &str) -> String {
    skills::plan_replacement_label(&to_domain_steps(steps_json))
}

/// [`skills::demotes_from_frontier`] — `false` for a `verdict` or `stage`
/// this build cannot parse.
#[wasm_bindgen]
pub fn grill_demotes_from_frontier(verdict: &str, stage: &str) -> bool {
    match (parse_verdict(verdict), hummingbird_domain::Stage::parse(stage)) {
        (Some(verdict), Some(stage)) => skills::demotes_from_frontier(verdict, stage),
        _ => false,
    }
}

#[wasm_bindgen]
pub fn grill_frontier_demotion_warning() -> String {
    skills::FRONTIER_DEMOTION_WARNING.to_string()
}

// ----------------------------------------------------------------- M4 (#533)
// The standing-question panes: the pane shell contract's decided half, the
// cross-pane sort, the zone bridge, and the waste pane. Same house style as
// everything above — free functions over JSON, no constructor, no state.
//
// **The zone bridge is why two of these take a second JSON argument.** The
// core owns no tzdb (`client/core/Cargo.toml`'s `chrono-tz` note), so a
// civil-date pane is answered in two phases: the core names the
// `(zone, civil-date)` facts it needs, the host resolves them with `Intl`
// (`screens/questions/zone-bridge.ts`), and the core ranks against the
// resolved table. A key the host omits is the unresolvable zone, and what
// that *means* stays a core decision — see
// `hummingbird_core::decisions::panes::zone`'s module header.

use hummingbird_core::decisions::panes::{
    self,
    contract::{pane_key, RankedPaneRecord},
    waste, PaneInputs, Surface, ZoneFacts, ZoneQuery,
};

fn parse_inputs(inputs_json: &str) -> Result<PaneInputs, String> {
    serde_json::from_str(inputs_json).map_err(|e| e.to_string())
}

fn parse_zone_facts(zone_facts_json: &str) -> Result<ZoneFacts, String> {
    serde_json::from_str(zone_facts_json).map_err(|e| e.to_string())
}

/// A query list as the host reads it: each [`ZoneQuery`]'s own JSON with
/// its [`ZoneQuery::key`] spliced in.
///
/// The key is sent rather than left for the host to derive, because it is
/// the *whole* protocol — a host that computed `civil:{zone}:{atMs}` itself
/// would be a second spelling of the one string both sides must agree on,
/// and a mismatch would present as an unresolvable zone rather than as a
/// bug.
fn queries_json(queries: Vec<ZoneQuery>) -> String {
    let rows: Vec<serde_json::Value> = queries
        .into_iter()
        .map(|query| {
            let key = query.key();
            let mut value = serde_json::to_value(&query).unwrap();
            if let Some(object) = value.as_object_mut() {
                object.insert("key".to_string(), serde_json::Value::String(key));
            }
            value
        })
        .collect();
    serde_json::to_string(&rows).unwrap()
}

/// Every `(zone, civil-date)` fact the waste pane needs, given these
/// inputs — phase one of the bridge, for the one-pane path `waste.ts`
/// takes.
#[wasm_bindgen]
pub fn waste_zone_queries_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => queries_json(waste::waste_zone_queries(&inputs)),
        Err(error) => error_json(error),
    }
}

/// [`waste::waste_facts`] — the whole answered fact set, or the reason
/// there is none, as `{"kind":"facts",…}` / `{"kind":"gap","gap":{…}}`.
/// **No rendered sentence crosses**: the client composes its own words from
/// these facts and from the gap's kind.
#[wasm_bindgen]
pub fn waste_facts_json(inputs_json: &str, zone_facts_json: &str) -> String {
    match (parse_inputs(inputs_json), parse_zone_facts(zone_facts_json)) {
        (Ok(inputs), Ok(facts)) => {
            serde_json::to_string(&waste::waste_facts(&inputs, &facts)).unwrap()
        }
        (Err(error), _) | (_, Err(error)) => error_json(error),
    }
}

/// [`waste::waste_answer`] — the pane shell's three decided fields
/// (`answerState`, `band`, `withinBand`) and nothing else.
#[wasm_bindgen]
pub fn waste_answer_json(inputs_json: &str, zone_facts_json: &str) -> String {
    match (parse_inputs(inputs_json), parse_zone_facts(zone_facts_json)) {
        (Ok(inputs), Ok(facts)) => {
            serde_json::to_string(&waste::waste_answer(&inputs, &facts)).unwrap()
        }
        (Err(error), _) | (_, Err(error)) => error_json(error),
    }
}

/// [`waste::waste_setup`] — whether the collection page has been set, and
/// which kind of not-set it is.
#[wasm_bindgen]
pub fn waste_setup_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&waste::waste_setup(&inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

/// [`waste::parse_waste_body`] over one `PaneSnapshotDTO`, as
/// `{"kind":"ok","body":{…}}` / `{"kind":"gap","gap":{…}}`. `null` is the
/// "no row at all" case, which is a gap kind of its own rather than an
/// error.
#[wasm_bindgen]
pub fn parse_waste_body_json(snapshot_json: &str) -> String {
    let snapshot: Option<hummingbird_core::decisions::panes::inputs::PaneSnapshotFacts> =
        match serde_json::from_str(snapshot_json) {
            Ok(snapshot) => snapshot,
            Err(error) => return error_json(error.to_string()),
        };
    match waste::parse_waste_body(snapshot.as_ref()) {
        Ok(body) => serde_json::json!({ "kind": "ok", "body": body }).to_string(),
        Err(gap) => serde_json::json!({ "kind": "gap", "gap": gap }).to_string(),
    }
}

/// [`panes::zone_queries`] — phase one for a whole surface. `surface` is
/// `"now"` or `"status"`; an unrecognised one asks for nothing rather than
/// panicking.
#[wasm_bindgen]
pub fn pane_zone_queries_json(inputs_json: &str, surface: &str) -> String {
    let Some(surface) = Surface::parse(surface) else {
        return "[]".to_string();
    };
    match parse_inputs(inputs_json) {
        Ok(inputs) => queries_json(panes::zone_queries(surface, &inputs)),
        Err(error) => error_json(error),
    }
}

/// [`panes::rank_panes`] — phase two for a whole surface, already in
/// display order. An unrecognised surface ranks nothing.
#[wasm_bindgen]
pub fn rank_panes_json(inputs_json: &str, zone_facts_json: &str, surface: &str) -> String {
    let Some(surface) = Surface::parse(surface) else {
        return "[]".to_string();
    };
    match (parse_inputs(inputs_json), parse_zone_facts(zone_facts_json)) {
        (Ok(inputs), Ok(facts)) => {
            serde_json::to_string(&panes::rank_panes(surface, &inputs, &facts)).unwrap()
        }
        (Err(error), _) | (_, Err(error)) => error_json(error),
    }
}

/// One pane as [`panes::order_panes`]/[`panes::same_pane_identity`] read it
/// — the four fields the sort touches, and nothing the shell draws with.
/// `paneKey` is optional on the wire and derived when absent, so a caller
/// holding a `RankedPane` can send its own identity rather than have this
/// boundary re-derive one that might disagree.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RankedPaneDTO {
    question: String,
    subject_key: String,
    pane_key: Option<String>,
    answer: hummingbird_core::decisions::panes::PaneAnswerCore,
}

fn to_record(dto: &RankedPaneDTO) -> RankedPaneRecord {
    RankedPaneRecord {
        pane_key: dto
            .pane_key
            .clone()
            .unwrap_or_else(|| pane_key(&dto.question, &dto.subject_key)),
        question: dto.question.clone(),
        subject_key: dto.subject_key.clone(),
        answer: dto.answer,
    }
}

/// [`panes::order_panes`], JSON-encoded as **the ordered input indices**.
///
/// Indices rather than pane keys, on the same "ids cross, not whole items"
/// reasoning as `order_frontier_ids` — but keyed by position rather than by
/// identity, because nothing at this boundary enforces that `paneKey` is
/// unique, and a duplicate would silently drop a pane from the region on
/// the way back. The caller maps indices onto the `RankedPane`s it already
/// holds, headline and glyphs included.
#[wasm_bindgen]
pub fn order_panes_json(panes_json: &str, question_order_json: &str) -> String {
    let panes: Vec<RankedPaneDTO> = match serde_json::from_str(panes_json) {
        Ok(panes) => panes,
        Err(error) => return error_json(error.to_string()),
    };
    let question_order: Vec<String> = match serde_json::from_str(question_order_json) {
        Ok(order) => order,
        Err(error) => return error_json(error.to_string()),
    };
    // The index rides along as the subject key's neighbour: `order_panes`
    // is total and non-mutating, so the ordered records can be matched back
    // to their input positions by identity plus first-unused occurrence.
    let records: Vec<RankedPaneRecord> = panes.iter().map(to_record).collect();
    let ordered = panes::order_panes(&records, &question_order);

    let mut taken = vec![false; records.len()];
    let mut indices: Vec<usize> = Vec::with_capacity(ordered.len());
    for pane in &ordered {
        let found = records
            .iter()
            .enumerate()
            .position(|(index, candidate)| !taken[index] && candidate == pane)
            .unwrap_or(0);
        taken[found] = true;
        indices.push(found);
    }
    serde_json::to_string(&indices).unwrap()
}

/// [`panes::same_pane_identity`] — whether two ranked lists describe the
/// same panes in the same answer states. Deliberately not a full equality;
/// see the core function's own doc.
#[wasm_bindgen]
pub fn same_pane_identity_json(a_json: &str, b_json: &str) -> bool {
    let parse = |s: &str| serde_json::from_str::<Vec<RankedPaneDTO>>(s);
    match (parse(a_json), parse(b_json)) {
        (Ok(a), Ok(b)) => {
            let a: Vec<RankedPaneRecord> = a.iter().map(to_record).collect();
            let b: Vec<RankedPaneRecord> = b.iter().map(to_record).collect();
            panes::same_pane_identity(&a, &b)
        }
        // An unreadable list is not "the same as" anything: answering
        // `true` here would freeze a captured order against a payload
        // nobody could read.
        _ => false,
    }
}

/// [`hummingbird_core::decisions::panes::BAND_ORDER`], JSON-encoded — the
/// pinning reader `seam.test.ts` holds `contract.ts`'s literal `BAND_ORDER`
/// against. Not called in production: that array is read at
/// module-evaluation time (see `seam.ts`'s M1-2 header for the constraint).
#[wasm_bindgen]
pub fn pane_band_order_json() -> String {
    serde_json::to_string(&hummingbird_core::decisions::panes::BAND_ORDER).unwrap()
}

/// [`hummingbird_core::decisions::panes::QUESTION_ORDER`], pinned the same
/// way — `registry.ts` builds `QUESTIONS` at module evaluation and would
/// throw the seam's "used before ready" guard on every page load if this
/// were a live call.
#[wasm_bindgen]
pub fn pane_question_order_json() -> String {
    serde_json::to_string(&hummingbird_core::decisions::panes::QUESTION_ORDER).unwrap()
}

/// The waste pane's four constants, pinned against `waste.ts`'s literals
/// for the same module-evaluation reason.
#[wasm_bindgen]
pub fn waste_constants_json() -> String {
    serde_json::json!({
        "source": waste::SOURCE,
        "snapshotKey": waste::SNAPSHOT_KEY,
        "bindingKey": waste::BINDING_KEY,
        "staleAfterMs": waste::STALE_AFTER_MS,
        "streamOrder": waste::STREAM_ORDER.map(|stream| stream.as_str()),
    })
    .to_string()
}

// -------------------------------------------------------------- #535 (M4)
// The Settings screen's decision half: the sync-status readout and the
// dead-letter heading. `sync_status_summary_json` takes the card's whole
// input as one JSON object (`decisions::settings::SyncStatusInput`) and
// answers all three of tone/label/word together — never three separate
// calls that could read three different snapshots of "now".

use hummingbird_core::decisions::settings::{self, SyncStatusInput};

fn parse_sync_status_input(input_json: &str) -> Result<SyncStatusInput, String> {
    serde_json::from_str(input_json).map_err(|e| e.to_string())
}

/// [`settings::sync_outcome_class`], by its wire spelling (`"held"`,
/// `"failed"`, `"not-run"`, `"landed"`).
#[wasm_bindgen]
pub fn sync_outcome_class(kind: &str) -> String {
    serde_json::to_value(settings::sync_outcome_class(kind))
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

#[wasm_bindgen]
pub fn is_informative_sync_outcome(kind: &str) -> bool {
    settings::is_informative_sync_outcome(kind)
}

#[wasm_bindgen]
pub fn relative_age(age_ms: f64) -> String {
    settings::relative_age(age_ms as i64)
}

/// `{"tone":"neutral"|"warn"|"danger"|"success","label":"...","toneWord":"..."}`,
/// or `{"error":"..."}` on unparseable input.
#[wasm_bindgen]
pub fn sync_status_summary_json(input_json: &str) -> String {
    match parse_sync_status_input(input_json) {
        Ok(input) => serde_json::json!({
            "tone": settings::sync_status_tone(&input),
            "label": settings::sync_status_label(&input),
            "toneWord": settings::sync_status_tone_word(&input),
        })
        .to_string(),
        Err(error) => error_json(error),
    }
}

// ------------------------------------------------------------------- #534
// The remaining seven panes: the status four (kimi/github/uptime/
// reachability) and the now three (race/weekend/vacation) — same house
// style as waste's own section above. Every wrapper returns structured
// values, never a rendered sentence; each pane's own TS module composes its
// words from these.

use hummingbird_core::decisions::panes::{kimi, github, homework, scps, uptime, reachability, race, vacation, weekend, zone};

/// `hummingbird_core::decisions::panes::zone::DEVICE_ZONE` — the sentinel
/// `zone-bridge.ts`'s `resolveZone` special-cases to mean "the reader's own
/// device zone". Pinned against the TS literal by `seam.test.ts` rather
/// than read through the seam at every call: a changed sentinel here and a
/// stale copy there would silently turn weekend/vacation into permanent
/// gap answers (`ZoneFacts` simply never resolving), the exact "used
/// before ready" style failure this crossing exists to catch loudly
/// instead.
#[wasm_bindgen]
pub fn device_zone() -> String {
    zone::DEVICE_ZONE.to_string()
}

fn snapshot_from_json(
    snapshot_json: &str,
) -> Result<Option<hummingbird_core::decisions::panes::inputs::PaneSnapshotFacts>, String> {
    serde_json::from_str(snapshot_json).map_err(|e| e.to_string())
}

// -- kimi (#313) -------------------------------------------------------

#[wasm_bindgen]
pub fn parse_kimi_body_json(snapshot_json: &str) -> String {
    match snapshot_from_json(snapshot_json) {
        Ok(snapshot) => match kimi::parse_kimi_body(snapshot.as_ref()) {
            Ok(body) => serde_json::json!({ "kind": "ok", "body": body }).to_string(),
            Err(gap) => serde_json::json!({ "kind": "gap", "gap": gap }).to_string(),
        },
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn dead_letter_heading(count: u32) -> String {
    settings::dead_letter_heading(count)
}

#[wasm_bindgen]
pub fn kimi_facts_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&kimi::kimi_facts(&inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn kimi_answer_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&kimi::kimi_answer(&inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn kimi_band_json(available_balance: f64) -> String {
    serde_json::to_string(&kimi::kimi_band(available_balance)).unwrap()
}

#[wasm_bindgen]
pub fn kimi_constants_json() -> String {
    serde_json::json!({
        "source": kimi::SOURCE,
        "snapshotKey": kimi::SNAPSHOT_KEY,
        "staleAfterMs": kimi::STALE_AFTER_MS,
        "imminentThresholdUsd": kimi::IMMINENT_THRESHOLD_USD,
        "nearThresholdUsd": kimi::NEAR_THRESHOLD_USD,
    })
    .to_string()
}

// -- github (#314) -------------------------------------------------------

#[wasm_bindgen]
pub fn parse_workflow_body_json(snapshot_json: &str) -> String {
    match snapshot_from_json(snapshot_json) {
        Ok(snapshot) => match github::parse_workflow_body(snapshot.as_ref()) {
            Ok(body) => serde_json::json!({ "kind": "ok", "body": body }).to_string(),
            Err(gap) => serde_json::json!({ "kind": "gap", "gap": gap }).to_string(),
        },
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn github_band_json(body_json: &str, now_ms: f64) -> String {
    match serde_json::from_str::<github::WorkflowBody>(body_json) {
        Ok(body) => serde_json::to_string(&github::github_band(&body, now_ms as i64)).unwrap(),
        Err(error) => error_json(error.to_string()),
    }
}

#[wasm_bindgen]
pub fn github_subjects_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&github::github_subjects(&inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn github_facts_json(subject_key: &str, inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&github::github_facts(subject_key, &inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn github_answer_json(subject_key: &str, inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&github::github_answer(subject_key, &inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn github_constants_json() -> String {
    serde_json::json!({
        "source": github::SOURCE,
        "neverPolledSubject": github::NEVER_POLLED_SUBJECT,
        "staleAfterMs": github::STALE_AFTER_MS,
        "overdueMultiplier": github::OVERDUE_MULTIPLIER,
    })
    .to_string()
}

// -- uptime (#315) -------------------------------------------------------

#[wasm_bindgen]
pub fn parse_uptime_body_json(snapshot_json: &str) -> String {
    match snapshot_from_json(snapshot_json) {
        Ok(snapshot) => match uptime::parse_uptime_body(snapshot.as_ref()) {
            Ok(body) => serde_json::json!({ "kind": "ok", "body": body }).to_string(),
            Err(gap) => serde_json::json!({ "kind": "gap", "gap": gap }).to_string(),
        },
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn uptime_band_json(body_json: &str) -> String {
    match serde_json::from_str::<uptime::ProbeBody>(body_json) {
        Ok(body) => serde_json::to_string(&uptime::uptime_band(&body)).unwrap(),
        Err(error) => error_json(error.to_string()),
    }
}

#[wasm_bindgen]
pub fn uptime_subjects_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&uptime::uptime_subjects(&inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn uptime_facts_json(subject_key: &str, inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&uptime::uptime_facts(subject_key, &inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn uptime_answer_json(subject_key: &str, inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&uptime::uptime_answer(subject_key, &inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn uptime_constants_json() -> String {
    serde_json::json!({
        "source": uptime::SOURCE,
        "neverPolledSubject": uptime::NEVER_POLLED_SUBJECT,
        "staleAfterMs": uptime::STALE_AFTER_MS,
    })
    .to_string()
}

// -- reachability (#316) --------------------------------------------------

#[wasm_bindgen]
pub fn reachability_facts_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&reachability::reachability_facts(&inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn reachability_answer_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => {
            serde_json::to_string(&reachability::reachability_answer(&inputs)).unwrap()
        }
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn reachability_constants_json() -> String {
    serde_json::json!({
        "subjectKey": reachability::SUBJECT_KEY,
        "graceMs": reachability::REACHABILITY_GRACE_MS,
    })
    .to_string()
}

// -- race (#119) -----------------------------------------------------------

#[wasm_bindgen]
pub fn parse_race_body_json(snapshot_json: &str) -> String {
    match snapshot_from_json(snapshot_json) {
        Ok(snapshot) => match race::parse_race_body(snapshot.as_ref()) {
            Ok(body) => serde_json::json!({ "kind": "ok", "body": body }).to_string(),
            Err(gap) => serde_json::json!({ "kind": "gap", "gap": gap }).to_string(),
        },
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn race_series_from_binding_json(text: &str) -> String {
    serde_json::to_string(&race::series_from_binding(text)).unwrap()
}

#[wasm_bindgen]
pub fn race_setup_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&race::race_setup(&inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn race_subjects_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&race::race_subjects(&inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn next_race_at_json(events_json: &str, now_ms: f64) -> String {
    match serde_json::from_str::<Vec<race::RaceEvent>>(events_json) {
        Ok(events) => {
            serde_json::to_string(&race::next_race_at(&events, now_ms as i64)).unwrap()
        }
        Err(error) => error_json(error.to_string()),
    }
}

#[wasm_bindgen]
pub fn race_facts_json(series: &str, inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&race::race_facts(series, &inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn race_answer_json(subject_key: &str, inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&race::race_answer(subject_key, &inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn race_constants_json() -> String {
    serde_json::json!({
        "source": race::SOURCE,
        "bindingKey": race::BINDING_KEY,
        "staleAfterMs": race::STALE_AFTER_MS,
        "setupSubject": race::SETUP_SUBJECT,
    })
    .to_string()
}

// -- weekend (#122) ---------------------------------------------------------

#[wasm_bindgen]
pub fn weekend_zone_queries_json(now_ms: f64) -> String {
    queries_json(weekend::weekend_zone_queries(now_ms as i64))
}

#[wasm_bindgen]
pub fn weekend_window_json(now_ms: f64, zone_facts_json: &str) -> String {
    match parse_zone_facts(zone_facts_json) {
        Ok(facts) => {
            serde_json::to_string(&weekend::weekend_window(now_ms as i64, &facts)).unwrap()
        }
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn weekend_facts_json(inputs_json: &str, zone_facts_json: &str) -> String {
    match (parse_inputs(inputs_json), parse_zone_facts(zone_facts_json)) {
        (Ok(inputs), Ok(facts)) => {
            serde_json::to_string(&weekend::weekend_facts(&inputs, &facts)).unwrap()
        }
        (Err(error), _) | (_, Err(error)) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn weekend_answer_json(inputs_json: &str, zone_facts_json: &str) -> String {
    match (parse_inputs(inputs_json), parse_zone_facts(zone_facts_json)) {
        (Ok(inputs), Ok(facts)) => {
            serde_json::to_string(&weekend::weekend_answer(&inputs, &facts)).unwrap()
        }
        (Err(error), _) | (_, Err(error)) => error_json(error),
    }
}

/// [`weekend::weekend_band`] — exposed standalone (not just via
/// `weekend_answer_json`) so `weekend.ts`'s locally-kept `weekendBand` can
/// be pinned against it directly by a shared cross-host test, on
/// `github_band_json`'s own precedent.
#[wasm_bindgen]
pub fn weekend_band_json(window_json: &str, now_ms: f64) -> String {
    match serde_json::from_str::<weekend::WeekendWindow>(window_json) {
        Ok(window) => serde_json::to_string(&weekend::weekend_band(&window, now_ms as i64)).unwrap(),
        Err(error) => error_json(error.to_string()),
    }
}

/// [`weekend::weekend_within_band`] — same reason as
/// [`weekend_band_json`].
#[wasm_bindgen]
pub fn weekend_within_band_json(window_json: &str) -> String {
    match serde_json::from_str::<weekend::WeekendWindow>(window_json) {
        Ok(window) => serde_json::to_string(&weekend::weekend_within_band(&window)).unwrap(),
        Err(error) => error_json(error.to_string()),
    }
}

#[wasm_bindgen]
pub fn weekend_constants_json() -> String {
    serde_json::json!({
        "subjectKey": weekend::SUBJECT_KEY,
        "calendarRequestKey": weekend::CALENDAR_REQUEST_KEY,
        "imminentWithinMs": weekend::IMMINENT_WITHIN_MS,
        "nearWithinMs": weekend::NEAR_WITHIN_MS,
    })
    .to_string()
}

// -- vacation (#121) ---------------------------------------------------------

#[wasm_bindgen]
pub fn vacation_zone_queries_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => queries_json(vacation::vacation_zone_queries(&inputs)),
        Err(error) => error_json(error),
    }
}

/// [`vacation::vacation_setup_kind`] — the kind-only projection
/// `vacation::VacationSetup` itself cannot cross (its `Bound` arm borrows).
/// `vacation.ts`'s `vacationSetup` is pinned against this, on
/// `race_setup_json`'s/`waste_setup_json`'s own precedent.
#[wasm_bindgen]
pub fn vacation_setup_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&vacation::vacation_setup_kind(&inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn trip_queue_json(events_json: &str, calendar_id: &str, today: &str, zone_facts_json: &str) -> String {
    match (
        serde_json::from_str::<Vec<hummingbird_core::decisions::panes::inputs::CalendarEventFacts>>(
            events_json,
        ),
        parse_zone_facts(zone_facts_json),
    ) {
        (Ok(events), Ok(facts)) => {
            let today = today.to_string();
            serde_json::to_string(&vacation::trip_queue(&events, calendar_id, &today, &facts)).unwrap()
        }
        (Err(error), _) => error_json(error.to_string()),
        (_, Err(error)) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn vacation_band_json(next_trip_json: &str) -> String {
    match serde_json::from_str::<Option<vacation::Trip>>(next_trip_json) {
        Ok(next) => serde_json::to_string(&vacation::vacation_band(next.as_ref())).unwrap(),
        Err(error) => error_json(error.to_string()),
    }
}

#[wasm_bindgen]
pub fn vacation_view_json(inputs_json: &str, zone_facts_json: &str) -> String {
    match (parse_inputs(inputs_json), parse_zone_facts(zone_facts_json)) {
        (Ok(inputs), Ok(facts)) => {
            serde_json::to_string(&vacation::vacation_view(&inputs, &facts)).unwrap()
        }
        (Err(error), _) | (_, Err(error)) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn vacation_answer_json(inputs_json: &str, zone_facts_json: &str) -> String {
    match (parse_inputs(inputs_json), parse_zone_facts(zone_facts_json)) {
        (Ok(inputs), Ok(facts)) => {
            serde_json::to_string(&vacation::vacation_answer(&inputs, &facts)).unwrap()
        }
        (Err(error), _) | (_, Err(error)) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn vacation_constants_json() -> String {
    serde_json::json!({
        "subjectKey": vacation::SUBJECT_KEY,
        "calendarRequestKey": vacation::CALENDAR_REQUEST_KEY,
        "horizonBeforeDays": vacation::HORIZON_BEFORE_DAYS,
        "horizonAheadDays": vacation::HORIZON_AHEAD_DAYS,
        "staleAfterMs": vacation::STALE_AFTER_MS,
        "imminentWithinDays": vacation::IMMINENT_WITHIN_DAYS,
        "nearWithinDays": vacation::NEAR_WITHIN_DAYS,
    })
    .to_string()
}

// -- scps (#693, ADR-0032) ---------------------------------------------------

#[wasm_bindgen]
pub fn scps_zone_queries_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => queries_json(scps::scps_zone_queries(&inputs)),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn scps_view_json(inputs_json: &str, zone_facts_json: &str) -> String {
    match (parse_inputs(inputs_json), parse_zone_facts(zone_facts_json)) {
        (Ok(inputs), Ok(facts)) => serde_json::to_string(&scps::scps_view(&inputs, &facts)).unwrap(),
        (Err(error), _) | (_, Err(error)) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn scps_answer_json(inputs_json: &str, zone_facts_json: &str) -> String {
    match (parse_inputs(inputs_json), parse_zone_facts(zone_facts_json)) {
        (Ok(inputs), Ok(facts)) => serde_json::to_string(&scps::scps_answer(&inputs, &facts)).unwrap(),
        (Err(error), _) | (_, Err(error)) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn scps_constants_json() -> String {
    serde_json::json!({
        "subjectKey": scps::SUBJECT_KEY,
        "calendarRequestKey": scps::CALENDAR_REQUEST_KEY,
        "questBindingKey": scps::SCPS_QUEST_BINDING_KEY,
        "horizonBeforeMs": scps::HORIZON_BEFORE_MS,
        "horizonAfterDays": scps::HORIZON_AFTER_DAYS,
        "staleAfterMs": scps::STALE_AFTER_MS,
    })
    .to_string()
}

// -- homework (#675) --------------------------------------------------------
//
// Three exports rather than two: the zone queries cross on their own door
// because this pane names them from the *items* (`homework.rs`'s own
// header), so a caller cannot compute them from `nowMs` alone the way
// `weekend_zone_queries_json` lets it. A fourth, `homework_link_json`,
// carries the standing session link — also its own door, and for the same
// kind of reason: it must answer in the arms where there are no facts.

#[wasm_bindgen]
pub fn homework_zone_queries_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => queries_json(homework::homework_zone_queries(&inputs)),
        Err(error) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn homework_facts_json(inputs_json: &str, zone_facts_json: &str) -> String {
    match (parse_inputs(inputs_json), parse_zone_facts(zone_facts_json)) {
        (Ok(inputs), Ok(facts)) => {
            serde_json::to_string(&homework::homework_facts(&inputs, &facts)).unwrap()
        }
        (Err(error), _) | (_, Err(error)) => error_json(error),
    }
}

#[wasm_bindgen]
pub fn homework_answer_json(inputs_json: &str, zone_facts_json: &str) -> String {
    match (parse_inputs(inputs_json), parse_zone_facts(zone_facts_json)) {
        (Ok(inputs), Ok(facts)) => {
            serde_json::to_string(&homework::homework_answer(&inputs, &facts)).unwrap()
        }
        (Err(error), _) | (_, Err(error)) => error_json(error),
    }
}

/// `hummingbird_core::decisions::panes::homework::homework_link` — the
/// standing session link as JSON `string | null`.
///
/// Its own door rather than a field on the facts: the link survives the
/// zone gap, which carries no facts at all (`homework.rs`'s own header).
#[wasm_bindgen]
pub fn homework_link_json(inputs_json: &str) -> String {
    match parse_inputs(inputs_json) {
        Ok(inputs) => serde_json::to_string(&homework::homework_link(&inputs)).unwrap(),
        Err(error) => error_json(error),
    }
}

/// The literals this pane is defined by — the context it matches, its
/// sentinel subject, the `near` cutoff, and the `settings` key its standing
/// link is held under — so no client retypes any of them
/// (`race_constants_json`'s own precedent).
#[wasm_bindgen]
pub fn homework_constants_json() -> String {
    serde_json::json!({
        "context": homework::HOMEWORK_CONTEXT,
        "subjectKey": homework::SUBJECT_KEY,
        "nearWithinDays": homework::NEAR_WITHIN_DAYS,
        "linkBindingKey": homework::HOMEWORK_LINK_BINDING_KEY,
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

    // ------------------------------------------------------------- #532 roster

    #[test]
    fn order_done_ids_orders_most_recently_touched_first() {
        let payload = r#"[{"id":"b","updatedAt":1000},{"id":"a","updatedAt":1000},{"id":"c","updatedAt":4000}]"#;
        assert_eq!(order_done_ids(payload), r#"["c","a","b"]"#);
    }

    #[test]
    fn ledger_row_state_json_reads_live_and_archived() {
        let live = r#"{"id":"a","updatedAt":1000,"archivedAt":null,"absentSinceMs":null}"#;
        assert_eq!(ledger_row_state_json(live), r#"{"kind":"live"}"#);

        let archived = r#"{"id":"a","updatedAt":1000,"archivedAt":5000,"absentSinceMs":6000}"#;
        assert_eq!(ledger_row_state_json(archived), r#"{"kind":"archived","sinceMs":5000}"#);
    }

    #[test]
    fn ledger_last_touched_ms_reads_the_latest_of_the_three_stamps() {
        let row = r#"{"id":"a","updatedAt":3000,"archivedAt":9000,"absentSinceMs":null}"#;
        assert_eq!(ledger_last_touched_ms(row), 9000.0);
    }

    #[test]
    fn order_ledger_ids_orders_last_touched_first() {
        let payload = r#"[
            {"id":"b","updatedAt":1000,"archivedAt":null,"absentSinceMs":null},
            {"id":"a","updatedAt":1000,"archivedAt":null,"absentSinceMs":null},
            {"id":"c","updatedAt":5000,"archivedAt":null,"absentSinceMs":null},
            {"id":"d","updatedAt":2000,"archivedAt":9000,"absentSinceMs":null}
        ]"#;
        assert_eq!(order_ledger_ids(payload), r#"["d","c","a","b"]"#);
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

    // ------------------------------------------------------------ M4 (#533)
    // Every binding below is a pass-through and nothing more — the rules
    // are tested in `hummingbird_core::decisions::panes`, and the shared
    // fixtures (`client/core/tests/pane_fixtures.rs` +
    // `shared-fixtures.test.ts`) pin the two clients against each other.
    // These pin that the JSON crossing did not grow an opinion of its own.

    /// The waste pane's inputs, `QuestionInputs`-shaped exactly as
    /// `seam.ts`'s `paneInputsPayload` sends them.
    fn waste_inputs(body: &str, bindings: serde_json::Value) -> String {
        serde_json::json!({
            "nowMs": 1_786_377_600_000i64,
            "bindings": bindings,
            "paneReads": {
                "city-waste/v2": {
                    "source": "city-waste/v2",
                    "snapshots": [{
                        "key": "collection",
                        "fetchedAtMs": 1_786_377_540_000i64,
                        "envelope": {"kind":"ok","schema":"city-waste/v2","polledEveryMs":86_400_000,"body":body},
                        "freshness": {"kind":"age","ageMs":60_000,"declaredCadenceMs":86_400_000},
                    }],
                    "liveAlerts": [],
                },
            },
        })
        .to_string()
    }

    fn bound_page() -> serde_json::Value {
        serde_json::json!([
            {"key":"city-waste-page","known":true,"pending":false,
             "value":{"state":"text","text":"https://example.gov"}}
        ])
    }

    const BODY: &str = r#"{"zone":"America/Los_Angeles","scheduled":"2026-08-17","collected_on":"2026-08-17","streams":["trash","yard"]}"#;
    const FACTS: &str = r#"{"civil:America/Los_Angeles:1786377600000":"2026-08-10","midnight:America/Los_Angeles:2026-08-17":1786950000000}"#;

    #[test]
    fn waste_zone_queries_json_names_both_facts_and_carries_each_ones_key() {
        let json = waste_zone_queries_json(&waste_inputs(BODY, bound_page()));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
        assert_eq!(parsed[0]["kind"], serde_json::json!("civilDate"));
        assert_eq!(parsed[0]["key"], serde_json::json!("civil:America/Los_Angeles:1786377600000"));
        assert_eq!(parsed[1]["kind"], serde_json::json!("midnight"));
        assert_eq!(
            parsed[1]["key"],
            serde_json::json!("midnight:America/Los_Angeles:2026-08-17"),
        );
    }

    #[test]
    fn waste_answer_json_is_the_three_decided_fields_and_no_rendering() {
        let json = waste_answer_json(&waste_inputs(BODY, bound_page()), FACTS);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["answerState"], serde_json::json!("answered"));
        assert_eq!(parsed["band"], serde_json::json!("dormant"));
        assert_eq!(parsed["withinBand"], serde_json::json!(1_786_950_000_000i64));
        assert_eq!(parsed.as_object().unwrap().len(), 3);
    }

    #[test]
    fn waste_facts_json_answers_facts_or_a_gap_kind_never_a_sentence() {
        let facts: serde_json::Value =
            serde_json::from_str(&waste_facts_json(&waste_inputs(BODY, bound_page()), FACTS))
                .unwrap();
        assert_eq!(facts["kind"], serde_json::json!("facts"));
        assert_eq!(facts["daysAway"], serde_json::json!(7));
        assert_eq!(facts["holiday"], serde_json::json!(false));
        assert_eq!(facts["weekdayIndex"], serde_json::json!(1));

        // An empty table is the unresolvable zone, and the refusal is the
        // core's — the binding invents nothing.
        let gap: serde_json::Value =
            serde_json::from_str(&waste_facts_json(&waste_inputs(BODY, bound_page()), "{}"))
                .unwrap();
        assert_eq!(gap["kind"], serde_json::json!("gap"));
        assert_eq!(gap["gap"]["gap"], serde_json::json!("unresolvableZone"));
        assert_eq!(gap["gap"]["zone"], serde_json::json!("America/Los_Angeles"));
    }

    #[test]
    fn waste_setup_json_distinguishes_unread_from_unset() {
        let unread = waste_setup_json(&waste_inputs(BODY, serde_json::Value::Null));
        assert_eq!(unread, r#"{"kind":"unread"}"#);
        let unset = waste_setup_json(&waste_inputs(BODY, serde_json::json!([])));
        assert_eq!(unset, r#"{"kind":"unset"}"#);
    }

    #[test]
    fn parse_waste_body_json_reads_a_body_or_names_the_gap_kind() {
        let snapshot = serde_json::json!({
            "key": "collection",
            "envelope": {"kind":"ok","schema":"city-waste/v2","body":BODY},
            "freshness": {"kind":"age","ageMs":0,"declaredCadenceMs":null},
        })
        .to_string();
        let ok: serde_json::Value = serde_json::from_str(&parse_waste_body_json(&snapshot)).unwrap();
        assert_eq!(ok["kind"], serde_json::json!("ok"));
        assert_eq!(ok["body"]["collectedOn"], serde_json::json!("2026-08-17"));
        assert_eq!(ok["body"]["streams"], serde_json::json!(["trash", "yard"]));

        let absent: serde_json::Value = serde_json::from_str(&parse_waste_body_json("null")).unwrap();
        assert_eq!(absent["gap"]["gap"], serde_json::json!("notFetched"));
    }

    #[test]
    fn rank_panes_json_ranks_the_surfaces_own_questions_and_no_others() {
        let inputs = waste_inputs(BODY, bound_page());
        let now: serde_json::Value =
            serde_json::from_str(&rank_panes_json(&inputs, FACTS, "now")).unwrap();
        // #534 grew Now to four questions (waste/weekend/vacation/race),
        // #675 added homework as a fifth, and #693 added scps as a sixth;
        // this fixture only binds waste's own page, so the rest rank
        // unbound (or, for homework/scps, on the zone bridge's own gap)
        // rather than vanishing — ADR-0017's own rule.
        let now = now.as_array().unwrap();
        assert_eq!(now.len(), 6);
        assert!(now.iter().any(|pane| pane["question"] == "homework"));
        let waste = now.iter().find(|pane| pane["question"] == "waste").unwrap();
        assert_eq!(waste["paneKey"], serde_json::json!("waste:collection"));
        // #534 also filled Status with the never-polled sentinel for its
        // four questions, rather than leaving the surface empty.
        let status: serde_json::Value =
            serde_json::from_str(&rank_panes_json(&inputs, FACTS, "status")).unwrap();
        assert_eq!(status.as_array().unwrap().len(), 4);
        assert_eq!(rank_panes_json(&inputs, FACTS, "not-a-surface"), "[]");
        assert_eq!(pane_zone_queries_json(&inputs, "not-a-surface"), "[]");
    }

    #[test]
    fn order_panes_json_answers_input_indices_in_display_order() {
        let panes = serde_json::json!([
            {"question":"alpha","subjectKey":"b","paneKey":"alpha:b",
             "answer":{"answerState":"answered","band":"dormant","withinBand":0}},
            {"question":"alpha","subjectKey":"a","paneKey":"alpha:a",
             "answer":{"answerState":"answered","band":"live","withinBand":0}},
        ])
        .to_string();
        assert_eq!(order_panes_json(&panes, r#"["alpha"]"#), "[1,0]");
    }

    /// Indices rather than pane keys is the whole reason this binding does
    /// not lose a pane when two rows share an identity.
    #[test]
    fn order_panes_json_keeps_every_input_even_when_two_share_a_pane_key() {
        let panes = serde_json::json!([
            {"question":"alpha","subjectKey":"a","paneKey":"dup",
             "answer":{"answerState":"answered","band":"dormant","withinBand":0}},
            {"question":"alpha","subjectKey":"a","paneKey":"dup",
             "answer":{"answerState":"answered","band":"dormant","withinBand":0}},
        ])
        .to_string();
        let mut indices: Vec<usize> =
            serde_json::from_str(&order_panes_json(&panes, r#"["alpha"]"#)).unwrap();
        indices.sort();
        assert_eq!(indices, vec![0, 1]);
    }

    #[test]
    fn same_pane_identity_json_ignores_the_clock_and_refuses_an_unreadable_list() {
        let a = r#"[{"question":"alpha","subjectKey":"a","answer":{"answerState":"answered","band":"dormant","withinBand":900}}]"#;
        let b = r#"[{"question":"alpha","subjectKey":"a","answer":{"answerState":"answered","band":"imminent","withinBand":5}}]"#;
        assert!(same_pane_identity_json(a, b));
        let gap = r#"[{"question":"alpha","subjectKey":"a","answer":{"answerState":"unbound","band":"dormant","withinBand":null}}]"#;
        assert!(!same_pane_identity_json(a, gap));
        assert!(!same_pane_identity_json(a, "not json"));
    }

    #[test]
    fn the_pinning_readers_are_the_core_vocabularies_verbatim() {
        assert_eq!(pane_band_order_json(), r#"["live","imminent","near","distant","dormant"]"#);
        assert_eq!(
            pane_question_order_json(),
            r#"["homework","scps","waste","weekend","vacation","race","kimi","github","uptime","reachability"]"#,
        );
        let constants: serde_json::Value =
            serde_json::from_str(&waste_constants_json()).unwrap();
        assert_eq!(constants["source"], serde_json::json!("city-waste/v2"));
        assert_eq!(constants["snapshotKey"], serde_json::json!("collection"));
        assert_eq!(constants["bindingKey"], serde_json::json!("city-waste-page"));
        assert_eq!(constants["staleAfterMs"], serde_json::json!(93_600_000));
        assert_eq!(constants["streamOrder"], serde_json::json!(["trash", "recycling", "yard"]));
    }

    #[test]
    fn every_pane_binding_answers_rather_than_panicking_on_junk() {
        for answer in [
            waste_zone_queries_json("not json"),
            waste_facts_json("not json", "{}"),
            waste_answer_json("{}", "not json"),
            waste_setup_json("not json"),
            parse_waste_body_json("not json"),
            pane_zone_queries_json("not json", "now"),
            rank_panes_json("not json", "{}", "now"),
            order_panes_json("not json", "[]"),
        ] {
            assert!(answer.contains("error"), "got {answer}");
        }
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

    // ---------------------------------------------------------- M4 (#538)

    #[test]
    fn classify_skill_line_is_the_core_rule_verbatim_as_json() {
        for text in [
            r#"{"type":"progress","message":"still running"}"#,
            r#"{"ok":true,"result":{"steps":[],"note":"n"},"backend":"b","model":"m"}"#,
            r#"{"ok":false,"error":"nope"}"#,
            r#"{"ok":"true"}"#,
            "not json",
        ] {
            assert_eq!(
                classify_skill_line(text),
                serde_json::to_string(&skills::classify_line(text)).unwrap(),
                "{text} disagreed across the binding",
            );
        }
        // The JSON spelling itself is the contract `envelope.ts` parses.
        assert_eq!(
            classify_skill_line(r#"{"type":"progress","message":"a"}"#),
            r#"{"kind":"progress","message":"a"}"#,
        );
        assert_eq!(classify_skill_line("not json"), r#"{"kind":"unreadable"}"#);
    }

    #[test]
    fn reduce_skill_run_round_trips_a_run_through_its_json() {
        let started = reduce_skill_run(r#"{"phase":"idle"}"#, r#"{"kind":"started"}"#);
        assert_eq!(started, r#"{"phase":"running","messages":[]}"#);
        let beat = reduce_skill_run(&started, r#"{"kind":"progress","message":"still running"}"#);
        // The heartbeat collapse, across the binding.
        assert_eq!(
            reduce_skill_run(&beat, r#"{"kind":"progress","message":"still running"}"#),
            beat,
        );
        let done = reduce_skill_run(
            &beat,
            r#"{"kind":"ok","result":{"steps":[],"note":"kept 2"},"backend":"b","model":"m"}"#,
        );
        assert_eq!(
            done,
            r#"{"phase":"done","messages":["still running"],"note":"kept 2","backend":"b","model":"m"}"#,
        );
        assert_eq!(skill_stamp_label(&done).as_deref(), Some("b · m"));
    }

    /// The duplicate-tap rule and the after-terminal no-op are the core's,
    /// and the binding must return a byte-identical state for them — the
    /// web wrapper compares the two strings to decide whether to keep its
    /// existing object (which is what keeps `run-state.test.ts`'s identity
    /// assertions passing unchanged).
    #[test]
    fn a_no_op_reduce_answers_the_state_text_byte_for_byte() {
        let running = reduce_skill_run(r#"{"phase":"idle"}"#, r#"{"kind":"started"}"#);
        assert_eq!(reduce_skill_run(&running, r#"{"kind":"started"}"#), running);
        assert_eq!(reduce_skill_run(&running, r#"{"kind":"unreadable"}"#), running);
        let done = reduce_skill_run(&running, r#"{"kind":"ok","backend":null,"model":null}"#);
        assert_eq!(reduce_skill_run(&done, r#"{"kind":"progress","message":"late"}"#), done);
    }

    /// A malformed argument is a caller bug; it must not wipe a live run.
    #[test]
    fn an_unreadable_state_or_event_is_a_strict_no_op() {
        let running = r#"{"phase":"running","messages":["a"]}"#;
        assert_eq!(reduce_skill_run(running, "not json"), running);
        assert_eq!(reduce_skill_run("not json", r#"{"kind":"started"}"#), "not json");
        assert_eq!(reduce_grill_turn(running, "not json"), running);
        assert_eq!(skill_stamp_label("not json"), None);
    }

    #[test]
    fn reduce_grill_turn_answers_the_question_phase_and_the_outside_schema_decline() {
        let asking = reduce_grill_turn(r#"{"phase":"idle"}"#, r#"{"kind":"started"}"#);
        assert_eq!(asking, r#"{"phase":"asking","messages":[]}"#);
        let question = reduce_grill_turn(
            &asking,
            r#"{"kind":"ok","result":{"kind":"question","question":{"prompt":"p","recommendedAnswer":"r","choices":["a","b"]}},"backend":"b","model":"m"}"#,
        );
        assert_eq!(
            question,
            r#"{"phase":"question","messages":[],"question":{"prompt":"p","recommendedAnswer":"r","choices":["a","b"]},"backend":"b","model":"m"}"#,
        );
        let outside = reduce_grill_turn(
            &asking,
            r#"{"kind":"ok","result":{"kind":"neither"},"backend":null,"model":null}"#,
        );
        assert!(outside.contains(&outside_schema_decline()), "{outside}");
        assert!(outside.contains(r#""answered":true"#), "{outside}");
    }

    #[test]
    fn the_result_readers_are_the_core_rules_verbatim() {
        assert_eq!(
            microtask_result_json(r#"{"steps":["a"],"note":"n"}"#),
            r#"{"steps":["a"],"note":"n"}"#,
        );
        assert_eq!(microtask_result_json(r#"{"steps":[],"note":""}"#), "null");
        assert_eq!(microtask_result_json("not json"), "null");
        assert_eq!(
            grill_result_json(r#"{"kind":"question","question":{"prompt":"p","recommendedAnswer":"r","choices":["a","b"]}}"#),
            r#"{"kind":"question","question":{"prompt":"p","recommendedAnswer":"r","choices":["a","b"]}}"#,
        );
        assert_eq!(grill_result_json(r#"{"kind":"neither"}"#), "null");
    }

    #[test]
    fn the_run_bodies_are_the_core_bytes_verbatim() {
        assert_eq!(
            microtask_run_body_json(r#"{"itemId":"i","replace":true}"#),
            r#"{"skill":"microtask","args":{"ref":"i","replace":true}}"#,
        );
        assert_eq!(
            grill_run_body_json("i", "[]"),
            r#"{"skill":"grill-me","args":{"ref":"i","turns":[]}}"#,
        );
        // An unreadable input posts nothing rather than a half-built body.
        assert_eq!(microtask_run_body_json("not json"), "");
        assert_eq!(grill_run_body_json("i", "not json"), "");
    }

    #[test]
    fn format_grill_transcript_is_the_core_rule_verbatim() {
        let turns = r#"[{"question":{"prompt":"Which airport?","recommendedAnswer":"SEA","choices":["SEA","PDX"]},"answer":"SEA"}]"#;
        assert_eq!(format_grill_transcript(turns), "Q: Which airport?\nA: SEA");
        assert_eq!(format_grill_transcript("[]"), "");
    }

    /// The four decline sentences cross unchanged — this is what
    /// `seam.test.ts` pins the web's literal TS copies against.
    #[test]
    fn the_decline_bindings_are_the_core_words_verbatim() {
        assert_eq!(no_token_decline(), skills::NO_TOKEN);
        assert_eq!(no_terminal_line_decline(), skills::NO_TERMINAL_LINE);
        assert_eq!(outside_schema_decline(), skills::OUTSIDE_SCHEMA);
        for status in [401u32, 403, 404, 500] {
            assert_eq!(
                decline_for_response(status),
                skills::decline_for_response(status as u16),
                "{status} disagreed across the binding",
            );
        }
        assert_eq!(decline_for_transport("  boom  "), "Could not reach the server: boom");
        assert_eq!(decline_for_transport(""), "Could not reach the server.");
    }

    fn step_json(id: &str, done: bool, deleted_at: Option<i64>) -> String {
        format!(
            r#"{{"id":"{id}","itemId":"item-1","body":"pack","done":{done},"position":0,"deletedAt":{},"version":1}}"#,
            deleted_at.map(|ms| ms.to_string()).unwrap_or_else(|| "null".to_string()),
        )
    }

    #[test]
    fn microtask_affordance_json_is_the_core_rule_verbatim() {
        assert_eq!(microtask_affordance_json("[]"), r#"{"kind":"break"}"#);
        let steps = format!("[{}]", step_json("a", false, None));
        assert_eq!(
            microtask_affordance_json(&steps),
            r#"{"kind":"rewrite","undoneCount":1}"#,
        );
    }

    #[test]
    fn the_backend_picker_doors_are_the_core_rules_verbatim() {
        assert_eq!(backend_auto_selection(), skills::AUTO_SELECTION);
        assert_eq!(
            fallback_backend_id(r#"["a","b"]"#, "a").as_deref(),
            Some("b"),
        );
        assert_eq!(fallback_backend_id(r#"["cloud"]"#, "cloud"), None);
        assert_eq!(
            resolve_backend_selection(Some("retired".to_string()), r#"["cloud"]"#),
            skills::AUTO_SELECTION,
        );
        assert_eq!(resolve_backend_selection(None, r#"["cloud"]"#), skills::AUTO_SELECTION);
    }

    #[test]
    fn declined_backend_fallback_maps_the_wire_state_and_answers_the_cores_verdict() {
        let declined =
            r#"{"phase":"declined","messages":[],"reason":"Could not reach the server.","backend":null,"model":null,"answered":false}"#;
        assert_eq!(
            declined_backend_fallback(declined, "cloud", r#"["cloud","home"]"#),
            Some("home".to_string()),
        );
        let idle = r#"{"phase":"idle"}"#;
        assert_eq!(declined_backend_fallback(idle, "cloud", r#"["cloud"]"#), None);
        assert_eq!(declined_backend_fallback("not json", "cloud", r#"["cloud"]"#), None);
    }

    #[test]
    fn the_grill_review_predicates_are_the_core_rules_verbatim() {
        let undone = format!("[{}]", step_json("a", false, None));
        assert!(grill_would_strand_plan("fog_remains", &undone));
        assert!(!grill_would_strand_plan("fog_remains", "[]"));
        assert!(!grill_would_strand_plan("resolved", &undone));
        assert!(!grill_would_strand_plan("not-a-verdict", &undone));

        assert_eq!(grill_plan_replacement_label(&undone), "Also delete 1 unfinished step");

        assert!(grill_demotes_from_frontier("fog_remains", "ready"));
        assert!(!grill_demotes_from_frontier("fog_remains", "triage"));
        assert!(!grill_demotes_from_frontier("resolved", "ready"));
        assert!(!grill_demotes_from_frontier("fog_remains", "not-a-stage"));

        assert_eq!(grill_frontier_demotion_warning(), skills::FRONTIER_DEMOTION_WARNING);
    }

    #[test]
    fn sync_status_summary_answers_tone_label_and_word_together() {
        let json = sync_status_summary_json(
            r#"{"online":true,"lastSyncOutcomeKind":"completed","lastSyncAtMs":0,"queueDepth":2,"nowMs":60000}"#,
        );
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["tone"], "success");
        assert_eq!(parsed["label"], "Synced — as of 1m ago · 2 queued");
        assert_eq!(parsed["toneWord"], "synced");
    }

    #[test]
    fn sync_status_summary_reports_unparseable_input_rather_than_panicking() {
        let json = sync_status_summary_json("not json");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("error").is_some());
    }

    #[test]
    fn dead_letter_heading_is_the_core_rule_verbatim() {
        assert_eq!(dead_letter_heading(1), "1 edit didn't apply");
        assert_eq!(dead_letter_heading(3), "3 edits didn't apply");
    }

    #[test]
    fn sync_outcome_class_and_informativeness_are_the_core_rules_verbatim() {
        assert_eq!(sync_outcome_class("held"), "held");
        assert_eq!(sync_outcome_class("pull_failed"), "failed");
        assert_eq!(sync_outcome_class("skipped"), "not-run");
        assert_eq!(sync_outcome_class("completed"), "landed");
        assert!(!is_informative_sync_outcome("skipped"));
        assert!(!is_informative_sync_outcome("busy"));
        assert!(is_informative_sync_outcome("completed"));
    }

    #[test]
    fn relative_age_is_the_core_rule_verbatim() {
        assert_eq!(relative_age(0.0), "just now");
        assert_eq!(relative_age(60_000.0), "1m ago");
    }
}
