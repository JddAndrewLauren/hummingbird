//! [`SyncMirror`]: the device's reconciling local read model of the owned
//! workspace (ADR-0008/0009), built from [`ChangesResponse`] pages.
//!
//! Two apply paths, one rule (ADR-0008 "reads: delta pull, full sweep as
//! backstop", ADR-0007's retention as it survives that amendment): a delta
//! pull is additive — it may upsert a row live or, on an explicit
//! soft-delete flag, mark it absent, but it never drops a row just because
//! this delta didn't mention it. Only [`SyncMirror::apply_sweep`], fed the
//! complete workspace, may demote a row by its absence from the response.
//!
//! **Retention, not deletion.** ADR-0008 dissolved absence-*inference* on
//! the wire (rows are never deleted server-side, only flagged), but ADR-0007
//! separately requires that the device's own read model "only grows or
//! updates" — reconciliation "never erases a record", so it "stays in the
//! snapshot forever" and every device stays a full export. A row that goes
//! absent (soft-delete flag, or missing from a complete sweep) is therefore
//! *retained* here with [`Presence::Absent`], not removed from the map —
//! the same rule [`crate::task::Mirror`] already enforces for the S1 model,
//! reused verbatim ([`crate::task::Presence`]) rather than redefined, since
//! it is a domain-agnostic notion. The live accessors (`item`, `all_items`,
//! `project`, `route`, `steps_for_item`, `blockers_of`) filter to
//! [`Presence::Live`] only; the `*_including_absent` accessors expose the
//! retained history for a caller — a debug view, a future undo — that needs
//! it.
//!
//! **Relationship to [`crate::task::Mirror`].** That type is the S1
//! (Linear-era) mirror; this one is its S2/#100 replacement for the owned
//! schema, built from `hummingbird_domain` types directly rather than a
//! Linear-shaped `Item`.
//!
//! **#103 resolves which mirror is persisted.** `SyncMirror` is
//! [`crate::storage::Persistable`] as of this issue — it is the read model
//! [`super::cycle::SyncCycle`] persists after every completed pull, and the
//! one going forward for the owned schema. `crate::task::Mirror` (the S1
//! Linear-era type, and its dead-letter journal, and the FFI-facing `Core`
//! handle built on it) is *not* migrated onto this type here — the two
//! mirrors coexist unchanged during the ADR-0008 cutover, and retiring the
//! Linear-era one is a separate, later concern this issue does not own.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use hummingbird_domain::{
    Alert, BlockedBy, ChangesResponse, ContextSnapshot, Fog, Item, Project, Route, Rule, Setting,
    Step,
};

use crate::storage::{Persistable, PersistableSealed};
use crate::task::Presence;

/// The schema version written into this mirror's snapshot envelope. Bump it
/// when the payload shape changes — see [`super::queue::QUEUE_SCHEMA_VERSION`]
/// for what that failure mode must never degrade into (this mirror included:
/// an empty result *is* harmless here, since the next sweep refills it, but
/// [`crate::storage::SnapshotError::Deserialize`] must still never be mapped
/// to `SyncMirror::default()` by a caller — that would discard retained
/// history no sweep can reconstruct, only refill going forward).
///
/// Bumped to 2 for #153's `domain::Item.due_date` → `.deadline` rename.
/// That rename is the shape change serde cannot catch: `deadline` is an
/// `Option<String>` and `Item` carries no `deny_unknown_fields`, so a v1
/// snapshot's stale `due_date` key is dropped and `deadline` comes back
/// `None` with no error at all. What converts that into a safe outcome is
/// not the constant on its own but the check against it in
/// [`super::cycle::SyncCycle::load`]: a stored mirror whose envelope
/// version is not this one is discarded and rebuilt by the next pull's
/// full-sweep backstop. Bumping this constant without that check would be
/// inert.
///
/// Bumped to 3 for #140's `rules` table: a new field on this struct, same
/// "the shape changed" reasoning as the bump to 2 — a v2 snapshot has no
/// `rules` key, and while serde would happily default it to an empty map,
/// that empty map would then look identical to "this device has synced and
/// genuinely has no rules," which the Rules screen cannot tell apart from
/// "this device hasn't re-swept since the field was added." Discarding and
/// resweeping is the same safe answer #153's bump already established.
pub const SYNC_MIRROR_SCHEMA_VERSION: u32 = 3;

/// One stored row plus whether it is currently live — the retained-history
/// half of the retention rule above.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Slot<T> {
    record: T,
    presence: Presence,
}

/// The device's local read model of every synced table, plus the delta
/// cursor (`meta.version` as of the last fully-applied pull).
///
/// **Persisted — the #103-resolved answer to the S2/#100 question this
/// module's docs used to leave open.** This is the mirror that gets
/// `Persistable` and carries the owned-schema read model forward; the
/// Linear-era [`crate::task::Mirror`] is not extended to the owned schema and
/// is left as-is for whatever cutover sequencing a later issue owns —
/// nothing here migrates its dead-letter journal or its FFI-facing `Core`
/// handle, only settles which *type* is the one going forward.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SyncMirror {
    version: i64,
    projects: BTreeMap<String, Slot<Project>>,
    routes: BTreeMap<String, Slot<Route>>,
    fog: BTreeMap<String, Slot<Fog>>,
    items: BTreeMap<String, Slot<Item>>,
    steps: BTreeMap<String, Slot<Step>>,
    /// `serde_json` cannot serialize a map keyed by a tuple (it requires
    /// string keys) — [`tuple_key_map`] carries this as an array of entries
    /// on the wire instead, same `BTreeMap` in memory.
    #[serde(with = "tuple_key_map")]
    blocked_by: BTreeMap<(String, String), Slot<BlockedBy>>,
    alerts: BTreeMap<String, Slot<Alert>>,
    #[serde(with = "tuple_key_map")]
    context_snapshots: BTreeMap<(String, String), Slot<ContextSnapshot>>,
    settings: BTreeMap<String, Slot<Setting>>,
    rules: BTreeMap<String, Slot<Rule>>,
}

impl PersistableSealed for SyncMirror {}
impl Persistable for SyncMirror {}

/// `serde(with = ...)` shim for a `BTreeMap` keyed by a tuple: serializes as
/// an array of `(key, value)` entries (which `serde_json` handles fine,
/// since only *map* serialization requires string keys) and rebuilds the
/// `BTreeMap` on the way back in.
mod tuple_key_map {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S, K, V>(map: &BTreeMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        K: Serialize + Ord,
        V: Serialize,
    {
        let entries: Vec<(&K, &V)> = map.iter().collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
    where
        D: Deserializer<'de>,
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
    {
        let entries: Vec<(K, V)> = Vec::deserialize(deserializer)?;
        Ok(entries.into_iter().collect())
    }
}

impl SyncMirror {
    pub fn new() -> Self {
        Self::default()
    }

    /// The delta cursor: the workspace `version` as of the last fully
    /// applied pull. `GET /api/changes?since=<version>` is the next call.
    pub fn version(&self) -> i64 {
        self.version
    }

    pub fn item(&self, id: &str) -> Option<&Item> {
        live(self.items.get(id))
    }

    /// The stored record regardless of presence — retained history, for a
    /// caller that needs it (a debug view, a future undo). Absence from
    /// this accessor, unlike [`SyncMirror::item`], means "never synced",
    /// not "archived".
    pub fn item_including_absent(&self, id: &str) -> Option<&Item> {
        self.items.get(id).map(|slot| &slot.record)
    }

    pub fn all_items(&self) -> impl Iterator<Item = &Item> {
        self.items.values().filter_map(live_slot)
    }

    /// Every stored item regardless of presence, id order, each paired with
    /// its [`Presence`] — [`SyncMirror::item_including_absent`]'s all-rows
    /// sibling, and the first realized caller of the retained history the
    /// module docs promised "a debug view" could have. The Ledger screen's
    /// roster read ([`crate::Core::ledger`]) starts here: an archived item is
    /// a row it *shows*, labelled, never one it hides.
    pub fn all_items_including_absent(&self) -> impl Iterator<Item = (&Item, &Presence)> {
        self.items
            .values()
            .map(|slot| (&slot.record, &slot.presence))
    }

    pub fn project(&self, id: &str) -> Option<&Project> {
        live(self.projects.get(id))
    }

    /// Every live project — grouping the frontier by project (issue #108,
    /// PR #200 review) needs a project's *name*, not just its id, and this
    /// is the read that resolves it: `TaskItemDTO.projectId` alone cannot
    /// name anything.
    pub fn all_projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.values().filter_map(live_slot)
    }

    /// Every stored project regardless of presence, id order, each paired
    /// with its [`Presence`] — [`SyncMirror::all_items_including_absent`]'s
    /// project-lane twin, and needed for the same reason the Ledger needed
    /// that one: **a project's `archived_at` is what demotes it here**
    /// (`apply_tables` passes it as `removed_since`), so an archived project
    /// is absent, and a surface that shows archived projects cannot be built
    /// on [`SyncMirror::all_projects`] at all. Absence has two causes and the
    /// caller must tell them apart — the record's own `archived_at`
    /// distinguishes an archived project from one that simply fell out of a
    /// complete sweep.
    pub fn all_projects_including_absent(&self) -> impl Iterator<Item = (&Project, &Presence)> {
        self.projects
            .values()
            .map(|slot| (&slot.record, &slot.presence))
    }

    pub fn route(&self, project_id: &str) -> Option<&Route> {
        live(self.routes.get(project_id))
    }

    /// Every live Step attached to `item_id`, id order — first-class
    /// records, never parsed from a body string.
    pub fn steps_for_item<'a>(&'a self, item_id: &'a str) -> impl Iterator<Item = &'a Step> {
        self.steps
            .values()
            .filter_map(live_slot)
            .filter(move |s| s.item_id == item_id)
    }

    /// Every id that blocks `item_id` — the direction the schema and the
    /// pull both fix: `item_id` is blocked by `blocker_id`. Getting this
    /// backwards inverts the frontier (which items are actionable).
    pub fn blockers_of<'a>(&'a self, item_id: &'a str) -> impl Iterator<Item = &'a str> {
        self.blocked_by
            .values()
            .filter_map(live_slot)
            .filter(move |edge| edge.item_id == item_id)
            .map(|edge| edge.blocker_id.as_str())
    }

    pub fn alert(&self, id: &str) -> Option<&Alert> {
        live(self.alerts.get(id))
    }

    /// One server-polled gauge, by its `(source, key)` identity — the row a
    /// standing question's pane reads (ADR-0015). Live only: a snapshot the
    /// server has stopped sending is demoted like every other record
    /// (ADR-0003), and reading a demoted gauge as a current answer is
    /// exactly the "quietly empty" ADR-0015 rules out.
    ///
    /// Takes the key by halves and allocates to look it up: the map is
    /// keyed by an owned tuple, which `BTreeMap` cannot probe with a
    /// `(&str, &str)`. Two allocations per pane read, against a map with a
    /// handful of entries.
    pub fn context_snapshot(&self, source: &str, key: &str) -> Option<&ContextSnapshot> {
        live(self
            .context_snapshots
            .get(&(source.to_string(), key.to_string())))
    }

    /// Every live snapshot row for one source, `key` order — the snapshot
    /// half of [`crate::Core::pane_read`] (#245, ADR-0015), where
    /// [`SyncMirror::context_snapshot`] is the point lookup beside it. A row
    /// the server has stopped sending is demoted like every other record
    /// (ADR-0003) and never appears.
    ///
    /// **This mirror has no clock**, so nothing here is filtered by time;
    /// freshness and alert liveness are applied by `Core::pane_read` from
    /// its caller-injected `now_ms`.
    pub fn context_snapshots_for<'a>(
        &'a self,
        source: &'a str,
    ) -> impl Iterator<Item = &'a ContextSnapshot> {
        self.context_snapshots
            .values()
            .filter_map(live_slot)
            .filter(move |snapshot| snapshot.source == source)
    }

    /// Every live alert raised by one source, id order — the alert half of
    /// [`crate::Core::pane_read`]. "Live" here is ADR-0003's presence only:
    /// ADR-0014's `is_live` predicate needs a clock this type does not have,
    /// so `Core::pane_read` applies it.
    pub fn alerts_for_source<'a>(&'a self, source: &'a str) -> impl Iterator<Item = &'a Alert> {
        self.alerts
            .values()
            .filter_map(live_slot)
            .filter(move |alert| alert.source == source)
    }

    /// Every live alert, id order — the whole-table read behind the Ledger's
    /// per-item alert badge, which joins on the alert's `source_key` across
    /// *every* source rather than one (unlike
    /// [`SyncMirror::alerts_for_source`], the per-source pane read). Live
    /// here is ADR-0003's presence only, as with `alerts_for_source`;
    /// ADR-0014's clocked `is_live` is the caller's to apply.
    pub fn all_alerts(&self) -> impl Iterator<Item = &Alert> {
        self.alerts.values().filter_map(live_slot)
    }

    pub fn setting(&self, key: &str) -> Option<&Setting> {
        live(self.settings.get(key))
    }

    /// Every live `settings` row, key order — the read behind #118's
    /// bindings editor. Deliberately the whole table rather than only the
    /// keys [`crate::bindings::BindingKey`] knows: a row a newer build
    /// wrote is still in the table, and a reader that filtered it out would
    /// show an editor claiming the table holds less than it does.
    pub fn all_settings(&self) -> impl Iterator<Item = &Setting> {
        self.settings.values().filter_map(live_slot)
    }

    pub fn rule(&self, id: &str) -> Option<&Rule> {
        live(self.rules.get(id))
    }

    /// Every live rule, id order — the read behind #140's rules screen.
    /// `rules` carries no soft-delete flag of its own (ADR-0003's
    /// retention still applies via a full sweep's absence-demotion, the
    /// same as `routes`/`fog`/`projects`).
    pub fn all_rules(&self) -> impl Iterator<Item = &Rule> {
        self.rules.values().filter_map(live_slot)
    }

    /// Live and not yet `Done` — the population ADR-0001's 250-issue
    /// watchline measures, ported to the owned schema (`crate::task::query`'s
    /// `active_count` is its S1/Linear-era twin).
    ///
    /// Surfaced as a query rather than left to be remembered: the watchline
    /// is a migration trigger, and a trigger nobody can see is not a
    /// trigger. [`super::cycle::SyncCycle`] exposes this after every cycle
    /// so a host can observe it without re-deriving the filter itself.
    pub fn active_item_count(&self) -> usize {
        self.all_items()
            .filter(|item| item.stage != hummingbird_domain::Stage::Done)
            .count()
    }

    /// Applies one delta pull: additive only. A row absent from `resp` is
    /// left exactly as it was; a row present with its soft-delete flag set
    /// is marked absent (retained, not removed — see the module docs) using
    /// the flag's own timestamp, since that is a more precise "since" than
    /// anything this call could invent. The cursor advances to
    /// `resp.version` only after every table below has been applied — built
    /// on a scratch copy and swapped in as the last step, so a caller that
    /// never observes the swap (a panic mid-apply) sees the previous,
    /// fully-consistent mirror, and a replay of the same delta is a
    /// harmless no-op.
    pub fn apply_delta(&mut self, resp: ChangesResponse) {
        let mut next = self.clone();
        next.apply_tables(resp, false, 0);
        *self = next;
    }

    /// Applies one full sweep: the complete workspace, as of `now_ms` (used
    /// only to stamp a row demoted purely by its absence from `resp` — a
    /// row's own soft-delete flag always wins when present, per
    /// [`SyncMirror::apply_delta`]). A row present is upserted live (or
    /// marked absent, on its soft-delete flag) exactly as a delta would; a
    /// row this device previously held live but that is missing from
    /// `resp` entirely is demoted, because the sweep is a complete picture
    /// and its absence is meaningful. Same panic-safety as `apply_delta`.
    pub fn apply_sweep(&mut self, resp: ChangesResponse, now_ms: i64) {
        let mut next = self.clone();
        next.apply_tables(resp, true, now_ms);
        *self = next;
    }

    fn apply_tables(&mut self, resp: ChangesResponse, full: bool, now_ms: i64) {
        apply_table(
            &mut self.projects,
            resp.projects,
            |p| p.id.clone(),
            |p| p.archived_at,
            full,
            now_ms,
        );
        apply_table(
            &mut self.routes,
            resp.routes,
            |r| r.project_id.clone(),
            |_| None,
            full,
            now_ms,
        );
        apply_table(
            &mut self.fog,
            resp.fog,
            |f| f.id.clone(),
            |_| None,
            full,
            now_ms,
        );
        apply_table(
            &mut self.items,
            resp.items,
            |i| i.id.clone(),
            |i| i.archived_at,
            full,
            now_ms,
        );
        apply_table(
            &mut self.steps,
            resp.steps,
            |s| s.id.clone(),
            |s| s.deleted_at,
            full,
            now_ms,
        );
        apply_table(
            &mut self.blocked_by,
            resp.blocked_by,
            |b| (b.item_id.clone(), b.blocker_id.clone()),
            |b| b.removed_at,
            full,
            now_ms,
        );
        apply_table(
            &mut self.alerts,
            resp.alerts,
            |a| a.id.clone(),
            |_| None,
            full,
            now_ms,
        );
        apply_table(
            &mut self.context_snapshots,
            resp.context_snapshots,
            |c| (c.source.clone(), c.key.clone()),
            |_| None,
            full,
            now_ms,
        );
        apply_table(
            &mut self.settings,
            resp.settings,
            |s| s.key.clone(),
            |_| None,
            full,
            now_ms,
        );
        apply_table(
            &mut self.rules,
            resp.rules,
            |r| r.id.clone(),
            |_| None,
            full,
            now_ms,
        );

        self.version = resp.version;
    }
}

fn live<T>(slot: Option<&Slot<T>>) -> Option<&T> {
    slot.and_then(live_slot)
}

fn live_slot<T>(slot: &Slot<T>) -> Option<&T> {
    slot.presence.is_live().then_some(&slot.record)
}

/// Applies one table's slice of a pull to `map`.
///
/// Every row in `incoming` is upserted: live if `removed_since` returns
/// `None`, absent (stamped with the returned timestamp) if it returns
/// `Some` — the additive half both a delta and a sweep share, and never a
/// removal from `map` itself (retention, per the module docs). When `full`
/// is set (a sweep), every key not touched by this call is demoted to
/// absent as of `now_ms` (preserving an earlier absence's original stamp):
/// the response was the complete workspace, so its absence is the
/// demotion signal. When `full` is unset (a delta), untouched keys are
/// left exactly as they were.
fn apply_table<K, T>(
    map: &mut BTreeMap<K, Slot<T>>,
    incoming: Vec<T>,
    key_of: impl Fn(&T) -> K,
    removed_since: impl Fn(&T) -> Option<i64>,
    full: bool,
    now_ms: i64,
) where
    K: Ord + Clone,
{
    let mut seen = std::collections::BTreeSet::new();
    for row in incoming {
        let key = key_of(&row);
        seen.insert(key.clone());
        let presence = match removed_since(&row) {
            Some(since_ms) => Presence::Absent { since_ms },
            None => Presence::Live,
        };
        map.insert(
            key,
            Slot {
                record: row,
                presence,
            },
        );
    }
    if full {
        for (key, slot) in map.iter_mut() {
            if !seen.contains(key) {
                slot.presence = slot.presence.demote(now_ms);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> Item {
        Item {
            id: id.to_string(),
            seq: Some(1),
            title: format!("item {id}"),
            description: None,
            stage: hummingbird_domain::Stage::Triage,
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
            created_at: 1,
            updated_at: 1,
            version: 1,
        }
    }

    fn project(id: &str) -> Project {
        Project {
            id: id.to_string(),
            name: format!("project {id}"),
            archived_at: None,
            created_at: 1,
            updated_at: 1,
            version: 1,
        }
    }

    fn route(project_id: &str) -> Route {
        Route {
            project_id: project_id.to_string(),
            destination: None,
            notes: None,
            updated_at: 1,
            version: 1,
        }
    }

    fn step(id: &str, item_id: &str) -> Step {
        Step {
            id: id.to_string(),
            item_id: item_id.to_string(),
            body: format!("step {id}"),
            done: false,
            position: 1,
            deleted_at: None,
            version: 1,
        }
    }

    fn blocked_by(item_id: &str, blocker_id: &str) -> BlockedBy {
        BlockedBy {
            item_id: item_id.to_string(),
            blocker_id: blocker_id.to_string(),
            version: 1,
            removed_at: None,
        }
    }

    /// A full fixture workspace touching every table, for the byte-identical
    /// full-sweep-vs-delta comparison.
    fn seeded_workspace(version: i64) -> ChangesResponse {
        ChangesResponse {
            version,
            projects: vec![project("p-1")],
            routes: vec![route("p-1")],
            fog: vec![],
            items: vec![item("a-1"), item("a-2")],
            steps: vec![step("s-1", "a-1")],
            blocked_by: vec![blocked_by("a-1", "a-2")],
            alerts: vec![],
            context_snapshots: vec![],
            settings: vec![],
            rules: vec![],
            grills: vec![],
        }
    }

    #[test]
    fn an_empty_delta_leaves_a_seeded_item_intact_and_advances_the_cursor() {
        let mut mirror = SyncMirror::new();
        mirror.apply_delta(ChangesResponse {
            version: 1,
            items: vec![item("a-1")],
            ..ChangesResponse::empty(1)
        });

        mirror.apply_delta(ChangesResponse::empty(5));

        assert_eq!(
            mirror.version(),
            5,
            "the cursor advances even on a no-op delta"
        );
        assert!(
            mirror.item("a-1").is_some(),
            "an empty delta must be handled as nothing changed, not as everything gone"
        );
    }

    /// #100 acceptance: "A delta pull carrying a subset of tables leaves
    /// untouched entities intact in the mirror; only the full sweep can
    /// demote by absence."
    #[test]
    fn a_delta_that_only_touches_one_table_leaves_the_others_intact() {
        let mut mirror = SyncMirror::new();
        mirror.apply_delta(seeded_workspace(1));

        // A delta that only mentions one item — every other table is empty
        // in this response, which must not demote anything.
        mirror.apply_delta(ChangesResponse {
            version: 2,
            items: vec![item("a-1")],
            ..ChangesResponse::empty(2)
        });

        assert!(
            mirror.project("p-1").is_some(),
            "delta must not demote projects it didn't mention"
        );
        assert!(
            mirror.route("p-1").is_some(),
            "delta must not demote routes it didn't mention"
        );
        assert!(
            mirror.item("a-2").is_some(),
            "delta must not demote items it didn't mention"
        );
        assert_eq!(
            mirror.steps_for_item("a-1").count(),
            1,
            "delta must not demote steps it didn't mention"
        );
        assert_eq!(
            mirror.blockers_of("a-1").count(),
            1,
            "delta must not demote blocked_by edges it didn't mention"
        );
    }

    #[test]
    fn only_a_full_sweep_demotes_an_id_missing_from_its_response() {
        let mut mirror = SyncMirror::new();
        mirror.apply_delta(seeded_workspace(1));

        // A sweep that omits "a-2" entirely (as opposed to a delta, where
        // omission just means "unchanged") demotes it.
        mirror.apply_sweep(
            ChangesResponse {
                version: 2,
                items: vec![item("a-1")],
                ..ChangesResponse::empty(2)
            },
            9_000,
        );

        assert!(mirror.item("a-1").is_some());
        assert!(
            mirror.item("a-2").is_none(),
            "a full sweep's completeness makes an id's absence meaningful"
        );
        assert_eq!(
            mirror.item_including_absent("a-2"),
            Some(&item("a-2")),
            "demotion retains the record — ADR-0007/0008 retention, not deletion"
        );
    }

    /// ADR-0007: "a formerly-absent id returned by a later sweep simply
    /// rejoins the live set" — forwarded by the #100 reviewer as untested.
    /// A full sweep is the only path that can demote *or* resurrect an id,
    /// since a delta's silence about an id means "unchanged", never
    /// "gone" or "back".
    #[test]
    fn a_formerly_absent_id_rejoins_the_live_set_on_a_later_sweep() {
        let mut mirror = SyncMirror::new();
        mirror.apply_sweep(seeded_workspace(1), 1_000);
        mirror.apply_sweep(
            ChangesResponse {
                version: 2,
                items: vec![item("a-1")],
                ..ChangesResponse::empty(2)
            },
            2_000,
        );
        assert!(
            mirror.item("a-2").is_none(),
            "a-2 must be absent before the resurrecting sweep"
        );

        mirror.apply_sweep(seeded_workspace(3), 3_000);

        assert!(
            mirror.item("a-2").is_some(),
            "a-2 simply rejoins the live set — ADR-0007 has no separate un-demotion step"
        );
        assert!(
            mirror.all_items().any(|i| i.id == "a-2"),
            "a rejoined id must also reappear in every live-filtered view"
        );
    }

    /// #100 acceptance: "A full sweep and a delta pull over the same seeded
    /// fixture workspace produce byte-identical mirrors" — the mirror-side
    /// half of #114's server assertion. Proven by *convergence*, not by
    /// applying one identical response to two empty mirrors (which cannot
    /// distinguish `apply_sweep` from `apply_delta` at all): mirror A is
    /// built from a sequence of deltas (create, update, soft-delete);
    /// mirror B is built from one sweep of the equivalent final state.
    #[test]
    fn a_mirror_built_from_a_delta_sequence_matches_one_built_from_the_equivalent_sweep() {
        let mut via_deltas = SyncMirror::new();
        via_deltas.apply_delta(ChangesResponse {
            version: 1,
            projects: vec![project("p-1")],
            routes: vec![route("p-1")],
            items: vec![item("a-1"), item("a-2")],
            steps: vec![step("s-1", "a-1")],
            blocked_by: vec![blocked_by("a-1", "a-2")],
            ..ChangesResponse::empty(1)
        });
        let mut renamed = item("a-1");
        renamed.title = "renamed".to_string();
        renamed.version = 2;
        via_deltas.apply_delta(ChangesResponse {
            version: 2,
            items: vec![renamed.clone()],
            ..ChangesResponse::empty(2)
        });
        let mut deleted_step = step("s-1", "a-1");
        deleted_step.deleted_at = Some(500);
        deleted_step.version = 3;
        via_deltas.apply_delta(ChangesResponse {
            version: 3,
            steps: vec![deleted_step.clone()],
            ..ChangesResponse::empty(3)
        });

        let mut via_sweep = SyncMirror::new();
        via_sweep.apply_sweep(
            ChangesResponse {
                version: 3,
                projects: vec![project("p-1")],
                routes: vec![route("p-1")],
                items: vec![renamed, item("a-2")],
                steps: vec![deleted_step],
                blocked_by: vec![blocked_by("a-1", "a-2")],
                ..ChangesResponse::empty(3)
            },
            9_999, // must not matter: nothing is absent-by-gap in this fixture
        );

        assert_eq!(via_deltas, via_sweep);
        // And the observable behaviour agrees too, not just internal state.
        assert_eq!(via_deltas.item("a-1").unwrap().title, "renamed");
        assert_eq!(via_deltas.steps_for_item("a-1").count(), 0);
    }

    /// #100 acceptance: "Soft-deleted rows are applied as removals" — the
    /// adapter maps the flag into the mirror's own absence rather than
    /// inferring deletion from a gap. "Removals" from the live view, per
    /// ADR-0007/0008 retention — never erased from the mirror itself.
    #[test]
    fn a_soft_deleted_item_leaves_the_live_view_but_stays_retrievable() {
        let mut mirror = SyncMirror::new();
        mirror.apply_delta(seeded_workspace(1));
        assert!(mirror.item("a-1").is_some());

        let mut archived = item("a-1");
        archived.archived_at = Some(9_999);
        mirror.apply_delta(ChangesResponse {
            version: 2,
            items: vec![archived.clone()],
            ..ChangesResponse::empty(2)
        });

        assert!(
            mirror.item("a-1").is_none(),
            "the explicit archived_at flag, not a gap, is the removal signal"
        );
        assert!(
            !mirror.all_items().any(|i| i.id == "a-1"),
            "an archived item must also leave all_items()"
        );
        assert_eq!(
            mirror.item_including_absent("a-1"),
            Some(&archived),
            "the record stays in the mirror forever — retention, not deletion"
        );
    }

    #[test]
    fn a_soft_deleted_step_and_a_removed_blocked_by_edge_leave_their_live_views() {
        let mut mirror = SyncMirror::new();
        mirror.apply_delta(seeded_workspace(1));

        let mut deleted_step = step("s-1", "a-1");
        deleted_step.deleted_at = Some(1);
        let mut removed_edge = blocked_by("a-1", "a-2");
        removed_edge.removed_at = Some(1);
        mirror.apply_delta(ChangesResponse {
            version: 2,
            steps: vec![deleted_step],
            blocked_by: vec![removed_edge],
            ..ChangesResponse::empty(2)
        });

        assert_eq!(mirror.steps_for_item("a-1").count(), 0);
        assert_eq!(mirror.blockers_of("a-1").count(), 0);
    }

    /// #100 acceptance: "Steps arrive as first-class records attached to
    /// their item, not parsed from any body text."
    #[test]
    fn steps_are_first_class_records_attached_to_their_item() {
        let mut mirror = SyncMirror::new();
        mirror.apply_delta(seeded_workspace(1));

        let steps: Vec<&Step> = mirror.steps_for_item("a-1").collect();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].id, "s-1");
        assert_eq!(steps[0].item_id, "a-1");
        assert_eq!(mirror.steps_for_item("a-2").count(), 0);
    }

    /// #100 acceptance: "`blocked_by` edges land in the right direction,
    /// proven by a fixture; getting it backwards inverts the frontier."
    #[test]
    fn blocked_by_edges_land_in_the_direction_the_schema_fixes() {
        let mut mirror = SyncMirror::new();
        // a-1 is blocked by a-2 — a-1 is not actionable, a-2 is.
        mirror.apply_delta(seeded_workspace(1));

        let blockers_of_a1: Vec<&str> = mirror.blockers_of("a-1").collect();
        assert_eq!(blockers_of_a1, vec!["a-2"], "a-1 is blocked by a-2");

        let blockers_of_a2: Vec<&str> = mirror.blockers_of("a-2").collect();
        assert!(
            blockers_of_a2.is_empty(),
            "a-2 is the blocker, not the blocked item — inverting this would wrongly mark it unactionable"
        );
    }

    /// #100 acceptance: "The cursor advances only on a fully applied pull,
    /// and a crash mid-apply replays the same delta harmlessly."
    #[test]
    fn replaying_the_same_delta_is_a_harmless_no_op() {
        let mut mirror = SyncMirror::new();
        mirror.apply_delta(seeded_workspace(5));
        let once = mirror.clone();

        // A crash before the cursor's advance was durably persisted would
        // cause the host to replay the identical delta on the next attempt.
        mirror.apply_delta(seeded_workspace(5));

        assert_eq!(
            mirror, once,
            "replaying an already-applied delta must be a no-op"
        );
        assert_eq!(mirror.version(), 5);
    }

    /// A second sweep that also misses an id must not refresh its absence
    /// stamp — the same "first, not latest" rule [`Presence::demote`]
    /// already enforces for the S1 mirror.
    #[test]
    fn a_second_sweep_that_also_misses_an_id_does_not_reset_its_absence_clock() {
        let mut mirror = SyncMirror::new();
        mirror.apply_delta(seeded_workspace(1));
        mirror.apply_sweep(
            ChangesResponse {
                version: 2,
                items: vec![item("a-1")],
                ..ChangesResponse::empty(2)
            },
            2_000,
        );
        mirror.apply_sweep(
            ChangesResponse {
                version: 3,
                items: vec![item("a-1")],
                ..ChangesResponse::empty(3)
            },
            9_000,
        );

        assert_eq!(
            mirror.items.get("a-2").unwrap().presence,
            Presence::Absent { since_ms: 2_000 }
        );
    }

    /// #118 acceptance: "settings rows ride the ordinary delta pull and full
    /// sweep — no bespoke sync path". The proof is that they behave exactly
    /// like every other table here: a delta upserts them, a delta that does
    /// not mention them leaves them alone, and only a sweep's completeness
    /// demotes one by absence.
    #[test]
    fn settings_rows_ride_the_ordinary_delta_pull_and_full_sweep() {
        fn setting(key: &str, value: &str, version: i64) -> Setting {
            Setting {
                key: key.to_string(),
                value: value.to_string(),
                updated_at: 1,
                version,
            }
        }

        let mut mirror = SyncMirror::new();
        mirror.apply_delta(ChangesResponse {
            version: 1,
            settings: vec![
                setting("race-series", "\"f1\"", 1),
                setting("trips-calendar", "\"cal-1\"", 1),
            ],
            ..ChangesResponse::empty(1)
        });
        assert_eq!(mirror.all_settings().count(), 2);

        // A delta touching only items must not disturb them...
        mirror.apply_delta(ChangesResponse {
            version: 2,
            items: vec![item("a-1")],
            ..ChangesResponse::empty(2)
        });
        assert_eq!(mirror.setting("race-series").unwrap().value, "\"f1\"");

        // ...an updated row upserts in place, at its new version...
        mirror.apply_delta(ChangesResponse {
            version: 3,
            settings: vec![setting("race-series", "\"motogp\"", 2)],
            ..ChangesResponse::empty(3)
        });
        let updated = mirror.setting("race-series").unwrap();
        assert_eq!(updated.value, "\"motogp\"");
        assert_eq!(updated.version, 2);

        // ...and only a sweep's completeness demotes one by absence.
        mirror.apply_sweep(
            ChangesResponse {
                version: 4,
                settings: vec![setting("race-series", "\"motogp\"", 2)],
                ..ChangesResponse::empty(4)
            },
            9_000,
        );
        assert!(mirror.setting("trips-calendar").is_none());
        assert_eq!(
            mirror.all_settings().map(|s| s.key.as_str()).collect::<Vec<_>>(),
            vec!["race-series"]
        );
    }

    /// #140 acceptance: rules ride the same delta-pull/full-sweep contract
    /// every other table here does — no bespoke sync path, same as #118's
    /// settings above.
    #[test]
    fn rules_rows_ride_the_ordinary_delta_pull_and_full_sweep() {
        fn rule(id: &str, enabled: bool, version: i64) -> Rule {
            Rule {
                id: id.to_string(),
                name: format!("rule {id}"),
                event_kind: None,
                conditions: vec![],
                severity: "normal".to_string(),
                tier: hummingbird_domain::Tier::Normal,
                enabled,
                updated_at: 1,
                version,
            }
        }

        let mut mirror = SyncMirror::new();
        mirror.apply_delta(ChangesResponse {
            version: 1,
            rules: vec![rule("r-1", true, 1), rule("r-2", true, 1)],
            ..ChangesResponse::empty(1)
        });
        assert_eq!(mirror.all_rules().count(), 2);

        // A delta touching only items must not disturb them...
        mirror.apply_delta(ChangesResponse {
            version: 2,
            items: vec![item("a-1")],
            ..ChangesResponse::empty(2)
        });
        assert!(mirror.rule("r-1").unwrap().enabled);

        // ...an updated row upserts in place, at its new version...
        mirror.apply_delta(ChangesResponse {
            version: 3,
            rules: vec![rule("r-1", false, 2)],
            ..ChangesResponse::empty(3)
        });
        let updated = mirror.rule("r-1").unwrap();
        assert!(!updated.enabled);
        assert_eq!(updated.version, 2);

        // ...and only a sweep's completeness demotes one by absence.
        mirror.apply_sweep(
            ChangesResponse {
                version: 4,
                rules: vec![rule("r-1", false, 2)],
                ..ChangesResponse::empty(4)
            },
            9_000,
        );
        assert!(mirror.rule("r-2").is_none());
        assert_eq!(
            mirror.all_rules().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["r-1"]
        );
    }

    /// #103 acceptance: "the cycle exposes the active-issue count, so the
    /// 250-issue watchline is observed rather than remembered." Live and
    /// `Done` are both excluded — a finished item is not what the watchline
    /// counts, and an absent one dropped out already.
    #[test]
    fn active_item_count_excludes_absent_and_done_items() {
        let mut done = item("a-3");
        done.stage = hummingbird_domain::Stage::Done;

        let mut mirror = SyncMirror::new();
        mirror.apply_sweep(
            ChangesResponse {
                version: 1,
                items: vec![item("a-1"), item("a-2"), done.clone()],
                ..ChangesResponse::empty(1)
            },
            1_000,
        );
        // a-2 goes absent on the next sweep; a-3 stays live but Done.
        mirror.apply_sweep(
            ChangesResponse {
                version: 2,
                items: vec![item("a-1"), done],
                ..ChangesResponse::empty(2)
            },
            2_000,
        );

        assert_eq!(
            mirror.active_item_count(),
            1,
            "only a-1 is both live and not Done"
        );
    }

    /// #103: `SyncMirror` is now `Persistable` — this must actually round
    /// trip through `serde_json`, tuple-keyed tables (`blocked_by`,
    /// `context_snapshots`) included. `serde_json` rejects a native
    /// `BTreeMap` with a tuple key outright (it requires string keys), which
    /// is exactly what `tuple_key_map` works around — a compiling
    /// `#[derive(Serialize, Deserialize)]` alone would not have caught a
    /// mistake there, only an actual round trip through the store does.
    #[tokio::test]
    async fn a_persisted_mirror_round_trips_through_the_snapshot_store_including_tuple_keyed_tables(
    ) {
        use crate::storage::{load_snapshot, save_snapshot, MemorySnapshotStore};

        let mut mirror = SyncMirror::new();
        mirror.apply_sweep(seeded_workspace(1), 1_000);

        let store = MemorySnapshotStore::default();
        save_snapshot(&store, 1, 1_000, &mirror)
            .await
            .unwrap();

        let loaded: SyncMirror = load_snapshot(&store).await.unwrap().unwrap().payload;

        assert_eq!(loaded, mirror);
        assert_eq!(loaded.version(), 1);
        assert_eq!(
            loaded.blockers_of("a-1").collect::<Vec<_>>(),
            vec!["a-2"],
            "the tuple-keyed blocked_by table must survive the round trip"
        );
    }
}
