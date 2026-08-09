//! [`SyncMirror`]: the device's reconciling local read model of the owned
//! workspace (ADR-0008/0009), built from [`ChangesResponse`] pages.
//!
//! Two apply paths, one rule (ADR-0007's amendment, ADR-0008 "reads: delta
//! pull, full sweep as backstop"): a delta pull is additive — it may
//! upsert or, on an explicit soft-delete flag, remove a row, but it never
//! drops a row just because this delta didn't mention it. Only
//! [`SyncMirror::apply_sweep`], fed the complete workspace, may demote an
//! id by its absence from the response.

use std::collections::BTreeMap;

use hummingbird_domain::{
    Alert, BlockedBy, ChangesResponse, ContextSnapshot, Fog, Item, Project, Route, Setting, Step,
};

/// The device's local read model of every synced table, plus the delta
/// cursor (`meta.version` as of the last fully-applied pull).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SyncMirror {
    version: i64,
    projects: BTreeMap<String, Project>,
    routes: BTreeMap<String, Route>,
    fog: BTreeMap<String, Fog>,
    items: BTreeMap<String, Item>,
    steps: BTreeMap<String, Step>,
    blocked_by: BTreeMap<(String, String), BlockedBy>,
    alerts: BTreeMap<String, Alert>,
    context_snapshots: BTreeMap<(String, String), ContextSnapshot>,
    settings: BTreeMap<String, Setting>,
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
        self.items.get(id)
    }

    pub fn all_items(&self) -> impl Iterator<Item = &Item> {
        self.items.values()
    }

    pub fn project(&self, id: &str) -> Option<&Project> {
        self.projects.get(id)
    }

    pub fn route(&self, project_id: &str) -> Option<&Route> {
        self.routes.get(project_id)
    }

    /// Every live Step attached to `item_id`, id order — first-class
    /// records, never parsed from a body string.
    pub fn steps_for_item<'a>(&'a self, item_id: &'a str) -> impl Iterator<Item = &'a Step> {
        self.steps.values().filter(move |s| s.item_id == item_id)
    }

    /// Every id that blocks `item_id` — the direction the schema and the
    /// pull both fix: `item_id` is blocked by `blocker_id`. Getting this
    /// backwards inverts the frontier (which items are actionable).
    pub fn blockers_of<'a>(&'a self, item_id: &'a str) -> impl Iterator<Item = &'a str> {
        self.blocked_by
            .values()
            .filter(move |edge| edge.item_id == item_id)
            .map(|edge| edge.blocker_id.as_str())
    }

    pub fn alert(&self, id: &str) -> Option<&Alert> {
        self.alerts.get(id)
    }

    pub fn setting(&self, key: &str) -> Option<&Setting> {
        self.settings.get(key)
    }

    /// Applies one delta pull: additive only. A row absent from `resp` is
    /// left exactly as it was; a row present with its soft-delete flag set
    /// is removed from the mirror (the flag *is* the removal signal, never
    /// the gap). The cursor advances to `resp.version` only after every
    /// table below has been applied — built on a scratch copy and swapped
    /// in as the last step, so a caller that never observes the swap (a
    /// crash mid-apply) sees the previous, fully-consistent mirror, and a
    /// replay of the same delta is a harmless no-op.
    pub fn apply_delta(&mut self, resp: ChangesResponse) {
        let mut next = self.clone();
        next.apply_tables(resp, false);
        *self = next;
    }

    /// Applies one full sweep: the complete workspace. A row present is
    /// upserted (or removed, on its soft-delete flag) exactly as a delta
    /// would; a row this device previously held live but that is missing
    /// from `resp` entirely is demoted — removed from the mirror, because
    /// the sweep is a complete picture and its absence is meaningful.
    pub fn apply_sweep(&mut self, resp: ChangesResponse) {
        let mut next = self.clone();
        next.apply_tables(resp, true);
        *self = next;
    }

    fn apply_tables(&mut self, resp: ChangesResponse, full: bool) {
        apply_table(
            &mut self.projects,
            resp.projects,
            |p| p.id.clone(),
            |p| p.archived_at.is_some(),
            full,
        );
        apply_table(
            &mut self.routes,
            resp.routes,
            |r| r.project_id.clone(),
            |_| false,
            full,
        );
        apply_table(&mut self.fog, resp.fog, |f| f.id.clone(), |_| false, full);
        apply_table(
            &mut self.items,
            resp.items,
            |i| i.id.clone(),
            |i| i.archived_at.is_some(),
            full,
        );
        apply_table(
            &mut self.steps,
            resp.steps,
            |s| s.id.clone(),
            |s| s.deleted_at.is_some(),
            full,
        );
        apply_table(
            &mut self.blocked_by,
            resp.blocked_by,
            |b| (b.item_id.clone(), b.blocker_id.clone()),
            |b| b.removed_at.is_some(),
            full,
        );
        apply_table(
            &mut self.alerts,
            resp.alerts,
            |a| a.id.clone(),
            |_| false,
            full,
        );
        apply_table(
            &mut self.context_snapshots,
            resp.context_snapshots,
            |c| (c.source.clone(), c.key.clone()),
            |_| false,
            full,
        );
        apply_table(
            &mut self.settings,
            resp.settings,
            |s| s.key.clone(),
            |_| false,
            full,
        );

        self.version = resp.version;
    }
}

/// Applies one table's slice of a pull to `map`.
///
/// Every row in `incoming` is either upserted (removed on `is_removed`) —
/// the additive half both a delta and a sweep share. When `full` is set
/// (a sweep), every key not touched by this call is dropped: the response
/// was the complete workspace, so its absence is the demotion signal. When
/// `full` is unset (a delta), untouched keys are left exactly as they were.
fn apply_table<K, T>(
    map: &mut BTreeMap<K, T>,
    incoming: Vec<T>,
    key_of: impl Fn(&T) -> K,
    is_removed: impl Fn(&T) -> bool,
    full: bool,
) where
    K: Ord + Clone,
{
    let mut seen = std::collections::BTreeSet::new();
    for row in incoming {
        let key = key_of(&row);
        seen.insert(key.clone());
        if is_removed(&row) {
            map.remove(&key);
        } else {
            map.insert(key, row);
        }
    }
    if full {
        map.retain(|k, _| seen.contains(k));
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
            due_date: None,
            scheduled_date: None,
            source: None,
            source_key: None,
            source_url: None,
            archived_at: None,
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
        mirror.apply_sweep(ChangesResponse {
            version: 2,
            items: vec![item("a-1")],
            ..ChangesResponse::empty(2)
        });

        assert!(mirror.item("a-1").is_some());
        assert!(
            mirror.item("a-2").is_none(),
            "a full sweep's completeness makes an id's absence meaningful"
        );
    }

    /// #100 acceptance: "A full sweep and a delta pull over the same seeded
    /// fixture workspace produce byte-identical mirrors."
    #[test]
    fn a_full_sweep_and_a_delta_over_the_same_fixture_produce_identical_mirrors() {
        let mut via_sweep = SyncMirror::new();
        via_sweep.apply_sweep(seeded_workspace(5));

        let mut via_delta = SyncMirror::new();
        via_delta.apply_delta(seeded_workspace(5));

        assert_eq!(via_sweep, via_delta);
    }

    /// #100 acceptance: "Soft-deleted rows are applied as removals" — the
    /// adapter maps the flag into the mirror's own absence rather than
    /// inferring deletion from a gap.
    #[test]
    fn a_soft_deleted_item_is_removed_from_the_mirror_by_a_delta() {
        let mut mirror = SyncMirror::new();
        mirror.apply_delta(seeded_workspace(1));
        assert!(mirror.item("a-1").is_some());

        let mut archived = item("a-1");
        archived.archived_at = Some(9_999);
        mirror.apply_delta(ChangesResponse {
            version: 2,
            items: vec![archived],
            ..ChangesResponse::empty(2)
        });

        assert!(
            mirror.item("a-1").is_none(),
            "the explicit archived_at flag, not a gap, is the removal signal"
        );
    }

    #[test]
    fn a_soft_deleted_step_and_a_removed_blocked_by_edge_are_removed_too() {
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
}
