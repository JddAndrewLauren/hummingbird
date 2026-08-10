//! [`Mirror`]: the device's reconciling local read model of team ION, and
//! [`Mirror::reconcile`]: ADR-0007's absence-demotes rule.
//!
//! One persisted type holding items, Routes and the dead-letter journal
//! together, rather than a slot each. ADR-0007 requires the sweep to apply
//! "atomically or not at all" — and the snapshot store's atomicity is
//! per-slot, so two slots would mean a crash between two writes could leave
//! items and Routes describing different moments. The journal lives here for
//! the same reason ADR-0007 puts it "in the mirror": an edit that lost a
//! conflict and the sweep that beat it are one fact, and they must commit or
//! roll back together.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::storage::{Persistable, PersistableSealed};

use super::deadletter::DeadLetterJournal;
use super::item::{Item, ItemId, Presence};
use super::route::Route;

/// The schema version written into the snapshot envelope.
///
/// Bump it when the payload shape changes. An *incompatible* shape change
/// surfaces on its own, without this constant: `load_snapshot` returns
/// `SnapshotError::Deserialize`, never an empty mirror — a caller that maps
/// that error to "start fresh" is the thing that would lose data. This
/// constant exists for the shape changes serde *cannot* see, which load
/// cleanly while meaning something else.
///
/// Bumped to 2 for #153's `Item.due_date` → `Item.deadline` rename, which
/// is exactly one of those: `deadline` has no `#[serde(default)]` but
/// `Option<T>` still deserializes a missing key as `None`, and the struct
/// carries no `deny_unknown_fields`, so a v1 snapshot's stale `due_date`
/// key is dropped and every stored deadline comes back `None` with no
/// error.
///
/// **The bump only helps a caller that compares it.** This S1 type has no
/// production load path — nothing outside this module's own tests calls
/// `load_snapshot::<Mirror, _>`; the live read model is
/// [`crate::sync::mirror::SyncMirror`], and it is
/// [`crate::sync::cycle::SyncCycle::load`] that does the comparing, by
/// discarding a mirror stored at any other version. Any future loader of
/// *this* type must do the same; carrying the constant into the envelope is
/// what leaves that option open, not a guarantee in itself.
pub const MIRROR_SCHEMA_VERSION: u32 = 2;

/// The result of applying one complete sweep.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    pub added: usize,
    pub updated: usize,
    /// Ids that were live and are now marked absent by this sweep.
    pub demoted: usize,
    /// Ids that were absent and came back.
    pub rejoined: usize,
}

/// A complete sweep's payload: every non-archived item in team ION, plus
/// every project.
///
/// "Complete" is load-bearing and is the caller's promise, not something this
/// type can check. ADR-0007 makes absence the deletion mechanism, so handing
/// a *partial* fetch to [`Mirror::reconcile`] would demote everything the
/// missing pages held. The adapter's complete-or-nothing pagination is what
/// keeps that promise; this type only carries the result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sweep {
    pub items: Vec<Item>,
    pub routes: Vec<Route>,
}

/// The device's derived, disposable, unified local read model.
///
/// Never a source of truth (`CONTEXT.md`): Linear is the authority and wins
/// conflicts (ADR-0001). What this type guarantees is the other half of that
/// bargain — it only ever grows or updates, so losing Linear is an
/// inconvenience rather than data loss.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mirror {
    /// Keyed and ordered by id so the serialized snapshot is byte-stable for
    /// the same content — a readable, diffable mirror file is worth real
    /// money when debugging (ADR-0003), and a `Vec` in sweep order would
    /// churn every write.
    items: BTreeMap<ItemId, Item>,
    routes: BTreeMap<String, Route>,
    dead_letters: DeadLetterJournal,
}

impl PersistableSealed for Mirror {}
impl Persistable for Mirror {}

impl Mirror {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, id: &str) -> Option<&Item> {
        self.items.get(id)
    }

    /// Every record, live and absent, in id order.
    pub fn all_items(&self) -> impl Iterator<Item = &Item> {
        self.items.values()
    }

    pub fn route(&self, project: &str) -> Option<&Route> {
        self.routes.get(project)
    }

    pub fn all_routes(&self) -> impl Iterator<Item = &Route> {
        self.routes.values()
    }

    pub fn dead_letters(&self) -> &DeadLetterJournal {
        &self.dead_letters
    }

    pub fn dead_letters_mut(&mut self) -> &mut DeadLetterJournal {
        &mut self.dead_letters
    }

    /// Inserts or replaces one record. Used by the capture path, which mints
    /// an item locally before any sweep has ever seen it.
    pub fn upsert(&mut self, item: Item) {
        self.items.insert(item.id.clone(), item);
    }

    pub fn upsert_route(&mut self, route: Route) {
        self.routes.insert(route.name.clone(), route);
    }

    /// Applies one **complete** sweep: present ids are upserted live, and ids
    /// in the mirror but missing from the sweep are demoted to
    /// [`Presence::Absent`] — never removed.
    ///
    /// `now_ms` stamps the demotion. It is caller-supplied rather than
    /// sampled here for the same reason [`crate::storage::Envelope`]'s
    /// `as_of` is: bare `wasm32-unknown-unknown` has no clock that does not
    /// panic, and a pure function is testable.
    pub fn reconcile(&mut self, sweep: Sweep, now_ms: i64) -> ReconcileReport {
        let mut report = ReconcileReport::default();

        let mut seen: Vec<ItemId> = Vec::with_capacity(sweep.items.len());
        for incoming in sweep.items {
            let id = incoming.id.clone();
            match self.items.get(&id) {
                None => report.added += 1,
                Some(existing) => {
                    if !existing.is_live() {
                        // ADR-0007: "A formerly-absent id returned by a later
                        // sweep simply rejoins the live set."
                        report.rejoined += 1;
                    }
                    report.updated += 1;
                }
            }
            seen.push(id.clone());
            // The sweep is authority, so its record replaces the local one
            // wholesale — with one exception: presence is this module's
            // bookkeeping, not Linear's, and an adapter has no business
            // setting it.
            let mut incoming = incoming;
            incoming.presence = Presence::Live;
            self.items.insert(id, incoming);
        }

        let seen: std::collections::BTreeSet<&ItemId> = seen.iter().collect();
        for (id, item) in self.items.iter_mut() {
            if seen.contains(id) {
                continue;
            }
            if item.is_live() {
                report.demoted += 1;
            }
            item.presence = item.presence.demote(now_ms);
        }

        // Routes demote by the same rule, for the same reason: a project
        // archived in Linear must stop appearing without taking its recorded
        // Destination and Fog with it.
        let swept_routes: std::collections::BTreeSet<String> =
            sweep.routes.iter().map(|r| r.name.clone()).collect();
        for mut route in sweep.routes {
            route.presence = Presence::Live;
            self.routes.insert(route.name.clone(), route);
        }
        for (name, route) in self.routes.iter_mut() {
            if !swept_routes.contains(name) {
                route.presence = route.presence.demote(now_ms);
            }
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{load_snapshot, save_snapshot, MemorySnapshotStore};
    use crate::task::item::Stage;

    fn item(id: &str, stage: Stage) -> Item {
        Item::new(id, format!("item {id}"), stage)
    }

    fn sweep(ids: &[&str]) -> Sweep {
        Sweep {
            items: ids.iter().map(|id| item(id, Stage::Ready)).collect(),
            routes: Vec::new(),
        }
    }

    #[test]
    fn an_id_missing_from_a_complete_sweep_is_demoted_not_deleted() {
        let mut mirror = Mirror::new();
        mirror.reconcile(sweep(&["a", "b"]), 1_000);

        let report = mirror.reconcile(sweep(&["a"]), 2_000);

        assert_eq!(report.demoted, 1);
        // Still there — the mirror only grows or updates.
        let b = mirror.get("b").expect("b must survive its own absence");
        assert_eq!(b.presence, Presence::Absent { since_ms: 2_000 });
        assert_eq!(mirror.all_items().count(), 2);
    }

    #[test]
    fn a_second_sweep_that_also_misses_an_id_does_not_reset_its_absence_clock() {
        let mut mirror = Mirror::new();
        mirror.reconcile(sweep(&["a", "b"]), 1_000);
        mirror.reconcile(sweep(&["a"]), 2_000);

        let report = mirror.reconcile(sweep(&["a"]), 9_000);

        // Already absent, so nothing was newly demoted...
        assert_eq!(report.demoted, 0);
        // ...and "how long has this been gone" still answers 2_000.
        assert_eq!(
            mirror.get("b").unwrap().presence,
            Presence::Absent { since_ms: 2_000 }
        );
    }

    #[test]
    fn a_formerly_absent_id_rejoins_the_live_set() {
        let mut mirror = Mirror::new();
        mirror.reconcile(sweep(&["a", "b"]), 1_000);
        mirror.reconcile(sweep(&["a"]), 2_000);

        let report = mirror.reconcile(sweep(&["a", "b"]), 3_000);

        assert_eq!(report.rejoined, 1);
        assert!(mirror.get("b").unwrap().is_live());
    }

    #[test]
    fn an_adapter_cannot_set_presence_through_a_sweep() {
        // Presence is the mirror's own bookkeeping. A record arriving from a
        // complete sweep is live by definition of having arrived.
        let mut absent = item("a", Stage::Ready);
        absent.presence = Presence::Absent { since_ms: 5 };
        let mut mirror = Mirror::new();

        mirror.reconcile(
            Sweep {
                items: vec![absent],
                routes: Vec::new(),
            },
            1_000,
        );

        assert!(mirror.get("a").unwrap().is_live());
    }

    #[test]
    fn the_first_sweep_is_not_a_special_case() {
        // ADR-0007: "no first-sync special case". An empty mirror reconciling
        // a full sweep is just every id being added.
        let mut mirror = Mirror::new();
        let report = mirror.reconcile(sweep(&["a", "b", "c"]), 1_000);

        assert_eq!(report.added, 3);
        assert_eq!(report.demoted, 0);
    }

    #[test]
    fn an_empty_sweep_demotes_everything_and_deletes_nothing() {
        // The pathological complete sweep: the team really is empty. Every
        // record survives, none is live.
        let mut mirror = Mirror::new();
        mirror.reconcile(sweep(&["a", "b"]), 1_000);

        mirror.reconcile(Sweep::default(), 2_000);

        assert_eq!(mirror.all_items().count(), 2);
        assert!(mirror.all_items().all(|i| !i.is_live()));
    }

    #[test]
    fn routes_demote_by_absence_and_keep_their_content() {
        let mut mirror = Mirror::new();
        mirror.reconcile(
            Sweep {
                items: Vec::new(),
                routes: vec![Route::new("P", Some("## Fog\nsome fog".to_string()))],
            },
            1_000,
        );

        mirror.reconcile(Sweep::default(), 2_000);

        let route = mirror
            .route("P")
            .expect("an archived project keeps its Route");
        assert_eq!(route.presence, Presence::Absent { since_ms: 2_000 });
        assert_eq!(route.fog(), Some("some fog"));
    }

    #[test]
    fn the_dead_letter_journal_survives_reconciliation() {
        // A sweep is exactly the event that creates journal entries; one that
        // erased them would defeat the point.
        use crate::task::deadletter::{DeadLetterEntry, DeadLetterReason};

        let mut mirror = Mirror::new();
        mirror.dead_letters_mut().record(DeadLetterEntry {
            item_id: "a".to_string(),
            identifier: "ION-1".to_string(),
            field: "title".to_string(),
            local_value: "mine".to_string(),
            reason: DeadLetterReason::ConflictLost {
                server_value: Some("theirs".to_string()),
            },
            at_ms: 1,
        });

        mirror.reconcile(sweep(&["a"]), 2_000);

        assert_eq!(mirror.dead_letters().len(), 1);
    }

    #[tokio::test]
    async fn the_mirror_round_trips_atomically_with_as_of_and_version_intact() {
        let store = MemorySnapshotStore::default();
        let mut mirror = Mirror::new();
        mirror.reconcile(sweep(&["a", "b"]), 1_000);
        mirror.reconcile(sweep(&["a"]), 2_000);

        save_snapshot(
            &store,
            MIRROR_SCHEMA_VERSION,
            1_700_000_000_000,
            mirror.clone(),
        )
        .await
        .unwrap();
        let envelope = load_snapshot::<Mirror, _>(&store).await.unwrap().unwrap();

        assert_eq!(envelope.schema_version, MIRROR_SCHEMA_VERSION);
        assert_eq!(envelope.as_of, 1_700_000_000_000);
        // Including the absence marker: demotion is state, not a view.
        assert_eq!(envelope.payload, mirror);
        assert_eq!(
            envelope.payload.get("b").unwrap().presence,
            Presence::Absent { since_ms: 2_000 }
        );
    }

    #[tokio::test]
    async fn the_serialized_mirror_is_byte_stable_for_the_same_content() {
        // Two mirrors built by sweeps in different orders must serialize
        // identically, or every sweep rewrites the whole snapshot for nothing
        // and the file stops being diffable.
        let mut forward = Mirror::new();
        forward.reconcile(sweep(&["a", "b", "c"]), 1_000);
        let mut backward = Mirror::new();
        backward.reconcile(sweep(&["c", "b", "a"]), 1_000);

        assert_eq!(
            serde_json::to_string(&forward).unwrap(),
            serde_json::to_string(&backward).unwrap()
        );
    }
}
