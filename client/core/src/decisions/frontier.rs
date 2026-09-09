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
//! **Grouping is clockless for four of the five axes.** Context, project,
//! size and energy are not time-varying, and nothing about how they bucket
//! can depend on when the caller asked. `urgency` (the fifth axis, added by
//! ADR-0021 decision 1's own amendment) is time-varying by construction —
//! it *is* a reading of a deadline against a clock — so [`group_frontier`]
//! takes `now` as an argument. It is injected, never read ambiently, in
//! exactly the form [`super::urgency::compute_urgency`] and
//! [`matches_facets`] already take it: a deadline-shaped local wall-clock
//! string, resolved by the reader in its own zone, because this crate holds
//! no tzdb.
//!
//! **Grouping never re-sorts, with one named exception.** The caller has
//! already ordered with [`by_priority_then_due`], and every column preserves
//! that order — except the `calm` column on the `urgency` axis, which is
//! re-sorted by `created_at` in the direction [`CalmOrder`] names.
//!
//! **What that column actually holds, stated precisely, because the obvious
//! reading of it is wrong.** `calm` is not "the deadline-less items": it is
//! everything the world is not pressing on, which is *both* the items naming
//! no deadline at all *and* every item due beyond
//! `urgency::SOON_WINDOW_MINUTES` (three days). So the re-sort does discard
//! a real ordering fact — an Urgent item due in five days sits by its
//! capture date rather than by its priority, until it crosses into `soon`
//! and rejoins [`by_priority_then_due`]'s order. That cost is **accepted,
//! not overlooked** (the operator chose this shape, 2026-09-08): `calm` is
//! the column you read when nothing is pressing, and "what has been sitting
//! here longest" / "what just turned up" is the question being asked of it —
//! the priority ordering is still exactly one axis-switch away.
//!
//! **Everything here works over ids, not whole items.** A caller (the web
//! seam, later Android) already holds the full item; handing the boundary
//! only what a decision needs — an id plus the handful of fields the rule
//! reads — keeps the crossing cheap and keeps this module from redefining
//! `hummingbird_domain::Item`'s shape a second time. The caller maps the
//! returned ids back onto its own items.
//!
//! **The board also writes, since #801.** [`drop_edits`] answers what one
//! field a card dropped into a column takes from it — the inverse of
//! [`group_frontier`], and here for that reason: it reads the same axis
//! vocabulary, folds a blank the same way, and asks the grouping's own
//! [`axis_value`] whether the card is already in that column. ADR-0021
//! decision 9 states the rule, ADR-0025 why it is Rust. It decides only
//! *what to write*; the gesture that carries the card — the physics, the
//! hit-testing, the auto-scroll — stays per client, over measured boxes.

use std::cmp::Ordering;
use std::collections::HashSet;

use hummingbird_domain::{deadline_sort_key, shift, DurationUnit};

use super::urgency::{compute_urgency, UrgencyBand};

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
    /// Epoch milliseconds, `hummingbird_domain::Item::created_at` verbatim.
    /// Read by the `urgency` axis alone, to order the `calm` column (see
    /// [`CalmOrder`]); no other axis and no ordering or facet function
    /// touches it.
    pub created_at: i64,
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

/// The axes the frontier can be grouped by (ADR-0021 decision 1, as amended
/// by its own decision-1 amendment) — `project` answers *what does this
/// belong to*, `context`/`size`/`energy` answer *what can I do right now,
/// from where I am, with the time and energy I have*, and `urgency` answers
/// *what is the world pressing on me*.
///
/// Still deliberately not [`super::vocabulary::FRONTIER_AXES`], though the
/// two lists now overlap in every member but one: that list is the *facet*
/// vocabulary and has no `project`, because a project column already
/// isolates one project. Urgency, which that comment used to give as the
/// other half of the same asymmetry, is now offered both ways — the
/// argument that colour already carries it holds against a *redundant*
/// encoding, not against grouping by it, and a reader who wants the board
/// partitioned by pressure could not get there from a filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontierAxis {
    Context,
    Project,
    Size,
    Energy,
    Urgency,
}

impl FrontierAxis {
    pub fn as_str(self) -> &'static str {
        match self {
            FrontierAxis::Context => "context",
            FrontierAxis::Project => "project",
            FrontierAxis::Size => "size",
            FrontierAxis::Energy => "energy",
            FrontierAxis::Urgency => "urgency",
        }
    }

    pub fn parse(s: &str) -> Option<FrontierAxis> {
        FRONTIER_GROUP_AXES.into_iter().find(|axis| axis.as_str() == s)
    }
}

/// Every grouping axis, in the order the switch offers them — `context`
/// leads because it is the default, `urgency` is last because it is the
/// newest and the switch's order is not a ranking.
pub const FRONTIER_GROUP_AXES: [FrontierAxis; 5] = [
    FrontierAxis::Context,
    FrontierAxis::Project,
    FrontierAxis::Size,
    FrontierAxis::Energy,
    FrontierAxis::Urgency,
];

pub const DEFAULT_FRONTIER_AXIS: FrontierAxis = FrontierAxis::Context;

/// Which way the `urgency` axis's `calm` column reads. That column is the
/// board's "nothing is pressing" pile — every deadline-less item, plus
/// everything due beyond the `soon` window — and the two useful readings of
/// such a pile are opposites: oldest-first is the backlog ("what has been
/// sitting here"), newest-first is the inbox ("what just turned up").
/// Neither is right enough to hard-code, so the reader picks. See the module
/// header for what ordering by arrival costs, and why it is accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalmOrder {
    Oldest,
    Newest,
}

impl CalmOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            CalmOrder::Oldest => "oldest",
            CalmOrder::Newest => "newest",
        }
    }

    pub fn parse(s: &str) -> Option<CalmOrder> {
        CALM_ORDERS.into_iter().find(|order| order.as_str() == s)
    }
}

pub const CALM_ORDERS: [CalmOrder; 2] = [CalmOrder::Oldest, CalmOrder::Newest];

/// Oldest first, matching [`super::queue::order_triage`] — the app's other
/// `created_at` ordering, and the same claim: the thing that has waited
/// longest is the thing most easily forgotten.
pub const DEFAULT_CALM_ORDER: CalmOrder = CalmOrder::Oldest;

/// The `urgency` axis's columns, in the order they are always emitted —
/// severity, never [`group_frontier`]'s fullest-first. A band's size is not
/// what makes it worth reading first, and a board whose column order moved
/// as items crossed band boundaries would re-arrange itself under the
/// reader for reasons they did not cause. Empty bands are omitted, not
/// rendered blank.
const URGENCY_COLUMN_ORDER: [UrgencyBand; 4] =
    [UrgencyBand::Overdue, UrgencyBand::Now, UrgencyBand::Soon, UrgencyBand::Calm];

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

fn raw_axis_value<'a>(item: &'a FrontierItem, axis: FrontierAxis, now: &str) -> Option<&'a str> {
    match axis {
        FrontierAxis::Context => item.context.as_deref(),
        FrontierAxis::Project => item.project_id.as_deref(),
        FrontierAxis::Size => item.size.as_deref(),
        FrontierAxis::Energy => item.energy.as_deref(),
        // Total, and that is the point: `compute_urgency` answers `calm`
        // for an item with no deadline and for one whose deadline will not
        // resolve, so this axis has no no-value bucket at all — the one
        // axis where ADR-0021's "no-value column always last" never fires.
        FrontierAxis::Urgency => Some(compute_urgency(item.deadline.as_deref(), now).as_str()),
    }
}

/// An empty string is not a value, and folds into the no-value bucket —
/// `items.context` is free text with no empty-string rejection on the write
/// path, so without this fold an API writer could land a *second* no-value
/// column sharing the caller's `value ?? ""` key.
fn axis_value(item: &FrontierItem, axis: FrontierAxis, now: &str) -> Option<String> {
    match raw_axis_value(item, axis, now) {
        None => None,
        Some("") => None,
        Some(v) => Some(v.to_string()),
    }
}

/// Columns for one axis: fullest first, and the no-value column always
/// last. Ties are broken by first appearance in `items`, and within a
/// bucket the input order is preserved — the caller has already ordered
/// with [`by_priority_then_due`], and grouping never re-sorts.
///
/// **The `urgency` axis is the exception to both sentences**, and to
/// nothing else: its columns come out in [`URGENCY_COLUMN_ORDER`] rather
/// than fullest-first, it has no no-value column to place last, and its
/// `calm` column is re-sorted by `created_at` per `calm_order`. See the
/// module header for why that exception is worth having.
///
/// `now` is a deadline-shaped local wall-clock string, read by the
/// `urgency` axis alone; the other four ignore it entirely and stay
/// clockless. `calm_order` is likewise read only by `urgency`.
pub fn group_frontier(
    items: &[FrontierItem],
    axis: FrontierAxis,
    projects: &[ProjectName],
    now: &str,
    calm_order: CalmOrder,
) -> Vec<FrontierColumn> {
    let mut order: Vec<Option<String>> = Vec::new();
    let mut buckets: std::collections::HashMap<Option<String>, Vec<String>> =
        std::collections::HashMap::new();

    for item in items {
        let value = axis_value(item, axis, now);
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

    if axis == FrontierAxis::Urgency {
        return order_urgency_columns(columns, items, calm_order);
    }

    let mut named: Vec<FrontierColumn> = Vec::new();
    let mut unnamed: Vec<FrontierColumn> = Vec::new();
    for column in columns.drain(..) {
        if column.value.is_some() {
            named.push(column);
        } else {
            unnamed.push(column);
        }
    }
    named.sort_by_key(|c| std::cmp::Reverse(c.ids.len()));

    named.into_iter().chain(unnamed).collect()
}

/// The `urgency` axis's own column and within-column order: bands in
/// [`URGENCY_COLUMN_ORDER`], and the `calm` column re-sorted by
/// `created_at` with `id` as the tiebreak — ascending both ways, exactly as
/// [`super::queue::order_triage`] breaks its own tie, so the direction
/// changes which item leads and never which of two same-instant items does.
fn order_urgency_columns(
    mut columns: Vec<FrontierColumn>,
    items: &[FrontierItem],
    calm_order: CalmOrder,
) -> Vec<FrontierColumn> {
    let created: std::collections::HashMap<&str, i64> =
        items.iter().map(|item| (item.id.as_str(), item.created_at)).collect();

    for column in columns.iter_mut() {
        if column.value.as_deref() != Some(UrgencyBand::Calm.as_str()) {
            continue;
        }
        column.ids.sort_by(|a, b| {
            let (left, right) = (
                created.get(a.as_str()).copied().unwrap_or_default(),
                created.get(b.as_str()).copied().unwrap_or_default(),
            );
            match calm_order {
                CalmOrder::Oldest => left.cmp(&right),
                CalmOrder::Newest => right.cmp(&left),
            }
            .then_with(|| a.cmp(b))
        });
    }

    columns.sort_by_key(|column| {
        URGENCY_COLUMN_ORDER
            .iter()
            .position(|band| Some(band.as_str()) == column.value.as_deref())
            .unwrap_or(usize::MAX)
    });
    columns
}


// ------------------------------------------------------------ the drop

/// One project's id and the context it lends to an item dropped into its
/// column — ADR-0030 decision 3's copy ("a project's default context is
/// written onto an action that names none"), reached from the board rather
/// than from `/to-actions`. Deliberately not a widening of [`ProjectName`]:
/// that type labels a column and is read on every render, this one is read
/// only when a card lands in a project column, and one type serving both
/// would make every grouping call carry a field it never looks at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDefault {
    pub id: String,
    pub default_context: Option<String>,
}

/// What one drop writes, in [`crate::TriagePatch`]'s own shape: the outer
/// `Option` says whether the field was touched at all, the inner one is the
/// value, so `Some(None)` clears the field and `None` leaves it alone.
/// Never more than one field, except the project axis's context copy — see
/// [`drop_edits`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DropEdits {
    pub context: Option<Option<String>>,
    pub size: Option<Option<String>>,
    pub energy: Option<Option<String>>,
    pub project_id: Option<Option<String>>,
    pub deadline: Option<Option<String>>,
}

/// A card dropped into a column takes that column's value (ADR-0021
/// decision 9). `target` is the column's `value` — `None` for the no-value
/// column, which *clears* the field rather than refusing. `None` comes back
/// for a drop that writes nothing: the item's own column (nothing to do),
/// or a refused one.
///
/// **This is a core decision, not a web one** (ADR-0025). The mapping reads
/// the same axis vocabulary and the same urgency arithmetic
/// [`group_frontier`] already owns — the same-column check is literally
/// [`axis_value`], the grouping's own reader, so "already there" can never
/// drift from where the board actually drew the card — and both clients
/// reach it through their seam. What stays per-client is the gesture: the
/// physics, the hit-testing and the auto-scroll all consume measured boxes,
/// which is the `frontier-lanes.rs` argument.
///
/// **`urgency` is the one axis whose drop writes a field other than the one
/// it groups by.** The other four group by a field and set that field;
/// urgency groups by a *reading of* `deadline` against `now`, so a drop
/// there has to invent a deadline that reads back as the band dropped into:
///
/// - `now` → today, which resolves to `T23:59` and so reads inside the
///   24-hour `now` window from any moment of the day;
/// - `soon` → today + 2 civil days. **Not +1**: a day-grained deadline
///   resolves to end-of-day, so tomorrow is under 24 hours away for
///   anything after `T00:00` and would read back as `now` for most of the
///   day. +2 is the smallest shift that reads `soon` at every hour.
/// - `calm` → cleared. Every deadline-less item is calm, and clearing is
///   the only edit that reads `calm` for certain (a distant deadline would
///   too, but the board must not invent one).
/// - `overdue` → refused. It is the one band that is a *fact about the
///   past*, and the app has no reason to let a reader manufacture one; the
///   card springs home instead, and no column is ever painted as refusing.
pub fn drop_edits(
    item: &FrontierItem,
    axis: FrontierAxis,
    target: Option<&str>,
    projects: &[ProjectDefault],
    now: &str,
) -> Option<DropEdits> {
    // The grouping's own `""`→`None` fold included, so a column drawn from
    // a blank field is the same column this comparison sees.
    let target = match target {
        Some("") | None => None,
        Some(v) => Some(v),
    };
    if axis_value(item, axis, now).as_deref() == target {
        return None;
    }

    let value = || Some(target.map(str::to_string));
    let edits = match axis {
        FrontierAxis::Context => DropEdits { context: value(), ..DropEdits::default() },
        FrontierAxis::Size => DropEdits { size: value(), ..DropEdits::default() },
        FrontierAxis::Energy => DropEdits { energy: value(), ..DropEdits::default() },
        FrontierAxis::Project => {
            let mut edits = DropEdits { project_id: value(), ..DropEdits::default() };
            // The one exception to "a drop writes one field", and the first
            // time ADR-0030 decision 3's copy exists in Rust: an item with
            // no context of its own takes the project's, so an action
            // dragged into a project is as complete as one `/to-actions`
            // minted there. An item that already names a context keeps it —
            // the reader's own answer outranks the project's default.
            if item.context.as_deref().unwrap_or("").is_empty() {
                if let Some(default) = target
                    .and_then(|id| projects.iter().find(|p| p.id == id))
                    .and_then(|p| p.default_context.as_deref())
                    .filter(|c| !c.is_empty())
                {
                    edits.context = Some(Some(default.to_string()));
                }
            }
            edits
        }
        FrontierAxis::Urgency => {
            let band = URGENCY_COLUMN_ORDER.into_iter().find(|b| Some(b.as_str()) == target)?;
            let deadline = match band {
                UrgencyBand::Overdue => return None,
                UrgencyBand::Calm => None,
                UrgencyBand::Now => Some(now.get(..10)?.to_string()),
                UrgencyBand::Soon => Some(shift(now.get(..10)?, 2, DurationUnit::Days)?),
            };
            DropEdits { deadline: Some(deadline), ..DropEdits::default() }
        }
    };
    Some(edits)
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

/// Contexts actually present in `items`, `suggested` first (in its own
/// order — the operator's list, `crate::contexts`, or the build's
/// `DEFAULT_CONTEXTS` until it is edited; ADR-0038), then any extra
/// alphabetically, then [`NO_CONTEXT`] last if anything names no context.
///
/// "Present" is judged by [`crate::contexts::same_context`] — the ranker's
/// rule, and the one the list's item counts and its removal cascade use —
/// so a list entry `@errands` orders the chip an item spells `@Errands`.
/// The chip's *value* stays the item's own spelling, because a facet
/// filters by exact value; two spellings of one context are two chips,
/// both placed at the entry's slot.
pub fn contexts_of(items: &[FrontierItem], suggested: &[String]) -> Vec<String> {
    let present: HashSet<&str> =
        items.iter().map(|item| item.context.as_deref().unwrap_or(NO_CONTEXT)).collect();

    let mut ordered: Vec<String> = Vec::new();
    for entry in suggested {
        let mut matching: Vec<&str> = present
            .iter()
            .copied()
            .filter(|c| *c != NO_CONTEXT && crate::contexts::same_context(entry, c))
            .collect();
        matching.sort();
        ordered.extend(matching.into_iter().map(|c| c.to_string()));
    }

    let mut extra: Vec<&str> = present
        .iter()
        .copied()
        .filter(|c| *c != NO_CONTEXT && !ordered.iter().any(|o| o == c))
        .collect();
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
            created_at: 0,
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

    /// The deadline-shaped wall clock every test below reads against — the
    /// `urgency` axis's bands, and the facet tests' own, are all stated
    /// relative to this instant.
    const NOW: &str = "2026-08-13T12:00";

    #[test]
    fn collects_items_by_their_value_on_the_live_axis_preserving_input_order() {
        for axis in FRONTIER_GROUP_AXES {
            // `urgency` has no field to set — its value is derived from the
            // deadline, and its bucketing, column order and `calm` ordering
            // are asserted by the tests that follow this section instead.
            if axis == FrontierAxis::Urgency {
                continue;
            }
            let field = |v: &str| -> FrontierItem {
                let mut it = item("x");
                match axis {
                    FrontierAxis::Context => it.context = Some(v.to_string()),
                    FrontierAxis::Project => it.project_id = Some(v.to_string()),
                    FrontierAxis::Size => it.size = Some(v.to_string()),
                    FrontierAxis::Energy => it.energy = Some(v.to_string()),
                    FrontierAxis::Urgency => unreachable!("skipped above"),
                }
                it
            };
            let a = FrontierItem { id: "a".into(), ..field("x") };
            let b = FrontierItem { id: "b".into(), ..field("y") };
            let c = FrontierItem { id: "c".into(), ..field("x") };

            let columns = group_frontier(&[a, b, c], axis, &[], NOW, DEFAULT_CALM_ORDER);

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

        let columns = group_frontier(&[unnamed, named], FrontierAxis::Context, &[], NOW, DEFAULT_CALM_ORDER);

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

        let columns = group_frontier(&[thin, fat1, fat2], FrontierAxis::Context, &[], NOW, DEFAULT_CALM_ORDER);

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
            group_frontier(&[unnamed1, unnamed2, unnamed3, named], FrontierAxis::Context, &[], NOW, DEFAULT_CALM_ORDER);

        assert_eq!(columns[0].value.as_deref(), Some("@computer"));
        assert_eq!(columns[1].value, None);
        assert_eq!(columns[1].ids.len(), 3);
    }

    #[test]
    fn breaks_a_tie_between_equal_columns_by_first_appearance() {
        let first = FrontierItem { context: Some("@phone".into()), ..item("a") };
        let second = FrontierItem { context: Some("@computer".into()), ..item("b") };

        let columns = group_frontier(&[first, second], FrontierAxis::Context, &[], NOW, DEFAULT_CALM_ORDER);

        assert_eq!(columns[0].value.as_deref(), Some("@phone"));
        assert_eq!(columns[1].value.as_deref(), Some("@computer"));
    }

    #[test]
    fn folds_an_empty_string_axis_value_into_the_no_value_column() {
        let empty = FrontierItem { context: Some("".into()), ..item("a") };
        let absent = item("b");
        let named = FrontierItem { context: Some("x".into()), ..item("c") };

        let columns = group_frontier(&[empty, absent, named], FrontierAxis::Context, &[], NOW, DEFAULT_CALM_ORDER);

        assert_eq!(columns[0].value.as_deref(), Some("x"));
        assert_eq!(columns[1].value, None);
        assert_eq!(columns[1].ids, vec!["a", "b"]);
    }

    #[test]
    fn resolves_a_project_columns_real_name_from_the_given_project_list() {
        let a = FrontierItem { project_id: Some("p-1".into()), ..item("a") };
        let projects = [ProjectName { id: "p-1".into(), name: "Ship the release".into() }];

        let columns = group_frontier(&[a], FrontierAxis::Project, &projects, NOW, DEFAULT_CALM_ORDER);

        assert_eq!(columns[0].value.as_deref(), Some("p-1"));
        assert_eq!(columns[0].label.as_deref(), Some("Ship the release"));
    }

    #[test]
    fn falls_back_to_a_null_label_for_a_project_id_not_yet_known() {
        let a = FrontierItem { project_id: Some("unknown-id".into()), ..item("a") };
        let projects = [ProjectName { id: "p-1".into(), name: "Ship the release".into() }];

        let columns = group_frontier(&[a], FrontierAxis::Project, &projects, NOW, DEFAULT_CALM_ORDER);

        assert_eq!(columns[0].label, None);
    }

    #[test]
    fn labels_every_other_axis_with_the_value_itself() {
        let a = FrontierItem { context: Some("raw-value".into()), ..item("a") };
        let projects = [ProjectName { id: "raw-value".into(), name: "Not this".into() }];

        let columns = group_frontier(&[a], FrontierAxis::Context, &projects, NOW, DEFAULT_CALM_ORDER);

        assert_eq!(columns[0].label.as_deref(), Some("raw-value"));
    }

    #[test]
    fn returns_no_columns_for_an_empty_frontier() {
        for axis in FRONTIER_GROUP_AXES {
            assert!(group_frontier(&[], axis, &[], NOW, DEFAULT_CALM_ORDER).is_empty());
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
                group_frontier(&input, axis, &[], NOW, DEFAULT_CALM_ORDER).into_iter().flat_map(|c| c.ids).collect();
            ids.sort();
            assert_eq!(ids, vec!["a", "b", "c", "d"]);
        }
    }

    // ------------------------------------------------- group_frontier: urgency
    // ADR-0021 decision 1's amendment: the fifth axis.

    /// Bands against `NOW` (`2026-08-13T12:00`): overdue is any past
    /// deadline, `now` is inside 24h, `soon` inside 3 days, `calm` beyond
    /// that — or no deadline at all.
    fn dated(id: &str, deadline: &str) -> FrontierItem {
        FrontierItem { deadline: Some(deadline.to_string()), ..item(id) }
    }

    #[test]
    fn urgency_emits_its_bands_in_severity_order_never_fullest_first() {
        // `calm` is deliberately the fullest, so fullest-first would put it
        // first if the axis had not opted out of that rule.
        let input = vec![
            item("calm-1"),
            item("calm-2"),
            item("calm-3"),
            dated("soon", "2026-08-15T12:00"),
            dated("now", "2026-08-13T18:00"),
            dated("overdue", "2026-08-12T12:00"),
        ];

        let columns = group_frontier(&input, FrontierAxis::Urgency, &[], NOW, DEFAULT_CALM_ORDER);

        assert_eq!(
            columns.iter().map(|c| c.value.clone()).collect::<Vec<_>>(),
            vec![
                Some("overdue".to_string()),
                Some("now".to_string()),
                Some("soon".to_string()),
                Some("calm".to_string()),
            ],
        );
    }

    #[test]
    fn urgency_omits_a_band_nothing_is_in() {
        let columns = group_frontier(
            &[dated("overdue", "2026-08-12T12:00"), item("calm")],
            FrontierAxis::Urgency,
            &[],
            NOW,
            DEFAULT_CALM_ORDER,
        );

        assert_eq!(
            columns.iter().map(|c| c.value.clone()).collect::<Vec<_>>(),
            vec![Some("overdue".to_string()), Some("calm".to_string())],
        );
    }

    #[test]
    fn urgency_never_yields_a_no_value_column() {
        // The one axis where ADR-0021's "no-value column always last" never
        // fires: `compute_urgency` is total, so a deadline-less item and one
        // whose deadline will not resolve both land in `calm` rather than in
        // a bucket of their own.
        let input = vec![item("no-deadline"), dated("unparseable", "sometime next week")];

        let columns = group_frontier(&input, FrontierAxis::Urgency, &[], NOW, DEFAULT_CALM_ORDER);

        assert!(columns.iter().all(|c| c.value.is_some()));
        assert_eq!(columns.len(), 1);
        assert_eq!(columns[0].value.as_deref(), Some("calm"));
        assert_eq!(columns[0].ids.len(), 2);
    }

    #[test]
    fn urgency_labels_each_band_by_its_own_wire_spelling() {
        let columns =
            group_frontier(&[dated("a", "2026-08-12T12:00")], FrontierAxis::Urgency, &[], NOW, DEFAULT_CALM_ORDER);

        assert_eq!(columns[0].label.as_deref(), Some("overdue"));
    }

    #[test]
    fn urgency_orders_the_calm_column_oldest_first_by_default() {
        let newest = FrontierItem { created_at: 3_000, ..item("c") };
        let oldest = FrontierItem { created_at: 1_000, ..item("a") };
        let middle = FrontierItem { created_at: 2_000, ..item("b") };

        let columns = group_frontier(
            &[newest, oldest, middle],
            FrontierAxis::Urgency,
            &[],
            NOW,
            DEFAULT_CALM_ORDER,
        );

        assert_eq!(columns[0].ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn urgency_reverses_the_calm_column_under_newest_first() {
        let newest = FrontierItem { created_at: 3_000, ..item("c") };
        let oldest = FrontierItem { created_at: 1_000, ..item("a") };
        let middle = FrontierItem { created_at: 2_000, ..item("b") };

        let columns = group_frontier(
            &[newest, oldest, middle],
            FrontierAxis::Urgency,
            &[],
            NOW,
            CalmOrder::Newest,
        );

        assert_eq!(columns[0].ids, vec!["c", "b", "a"]);
    }

    #[test]
    fn urgency_breaks_a_created_at_tie_by_id_in_both_directions() {
        let b = FrontierItem { created_at: 1_000, ..item("b") };
        let a = FrontierItem { created_at: 1_000, ..item("a") };

        for order in CALM_ORDERS {
            let columns =
                group_frontier(&[b.clone(), a.clone()], FrontierAxis::Urgency, &[], NOW, order);
            assert_eq!(columns[0].ids, vec!["a", "b"], "{}", order.as_str());
        }
    }

    #[test]
    fn the_calm_column_orders_a_far_future_deadline_by_arrival_too() {
        // The case the module header calls out: `calm` is not "the
        // deadline-less items", it is everything the world is not pressing
        // on — so a dated, prioritised item beyond the `soon` window is in
        // here with the undated ones and is ordered by arrival like them.
        // Pinned because it is the cost of this axis's one departure from
        // `by_priority_then_due`, and a silent change to it would look like
        // a bug fix.
        let urgent_but_distant = FrontierItem {
            priority: 1,
            deadline: Some("2026-09-30T12:00".into()),
            created_at: 5_000,
            ..item("distant")
        };
        let undated_older = FrontierItem { created_at: 1_000, ..item("undated") };

        let columns = group_frontier(
            &[urgent_but_distant, undated_older],
            FrontierAxis::Urgency,
            &[],
            NOW,
            DEFAULT_CALM_ORDER,
        );

        assert_eq!(columns.len(), 1);
        assert_eq!(columns[0].value.as_deref(), Some("calm"));
        // Arrival, not priority: the Urgent item is second because it is
        // newer, which `by_priority_then_due` would never do.
        assert_eq!(columns[0].ids, vec!["undated", "distant"]);
    }

    #[test]
    fn urgency_leaves_every_dated_band_in_the_callers_order() {
        // Only `calm` is re-sorted. The other three keep whatever order
        // `by_priority_then_due` gave the caller, `created_at` and the
        // direction notwithstanding.
        let first = FrontierItem { created_at: 9_000, ..dated("first", "2026-08-12T12:00") };
        let second = FrontierItem { created_at: 1_000, ..dated("second", "2026-08-12T09:00") };

        for order in CALM_ORDERS {
            let columns =
                group_frontier(&[first.clone(), second.clone()], FrontierAxis::Urgency, &[], NOW, order);
            assert_eq!(columns[0].ids, vec!["first", "second"], "{}", order.as_str());
        }
    }

    #[test]
    fn the_calm_order_vocabulary_round_trips_and_refuses_anything_else() {
        for order in CALM_ORDERS {
            assert_eq!(CalmOrder::parse(order.as_str()), Some(order));
        }
        assert_eq!(CalmOrder::parse("chronological"), None);
        assert_eq!(DEFAULT_CALM_ORDER, CalmOrder::Oldest);
    }

    #[test]
    fn urgency_is_a_grouping_axis_by_its_wire_spelling() {
        assert_eq!(FrontierAxis::parse("urgency"), Some(FrontierAxis::Urgency));
        assert_eq!(FrontierAxis::Urgency.as_str(), "urgency");
        assert!(FRONTIER_GROUP_AXES.contains(&FrontierAxis::Urgency));
    }

    // ----------------------------------------------------------- drop_edits
    // ADR-0021 decision 9: a card dropped into a column takes its value.

    fn projects_with(id: &str, default_context: Option<&str>) -> Vec<ProjectDefault> {
        vec![ProjectDefault {
            id: id.to_string(),
            default_context: default_context.map(str::to_string),
        }]
    }

    #[test]
    fn a_drop_sets_the_axis_field_to_the_column_it_landed_in() {
        let edits = drop_edits(&item("a"), FrontierAxis::Context, Some("@phone"), &[], NOW);
        assert_eq!(
            edits,
            Some(DropEdits { context: Some(Some("@phone".to_string())), ..Default::default() }),
        );

        let sized = FrontierItem { size: Some("deep".to_string()), ..item("a") };
        assert_eq!(
            drop_edits(&sized, FrontierAxis::Size, Some("quick"), &[], NOW),
            Some(DropEdits { size: Some(Some("quick".to_string())), ..Default::default() }),
        );

        let tired = FrontierItem { energy: Some("low".to_string()), ..item("a") };
        assert_eq!(
            drop_edits(&tired, FrontierAxis::Energy, Some("high"), &[], NOW),
            Some(DropEdits { energy: Some(Some("high".to_string())), ..Default::default() }),
        );
    }

    #[test]
    fn the_no_value_column_clears_the_field_rather_than_refusing_the_drop() {
        let carried = FrontierItem {
            context: Some("@phone".to_string()),
            size: Some("quick".to_string()),
            energy: Some("high".to_string()),
            project_id: Some("p1".to_string()),
            ..item("a")
        };

        assert_eq!(
            drop_edits(&carried, FrontierAxis::Context, None, &[], NOW),
            Some(DropEdits { context: Some(None), ..Default::default() }),
        );
        assert_eq!(
            drop_edits(&carried, FrontierAxis::Size, None, &[], NOW),
            Some(DropEdits { size: Some(None), ..Default::default() }),
        );
        assert_eq!(
            drop_edits(&carried, FrontierAxis::Energy, None, &[], NOW),
            Some(DropEdits { energy: Some(None), ..Default::default() }),
        );
        assert_eq!(
            drop_edits(&carried, FrontierAxis::Project, None, &[], NOW),
            Some(DropEdits { project_id: Some(None), ..Default::default() }),
        );
    }

    #[test]
    fn a_card_dropped_back_into_its_own_column_writes_nothing() {
        let carried = FrontierItem {
            context: Some("@phone".to_string()),
            size: Some("quick".to_string()),
            energy: Some("high".to_string()),
            project_id: Some("p1".to_string()),
            ..item("a")
        };

        assert_eq!(drop_edits(&carried, FrontierAxis::Context, Some("@phone"), &[], NOW), None);
        assert_eq!(drop_edits(&carried, FrontierAxis::Size, Some("quick"), &[], NOW), None);
        assert_eq!(drop_edits(&carried, FrontierAxis::Energy, Some("high"), &[], NOW), None);
        assert_eq!(drop_edits(&carried, FrontierAxis::Project, Some("p1"), &[], NOW), None);
        // The no-value column is a column too, and an item already in it is
        // already there — the `""`→`None` fold on both sides included.
        assert_eq!(drop_edits(&item("a"), FrontierAxis::Context, None, &[], NOW), None);
        let blank = FrontierItem { context: Some(String::new()), ..item("a") };
        assert_eq!(drop_edits(&blank, FrontierAxis::Context, Some(""), &[], NOW), None);
    }

    #[test]
    fn a_project_drop_copies_the_projects_default_context_onto_a_contextless_item() {
        assert_eq!(
            drop_edits(
                &item("a"),
                FrontierAxis::Project,
                Some("p1"),
                &projects_with("p1", Some("@desk")),
                NOW,
            ),
            Some(DropEdits {
                project_id: Some(Some("p1".to_string())),
                context: Some(Some("@desk".to_string())),
                ..Default::default()
            }),
        );
    }

    #[test]
    fn the_default_context_copy_never_overwrites_a_context_the_item_already_names() {
        let placed = FrontierItem { context: Some("@phone".to_string()), ..item("a") };
        assert_eq!(
            drop_edits(
                &placed,
                FrontierAxis::Project,
                Some("p1"),
                &projects_with("p1", Some("@desk")),
                NOW,
            ),
            Some(DropEdits { project_id: Some(Some("p1".to_string())), ..Default::default() }),
        );
    }

    #[test]
    fn the_default_context_copy_needs_a_project_that_names_one() {
        // No default on the project it landed in, an unknown project, and
        // the no-project column: the project id moves, nothing else does.
        for projects in [projects_with("p1", None), projects_with("other", Some("@desk")), vec![]] {
            assert_eq!(
                drop_edits(&item("a"), FrontierAxis::Project, Some("p1"), &projects, NOW),
                Some(DropEdits { project_id: Some(Some("p1".to_string())), ..Default::default() }),
            );
        }
        let placed = FrontierItem { project_id: Some("p1".to_string()), ..item("a") };
        assert_eq!(
            drop_edits(&placed, FrontierAxis::Project, None, &projects_with("p1", Some("@desk")), NOW),
            Some(DropEdits { project_id: Some(None), ..Default::default() }),
        );
    }

    #[test]
    fn overdue_is_the_one_column_no_drop_may_land_in() {
        assert_eq!(drop_edits(&item("a"), FrontierAxis::Urgency, Some("overdue"), &[], NOW), None);
        // And a band the vocabulary does not know is refused the same way,
        // rather than writing a deadline of that word.
        assert_eq!(drop_edits(&item("a"), FrontierAxis::Urgency, Some("later"), &[], NOW), None);
    }

    #[test]
    fn an_urgency_drop_writes_a_deadline_that_reads_back_as_the_band_dropped_into() {
        // Every hour of the day, not just noon: the +2 for `soon` exists
        // precisely because a day-grained deadline resolves to `T23:59`.
        for now in ["2026-08-13T00:01", "2026-08-13T12:00", "2026-08-13T23:58"] {
            for band in [UrgencyBand::Now, UrgencyBand::Soon] {
                let edits = drop_edits(&item("a"), FrontierAxis::Urgency, Some(band.as_str()), &[], now)
                    .expect("a legal band writes a deadline");
                let deadline = edits.deadline.clone().expect("the deadline is touched");
                assert_eq!(
                    compute_urgency(deadline.as_deref(), now),
                    band,
                    "{band:?} at {now} wrote {deadline:?}",
                );
            }
        }

        assert_eq!(
            drop_edits(&item("a"), FrontierAxis::Urgency, Some("now"), &[], NOW),
            Some(DropEdits {
                deadline: Some(Some("2026-08-13".to_string())),
                ..Default::default()
            }),
        );
        assert_eq!(
            drop_edits(&item("a"), FrontierAxis::Urgency, Some("soon"), &[], NOW),
            Some(DropEdits {
                deadline: Some(Some("2026-08-15".to_string())),
                ..Default::default()
            }),
        );
    }

    #[test]
    fn the_two_day_shift_rolls_over_a_month_end() {
        assert_eq!(
            drop_edits(&item("a"), FrontierAxis::Urgency, Some("soon"), &[], "2026-08-30T09:00"),
            Some(DropEdits {
                deadline: Some(Some("2026-09-01".to_string())),
                ..Default::default()
            }),
        );
    }

    #[test]
    fn dropping_into_calm_clears_the_deadline_and_a_calm_item_stays_put() {
        let dated = dated("a", "2026-08-13T18:00");
        assert_eq!(
            drop_edits(&dated, FrontierAxis::Urgency, Some("calm"), &[], NOW),
            Some(DropEdits { deadline: Some(None), ..Default::default() }),
        );
        assert_eq!(drop_edits(&item("a"), FrontierAxis::Urgency, Some("calm"), &[], NOW), None);
    }

    /// The gate that keeps [`drop_edits`] and [`group_frontier`] from
    /// drifting apart: for every axis and every column the board can draw,
    /// apply the edits and re-group — the item must land in exactly the
    /// column it was dropped into. A rule stated twice would pass every
    /// test above and still put the card back where it came from.
    #[test]
    fn an_edited_item_regroups_into_the_column_it_was_dropped_into() {
        let projects = projects_with("p1", Some("@desk"));
        let names = vec![ProjectName { id: "p1".to_string(), name: "Kitchen".to_string() }];
        let source = FrontierItem {
            context: Some("@phone".to_string()),
            size: Some("deep".to_string()),
            energy: Some("low".to_string()),
            project_id: Some("p0".to_string()),
            deadline: Some("2026-08-12T12:00".to_string()),
            ..item("a")
        };

        for axis in FRONTIER_GROUP_AXES {
            let targets: Vec<Option<&str>> = match axis {
                FrontierAxis::Context => vec![Some("@desk"), Some("@errand"), None],
                FrontierAxis::Project => vec![Some("p1"), None],
                FrontierAxis::Size => vec![Some("quick"), Some("normal"), None],
                FrontierAxis::Energy => vec![Some("high"), Some("medium"), None],
                // `overdue` is refused, and the source is already overdue,
                // so the three writable bands are the whole set here.
                FrontierAxis::Urgency => vec![Some("now"), Some("soon"), Some("calm")],
            };

            for target in targets {
                let Some(edits) = drop_edits(&source, axis, target, &projects, NOW) else {
                    panic!("{axis:?} refused a drop into {target:?}");
                };
                let mut patched = source.clone();
                if let Some(v) = edits.context {
                    patched.context = v;
                }
                if let Some(v) = edits.size {
                    patched.size = v;
                }
                if let Some(v) = edits.energy {
                    patched.energy = v;
                }
                if let Some(v) = edits.project_id {
                    patched.project_id = v;
                }
                if let Some(v) = edits.deadline {
                    patched.deadline = v;
                }

                let columns =
                    group_frontier(&[patched], axis, &names, NOW, DEFAULT_CALM_ORDER);
                assert_eq!(columns.len(), 1, "{axis:?} → {target:?}");
                assert_eq!(
                    columns[0].value.as_deref(),
                    target,
                    "{axis:?} → {target:?} landed in {:?}",
                    columns[0].value,
                );
            }
        }
    }


    // -------------------------------------------------------------------- facets
    // Ported from `frontier-facets.test.ts`.

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
            contexts_of(&items, &crate::contexts::default_contexts()),
            vec!["@computer", "@phone", "@alpha", "@zeta", NO_CONTEXT],
        );
    }

    #[test]
    fn contexts_of_places_a_spelling_variant_at_its_entry_s_slot() {
        // ADR-0038: the list says `@errands`; an item says `@Errands`. The
        // chip keeps the item's spelling (a facet filters by exact value)
        // but sorts where the list puts it, not among the extras.
        let mut a = item("a");
        a.context = Some("@Errands".to_string());
        let mut b = item("b");
        b.context = Some("@alpha".to_string());
        let mut c = item("c");
        c.context = Some("@home".to_string());
        let suggested = vec!["@home".to_string(), "@errands".to_string()];
        assert_eq!(contexts_of(&[a, b, c], &suggested), vec!["@home", "@Errands", "@alpha"]);
    }

    #[test]
    fn contexts_of_omits_the_absent_chip_when_every_item_names_one() {
        let items = vec![FrontierItem { context: Some("@computer".into()), ..item("a") }];
        assert_eq!(contexts_of(&items, &crate::contexts::default_contexts()), vec!["@computer"]);
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
