//! The mechanical filters: one sweep payload's `items` narrowed to the
//! candidates `rank` is allowed to see.
//!
//! These are the rules `/next-up-personal`'s `linear.sh survey` did in
//! `jq` and `Core::frontier` does over the mirror, restated once here
//! against the owned schema — a *selection*, never a ranking. Nothing in
//! this module reads energy, size, context or priority; that is
//! [`hummingbird_core::rank`]'s whole job and a second opinion here would
//! be exactly the drift #116 exists to delete.
//!
//! Every deadline comparison goes through
//! [`hummingbird_domain::deadline_sort_key`] — the same key `rank` step 2
//! uses and the same one the mirror's display sort uses — never a second,
//! divergent one built here.

use std::collections::{HashMap, HashSet};

use hummingbird_core::rank::Now;
use hummingbird_domain::{deadline_sort_key, ChangesResponse, Item, Stage};

/// How far ahead a not-yet-frontier item's deadline may sit and still pull
/// it into the candidate list. Mirrors `linear.sh survey`'s own
/// `DUE_SOON_DAYS`.
pub const DUE_SOON_DAYS: i64 = 7;

/// What [`select`] decided: the candidates, and how many were dropped for
/// a live blocker — the footer's "4 more blocked upstream" is that count,
/// so it has to survive the filter rather than being recomputed later from
/// a list that no longer contains them.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub candidates: Vec<Item>,
    pub blocked_dropped: usize,
}

/// #10's fourth axis, *who does this*, as a selection question.
///
/// It lives here and not in [`hummingbird_core::rank`] deliberately.
/// `rank`'s six steps are #162's and frozen; a seventh concern there would
/// be exactly the drift both crates' module docs warn about. And it is
/// genuinely a *selection*: "only what I could hand off" narrows which
/// items are eligible, it does not reorder eligible ones. Context is the
/// one hard filter inside ranking because an unmarked item survives it;
/// this filter is the opposite — unmarked means the human does it, so an
/// unmarked item is precisely what [`Who::AgentOnly`] excludes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Who {
    /// Everything that qualifies, marked or not. The default, and the
    /// common case: a survey is normally "what should *I* do".
    #[default]
    Anyone,
    /// Only items carrying `items.agent` — the 9pm case where the honest
    /// answer is not a smaller task but *not one of mine*.
    AgentOnly,
}

/// The selection rules, in order:
///
/// - an archived item is gone, whatever its stage (ADR-0003: rows are
///   flagged, never deleted);
/// - `Ready` / `InProgress` are the frontier and always qualify;
/// - `Triage` / `Grilling` qualify **only** when overdue, due today, or due
///   within [`DUE_SOON_DAYS`] — untriaged work that has run out of runway
///   is actionable whether or not it has been groomed;
/// - `Blocked` is waiting on the world and `Done` is shut; neither ever
///   enters the candidates;
/// - a declared [`Who::AgentOnly`] keeps only `agent`-marked items;
/// - finally, an item with a live `blocked_by` edge whose blocker is not
///   itself shut is dropped and counted.
///
/// The `agent` filter runs **before** the blocked partition, so
/// `blocked_dropped` counts what was dropped from the list actually being
/// offered: on an agent-only survey, "3 more blocked upstream" must mean
/// three agent-doable chores, not three of the human's.
pub fn select(sweep: &ChangesResponse, now: &Now, who: Who) -> Selection {
    let horizon = due_horizon(now);

    let staged: Vec<&Item> = sweep
        .items
        .iter()
        .filter(|item| item.archived_at.is_none())
        .filter(|item| match item.stage {
            Stage::Ready | Stage::InProgress => true,
            Stage::Triage | Stage::Grilling => due_within(item, horizon.as_deref()),
            Stage::Blocked | Stage::Done => false,
        })
        .filter(|item| match who {
            Who::Anyone => true,
            Who::AgentOnly => item.agent,
        })
        .collect();

    let blocked = blocked_item_ids(sweep);
    let (candidates, dropped): (Vec<&Item>, Vec<&Item>) = staged
        .into_iter()
        .partition(|item| !blocked.contains(item.id.as_str()));

    Selection {
        candidates: candidates.into_iter().cloned().collect(),
        blocked_dropped: dropped.len(),
    }
}

/// The last day a deadline may fall on and still count as "due soon":
/// `now`'s own calendar day plus [`DUE_SOON_DAYS`].
///
/// `None` when `now.local` is not a readable `YYYY-MM-DD…` — the same
/// total-form discipline `rank`'s own `deadline_bucket` follows. A `now`
/// this crate cannot read is a caller bug, and the safe reading is that no
/// *ungroomed* item is pulled forward by it; the frontier itself is
/// unaffected, since `Ready`/`InProgress` never consult the horizon.
fn due_horizon(now: &Now) -> Option<String> {
    let day = now.local.get(..10)?;
    add_days(day, DUE_SOON_DAYS)
}

/// Whether this item's deadline falls on or before the horizon day. One
/// comparison covers all three readings the rule names: overdue sorts
/// before today, which sorts before the horizon.
fn due_within(item: &Item, horizon: Option<&str>) -> bool {
    let (Some(deadline), Some(horizon)) = (item.deadline.as_deref(), horizon) else {
        return false;
    };
    deadline_sort_key(deadline) <= deadline_sort_key(horizon)
}

/// The ids of items carrying at least one live blocker.
///
/// An edge counts when `removed_at` is `None` (the edge itself is live)
/// **and** the blocker is not shut — `Done` or archived. A blocker id that
/// is absent from this sweep is treated as blocking: a payload that does
/// not mention the row cannot be read as evidence the row is finished.
fn blocked_item_ids(sweep: &ChangesResponse) -> HashSet<&str> {
    let by_id: HashMap<&str, &Item> = sweep
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();

    sweep
        .blocked_by
        .iter()
        .filter(|edge| edge.removed_at.is_none())
        .filter(|edge| match by_id.get(edge.blocker_id.as_str()) {
            Some(blocker) => !is_shut(blocker),
            None => true,
        })
        .map(|edge| edge.item_id.as_str())
        .collect()
}

fn is_shut(item: &Item) -> bool {
    item.stage == Stage::Done || item.archived_at.is_some()
}

/// `YYYY-MM-DD` plus `days`, on the proleptic Gregorian calendar.
///
/// Dependency-free integer arithmetic on day numbers (Howard Hinnant's
/// `days_from_civil` / `civil_from_days`), the same shape
/// `server/city-waste/src/date.rs` uses: this crate has one date question
/// and no business pulling a calendar library in for it. `None` on
/// anything that is not a readable date.
fn add_days(day: &str, days: i64) -> Option<String> {
    let (y, m, d) = parse_day(day)?;
    let (y, m, d) = civil_from_days(days_from_civil(y, m, d) + days);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

fn parse_day(day: &str) -> Option<(i64, i64, i64)> {
    let bytes = day.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = day.get(..4)?.parse().ok()?;
    let month = day.get(5..7)?.parse().ok()?;
    let dom = day.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&dom) {
        return None;
    }
    Some((year, month, dom))
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_domain::BlockedBy;

    fn sweep(items: Vec<Item>, blocked_by: Vec<BlockedBy>) -> ChangesResponse {
        ChangesResponse {
            items,
            blocked_by,
            ..ChangesResponse::empty(1)
        }
    }

    fn item(id: &str, stage: Stage) -> Item {
        Item {
            id: id.to_string(),
            seq: None,
            title: format!("item {id}"),
            description: None,
            stage,
            size: None,
            energy: None,
            context: None,
            priority: 0,
            project_id: None,
            project_pos: None,
            deadline: None,
            scheduled_date: None,
            source: None,
            source_key: None,
            source_url: None,
            archived_at: None,
            agent: false,
            created_at: 1_000,
            updated_at: 1_000,
            version: 1,
        }
    }

    fn edge(item_id: &str, blocker_id: &str) -> BlockedBy {
        BlockedBy {
            item_id: item_id.to_string(),
            blocker_id: blocker_id.to_string(),
            version: 1,
            removed_at: None,
        }
    }

    fn now() -> Now {
        Now {
            local: "2026-08-11T09:53".to_string(),
            epoch_ms: 1_786_553_580_000,
        }
    }

    fn ids(selection: &Selection) -> Vec<&str> {
        selection
            .candidates
            .iter()
            .map(|item| item.id.as_str())
            .collect()
    }

    /// Most of these tests are about the stage and deadline rules, which
    /// the delegation axis does not touch, so they read against the
    /// default arm. The `agent` tests below call `super::select` directly
    /// with the arm they are actually about.
    fn select(sweep: &ChangesResponse, now: &Now) -> Selection {
        super::select(sweep, now, Who::Anyone)
    }

    #[test]
    fn ready_and_in_progress_are_the_frontier_and_need_no_deadline() {
        let selection = select(
            &sweep(
                vec![item("ready", Stage::Ready), item("wip", Stage::InProgress)],
                vec![],
            ),
            &now(),
        );
        assert_eq!(ids(&selection), ["ready", "wip"]);
    }

    #[test]
    fn blocked_and_done_never_enter_the_candidates() {
        let selection = select(
            &sweep(
                vec![item("blocked", Stage::Blocked), item("done", Stage::Done)],
                vec![],
            ),
            &now(),
        );
        assert!(selection.candidates.is_empty());
        // Nothing was dropped for a *blocker*: `Blocked` is a stage, and
        // the footer's blocked-upstream count is about edges.
        assert_eq!(selection.blocked_dropped, 0);
    }

    #[test]
    fn an_archived_item_is_gone_whatever_its_stage() {
        let mut archived = item("archived", Stage::InProgress);
        archived.archived_at = Some(1);
        let selection = select(&sweep(vec![archived], vec![]), &now());
        assert!(selection.candidates.is_empty());
    }

    #[test]
    fn triage_and_grilling_stay_out_with_no_deadline() {
        let selection = select(
            &sweep(
                vec![item("t", Stage::Triage), item("g", Stage::Grilling)],
                vec![],
            ),
            &now(),
        );
        assert!(selection.candidates.is_empty());
    }

    #[test]
    fn triage_and_grilling_enter_when_overdue_due_today_or_due_within_seven_days() {
        let mut overdue = item("overdue", Stage::Triage);
        overdue.deadline = Some("2026-08-01".to_string());
        let mut today = item("today", Stage::Grilling);
        today.deadline = Some("2026-08-11".to_string());
        let mut soon = item("soon", Stage::Triage);
        soon.deadline = Some("2026-08-18".to_string());
        let selection = select(&sweep(vec![overdue, today, soon], vec![]), &now());
        assert_eq!(ids(&selection), ["overdue", "today", "soon"]);
    }

    #[test]
    fn a_grilling_item_one_day_past_the_horizon_stays_out() {
        let mut just_past = item("just-past", Stage::Grilling);
        just_past.deadline = Some("2026-08-19".to_string());
        let selection = select(&sweep(vec![just_past], vec![]), &now());
        assert!(selection.candidates.is_empty());
    }

    /// The horizon is a whole day, not an instant: a deadline at 00:01 on
    /// the horizon day is still due soon. This is exactly what
    /// `deadline_sort_key` buys — a day-only value resolves to `T23:59`.
    #[test]
    fn a_timed_deadline_on_the_horizon_day_is_still_due_soon() {
        let mut edge_case = item("edge", Stage::Triage);
        edge_case.deadline = Some("2026-08-18T00:01".to_string());
        let selection = select(&sweep(vec![edge_case], vec![]), &now());
        assert_eq!(ids(&selection), ["edge"]);
    }

    #[test]
    fn a_live_edge_with_an_open_blocker_drops_the_item_and_is_counted() {
        let selection = select(
            &sweep(
                vec![item("a", Stage::Ready), item("blocker", Stage::Ready)],
                vec![edge("a", "blocker")],
            ),
            &now(),
        );
        assert_eq!(ids(&selection), ["blocker"]);
        assert_eq!(selection.blocked_dropped, 1);
    }

    #[test]
    fn a_shut_blocker_no_longer_blocks() {
        let mut blocker = item("blocker", Stage::Done);
        blocker.stage = Stage::Done;
        let selection = select(
            &sweep(
                vec![item("a", Stage::Ready), blocker],
                vec![edge("a", "blocker")],
            ),
            &now(),
        );
        assert_eq!(ids(&selection), ["a"]);
        assert_eq!(selection.blocked_dropped, 0);
    }

    #[test]
    fn an_archived_blocker_no_longer_blocks_either() {
        let mut blocker = item("blocker", Stage::Ready);
        blocker.archived_at = Some(5);
        let selection = select(
            &sweep(
                vec![item("a", Stage::Ready), blocker],
                vec![edge("a", "blocker")],
            ),
            &now(),
        );
        assert_eq!(ids(&selection), ["a"]);
    }

    #[test]
    fn a_removed_edge_does_not_block() {
        let mut removed = edge("a", "blocker");
        removed.removed_at = Some(9);
        let selection = select(
            &sweep(
                vec![item("a", Stage::Ready), item("blocker", Stage::Ready)],
                vec![removed],
            ),
            &now(),
        );
        assert_eq!(ids(&selection), ["a", "blocker"]);
        assert_eq!(selection.blocked_dropped, 0);
    }

    /// A delta payload can name a blocker it does not carry. Reading that
    /// silence as "finished" would surface an item whose blocker is very
    /// much alive.
    #[test]
    fn a_blocker_absent_from_the_payload_still_blocks() {
        let selection = select(
            &sweep(vec![item("a", Stage::Ready)], vec![edge("a", "ghost")]),
            &now(),
        );
        assert!(selection.candidates.is_empty());
        assert_eq!(selection.blocked_dropped, 1);
    }

    /// One item, two live edges: the footer counts *items* held upstream,
    /// not edges.
    #[test]
    fn two_live_edges_on_one_item_count_once() {
        let selection = select(
            &sweep(
                vec![
                    item("a", Stage::Ready),
                    item("b1", Stage::Ready),
                    item("b2", Stage::Ready),
                ],
                vec![edge("a", "b1"), edge("a", "b2")],
            ),
            &now(),
        );
        assert_eq!(selection.blocked_dropped, 1);
    }

    #[test]
    fn an_unreadable_now_pulls_no_ungroomed_item_forward_and_leaves_the_frontier_alone() {
        let mut overdue = item("overdue", Stage::Triage);
        overdue.deadline = Some("2026-08-01".to_string());
        let unreadable = Now {
            local: "nope".to_string(),
            epoch_ms: 0,
        };
        let selection = select(
            &sweep(vec![overdue, item("ready", Stage::Ready)], vec![]),
            &unreadable,
        );
        assert_eq!(ids(&selection), ["ready"]);
    }

    // ------------------------------------------- #10's fourth axis

    #[test]
    fn the_default_arm_offers_marked_and_unmarked_alike() {
        let mut marked = item("marked", Stage::Ready);
        marked.agent = true;
        let selection = select(
            &sweep(vec![marked, item("mine", Stage::Ready)], vec![]),
            &now(),
        );
        assert_eq!(
            ids(&selection),
            ["marked", "mine"],
            "an ordinary survey is not narrowed by the axis at all",
        );
    }

    #[test]
    fn agent_only_keeps_marked_items_and_drops_the_human_s() {
        let mut marked = item("marked", Stage::Ready);
        marked.agent = true;
        let selection = super::select(
            &sweep(vec![marked, item("mine", Stage::Ready)], vec![]),
            &now(),
            Who::AgentOnly,
        );
        assert_eq!(
            ids(&selection),
            ["marked"],
            "unmarked means the human does it, so unmarked is what this arm excludes",
        );
    }

    /// The order the two filters run in is the assertion, not an
    /// implementation detail: `blocked_dropped` is the footer's "N more
    /// blocked upstream", and on an agent-only survey it has to count
    /// agent-doable chores. Filtering after the partition would count the
    /// human's blocked work into a hand-off footer.
    #[test]
    fn agent_only_counts_only_agent_doable_work_as_blocked_upstream() {
        let mut marked = item("marked", Stage::Ready);
        marked.agent = true;
        let mut marked_blocked = item("marked-blocked", Stage::Ready);
        marked_blocked.agent = true;
        let mine_blocked = item("mine-blocked", Stage::Ready);
        let blocker = item("blocker", Stage::Ready);

        let selection = super::select(
            &sweep(
                vec![marked, marked_blocked, mine_blocked, blocker],
                vec![
                    edge("marked-blocked", "blocker"),
                    edge("mine-blocked", "blocker"),
                ],
            ),
            &now(),
            Who::AgentOnly,
        );
        assert_eq!(ids(&selection), ["marked"]);
        assert_eq!(
            selection.blocked_dropped, 1,
            "one agent-doable chore is blocked upstream; the human's is not this arm's business",
        );
    }

    #[test]
    fn add_days_crosses_a_month_a_year_and_a_leap_day() {
        assert_eq!(add_days("2026-08-11", 7).as_deref(), Some("2026-08-18"));
        assert_eq!(add_days("2026-08-28", 7).as_deref(), Some("2026-09-04"));
        assert_eq!(add_days("2026-12-28", 7).as_deref(), Some("2027-01-04"));
        assert_eq!(add_days("2028-02-26", 7).as_deref(), Some("2028-03-04"));
        assert_eq!(add_days("2026-13-01", 7), None);
        assert_eq!(add_days("2026-08", 7), None);
    }
}
