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
// values cross as JSON, the same convention `decisions_probe_item_payload`
// established: `wasm_bindgen` has no derive for an arbitrary struct without
// pulling in `serde-wasm-bindgen`, and JSON is what `seam.ts` already
// parses for the probe.

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

/// The wire vocabulary both `TaskStageName` (`client/web/src/store/protocol.ts`)
/// and `hummingbird_domain::Stage::as_str` already spell the same way
/// (snake_case, byte for byte the DDL `CHECK` literal) — `Stage::parse`
/// does the actual work, this just names the boundary.
fn parse_stage(stage: &str) -> Option<Stage> {
    Stage::parse(stage)
}

/// S11/#109's closed act vocabulary, the same string set
/// `client/ffi-web/src/task_host.rs`'s own `parse_action` accepts — named
/// again here rather than reused because that one is `fn`-private to its
/// module and this is a second, independent boundary (this file's own doc
/// explains why a second wasm instantiation does not touch ADR-0010; the
/// same "name the boundary shape" reasoning applies to the small string
/// vocabularies crossing it).
fn parse_action(action: &str) -> Option<ItemAction> {
    match action {
        "start" => Some(ItemAction::Start),
        "complete" => Some(ItemAction::Complete),
        "block" => Some(ItemAction::Block),
        "cancel" => Some(ItemAction::Cancel),
        _ => None,
    }
}

/// S11/#109's act affordances for `stage` — `hummingbird_core::decisions::available_actions`
/// verbatim, wire-vocabulary strings in, a JSON array of the same strings
/// out. An unrecognised stage answers with an empty array rather than
/// panicking; the TS side is typed to `TaskStageName` so this never fires
/// in practice, but the boundary does not trust that from the wasm side.
#[wasm_bindgen]
pub fn item_available_actions(stage: &str) -> String {
    let actions: Vec<&'static str> = match parse_stage(stage) {
        Some(stage) => hummingbird_core::decisions::available_actions(stage)
            .iter()
            .map(|action| match action {
                ItemAction::Start => "start",
                ItemAction::Complete => "complete",
                ItemAction::Block => "block",
                ItemAction::Cancel => "cancel",
            })
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
    parse_action(action).and_then(hummingbird_core::decisions::applied_stage).map(|stage| stage.as_str().to_string())
}

/// Whether `stage` offers the one-click mark-done checkmark —
/// `hummingbird_core::decisions::can_mark_done` verbatim. An unrecognised
/// stage answers `false`.
#[wasm_bindgen]
pub fn item_can_mark_done(stage: &str, archived: bool) -> bool {
    match parse_stage(stage) {
        Some(stage) => hummingbird_core::decisions::can_mark_done(stage, archived),
        None => false,
    }
}

/// Whether `stage` offers "Grill me" — `hummingbird_core::decisions::can_grill`
/// verbatim. An unrecognised stage answers `false`.
#[wasm_bindgen]
pub fn item_can_grill(stage: &str) -> bool {
    match parse_stage(stage) {
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

/// One item as the *main thread* holds it: `TaskItemDTO`
/// (`client/web/src/store/protocol.ts`), camelCase, already mapped out of
/// the worker's snake_case wire shape by `task-worker.ts`.
///
/// Deliberately a second shape rather than a reuse of
/// [`crate::task_host::FrontierItemDTO`]: that one is what the *worker*
/// serializes on its way out of the core, and the main-thread seam's input
/// is whatever the store is already holding. Naming the real shape is the
/// point — a benchmark over a shape nobody sends measures nothing.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MainThreadItemDTO {
    pub id: String,
    pub seq: Option<i64>,
    pub title: String,
    pub description: Option<String>,
    pub stage: String,
    pub size: Option<String>,
    pub energy: Option<String>,
    pub context: Option<String>,
    pub priority: i64,
    pub project_id: Option<String>,
    pub project_pos: Option<i64>,
    pub deadline: Option<String>,
    pub scheduled_date: Option<String>,
    pub source: Option<String>,
    pub source_key: Option<String>,
    pub source_url: Option<String>,
    pub archived_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub version: i64,
    pub pending: bool,
}

/// **M1-1's measuring instrument, not a product decision.** Crosses a whole
/// frontier's worth of [`MainThreadItemDTO`]s into wasm as JSON, walks
/// them, and returns a JSON answer — the serialize/deserialize cost that
/// M1-3's per-render `orderFrontier`/`applyFacets` calls would pay on every
/// facet toggle, which no instantiation-time measurement can see.
///
/// The work it does between the two conversions is deliberately trivial
/// (count the items and the non-`done` ones): the number this exists to
/// produce is the *boundary* cost, and real ranking work on top would only
/// blur it. M1-3 replaces this with the real ordering call and deletes it;
/// if M1-3 lands and this is still here, that is the leftover to remove.
#[wasm_bindgen]
pub fn decisions_probe_item_payload(items_json: &str) -> String {
    match serde_json::from_str::<Vec<MainThreadItemDTO>>(items_json) {
        Ok(items) => {
            let open = items.iter().filter(|item| item.stage != "done").count();
            serde_json::json!({ "count": items.len(), "open": open }).to_string()
        }
        // A parse failure is an answer, not a panic: a wasm panic poisons
        // the whole module for the page, the same reason
        // `calendar_host`'s `parse_selections` swallows bad JSON.
        Err(error) => serde_json::json!({ "error": error.to_string() }).to_string(),
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

    fn one_item(id: &str, stage: &str) -> String {
        serde_json::json!({
            "id": id,
            "seq": 42,
            "title": "buy milk",
            "description": null,
            "stage": stage,
            "size": "quick",
            "energy": "low",
            "context": "@errands",
            "priority": 2,
            "projectId": null,
            "projectPos": null,
            "deadline": "2026-08-20",
            "scheduledDate": null,
            "source": "web/v1",
            "sourceKey": null,
            "sourceUrl": null,
            "archivedAt": null,
            "createdAt": 1_755_000_000_000i64,
            "updatedAt": 1_755_000_000_000i64,
            "version": 1,
            "pending": false
        })
        .to_string()
    }

    #[test]
    fn the_probe_reads_the_main_threads_camel_case_shape() {
        let payload = format!("[{}, {}]", one_item("a", "ready"), one_item("b", "done"));
        assert_eq!(
            decisions_probe_item_payload(&payload),
            r#"{"count":2,"open":1}"#,
        );
    }

    #[test]
    fn the_probe_answers_rather_than_panicking_on_junk() {
        let answer = decisions_probe_item_payload("not json at all");
        assert!(answer.contains("error"), "got {answer}");
    }
}
