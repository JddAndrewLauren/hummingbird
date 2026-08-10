//! Acceptance tests for the task mirror contract (S1).
//!
//! Golden snapshots built by hand — no fixtures from Linear and no network,
//! because at this layer there is no adapter yet and the mirror's rules are
//! meant to hold independently of where records came from.

use hummingbird_core::storage::{load_snapshot, save_snapshot, MemorySnapshotStore};
use hummingbird_core::task::{
    by_priority_then_due, DeadLetterEntry, DeadLetterReason, Item, Mirror, Presence, Priority,
    Route, Stage, Sweep, MIRROR_SCHEMA_VERSION,
};

/// A small but complete team: one of every funnel stage, a blocked chain, a
/// project with a Route, and one item carrying an unmodelled field.
fn golden_sweep() -> Sweep {
    let mut capture = Item::new("id-capture", "buy milk", Stage::Triage);
    capture.identifier = "ION-1".to_string();
    capture.created_at_ms = 1_000;
    capture.updated_at_ms = 1_000;

    let mut fog = Item::new("id-fog", "figure out the capture UX", Stage::Grilling);
    fog.identifier = "ION-2".to_string();

    let mut blocker = Item::new("id-blocker", "mint the Linear key", Stage::Ready);
    blocker.identifier = "ION-3".to_string();
    blocker.priority = Priority::Urgent;
    blocker.project = Some("Build the client".to_string());

    let mut blocked = Item::new("id-blocked", "run the live probe", Stage::Ready);
    blocked.identifier = "ION-4".to_string();
    blocked.blockers = vec!["id-blocker".to_string()];
    blocked.project = Some("Build the client".to_string());

    let mut started = Item::new("id-started", "write the mirror", Stage::InProgress);
    started.identifier = "ION-5".to_string();
    started.priority = Priority::High;
    started.deadline = Some("2026-08-09".to_string());
    started.project = Some("Build the client".to_string());
    started
        .extra
        .insert("estimate".to_string(), serde_json::Value::Number(3.into()));

    let mut waiting = Item::new("id-waiting", "hear back from Cloudflare", Stage::Blocked);
    waiting.identifier = "ION-6".to_string();

    let mut done = Item::new("id-done", "land the client shell", Stage::Done);
    done.identifier = "ION-7".to_string();

    let route = Route::new(
        "Build the client",
        Some(
            "## Destination\nA usable desktop task client.\n\n\
             ## Fog\n* Capture UX per device.\n\n\
             ## Notes\nCore before UI.\n"
                .to_string(),
        ),
    );

    Sweep {
        items: vec![capture, fog, blocker, blocked, started, waiting, done],
        routes: vec![route],
    }
}

fn ids(items: &[&Item]) -> Vec<String> {
    items.iter().map(|i| i.id.clone()).collect()
}

#[test]
fn the_golden_sweep_lands_the_expected_working_views() {
    let mut mirror = Mirror::new();
    let report = mirror.reconcile(golden_sweep(), 1_000);

    assert_eq!(report.added, 7);
    assert_eq!(report.demoted, 0);

    // Ready/In Progress, minus the relation-blocked one.
    assert_eq!(
        ids(&mirror.frontier()),
        vec!["id-blocker", "id-started"],
        "the frontier is what can be started right now"
    );
    assert_eq!(ids(&mirror.triage_inbox()), vec!["id-capture"]);
    assert_eq!(ids(&mirror.grilling()), vec!["id-fog"]);

    // Blocked-by-relation is explained, not merely absent.
    let blocked = mirror.blocked_by_relation();
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].0.id, "id-blocked");
    assert_eq!(ids(&blocked[0].1), vec!["id-blocker"]);

    // Six live and unshut out of seven records.
    assert_eq!(mirror.active_count(), 6);
}

#[test]
fn closing_a_blocker_moves_the_blocked_item_onto_the_frontier() {
    let mut mirror = Mirror::new();
    mirror.reconcile(golden_sweep(), 1_000);

    let mut next = golden_sweep();
    for item in next.items.iter_mut() {
        if item.id == "id-blocker" {
            item.stage = Stage::Done;
        }
    }
    mirror.reconcile(next, 2_000);

    assert!(ids(&mirror.frontier()).contains(&"id-blocked".to_string()));
    assert!(mirror.blocked_by_relation().is_empty());
}

#[test]
fn a_route_reaches_the_mirror_with_its_sections_verbatim() {
    let mut mirror = Mirror::new();
    mirror.reconcile(golden_sweep(), 1_000);

    let route = mirror.route("Build the client").unwrap();
    assert_eq!(route.destination(), Some("A usable desktop task client."));
    assert_eq!(route.fog(), Some("* Capture UX per device."));
    // The Notes section is neither swallowed into Fog nor lost from content.
    assert!(route
        .content
        .as_deref()
        .unwrap()
        .contains("Core before UI."));
}

#[test]
fn a_full_absence_cycle_demotes_then_rejoins_and_never_erases() {
    let mut mirror = Mirror::new();
    mirror.reconcile(golden_sweep(), 1_000);

    // A complete sweep that no longer returns the finished item — Linear
    // archived it.
    let mut without_done = golden_sweep();
    without_done.items.retain(|i| i.id != "id-done");
    let report = mirror.reconcile(without_done.clone(), 2_000);
    assert_eq!(report.demoted, 1);

    assert_eq!(
        mirror.get("id-done").unwrap().presence,
        Presence::Absent { since_ms: 2_000 },
        "absence demotes, never deletes"
    );
    assert_eq!(mirror.all_items().count(), 7);

    // A later sweep still missing it must not restamp the absence.
    mirror.reconcile(without_done, 5_000);
    assert_eq!(
        mirror.get("id-done").unwrap().presence,
        Presence::Absent { since_ms: 2_000 }
    );

    // Un-archived in Linear: it simply rejoins.
    let report = mirror.reconcile(golden_sweep(), 9_000);
    assert_eq!(report.rejoined, 1);
    assert!(mirror.get("id-done").unwrap().is_live());
}

#[tokio::test]
async fn the_whole_mirror_round_trips_through_the_snapshot_store() {
    let store = MemorySnapshotStore::default();
    let mut mirror = Mirror::new();
    mirror.reconcile(golden_sweep(), 1_000);
    mirror.dead_letters_mut().record(DeadLetterEntry {
        item_id: "id-started".to_string(),
        identifier: "ION-5".to_string(),
        field: "title".to_string(),
        local_value: "write the mirror properly".to_string(),
        reason: DeadLetterReason::ConflictLost {
            server_value: Some("write the mirror".to_string()),
        },
        at_ms: 1_500,
    });

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
    assert_eq!(envelope.payload, mirror);
    assert_eq!(envelope.payload.dead_letters().len(), 1);
    // The unmodelled field came back too — the mirror is the export.
    assert_eq!(
        envelope
            .payload
            .get("id-started")
            .unwrap()
            .extra
            .get("estimate")
            .and_then(|v| v.as_i64()),
        Some(3)
    );
}

#[test]
fn the_snapshot_is_human_readable_json() {
    // ADR-0003 keeps this property deliberately: "a readable mirror file is
    // worth real money when debugging".
    let mut mirror = Mirror::new();
    mirror.reconcile(golden_sweep(), 1_000);

    let json = serde_json::to_string(&mirror).unwrap();
    assert!(json.contains("\"buy milk\""), "{json}");
    assert!(json.contains("\"Triage\""), "{json}");
}

#[test]
fn ranking_the_frontier_is_stable_and_priority_correct() {
    let mut mirror = Mirror::new();
    mirror.reconcile(golden_sweep(), 1_000);

    let mut items = mirror.frontier();
    by_priority_then_due(&mut items);

    // Urgent before High, and neither sorted on the raw wire number.
    assert_eq!(ids(&items), vec!["id-blocker", "id-started"]);
}

/// ADR-0001 seam rule 1, enforced mechanically rather than by convention:
/// the domain model must not carry Linear's vocabulary.
///
/// Modelled on `core`'s `cargo_toml_has_no_binding_macro_dependencies` test —
/// the same trick of asserting against the source text, because the property
/// is about what the code is *allowed to name*, which no type system checks.
#[test]
fn no_linear_vocabulary_leaks_into_the_domain_model() {
    let sources = [
        ("item.rs", include_str!("../src/task/item.rs")),
        ("mirror.rs", include_str!("../src/task/mirror.rs")),
        ("query.rs", include_str!("../src/task/query.rs")),
        ("route.rs", include_str!("../src/task/route.rs")),
        ("deadletter.rs", include_str!("../src/task/deadletter.rs")),
    ];

    // Identifiers, not prose: the doc comments legitimately explain *why*
    // each of these is absent, and citing the reason is the opposite of
    // leaking the vocabulary. So this checks code lines only.
    let forbidden = [
        "stateId",
        "state_id",
        "labelId",
        "label_id",
        "teamId",
        "team_id",
        "issueId",
        "issue_id",
        "includeArchived",
        "inverseRelations",
    ];

    for (name, source) in sources {
        for line in source.lines() {
            let code = line.trim_start();
            if code.starts_with("//") || code.starts_with("//!") {
                continue;
            }
            for needle in forbidden {
                assert!(
                    !code.contains(needle),
                    "`{needle}` appears in code in task/{name} — the domain \
                     model must not name Linear's shape (ADR-0001 seam rule 1); \
                     translation belongs in the adapter",
                );
            }
        }
    }
}
