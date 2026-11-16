//! What a **Glance host** (ADR-0041) publishes beside the Frontier: the
//! items due today.
//!
//! Decided here rather than in the host for ADR-0025's reason. The host maps
//! this function's output to a payload and the panel renders it, so neither
//! decides which item is on today's plate.
//!
//! Like [`super::frontier`], it reads a slim slice of each item and returns
//! ordered ids, so a caller maps a result back onto its own records.
//! Liveness is the caller's: [`crate::Core::today`] hands it live items only
//! (not Done, not archived).

use super::urgency::{compute_urgency, deadline_sort_key_field, UrgencyBand};

/// The facts [`today`] reads from one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodayItem {
    pub id: String,
    pub deadline: Option<String>,
    pub scheduled_date: Option<String>,
}

/// The live items due today, in the order a glance reads them.
///
/// An item is due today when either:
/// - its **Deadline** bands `overdue` or `now` under [`compute_urgency`]
///   (past due, or due within the next day), or
/// - its **Scheduled date** is today or earlier. A do-date that passed with
///   the item still undone stays on today's plate rather than dropping off
///   unseen.
///
/// Order: items in by their deadline first, by deadline (so overdue comes
/// before due-soon), then items in by their scheduled date alone, by that
/// date. The id breaks ties, so reading it twice gives the same order.
///
/// `now` is the reader's local wall clock in deadline shape
/// (`YYYY-MM-DDTHH:MM`), as [`compute_urgency`] takes it.
pub fn today(items: &[TodayItem], now: &str) -> Vec<String> {
    let today_date = now.get(..10).unwrap_or(now);

    let mut by_deadline: Vec<(String, &str)> = Vec::new();
    let mut by_schedule: Vec<(&str, &str)> = Vec::new();
    for item in items {
        let band = compute_urgency(item.deadline.as_deref(), now);
        if matches!(band, UrgencyBand::Overdue | UrgencyBand::Now) {
            let deadline = item.deadline.as_deref().unwrap_or_default();
            by_deadline.push((deadline_sort_key_field(deadline), item.id.as_str()));
        } else if let Some(scheduled) = item.scheduled_date.as_deref() {
            if scheduled <= today_date {
                by_schedule.push((scheduled, item.id.as_str()));
            }
        }
    }
    by_deadline.sort();
    by_schedule.sort();

    by_deadline
        .into_iter()
        .map(|(_, id)| id.to_string())
        .chain(by_schedule.into_iter().map(|(_, id)| id.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-11-16T09:00";

    fn item(id: &str, deadline: Option<&str>, scheduled_date: Option<&str>) -> TodayItem {
        TodayItem {
            id: id.into(),
            deadline: deadline.map(str::to_string),
            scheduled_date: scheduled_date.map(str::to_string),
        }
    }

    #[test]
    fn an_item_with_nothing_dated_is_not_due_today() {
        assert!(today(&[item("a", None, None)], NOW).is_empty());
    }

    #[test]
    fn a_deadline_today_or_within_a_day_is_due_today() {
        let items = [
            item("today", Some("2026-11-16"), None),
            item("tonight", Some("2026-11-16T21:00"), None),
            item("tomorrow-morning", Some("2026-11-17T08:00"), None),
        ];
        // A whole-day deadline runs until the day ends, so it sorts after
        // tonight's 21:00.
        assert_eq!(today(&items, NOW), ["tonight", "today", "tomorrow-morning"]);
    }

    #[test]
    fn a_deadline_further_off_is_not_due_today() {
        assert!(today(&[item("friday", Some("2026-11-20"), None)], NOW).is_empty());
    }

    #[test]
    fn an_overdue_deadline_is_due_today_and_comes_first() {
        let items = [
            item("today", Some("2026-11-16"), None),
            item("last-week", Some("2026-11-09"), None),
        ];
        assert_eq!(today(&items, NOW), ["last-week", "today"]);
    }

    #[test]
    fn a_scheduled_date_today_or_earlier_is_due_today() {
        let items = [
            item("planned-today", None, Some("2026-11-16")),
            item("planned-yesterday", None, Some("2026-11-15")),
            item("planned-tomorrow", None, Some("2026-11-17")),
        ];
        assert_eq!(today(&items, NOW), ["planned-yesterday", "planned-today"]);
    }

    #[test]
    fn deadline_items_come_before_scheduled_only_items() {
        let items = [
            item("planned", None, Some("2026-11-10")),
            item("due", Some("2026-11-16T17:00"), None),
        ];
        assert_eq!(today(&items, NOW), ["due", "planned"]);
    }

    #[test]
    fn an_item_with_a_far_deadline_but_scheduled_today_is_in_by_its_schedule() {
        let items = [
            item("far-but-planned", Some("2026-12-01"), Some("2026-11-16")),
            item("due", Some("2026-11-16"), None),
        ];
        assert_eq!(today(&items, NOW), ["due", "far-but-planned"]);
    }

    #[test]
    fn an_item_in_by_both_appears_once_in_the_deadline_group() {
        let items = [
            item("planned", None, Some("2026-11-16")),
            item("both", Some("2026-11-16"), Some("2026-11-16")),
        ];
        assert_eq!(today(&items, NOW), ["both", "planned"]);
    }

    #[test]
    fn ties_break_by_id_whatever_the_input_order() {
        let forward = [
            item("b", Some("2026-11-16"), None),
            item("a", Some("2026-11-16"), None),
        ];
        let mut backward = forward.clone();
        backward.reverse();
        assert_eq!(today(&forward, NOW), ["a", "b"]);
        assert_eq!(today(&backward, NOW), ["a", "b"]);
    }

    #[test]
    fn a_day_grained_deadline_sorts_after_a_timed_one_the_same_day() {
        let items = [
            item("whole-day", Some("2026-11-16"), None),
            item("at-noon", Some("2026-11-16T12:00"), None),
        ];
        assert_eq!(today(&items, NOW), ["at-noon", "whole-day"]);
    }
}
