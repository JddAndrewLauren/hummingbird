//! The frontier's ordering, grouping and faceting — sunk here from the
//! web's `frontier-order.ts`, `frontier-columns.ts` and `frontier-facets.ts`
//! by ADR-0025 (#141/M1-3).
//!
//! **[`by_priority_then_due`] replaces `client/core/src/task/query.rs`'s
//! function of the same name** (the S1/Linear-era mirror's own copy of this
//! rule, over `crate::task::item::Item`/`Priority` rather than the owned
//! schema). ADR-0021 decision 1 is flat that there is one spelling of
//! frontier order; the old body is deleted, not kept as a second live
//! implementation, and this is the only place the name survives.
//!
//! **Grouping stays clockless, on purpose.** None of the four axes
//! (context, project, size, energy) is time-varying, so [`group_frontier`]
//! takes no `now` at all — there is nothing for a clock to leak into, and a
//! caller cannot accidentally make a column's membership depend on when it
//! asked.
//!
//! **Everything here works over ids, not whole items.** A caller (the web
//! seam, later Android) already holds the full item; handing the boundary
//! only what a decision needs — an id plus the handful of fields the rule
//! reads — keeps the crossing cheap and keeps this module from redefining
//! `hummingbird_domain::Item`'s shape a second time. The caller maps the
//! returned ids back onto its own items.

use std::cmp::Ordering;
use std::collections::HashSet;

use hummingbird_domain::deadline_sort_key;

use super::urgency::compute_urgency;
use super::vocabulary::CONTEXTS;

/// The frontier-relevant slice of one item: what [`by_priority_then_due`],
/// [`group_frontier`] and the facet functions below all read. `id` is
/// carried through untouched so a caller can map a result back onto its own
/// full record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontierItem {
    pub id: String,
    pub priority: i64,
    pub deadline: Option<String>,
    pub context: Option<String>,
    pub size: Option<String>,
    pub energy: Option<String>,
    pub project_id: Option<String>,
}

// --------------------------------------------------------------- ordering

/// `items.priority`'s wire encoding (ADR-0008: "priority survives — it is
/// human-set intent, not a Linear-ism") is inverted and holed: `0` means
/// "No priority" and sorts *last*, `1..=4` are Urgent..Low in that order.
/// Nothing may sort or render the raw number directly — this is the one
/// rank a display order reads. An unrecognised value degrades to the same
/// rank as "no priority" rather than panicking or sorting as Urgent.
pub fn priority_rank(raw: i64) -> i64 {
    match raw {
        1 => 0, // Urgent
        2 => 1, // High
        3 => 2, // Medium
        4 => 3, // Low
        _ => 4, // No priority (0, or anything unrecognised) — sorts last
    }
}

/// A stable display order: most urgent priority first, then soonest
/// deadline, then id as the tie-break. Pure — never mutates `items`, and
/// reading it twice with the same input yields the same output — and
/// returns the ordered ids only (see the module header for why).
pub fn by_priority_then_due(items: &[FrontierItem]) -> Vec<String> {
    let mut ordered: Vec<&FrontierItem> = items.iter().collect();
    ordered.sort_by(|a, b| {
        priority_rank(a.priority)
            .cmp(&priority_rank(b.priority))
            .then_with(|| compare_deadlines(a.deadline.as_deref(), b.deadline.as_deref()))
            .then_with(|| a.id.cmp(&b.id))
    });
    ordered.into_iter().map(|item| item.id.clone()).collect()
}

/// `None` (no deadline) sorts last — an item with nothing pressing it is not
/// the same as one due infinitely soon.
fn compare_deadlines(a: Option<&str>, b: Option<&str>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => deadline_sort_key(x).cmp(&deadline_sort_key(y)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

// --------------------------------------------------------------- grouping

/// The axes the frontier can be grouped by (ADR-0021 decision 1) — `project`
/// answers *what does this belong to*, the other three answer *what can I
/// do right now, from where I am, with the time and energy I have*.
/// Deliberately not [`super::vocabulary::FRONTIER_AXES`]: that list is the
/// *facet* vocabulary (urgency instead of project — colour already carries
/// urgency across whatever axis is live, and a project column already
/// isolates one project, so neither is offered the other's way).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontierAxis {
    Context,
    Project,
    Size,
    Energy,
}

impl FrontierAxis {
    pub fn as_str(self) -> &'static str {
        match self {
            FrontierAxis::Context => "context",
            FrontierAxis::Project => "project",
            FrontierAxis::Size => "size",
            FrontierAxis::Energy => "energy",
        }
    }

    pub fn parse(s: &str) -> Option<FrontierAxis> {
        FRONTIER_GROUP_AXES.into_iter().find(|axis| axis.as_str() == s)
    }
}

/// Every grouping axis, in the order the switch offers them — `context`
/// leads because it is the default.
pub const FRONTIER_GROUP_AXES: [FrontierAxis; 4] = [
    FrontierAxis::Context,
    FrontierAxis::Project,
    FrontierAxis::Size,
    FrontierAxis::Energy,
];

pub const DEFAULT_FRONTIER_AXIS: FrontierAxis = FrontierAxis::Context;

/// One project's id and display name — the wire hop [`group_frontier`]
/// needs to label a `project` column with a real name rather than a raw
/// uuid (PR #200's review, carried forward from `frontier-groups.ts`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectName {
    pub id: String,
    pub name: String,
}

/// One column of [`group_frontier`]'s output. `value` is `None` for the
/// bucket of items naming no value on the live axis; `label` is `value`'s
/// display name (the project's real name on the `project` axis, `value`
/// itself on every other axis, `None` exactly when `value` is `None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontierColumn {
    pub value: Option<String>,
    pub label: Option<String>,
    pub ids: Vec<String>,
}

fn raw_axis_value(item: &FrontierItem, axis: FrontierAxis) -> Option<&str> {
    match axis {
        FrontierAxis::Context => item.context.as_deref(),
        FrontierAxis::Project => item.project_id.as_deref(),
        FrontierAxis::Size => item.size.as_deref(),
        FrontierAxis::Energy => item.energy.as_deref(),
    }
}

/// An empty string is not a value, and folds into the no-value bucket —
/// `items.context` is free text with no empty-string rejection on the write
/// path, so without this fold an API writer could land a *second* no-value
/// column sharing the caller's `value ?? ""` key.
fn axis_value(item: &FrontierItem, axis: FrontierAxis) -> Option<String> {
    match raw_axis_value(item, axis) {
        None => None,
        Some("") => None,
        Some(v) => Some(v.to_string()),
    }
}

/// Columns for one axis: fullest first, and the no-value column always
/// last whichever axis is live. Ties are broken by first appearance in
/// `items`, and within a bucket the input order is preserved — the caller
/// has already ordered with [`by_priority_then_due`], and grouping never
/// re-sorts.
pub fn group_frontier(
    items: &[FrontierItem],
    axis: FrontierAxis,
    projects: &[ProjectName],
) -> Vec<FrontierColumn> {
    let mut order: Vec<Option<String>> = Vec::new();
    let mut buckets: std::collections::HashMap<Option<String>, Vec<String>> =
        std::collections::HashMap::new();

    for item in items {
        let value = axis_value(item, axis);
        let bucket = buckets.entry(value.clone()).or_insert_with(|| {
            order.push(value.clone());
            Vec::new()
        });
        bucket.push(item.id.clone());
    }

    let mut columns: Vec<FrontierColumn> = order
        .into_iter()
        .map(|value| {
            let ids = buckets.remove(&value).unwrap_or_default();
            let label = match &value {
                None => None,
                Some(v) => {
                    if axis == FrontierAxis::Project {
                        projects.iter().find(|p| &p.id == v).map(|p| p.name.clone())
                    } else {
                        Some(v.clone())
                    }
                }
            };
            FrontierColumn { value, label, ids }
        })
        .collect();

    let mut named: Vec<FrontierColumn> = Vec::new();
    let mut unnamed: Vec<FrontierColumn> = Vec::new();
    for column in columns.drain(..) {
        if column.value.is_some() {
            named.push(column);
        } else {
            unnamed.push(column);
        }
    }
    named.sort_by(|a, b| b.ids.len().cmp(&a.ids.len()));

    named.into_iter().chain(unnamed).collect()
}

// --------------------------------------------------------------- facets

/// The facets the filter panel offers — `hummingbird_domain`'s wire
/// vocabulary plus `urgency`, exactly [`super::vocabulary::FRONTIER_AXES`]
/// in order (unlike [`FrontierAxis`] above, which swaps `urgency` for
/// `project`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Facet {
    Context,
    Size,
    Energy,
    Urgency,
}

impl Facet {
    pub fn as_str(self) -> &'static str {
        match self {
            Facet::Context => "context",
            Facet::Size => "size",
            Facet::Energy => "energy",
            Facet::Urgency => "urgency",
        }
    }

    pub fn parse(s: &str) -> Option<Facet> {
        FACETS.into_iter().find(|f| f.as_str() == s)
    }
}

pub const FACETS: [Facet; 4] = [Facet::Context, Facet::Size, Facet::Energy, Facet::Urgency];

/// Display token for the column and chip of items naming no value —
/// `context` is free text, so unlike `size`/`energy` its vocabulary is not
/// closed and the absent case needs a name of its own.
pub const NO_CONTEXT: &str = "no context";

/// One facet's picked values. OR-ed within a facet, AND-ed across facets;
/// an unpicked facet (empty set) matches everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FacetSelection {
    pub context: HashSet<String>,
    pub size: HashSet<String>,
    pub energy: HashSet<String>,
    pub urgency: HashSet<String>,
}

impl FacetSelection {
    fn field(&self, facet: Facet) -> &HashSet<String> {
        match facet {
            Facet::Context => &self.context,
            Facet::Size => &self.size,
            Facet::Energy => &self.energy,
            Facet::Urgency => &self.urgency,
        }
    }

    fn field_mut(&mut self, facet: Facet) -> &mut HashSet<String> {
        match facet {
            Facet::Context => &mut self.context,
            Facet::Size => &mut self.size,
            Facet::Energy => &mut self.energy,
            Facet::Urgency => &mut self.urgency,
        }
    }
}

pub fn facet_count(picked: &FacetSelection) -> usize {
    FACETS.iter().map(|f| picked.field(*f).len()).sum()
}

/// Adds `value` to `facet` if absent, removes it if present. Pure — never
/// mutates `picked`.
pub fn toggle_facet(picked: &FacetSelection, facet: Facet, value: &str) -> FacetSelection {
    let mut next = picked.clone();
    let field = next.field_mut(facet);
    if !field.remove(value) {
        field.insert(value.to_string());
    }
    next
}

/// Whether `item` matches every picked facet — `now` is a deadline-shaped
/// local wall-clock string (`urgency.rs`'s own convention: no clock is read
/// ambiently here either).
pub fn matches_facets(item: &FrontierItem, picked: &FacetSelection, now: &str) -> bool {
    if !picked.context.is_empty() {
        let value = item.context.as_deref().unwrap_or(NO_CONTEXT);
        if !picked.context.contains(value) {
            return false;
        }
    }
    // `size`/`energy` have closed vocabularies and no "none" chip: picking a
    // value is a claim about the work's shape, and an unjudged item makes no
    // such claim, so it is excluded the moment either facet is picked at all.
    if !picked.size.is_empty() {
        match &item.size {
            Some(size) if picked.size.contains(size) => {}
            _ => return false,
        }
    }
    if !picked.energy.is_empty() {
        match &item.energy {
            Some(energy) if picked.energy.contains(energy) => {}
            _ => return false,
        }
    }
    if !picked.urgency.is_empty() {
        let band = compute_urgency(item.deadline.as_deref(), now).as_str();
        if !picked.urgency.contains(band) {
            return false;
        }
    }
    true
}

/// The picked ids, in the order given — filtering never reorders.
pub fn apply_facets(items: &[FrontierItem], picked: &FacetSelection, now: &str) -> Vec<String> {
    items
        .iter()
        .filter(|item| matches_facets(item, picked, now))
        .map(|item| item.id.clone())
        .collect()
}

/// Contexts actually present in `items`, suggested vocabulary first (in
/// [`CONTEXTS`]'s own order), then any extra alphabetically, then
/// [`NO_CONTEXT`] last if anything names no context.
pub fn contexts_of(items: &[FrontierItem]) -> Vec<String> {
    let present: HashSet<&str> =
        items.iter().map(|item| item.context.as_deref().unwrap_or(NO_CONTEXT)).collect();

    let mut ordered: Vec<String> =
        CONTEXTS.iter().filter(|c| present.contains(*c)).map(|c| c.to_string()).collect();

    let mut extra: Vec<&str> =
        present.iter().copied().filter(|c| *c != NO_CONTEXT && !CONTEXTS.contains(c)).collect();
    extra.sort();
    ordered.extend(extra.into_iter().map(|c| c.to_string()));

    if present.contains(NO_CONTEXT) {
        ordered.push(NO_CONTEXT.to_string());
    }
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> FrontierItem {
        FrontierItem {
            id: id.to_string(),
            priority: 0,
            deadline: None,
            context: None,
            size: None,
            energy: None,
            project_id: None,
        }
    }

    // ------------------------------------------------------ by_priority_then_due
    // Ported from `frontier-order.test.ts`.

    #[test]
    fn ranks_by_priority_label_never_the_raw_wire_number() {
        let none = FrontierItem { priority: 0, ..item("none") };
        let urgent = FrontierItem { priority: 1, ..item("urgent") };
        let low = FrontierItem { priority: 4, ..item("low") };

        assert_eq!(
            by_priority_then_due(&[none, low, urgent]),
            vec!["urgent", "low", "none"],
        );
    }

    #[test]
    fn within_the_same_priority_orders_by_deadline_chronologically() {
        let soon =
            FrontierItem { priority: 1, deadline: Some("2026-08-15".into()), ..item("soon") };
        let later =
            FrontierItem { priority: 1, deadline: Some("2026-08-20".into()), ..item("later") };
        let none = FrontierItem { priority: 1, ..item("none-deadline") };

        assert_eq!(
            by_priority_then_due(&[none, later, soon]),
            vec!["soon", "later", "none-deadline"],
        );
    }

    #[test]
    fn a_day_only_deadline_sorts_after_an_explicit_same_day_time() {
        let day_only =
            FrontierItem { priority: 1, deadline: Some("2026-08-15".into()), ..item("day-only") };
        let explicit_late = FrontierItem {
            priority: 1,
            deadline: Some("2026-08-15T18:00".into()),
            ..item("explicit-late")
        };

        assert_eq!(
            by_priority_then_due(&[day_only, explicit_late]),
            vec!["explicit-late", "day-only"],
        );
    }

    #[test]
    fn falls_back_to_id_for_a_fully_tied_ordering() {
        let a = FrontierItem { priority: 2, ..item("a") };
        let b = FrontierItem { priority: 2, ..item("b") };

        assert_eq!(by_priority_then_due(&[b, a]), vec!["a", "b"]);
    }

    #[test]
    fn is_a_pure_function_returning_the_same_order_every_call() {
        let items = vec![
            FrontierItem { priority: 3, ..item("a") },
            FrontierItem { priority: 1, ..item("b") },
        ];
        assert_eq!(by_priority_then_due(&items), by_priority_then_due(&items));
    }

    // ------------------------------------------------------------ group_frontier
    // Ported from `frontier-columns.test.ts`.

    #[test]
    fn collects_items_by_their_value_on_the_live_axis_preserving_input_order() {
        for axis in FRONTIER_GROUP_AXES {
            let field = |v: &str| -> FrontierItem {
                let mut it = item("x");
                match axis {
                    FrontierAxis::Context => it.context = Some(v.to_string()),
                    FrontierAxis::Project => it.project_id = Some(v.to_string()),
                    FrontierAxis::Size => it.size = Some(v.to_string()),
                    FrontierAxis::Energy => it.energy = Some(v.to_string()),
                }
                it
            };
            let a = FrontierItem { id: "a".into(), ..field("x") };
            let b = FrontierItem { id: "b".into(), ..field("y") };
            let c = FrontierItem { id: "c".into(), ..field("x") };

            let columns = group_frontier(&[a, b, c], axis, &[]);

            assert_eq!(columns[0].value.as_deref(), Some("x"));
            assert_eq!(columns[0].ids, vec!["a", "c"]);
            assert_eq!(columns[1].value.as_deref(), Some("y"));
            assert_eq!(columns[1].ids, vec!["b"]);
        }
    }

    #[test]
    fn sorts_the_unnamed_column_last_even_when_it_comes_first() {
        let unnamed = item("a");
        let named = FrontierItem { context: Some("x".into()), ..item("b") };

        let columns = group_frontier(&[unnamed, named], FrontierAxis::Context, &[]);

        assert_eq!(
            columns.iter().map(|c| c.value.clone()).collect::<Vec<_>>(),
            vec![Some("x".to_string()), None],
        );
    }

    #[test]
    fn puts_the_fullest_column_first_whatever_order_the_input_arrived_in() {
        let thin = FrontierItem { context: Some("thin".into()), ..item("a") };
        let fat1 = FrontierItem { context: Some("fat".into()), ..item("b") };
        let fat2 = FrontierItem { context: Some("fat".into()), ..item("c") };

        let columns = group_frontier(&[thin, fat1, fat2], FrontierAxis::Context, &[]);

        assert_eq!(
            columns.iter().map(|c| c.value.clone()).collect::<Vec<_>>(),
            vec![Some("fat".to_string()), Some("thin".to_string())],
        );
    }

    #[test]
    fn keeps_the_unnamed_column_last_even_when_it_is_fullest() {
        let unnamed1 = item("a");
        let unnamed2 = item("b");
        let unnamed3 = item("c");
        let named = FrontierItem { context: Some("@computer".into()), ..item("d") };

        let columns =
            group_frontier(&[unnamed1, unnamed2, unnamed3, named], FrontierAxis::Context, &[]);

        assert_eq!(columns[0].value.as_deref(), Some("@computer"));
        assert_eq!(columns[1].value, None);
        assert_eq!(columns[1].ids.len(), 3);
    }

    #[test]
    fn breaks_a_tie_between_equal_columns_by_first_appearance() {
        let first = FrontierItem { context: Some("@phone".into()), ..item("a") };
        let second = FrontierItem { context: Some("@computer".into()), ..item("b") };

        let columns = group_frontier(&[first, second], FrontierAxis::Context, &[]);

        assert_eq!(columns[0].value.as_deref(), Some("@phone"));
        assert_eq!(columns[1].value.as_deref(), Some("@computer"));
    }

    #[test]
    fn folds_an_empty_string_axis_value_into_the_no_value_column() {
        let empty = FrontierItem { context: Some("".into()), ..item("a") };
        let absent = item("b");
        let named = FrontierItem { context: Some("x".into()), ..item("c") };

        let columns = group_frontier(&[empty, absent, named], FrontierAxis::Context, &[]);

        assert_eq!(columns[0].value.as_deref(), Some("x"));
        assert_eq!(columns[1].value, None);
        assert_eq!(columns[1].ids, vec!["a", "b"]);
    }

    #[test]
    fn resolves_a_project_columns_real_name_from_the_given_project_list() {
        let a = FrontierItem { project_id: Some("p-1".into()), ..item("a") };
        let projects = [ProjectName { id: "p-1".into(), name: "Ship the release".into() }];

        let columns = group_frontier(&[a], FrontierAxis::Project, &projects);

        assert_eq!(columns[0].value.as_deref(), Some("p-1"));
        assert_eq!(columns[0].label.as_deref(), Some("Ship the release"));
    }

    #[test]
    fn falls_back_to_a_null_label_for_a_project_id_not_yet_known() {
        let a = FrontierItem { project_id: Some("unknown-id".into()), ..item("a") };
        let projects = [ProjectName { id: "p-1".into(), name: "Ship the release".into() }];

        let columns = group_frontier(&[a], FrontierAxis::Project, &projects);

        assert_eq!(columns[0].label, None);
    }

    #[test]
    fn labels_every_other_axis_with_the_value_itself() {
        let a = FrontierItem { context: Some("raw-value".into()), ..item("a") };
        let projects = [ProjectName { id: "raw-value".into(), name: "Not this".into() }];

        let columns = group_frontier(&[a], FrontierAxis::Context, &projects);

        assert_eq!(columns[0].label.as_deref(), Some("raw-value"));
    }

    #[test]
    fn returns_no_columns_for_an_empty_frontier() {
        for axis in FRONTIER_GROUP_AXES {
            assert!(group_frontier(&[], axis, &[]).is_empty());
        }
    }

    #[test]
    fn accounts_for_every_input_item_exactly_once_on_every_axis() {
        let input = vec![
            FrontierItem { context: Some("@computer".into()), ..item("a") },
            FrontierItem { context: Some("@phone".into()), ..item("b") },
            item("c"),
            FrontierItem { context: Some("@computer".into()), ..item("d") },
        ];
        for axis in FRONTIER_GROUP_AXES {
            let mut ids: Vec<String> =
                group_frontier(&input, axis, &[]).into_iter().flat_map(|c| c.ids).collect();
            ids.sort();
            assert_eq!(ids, vec!["a", "b", "c", "d"]);
        }
    }

    // -------------------------------------------------------------------- facets
    // Ported from `frontier-facets.test.ts`.

    const NOW: &str = "2026-08-13T12:00";

    #[test]
    fn matches_everything_when_nothing_is_picked() {
        let picked = FacetSelection::default();
        let it =
            FrontierItem { context: Some("@phone".into()), size: Some("deep".into()), ..item("a") };
        assert!(matches_facets(&it, &picked, NOW));
    }

    #[test]
    fn ors_within_a_facet_picking_two_contexts_widens() {
        let mut picked = toggle_facet(&FacetSelection::default(), Facet::Context, "@computer");
        let phone = FrontierItem { context: Some("@phone".into()), ..item("a") };
        assert!(!matches_facets(&phone, &picked, NOW));

        picked = toggle_facet(&picked, Facet::Context, "@phone");
        assert!(matches_facets(&phone, &picked, NOW));
        let computer = FrontierItem { context: Some("@computer".into()), ..item("b") };
        assert!(matches_facets(&computer, &picked, NOW));
        let garden = FrontierItem { context: Some("@garden".into()), ..item("c") };
        assert!(!matches_facets(&garden, &picked, NOW));
    }

    #[test]
    fn ands_across_facets_picking_a_context_and_a_size_narrows() {
        let mut picked = toggle_facet(&FacetSelection::default(), Facet::Context, "@computer");
        picked = toggle_facet(&picked, Facet::Size, "quick");

        let matches = FrontierItem {
            context: Some("@computer".into()),
            size: Some("quick".into()),
            ..item("a")
        };
        assert!(matches_facets(&matches, &picked, NOW));

        let wrong_size = FrontierItem {
            context: Some("@computer".into()),
            size: Some("deep".into()),
            ..item("b")
        };
        assert!(!matches_facets(&wrong_size, &picked, NOW));

        let wrong_context = FrontierItem {
            context: Some("@phone".into()),
            size: Some("quick".into()),
            ..item("c")
        };
        assert!(!matches_facets(&wrong_context, &picked, NOW));
    }

    #[test]
    fn treats_an_absent_context_as_its_own_pickable_value() {
        let picked = toggle_facet(&FacetSelection::default(), Facet::Context, NO_CONTEXT);
        assert!(matches_facets(&item("a"), &picked, NOW));
        let computer = FrontierItem { context: Some("@computer".into()), ..item("b") };
        assert!(!matches_facets(&computer, &picked, NOW));
    }

    #[test]
    fn excludes_an_unjudged_size_or_energy_once_that_facet_is_picked() {
        let by_size = toggle_facet(&FacetSelection::default(), Facet::Size, "quick");
        assert!(!matches_facets(&item("a"), &by_size, NOW));

        let by_energy = toggle_facet(&FacetSelection::default(), Facet::Energy, "low");
        assert!(!matches_facets(&item("b"), &by_energy, NOW));
    }

    #[test]
    fn filters_on_urgency_computed_from_the_given_clock() {
        let overdue = FrontierItem { deadline: Some("2020-01-01".into()), ..item("a") };
        let picked = toggle_facet(&FacetSelection::default(), Facet::Urgency, "overdue");

        assert!(matches_facets(&overdue, &picked, NOW));
        assert!(!matches_facets(&overdue, &picked, "2019-01-01T12:00"));
    }

    #[test]
    fn toggle_facet_adds_then_removes_a_value_without_mutating_what_it_was_given() {
        let base = FacetSelection::default();
        let once = toggle_facet(&base, Facet::Size, "deep");
        assert_eq!(once.size.len(), 1);
        assert!(base.size.is_empty());

        let twice = toggle_facet(&once, Facet::Size, "deep");
        assert!(twice.size.is_empty());
        assert_eq!(once.size.len(), 1);
    }

    #[test]
    fn facet_count_counts_picked_values_across_every_facet() {
        let mut picked = FacetSelection::default();
        assert_eq!(facet_count(&picked), 0);
        picked = toggle_facet(&picked, Facet::Context, "@computer");
        assert_eq!(facet_count(&picked), 1);
        picked = toggle_facet(&picked, Facet::Context, "@phone");
        picked = toggle_facet(&picked, Facet::Urgency, "overdue");
        assert_eq!(facet_count(&picked), 3);
    }

    #[test]
    fn contexts_of_offers_the_known_vocabulary_first_extras_sorted_absent_last() {
        let items = vec![
            item("a"),
            FrontierItem { context: Some("@zeta".into()), ..item("b") },
            FrontierItem { context: Some("@phone".into()), ..item("c") },
            FrontierItem { context: Some("@computer".into()), ..item("d") },
            FrontierItem { context: Some("@alpha".into()), ..item("e") },
        ];

        assert_eq!(
            contexts_of(&items),
            vec!["@computer", "@phone", "@alpha", "@zeta", NO_CONTEXT],
        );
    }

    #[test]
    fn contexts_of_omits_the_absent_chip_when_every_item_names_one() {
        let items = vec![FrontierItem { context: Some("@computer".into()), ..item("a") }];
        assert_eq!(contexts_of(&items), vec!["@computer"]);
    }

    #[test]
    fn apply_facets_keeps_the_given_order() {
        let input = vec![
            FrontierItem { context: Some("@computer".into()), ..item("a") },
            FrontierItem { context: Some("@phone".into()), ..item("b") },
            FrontierItem { context: Some("@computer".into()), ..item("c") },
        ];
        let picked = toggle_facet(&FacetSelection::default(), Facet::Context, "@computer");

        assert_eq!(apply_facets(&input, &picked, NOW), vec!["a", "c"]);
    }
}
