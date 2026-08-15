//! The one Now/Triage membership + order function — sunk here from the
//! web's `triage-order.ts` and `triage-process-order.ts` by ADR-0025
//! (#141/M1-3).
//!
//! Before #357, "what is in the triage process" (CONTEXT.md's pair of
//! pre-action stages, Triage and Grilling, together) was inferred
//! independently by the Triage screen, Now's collapsible triage area and
//! whatever renders the counts. [`triage_process_queue`] is the one
//! function every reader shares; neither screen filters by stage on its
//! own, and now neither client language redefines the rule either.

/// One item's queue-relevant fields: an id and its capture time. Pure and
/// clockless, same convention as [`super::frontier::FrontierItem`] — a
/// caller maps the returned ids back onto its own full record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub id: String,
    pub created_at: i64,
}

/// Triage's display order (issue #110/S12's "three captures then
/// reconnecting produces three Triage items in order" acceptance
/// criterion): oldest capture first, id as the tie-break for two items that
/// landed in the same millisecond. Pure — never mutates `items`.
pub fn order_triage(items: &[QueueItem]) -> Vec<String> {
    let mut ordered: Vec<&QueueItem> = items.iter().collect();
    ordered.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
    ordered.into_iter().map(|item| item.id.clone()).collect()
}

/// The combined, ordered "triage process" queue both surfaces render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageProcessQueue {
    /// The combined, ordered read: local drafts first, then Grilling-stage
    /// items, then captured Triage items. A draft is device-local and can
    /// sit on an item of either pre-action stage — it wins the front of the
    /// queue regardless of which stage it came from, since resuming an
    /// already-started interview is the most actionable thing in the pile.
    /// Within each of the three groups, order is [`order_triage`]'s own
    /// oldest-capture-first.
    pub ids: Vec<String>,
    /// Exact count of live Triage-stage items — never folded into
    /// `grilling_count`.
    pub captured_count: usize,
    /// Exact count of live Grilling-stage items.
    pub grilling_count: usize,
}

/// Combines `triage_items` (Stage::Triage only) and `grilling_items`
/// (Stage::Grilling only) into one ordered queue, with `draft_item_ids`
/// deciding which items are drafts. Pure: never mutates either input slice,
/// and reading it twice with the same input yields the same output.
pub fn triage_process_queue(
    triage_items: &[QueueItem],
    grilling_items: &[QueueItem],
    draft_item_ids: &[String],
) -> TriageProcessQueue {
    let draft_ids: std::collections::HashSet<&str> =
        draft_item_ids.iter().map(|id| id.as_str()).collect();
    let is_draft = |item: &QueueItem| draft_ids.contains(item.id.as_str());

    let all: Vec<QueueItem> =
        triage_items.iter().chain(grilling_items.iter()).cloned().collect();

    let drafts = order_triage(
        &all.into_iter().filter(is_draft).collect::<Vec<_>>(),
    );
    let grilling = order_triage(
        &grilling_items.iter().filter(|i| !is_draft(i)).cloned().collect::<Vec<_>>(),
    );
    let captured = order_triage(
        &triage_items.iter().filter(|i| !is_draft(i)).cloned().collect::<Vec<_>>(),
    );

    TriageProcessQueue {
        ids: drafts.into_iter().chain(grilling).chain(captured).collect(),
        captured_count: triage_items.len(),
        grilling_count: grilling_items.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, created_at: i64) -> QueueItem {
        QueueItem { id: id.to_string(), created_at }
    }

    // -------------------------------------------------------------- order_triage
    // Ported from `triage-order.test.ts`.

    #[test]
    fn orders_three_offline_captures_by_capture_time_oldest_first() {
        let third = item("c", 3_000);
        let first = item("a", 1_000);
        let second = item("b", 2_000);

        assert_eq!(order_triage(&[third, first, second]), vec!["a", "b", "c"]);
    }

    #[test]
    fn breaks_a_tie_on_created_at_by_id() {
        let b = item("b", 1_000);
        let a = item("a", 1_000);

        assert_eq!(order_triage(&[b, a]), vec!["a", "b"]);
    }

    #[test]
    fn reading_it_twice_with_the_same_input_yields_the_same_output() {
        let input = vec![item("b", 2), item("a", 1)];
        assert_eq!(order_triage(&input), order_triage(&input));
    }

    // ------------------------------------------------------- triage_process_queue
    // Ported from `triage-process-order.test.ts`.

    #[test]
    fn orders_local_drafts_first_then_grilling_then_captured_triage() {
        let captured = item("c", 1_000);
        let drafted = item("d", 2_000);
        let grilling = item("g", 3_000);

        let queue = triage_process_queue(
            &[captured, drafted],
            &[grilling],
            &["d".to_string()],
        );

        assert_eq!(queue.ids, vec!["d", "g", "c"]);
    }

    #[test]
    fn preserves_order_triages_oldest_first_order_within_the_captured_group() {
        let second = item("b", 2_000);
        let first = item("a", 1_000);

        let queue = triage_process_queue(&[second, first], &[], &[]);

        assert_eq!(queue.ids, vec!["a", "b"]);
    }

    #[test]
    fn counts_are_exact_captured_grilling_totals_never_a_single_sum() {
        let captured = vec![item("a", 1), item("b", 2)];
        let grilling = vec![item("c", 3)];

        let queue = triage_process_queue(&captured, &grilling, &[]);

        assert_eq!(queue.captured_count, 2);
        assert_eq!(queue.grilling_count, 1);
    }

    #[test]
    fn a_draft_still_counts_toward_its_own_stages_total_not_a_third_bucket() {
        let drafted = item("a", 1);

        let queue = triage_process_queue(&[drafted], &[], &["a".to_string()]);

        assert_eq!(queue.captured_count, 1);
        assert_eq!(queue.grilling_count, 0);
        assert_eq!(queue.ids, vec!["a"]);
    }

    #[test]
    fn reading_it_twice_with_the_same_input_yields_the_same_output_too() {
        let triage = vec![item("b", 2), item("a", 1)];
        let grilling = vec![item("c", 3)];

        assert_eq!(
            triage_process_queue(&triage, &grilling, &["a".to_string()]),
            triage_process_queue(&triage, &grilling, &["a".to_string()]),
        );
    }
}
