//! Read-time queries over the [`Mirror`].
//!
//! Every one of these is a filter computed on each call, never a
//! materialized view. ADR-0002 puts urgency — and everything derived from it
//! — on the read side by design: "no context materialization, ever". So this
//! module holds no state, caches nothing, and returns borrowed records in the
//! mirror's own stable id order. Ranking is a separate, explicit step
//! ([`by_priority_then_due`]) that consumers opt into, rather than an order
//! silently baked into the query.

use hummingbird_domain::deadline_sort_key;

use super::item::{Item, Stage};
use super::mirror::Mirror;

impl Mirror {
    /// Items awaiting triage: captured, not yet an Action.
    pub fn triage_inbox(&self) -> Vec<&Item> {
        self.all_items()
            .filter(|i| i.is_live() && i.stage == Stage::Triage)
            .collect()
    }

    /// Items in Grilling — `CONTEXT.md`'s fog, waiting on thinking.
    pub fn grilling(&self) -> Vec<&Item> {
        self.all_items()
            .filter(|i| i.is_live() && i.stage == Stage::Grilling)
            .collect()
    }

    /// What can actually be started right now: Ready or In Progress, live,
    /// and not waiting on an open blocker.
    ///
    /// [`Stage::Blocked`] is excluded by construction rather than by filter —
    /// it is not one of the two stages admitted here. That stage means an
    /// **external** wait (`CONTEXT.md`), which is a different fact from the
    /// relation-blocking below and must not be conflated with it.
    pub fn frontier(&self) -> Vec<&Item> {
        self.all_items()
            .filter(|i| i.is_live())
            .filter(|i| matches!(i.stage, Stage::Ready | Stage::InProgress))
            .filter(|i| self.open_blockers(i).is_empty())
            .collect()
    }

    /// The items [`Mirror::frontier`] dropped, and what is blocking them —
    /// so a short frontier can be explained rather than just being short.
    pub fn blocked_by_relation(&self) -> Vec<(&Item, Vec<&Item>)> {
        self.all_items()
            .filter(|i| i.is_live())
            .filter(|i| matches!(i.stage, Stage::Ready | Stage::InProgress))
            .map(|i| (i, self.open_blockers(i)))
            .filter(|(_, blockers)| !blockers.is_empty())
            .collect()
    }

    /// The blockers of `item` that are still open.
    ///
    /// A blocker counts while it is **not shut** — Done *or* Canceled clears
    /// it. Blockers are stored unfiltered on the record precisely so this is
    /// decided here: a blocker completed by the last sweep must stop blocking
    /// without anything re-fetching the blocked item.
    ///
    /// An id this mirror has never seen does **not** block. It cannot be
    /// shown to be open, and the mirror holds every non-archived ION issue,
    /// so an unknown id is one that has been archived or lives outside the
    /// sweep window. Blocking on it would wedge the frontier on something the
    /// user can no longer even see.
    pub fn open_blockers(&self, item: &Item) -> Vec<&Item> {
        item.blockers
            .iter()
            .filter_map(|id| self.get(id))
            .filter(|b| b.is_live() && !b.stage.is_shut())
            .collect()
    }

    pub fn by_project(&self, project: &str) -> Vec<&Item> {
        self.all_items()
            .filter(|i| i.is_live() && i.project.as_deref() == Some(project))
            .collect()
    }

    pub fn by_stage(&self, stage: &Stage) -> Vec<&Item> {
        self.all_items()
            .filter(|i| i.is_live() && &i.stage == stage)
            .collect()
    }

    /// Live and unshut — the population ADR-0001's 250-issue watchline
    /// measures.
    ///
    /// Surfaced as a query rather than left to be remembered: the watchline is
    /// a migration trigger, and a trigger nobody can see is not a trigger.
    pub fn active_count(&self) -> usize {
        self.all_items().filter(|i| i.is_active()).count()
    }
}

/// A stable display order: most urgent priority first, then soonest
/// deadline, then identifier as the tie-break.
///
/// Separate from the queries on purpose (ADR-0002 — consumers rank). Sorts on
/// [`super::item::Priority::rank`], never the raw wire value, which is
/// inverted and holed. Deadlines compare on [`deadline_sort_key`], not the
/// raw string: ADR-0013 requires a day-only deadline to resolve to end of
/// day before comparing, so it ranks after an explicit same-day time rather
/// than before it — the one comparison key `domain` defines, shared with
/// #133's rule evaluator so the two can never disagree on the same rows.
pub fn by_priority_then_due(items: &mut [&Item]) {
    items.sort_by(|a, b| {
        a.priority
            .rank()
            .cmp(&b.priority.rank())
            // `None` sorts last: no deadline is not an infinitely near one.
            .then_with(|| match (&a.deadline, &b.deadline) {
                (Some(x), Some(y)) => deadline_sort_key(x).cmp(&deadline_sort_key(y)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| a.identifier.cmp(&b.identifier))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::item::{Presence, Priority};
    use crate::task::mirror::Sweep;

    fn item(id: &str, stage: Stage) -> Item {
        let mut item = Item::new(id, format!("item {id}"), stage);
        item.identifier = format!("ION-{id}");
        item
    }

    fn mirror_of(items: Vec<Item>) -> Mirror {
        let mut mirror = Mirror::new();
        mirror.reconcile(
            Sweep {
                items,
                routes: Vec::new(),
            },
            1_000,
        );
        mirror
    }

    fn ids(items: &[&Item]) -> Vec<String> {
        items.iter().map(|i| i.id.clone()).collect()
    }

    #[test]
    fn the_frontier_holds_ready_and_in_progress_only() {
        let mirror = mirror_of(vec![
            item("1", Stage::Ready),
            item("2", Stage::InProgress),
            item("3", Stage::Backlog),
            item("4", Stage::Triage),
            item("5", Stage::Done),
        ]);

        assert_eq!(ids(&mirror.frontier()), vec!["1", "2"]);
    }

    #[test]
    fn an_externally_blocked_item_is_not_on_the_frontier() {
        // CONTEXT.md: Blocked means an external wait, and nothing the user
        // can start.
        let mirror = mirror_of(vec![item("1", Stage::Blocked)]);
        assert!(mirror.frontier().is_empty());
    }

    #[test]
    fn an_open_blocker_keeps_an_item_off_the_frontier_and_explains_why() {
        let mut blocked = item("2", Stage::Ready);
        blocked.blockers = vec!["1".to_string()];
        let mirror = mirror_of(vec![item("1", Stage::Ready), blocked]);

        assert_eq!(ids(&mirror.frontier()), vec!["1"]);

        let explained = mirror.blocked_by_relation();
        assert_eq!(explained.len(), 1);
        assert_eq!(explained[0].0.id, "2");
        assert_eq!(ids(&explained[0].1), vec!["1"]);
    }

    #[test]
    fn a_shut_blocker_stops_blocking_without_the_blocked_item_changing() {
        // The direction that matters: blockers live on the *blocked* record
        // (Linear's `inverseRelations`), so closing the blocker has to be
        // enough on its own.
        let mut blocked = item("2", Stage::Ready);
        blocked.blockers = vec!["1".to_string()];

        for shut in [Stage::Done, Stage::Canceled] {
            let mirror = mirror_of(vec![item("1", shut), blocked.clone()]);
            assert_eq!(ids(&mirror.frontier()), vec!["2"]);
        }
    }

    #[test]
    fn an_absent_blocker_stops_blocking() {
        // Archived in Linear: the user cannot see it, so it must not wedge
        // the frontier.
        let mut blocked = item("2", Stage::Ready);
        blocked.blockers = vec!["1".to_string()];
        let mut mirror = mirror_of(vec![item("1", Stage::Ready), blocked]);

        mirror.reconcile(
            Sweep {
                items: vec![{
                    let mut b = item("2", Stage::Ready);
                    b.blockers = vec!["1".to_string()];
                    b
                }],
                routes: Vec::new(),
            },
            2_000,
        );

        assert_eq!(
            mirror.get("1").unwrap().presence,
            Presence::Absent { since_ms: 2_000 }
        );
        assert_eq!(ids(&mirror.frontier()), vec!["2"]);
    }

    #[test]
    fn a_blocker_id_the_mirror_has_never_seen_does_not_block() {
        let mut blocked = item("2", Stage::Ready);
        blocked.blockers = vec!["never-swept".to_string()];
        let mirror = mirror_of(vec![blocked]);

        assert_eq!(ids(&mirror.frontier()), vec!["2"]);
    }

    #[test]
    fn a_chain_of_blockers_only_reports_the_open_ones() {
        let mut third = item("3", Stage::Ready);
        third.blockers = vec!["1".to_string(), "2".to_string()];
        let mirror = mirror_of(vec![item("1", Stage::Done), item("2", Stage::Ready), third]);

        let explained = mirror.blocked_by_relation();
        assert_eq!(ids(&explained[0].1), vec!["2"]);
    }

    #[test]
    fn absent_items_appear_in_no_working_query() {
        let mut mirror = mirror_of(vec![
            item("1", Stage::Ready),
            item("2", Stage::Triage),
            item("3", Stage::Grilling),
        ]);
        mirror.reconcile(Sweep::default(), 2_000);

        assert!(mirror.frontier().is_empty());
        assert!(mirror.triage_inbox().is_empty());
        assert!(mirror.grilling().is_empty());
        assert!(mirror.by_stage(&Stage::Ready).is_empty());
        assert_eq!(mirror.active_count(), 0);
        // ...but every one of them is still in the mirror.
        assert_eq!(mirror.all_items().count(), 3);
    }

    #[test]
    fn active_count_excludes_shut_work() {
        let mirror = mirror_of(vec![
            item("1", Stage::Ready),
            item("2", Stage::Triage),
            item("3", Stage::Done),
            item("4", Stage::Canceled),
        ]);

        assert_eq!(mirror.active_count(), 2);
    }

    #[test]
    fn by_project_matches_on_name_and_skips_unassigned_items() {
        let mut assigned = item("1", Stage::Ready);
        assigned.project = Some("Build the client".to_string());
        let mirror = mirror_of(vec![assigned, item("2", Stage::Ready)]);

        assert_eq!(ids(&mirror.by_project("Build the client")), vec!["1"]);
        assert!(mirror.by_project("Nonexistent").is_empty());
    }

    #[test]
    fn an_empty_mirror_answers_every_query_without_panicking() {
        let mirror = Mirror::new();
        assert!(mirror.frontier().is_empty());
        assert!(mirror.triage_inbox().is_empty());
        assert!(mirror.blocked_by_relation().is_empty());
        assert_eq!(mirror.active_count(), 0);
        assert_eq!(mirror.get("nope"), None);
    }

    #[test]
    fn ranking_sorts_urgent_first_none_last_and_undated_after_dated() {
        let mut none = item("1", Stage::Ready);
        none.priority = Priority::None;
        let mut urgent = item("2", Stage::Ready);
        urgent.priority = Priority::Urgent;
        let mut low_soon = item("3", Stage::Ready);
        low_soon.priority = Priority::Low;
        low_soon.deadline = Some("2026-08-09".to_string());
        let mut low_undated = item("4", Stage::Ready);
        low_undated.priority = Priority::Low;

        let mirror = mirror_of(vec![none, urgent, low_soon, low_undated]);
        let mut items = mirror.frontier();
        by_priority_then_due(&mut items);

        assert_eq!(ids(&items), vec!["2", "3", "4", "1"]);
    }

    /// The ordering comparison goes through [`deadline_sort_key`], not a
    /// raw string `cmp` — this pins that the widened field (a calendar
    /// date, or a minute-precision date-time — #153) keeps sorting
    /// chronologically per ADR-0013: a day-only deadline means *end* of
    /// day, so it ranks *after* an explicit same-day time, not before it,
    /// and chronological order otherwise holds across the mixed set.
    #[test]
    fn same_priority_items_sort_chronologically_across_mixed_deadline_forms() {
        let mut same_day_early = item("2", Stage::Ready);
        same_day_early.priority = Priority::Low;
        same_day_early.deadline = Some("2026-08-09T00:01".to_string());

        let mut same_day_late = item("3", Stage::Ready);
        same_day_late.priority = Priority::Low;
        same_day_late.deadline = Some("2026-08-09T18:00".to_string());

        // Day-only: resolves to 23:59 that day, so it ranks after every
        // explicit same-day time, however late.
        let mut same_day_dated = item("1", Stage::Ready);
        same_day_dated.priority = Priority::Low;
        same_day_dated.deadline = Some("2026-08-09".to_string());

        let mut next_day = item("4", Stage::Ready);
        next_day.priority = Priority::Low;
        next_day.deadline = Some("2026-08-10".to_string());

        let mirror = mirror_of(vec![next_day, same_day_dated, same_day_late, same_day_early]);
        let mut items = mirror.frontier();
        by_priority_then_due(&mut items);

        assert_eq!(
            ids(&items),
            vec!["2", "3", "1", "4"],
            "an explicit same-day time sorts before a day-only deadline (which means end \
             of day), and chronological order otherwise holds across the mixed set",
        );
    }
}
