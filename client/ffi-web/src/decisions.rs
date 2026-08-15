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
