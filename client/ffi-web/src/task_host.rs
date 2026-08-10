//! [`TaskHostCore`]: the web host's one door into #104's `Core` (the
//! owned-schema sync engine, ADR-0008/0009), kept free of `wasm_bindgen` so
//! it is testable with plain `cargo test` on any target — `lib.rs`'s
//! `wasm_bindings` module is the thin JS-facing shim over this, the same
//! split `calendar_host.rs` (issue #73) already proved.
//!
//! Every method returns a JSON-serializable DTO rather than a `Core` type
//! directly, so the wire shape lives in one place (this file) rather than
//! being re-derived on the TypeScript side from `hummingbird_domain`'s own
//! serde output.

use hummingbird_core::sync::queue::{DeadLetterEntry, DeadLetterReason, MutationIntent};
use hummingbird_core::sync::write::ReqwestMutationTransport;
use hummingbird_core::sync::{ReqwestSyncTransport, Trigger};
use hummingbird_core::{
    ActError, Core, CoreCycleOutcome, CoreEvent, CoreInitError, ItemAction, TriageDestination,
    TriagePatch,
};
use hummingbird_domain::{Energy, Item, Project, Size, Stage};

// The real, target-specific store `Core::init` resolves to internally is a
// *private* type alias (`hummingbird_core::CoreStore`) — this crate cannot
// name it. It names its own copy of the same per-target split instead
// (`calendar_host.rs`'s `StoreImpl` is the identical pattern): the
// underlying concrete type on each target (`IndexedDbSnapshotStore` /
// `FsSnapshotStore`) is public and identical either way, so this alias and
// `Core::init`'s return type unify without either side knowing about the
// other's name for it.
#[cfg(target_arch = "wasm32")]
type TaskStore = hummingbird_core::storage::IndexedDbSnapshotStore;
#[cfg(not(target_arch = "wasm32"))]
type TaskStore = hummingbird_core::storage::FsSnapshotStore;

/// One drained [`CoreEvent`], as the web host's JSON shape
/// (`client/web/src/store/protocol.ts`'s `TaskEventDTO`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TaskEventDTO {
    pub kind: &'static str,
    pub at_ms: i64,
}

fn map_event(event: CoreEvent) -> TaskEventDTO {
    match event {
        CoreEvent::CredentialNeeded { at_ms } => TaskEventDTO {
            kind: "credential_needed",
            at_ms,
        },
    }
}

/// What [`TaskHostCore::capture`] resolves to. `"failed"` covers both an
/// unrecognised `stage` string and a durability failure enqueueing the
/// capture (`SnapshotError`) — the caller has no differing recovery for
/// either, and `error` carries the detail for a log.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CaptureResponse {
    pub kind: &'static str,
    pub id: Option<String>,
    pub error: Option<String>,
}

/// What [`TaskHostCore::act`] resolves to. `"not_found"` is distinct from
/// `"failed"`: the former is "no such item to act on" (a caller mistake —
/// `Core::act`'s [`ActError::ItemNotFound`]), the latter every other
/// failure (an unrecognised `action` string, or a durability failure
/// enqueueing the mutation).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ActResponse {
    pub kind: &'static str,
    pub error: Option<String>,
}

/// What [`TaskHostCore::triage`] resolves to: `"ok"`, `"not_found"` (no such
/// item — [`ActError::ItemNotFound`]) or `"failed"` (an unrecognised
/// `destination`, an unrecognised `size`/`energy` name, or a durability
/// failure enqueueing the mutation — the caller has no differing recovery
/// for any of those). Same three-way split as [`ActResponse`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TriageResponse {
    pub kind: &'static str,
    pub error: Option<String>,
}

/// The wire-string fields a triage request carries beyond `destination` —
/// [`TaskHostCore::triage`]'s own parameter list already reads long, so
/// grouping the optional edit fields here keeps that signature to one
/// struct plus the four "which item, which mutation" scalars every other
/// method here takes individually (`seed`, `item_id`, `now_ms`).
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub struct TriageEdits {
    pub title: Option<String>,
    pub project_id: Option<String>,
    /// The wire's snake_case size name (`hummingbird_domain::Size::parse`);
    /// resolved by name through the vocabulary, never a raw index or a
    /// hardcoded id.
    pub size: Option<String>,
    /// Same "resolved by name" contract as `size`
    /// (`hummingbird_domain::Energy::parse`).
    pub energy: Option<String>,
    pub context: Option<String>,
}

/// Maps S11/#109's wire action name to [`ItemAction`] — the one place a
/// string crossing the JS boundary becomes the closed act vocabulary, the
/// same "reject before the seam" discipline [`TaskHostCore::capture`]
/// already applies to `stage`. Never a raw [`hummingbird_domain::Stage`]:
/// there is no wire action that lets a caller send an arbitrary stage id.
fn parse_action(action: &str) -> Option<ItemAction> {
    match action {
        "start" => Some(ItemAction::Start),
        "complete" => Some(ItemAction::Complete),
        "block" => Some(ItemAction::Block),
        "cancel" => Some(ItemAction::Cancel),
        _ => None,
    }
}

/// Maps S13/#111's wire destination name to [`TriageDestination`] — the one
/// place a triage promotion's target crosses the JS boundary and becomes
/// the closed destination vocabulary, same "reject before the seam"
/// discipline [`parse_action`] applies to its own wire strings. Never a raw
/// [`hummingbird_domain::Stage`]: there is no wire name that lets a caller
/// send an arbitrary stage id, and there is deliberately no `"backlog"`
/// spelling here — the owned schema has no such stage (see
/// [`TriageDestination`]'s own doc).
fn parse_destination(destination: &str) -> Option<TriageDestination> {
    match destination {
        "grilling" => Some(TriageDestination::Grilling),
        "ready" => Some(TriageDestination::Ready),
        _ => None,
    }
}

/// One item, plus whether it is currently overlaid by an unconfirmed local
/// mutation (`Core::is_pending`) — S10's "a pending item is marked as such"
/// acceptance criterion (issue #108). A wrapper around
/// [`hummingbird_domain::Item`] rather than a field added to it: `pending`
/// is a purely client-side, read-time fact about the overlay, never a
/// schema column (ADR-0001 rule 1 makes the schema itself the domain
/// model). `#[serde(flatten)]` puts `item`'s own fields at the same JSON
/// level as `pending`, so the wire shape a frontier/blocked entry produces
/// is one flat object — `task-worker.ts`'s `RawItem` just gains the one
/// extra key.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FrontierItemDTO {
    #[serde(flatten)]
    pub item: Item,
    pub pending: bool,
}

/// The wrapper around a live read ([`TaskHostCore::frontier`] /
/// [`TaskHostCore::triage_inbox`]): `"busy"` when the core is checked out
/// mid-poll, carrying no items — the same "no new information, don't blank
/// the view" contract `calendar_host.rs`'s `CalendarListResponse` documents.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ItemListResponse {
    pub kind: &'static str,
    pub items: Vec<FrontierItemDTO>,
}

/// One [`TaskHostCore::blocked`] entry: an item and the open blockers
/// [`Core::blocked`] paired it with — S10's "relation-blocked … the reason
/// visible" (issue #108).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BlockedEntryDTO {
    pub item: FrontierItemDTO,
    pub blocked_by: Vec<FrontierItemDTO>,
}

/// The wrapper around [`TaskHostCore::blocked`]'s answer. Same `"busy"`
/// contract as [`ItemListResponse`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BlockedListResponse {
    pub kind: &'static str,
    pub entries: Vec<BlockedEntryDTO>,
}

/// The wrapper around [`TaskHostCore::steps`]'s answer — item detail's
/// checklist (issue #96, S10). Same `"busy"` contract as [`ItemListResponse`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct StepListResponse {
    pub kind: &'static str,
    pub steps: Vec<hummingbird_domain::Step>,
}

/// The wrapper around [`TaskHostCore::projects`]'s answer — resolves a
/// `TaskItemDTO.projectId` to a real name for the frontier's "grouped by
/// project" display (issue #108, PR #200 review). Same `"busy"` contract as
/// [`ItemListResponse`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProjectListResponse {
    pub kind: &'static str,
    pub projects: Vec<Project>,
}

/// The wrapper around [`TaskHostCore::is_pending`]'s answer.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IsPendingResponse {
    pub kind: &'static str,
    pub pending: bool,
}

/// Maps a [`CoreCycleOutcome`] to the stable string name the web host's
/// protocol (`client/web/src/store/protocol.ts`) matches on, plus whatever
/// payload the S9 "1 edit didn't apply" / sync-status affordance needs.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RunResponse {
    pub kind: &'static str,
    pub retry_after_ms: Option<i64>,
    pub active_item_count: Option<usize>,
    pub was_full_sweep: Option<bool>,
    pub dead_lettered: Option<usize>,
}

fn run_response(kind: &'static str) -> RunResponse {
    RunResponse {
        kind,
        retry_after_ms: None,
        active_item_count: None,
        was_full_sweep: None,
        dead_lettered: None,
    }
}

fn map_run_outcome(outcome: CoreCycleOutcome) -> RunResponse {
    match outcome {
        CoreCycleOutcome::NoCredential => run_response("no_credential"),
        CoreCycleOutcome::Held => run_response("held"),
        CoreCycleOutcome::Cycle(cycle) => match cycle {
            hummingbird_core::sync::CycleOutcome::Skipped => run_response("skipped"),
            hummingbird_core::sync::CycleOutcome::Blocked { drain, retry_after_ms } => {
                RunResponse {
                    dead_lettered: Some(drain.dead_lettered()),
                    retry_after_ms: Some(retry_after_ms),
                    ..run_response("blocked")
                }
            }
            hummingbird_core::sync::CycleOutcome::CredentialNeeded { drain } => RunResponse {
                dead_lettered: Some(drain.dead_lettered()),
                ..run_response("credential_needed")
            },
            hummingbird_core::sync::CycleOutcome::PersistFailed { retry_after_ms, .. } => {
                RunResponse {
                    retry_after_ms: Some(retry_after_ms),
                    ..run_response("persist_failed")
                }
            }
            hummingbird_core::sync::CycleOutcome::PullFailed { drain, retry_after_ms } => {
                RunResponse {
                    dead_lettered: Some(drain.dead_lettered()),
                    retry_after_ms: Some(retry_after_ms),
                    ..run_response("pull_failed")
                }
            }
            hummingbird_core::sync::CycleOutcome::Completed {
                drain,
                active_item_count,
                was_full_sweep,
            } => RunResponse {
                dead_lettered: Some(drain.dead_lettered()),
                active_item_count: Some(active_item_count),
                was_full_sweep: Some(was_full_sweep),
                ..run_response("completed")
            },
        },
    }
}

/// The wrapper around [`TaskHostCore::queue_depth`]'s answer — S9's
/// sync-status "queued" figure.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct QueueDepthResponse {
    pub kind: &'static str,
    pub depth: usize,
}

/// One field a dead-lettered [`hummingbird_core::sync::queue::DeadLetterReason::Conflict`]
/// disagreed on — S9's "1 edit didn't apply" affordance shows exactly this
/// triple per field so a person can judge whose value to keep.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DeadLetterFieldDTO {
    pub field: String,
    pub local: serde_json::Value,
    pub server: serde_json::Value,
}

/// One dead-lettered entry, as the web host's JSON shape. `"permanent"`
/// carries `message` and no `fields` (there is no local/server disagreement
/// to show — the write itself was rejected outright); `"conflict"` carries
/// `fields` and no `message`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DeadLetterEntryDTO {
    pub id: String,
    pub reason: &'static str,
    pub message: Option<String>,
    pub fields: Vec<DeadLetterFieldDTO>,
    pub at_ms: i64,
}

/// The wrapper around [`TaskHostCore::dead_letters`]'s answer.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DeadLettersResponse {
    pub kind: &'static str,
    pub entries: Vec<DeadLetterEntryDTO>,
}

/// The wrapper around [`TaskHostCore::mirror_snapshot`]'s answer — S9's
/// mirror download button.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MirrorSnapshotResponse {
    pub kind: &'static str,
    pub mirror: serde_json::Value,
}

/// The touched fields' *intended* (local) values a [`MutationIntent`]
/// carries — only a `Patch` has any; a `Create` never conflicts (deterministic
/// ids, ADR-0007), so it is never the intent behind a `Conflict` reason.
fn local_field_values(intent: &MutationIntent) -> serde_json::Map<String, serde_json::Value> {
    match intent {
        MutationIntent::Patch { patch_fields, .. } => {
            patch_fields.as_object().cloned().unwrap_or_default()
        }
        MutationIntent::Create { .. } => serde_json::Map::new(),
    }
}

fn map_dead_letter(entry: &DeadLetterEntry) -> DeadLetterEntryDTO {
    match &entry.reason {
        DeadLetterReason::Permanent(message) => DeadLetterEntryDTO {
            id: entry.entry.id.clone(),
            reason: "permanent",
            message: Some(message.clone()),
            fields: Vec::new(),
            at_ms: entry.at_ms,
        },
        DeadLetterReason::Conflict { fields, current } => {
            let local = local_field_values(&entry.entry.intent);
            let mapped_fields = fields
                .iter()
                .map(|field| DeadLetterFieldDTO {
                    field: field.clone(),
                    local: local.get(field).cloned().unwrap_or(serde_json::Value::Null),
                    server: current.get(field).cloned().unwrap_or(serde_json::Value::Null),
                })
                .collect();
            DeadLetterEntryDTO {
                id: entry.entry.id.clone(),
                reason: "conflict",
                message: None,
                fields: mapped_fields,
                at_ms: entry.at_ms,
            }
        }
    }
}

/// Plain-Rust wrapper over one owned-schema [`Core`], holding exactly the
/// operations the web host needs plus the two live `reqwest`-backed
/// transports [`Core::run`] takes as call-time arguments.
pub struct TaskHostCore {
    core: Core<TaskStore, TaskStore>,
    read_transport: ReqwestSyncTransport,
    write_transport: ReqwestMutationTransport,
}

impl TaskHostCore {
    /// `base_url` is the authority's origin, host-supplied per ADR-0003 —
    /// this crate invents no deployment address of its own. `api_key` is
    /// whatever credential the host already holds at construction time
    /// (empty until #106/S8 lands a device-token entry flow); it is never
    /// persisted by `Core::init` (see that method's own doc), and
    /// [`TaskHostCore::push_api_key`] is how a host supplies — or rotates —
    /// a real one afterwards.
    pub async fn init(
        namespace: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, CoreInitError> {
        let base_url = base_url.into();
        let core = Core::init(namespace, api_key).await?;
        let client = reqwest::Client::new();
        let read_transport = ReqwestSyncTransport::new(client.clone(), base_url.clone());
        let write_transport = ReqwestMutationTransport::new(client, base_url);
        Ok(Self {
            core,
            read_transport,
            write_transport,
        })
    }

    /// The host calls this at startup (once a stored device token is known)
    /// and on every rotation. Always resumes a hold — see
    /// [`Core::push_api_key`].
    pub fn push_api_key(&mut self, api_key: String) {
        self.core.push_api_key(api_key);
    }

    /// "Forget token" (#106/S8): clears the in-memory credential this host
    /// holds. Never persisted in the first place (`Core::init`'s own doc),
    /// so there is nothing durable to clean up here — see
    /// [`Core::clear_api_key`] for why this reports `no_credential`, not
    /// `held`, on the next [`TaskHostCore::run`].
    pub fn clear_api_key(&mut self) {
        self.core.clear_api_key();
    }

    /// Whether `item_id` currently has an unconfirmed capture overlaid on
    /// it.
    pub fn is_pending(&self, item_id: &str) -> IsPendingResponse {
        IsPendingResponse {
            kind: "ok",
            pending: self.core.is_pending(item_id),
        }
    }

    /// Wraps `item` with whether it is currently overlaid — the one place
    /// every frontier/triage/blocked read stamps [`FrontierItemDTO::pending`],
    /// so it is computed the same way (`Core::is_pending`) everywhere it
    /// appears rather than risking the answer drifting between call sites.
    fn with_pending(&self, item: Item) -> FrontierItemDTO {
        let pending = self.core.is_pending(&item.id);
        FrontierItemDTO { item, pending }
    }

    /// The frontier — what can be started right now, per [`Core::frontier`].
    /// Each item carries whether it is still an unconfirmed local capture
    /// (issue #108's "a pending item is marked as such"): the only true
    /// runtime source of that fact is `Core::is_pending`, so it is stamped
    /// here rather than left to a caller that would otherwise need one
    /// `isPending` request per item.
    pub fn frontier(&self) -> ItemListResponse {
        ItemListResponse {
            kind: "ok",
            items: self
                .core
                .frontier()
                .into_iter()
                .map(|item| self.with_pending(item))
                .collect(),
        }
    }

    /// The triage inbox — captured, not yet promoted, per
    /// [`Core::triage_inbox`]. Same per-item `pending` stamp as
    /// [`TaskHostCore::frontier`].
    pub fn triage_inbox(&self) -> ItemListResponse {
        ItemListResponse {
            kind: "ok",
            items: self
                .core
                .triage_inbox()
                .into_iter()
                .map(|item| self.with_pending(item))
                .collect(),
        }
    }

    /// Relation-blocked items with the reason visible, per [`Core::blocked`].
    /// Same per-item `pending` stamp as [`TaskHostCore::frontier`], on both
    /// the blocked item and the blockers it is paired with.
    pub fn blocked(&self) -> BlockedListResponse {
        BlockedListResponse {
            kind: "ok",
            entries: self
                .core
                .blocked()
                .into_iter()
                .map(|(item, blocked_by)| BlockedEntryDTO {
                    item: self.with_pending(item),
                    blocked_by: blocked_by.into_iter().map(|b| self.with_pending(b)).collect(),
                })
                .collect(),
        }
    }

    /// One item's Steps, per [`Core::steps_for`] — item detail (issue #96).
    pub fn steps(&self, item_id: &str) -> StepListResponse {
        StepListResponse {
            kind: "ok",
            steps: self.core.steps_for(item_id),
        }
    }

    /// Every live project, per [`Core::projects`] — resolves the frontier's
    /// grouping to real project names (issue #108, PR #200 review).
    pub fn projects(&self) -> ProjectListResponse {
        ProjectListResponse {
            kind: "ok",
            projects: self.core.projects(),
        }
    }

    /// Drains every [`CoreEvent`] since the last drain, mapped to this
    /// host's JSON shape.
    pub fn take_events(&mut self) -> Vec<TaskEventDTO> {
        self.core.take_events().into_iter().map(map_event).collect()
    }

    /// Captures a new item. `stage` is the wire's snake_case stage name
    /// (`hummingbird_domain::Stage::parse`); an unrecognised one fails
    /// without ever touching [`Core::capture`], the same "reject before the
    /// seam" discipline `calendar_host.rs` uses for its own inputs.
    pub async fn capture(
        &mut self,
        seed: &str,
        title: &str,
        stage: &str,
        now_ms: i64,
    ) -> CaptureResponse {
        let Some(stage) = Stage::parse(stage) else {
            return CaptureResponse {
                kind: "failed",
                id: None,
                error: Some(format!("unrecognised stage {stage:?}")),
            };
        };
        match self.core.capture(seed, title, stage, now_ms).await {
            Ok(id) => CaptureResponse {
                kind: "ok",
                id: Some(id),
                error: None,
            },
            Err(error) => CaptureResponse {
                kind: "failed",
                id: None,
                error: Some(error.to_string()),
            },
        }
    }

    /// Acts on an already-existing item (S11/#109: start, complete, block,
    /// cancel). `action` is the wire's snake_case action name
    /// ([`parse_action`]); an unrecognised one fails without ever touching
    /// [`Core::act`], the same "reject before the seam" discipline
    /// [`TaskHostCore::capture`] uses for `stage`.
    pub async fn act(&mut self, seed: &str, item_id: &str, action: &str, now_ms: i64) -> ActResponse {
        let Some(action) = parse_action(action) else {
            return ActResponse {
                kind: "failed",
                error: Some(format!("unrecognised action {action:?}")),
            };
        };
        match self.core.act(seed, item_id, action, now_ms).await {
            Ok(()) => ActResponse { kind: "ok", error: None },
            Err(ActError::ItemNotFound) => ActResponse {
                kind: "not_found",
                error: Some("item not found".to_string()),
            },
            Err(error) => ActResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
        }
    }

    /// Triages an already-captured item (S13/#111): edits whatever
    /// `edits` sets and promotes it to `destination`, as one CAS `PATCH`
    /// (never four separate mutations — [`Core::triage`]'s own doc).
    /// `destination` is the wire's snake_case destination name
    /// ([`parse_destination`]); `edits.size`/`edits.energy` are each
    /// resolved by name through `hummingbird_domain`'s own vocabulary
    /// (`Size::parse`/`Energy::parse`). Any unrecognised name fails without
    /// ever touching [`Core::triage`], the same "reject before the seam"
    /// discipline [`TaskHostCore::capture`]/[`TaskHostCore::act`] use for
    /// their own inputs.
    pub async fn triage(
        &mut self,
        seed: &str,
        item_id: &str,
        destination: &str,
        edits: TriageEdits,
        now_ms: i64,
    ) -> TriageResponse {
        let Some(destination) = parse_destination(destination) else {
            return TriageResponse {
                kind: "failed",
                error: Some(format!("unrecognised triage destination {destination:?}")),
            };
        };
        let size = match edits.size {
            Some(raw) => match Size::parse(&raw) {
                Some(size) => Some(size),
                None => {
                    return TriageResponse {
                        kind: "failed",
                        error: Some(format!("unrecognised size {raw:?}")),
                    };
                }
            },
            None => None,
        };
        let energy = match edits.energy {
            Some(raw) => match Energy::parse(&raw) {
                Some(energy) => Some(energy),
                None => {
                    return TriageResponse {
                        kind: "failed",
                        error: Some(format!("unrecognised energy {raw:?}")),
                    };
                }
            },
            None => None,
        };
        let patch = TriagePatch {
            title: edits.title,
            project_id: edits.project_id,
            size,
            energy,
            context: edits.context,
        };
        match self.core.triage(seed, item_id, destination, patch, now_ms).await {
            Ok(()) => TriageResponse { kind: "ok", error: None },
            Err(ActError::ItemNotFound) => TriageResponse {
                kind: "not_found",
                error: Some("item not found".to_string()),
            },
            Err(error) => TriageResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
        }
    }

    /// Runs one [`Core::run`] cycle against the live `reqwest` transports.
    pub async fn run(
        &mut self,
        now_ms: i64,
        trigger: &str,
        force_full_sweep: bool,
        jitter_unit: f64,
    ) -> RunResponse {
        let trigger = match trigger {
            "timer" => Trigger::Timer,
            _ => Trigger::User,
        };
        let outcome = self
            .core
            .run(
                &self.read_transport,
                &self.write_transport,
                now_ms,
                trigger,
                force_full_sweep,
                jitter_unit,
            )
            .await;
        map_run_outcome(outcome)
    }

    /// The outbound queue's current depth — S9's sync-status "queued"
    /// figure.
    pub fn queue_depth(&self) -> QueueDepthResponse {
        QueueDepthResponse {
            kind: "ok",
            depth: self.core.queue_depth(),
        }
    }

    /// Every dead-lettered entry, mapped to this host's JSON shape — S9's
    /// "1 edit didn't apply" affordance.
    pub fn dead_letters(&self) -> DeadLettersResponse {
        DeadLettersResponse {
            kind: "ok",
            entries: self.core.dead_letters().iter().map(map_dead_letter).collect(),
        }
    }

    /// The local mirror, serialized whole — S9's mirror download button.
    pub fn mirror_snapshot(&self) -> MirrorSnapshotResponse {
        MirrorSnapshotResponse {
            kind: "ok",
            mirror: self.core.mirror_snapshot(),
        }
    }
}

#[cfg(test)]
mod act_tests {
    use super::*;

    #[test]
    fn act_response_serializes_with_the_exact_keys_task_worker_ts_parses() {
        let ok = ActResponse { kind: "ok", error: None };
        assert_eq!(serde_json::to_string(&ok).unwrap(), r#"{"kind":"ok","error":null}"#);

        let not_found = ActResponse {
            kind: "not_found",
            error: Some("item not found".to_string()),
        };
        assert_eq!(
            serde_json::to_string(&not_found).unwrap(),
            r#"{"kind":"not_found","error":"item not found"}"#
        );
    }

    #[tokio::test]
    async fn acting_with_an_unrecognised_action_never_reaches_core_act() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-act-1");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "buy milk", "ready", 1_000).await;
        let id = host.frontier().items[0].item.id.clone();

        let response = host.act("seed-act-1", &id, "not-an-action", 2_000).await;

        assert_eq!(response.kind, "failed");
        assert!(response.error.is_some());
        // The item is untouched: still Ready, not overlaid by a second
        // mutation.
        assert_eq!(host.frontier().items.len(), 1);
    }

    #[tokio::test]
    async fn acting_on_an_unknown_item_id_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-act-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host.act("seed-act-1", "no-such-item", "start", 1_000).await;

        assert_eq!(response.kind, "not_found");
        assert!(response.error.is_some());
    }

    #[tokio::test]
    async fn completing_a_captured_item_shows_done_immediately_offline() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-act-3");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "buy milk", "ready", 1_000).await;
        let id = host.frontier().items[0].item.id.clone();

        let response = host.act("seed-act-1", &id, "complete", 2_000).await;

        assert_eq!(response.kind, "ok");
        assert!(response.error.is_none());
        assert!(host.is_pending(&id).pending);
        assert_eq!(
            host.frontier().items.len(),
            0,
            "a completed item drops off the frontier immediately"
        );
    }

    #[tokio::test]
    async fn blocking_an_item_never_shows_up_as_a_relation_block() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-act-4");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "buy milk", "ready", 1_000).await;
        let id = host.frontier().items[0].item.id.clone();

        let response = host.act("seed-act-1", &id, "block", 2_000).await;

        assert_eq!(response.kind, "ok");
        assert_eq!(
            host.blocked(),
            BlockedListResponse { kind: "ok", entries: Vec::new() },
            "Stage::Blocked is never expressed through the relation-blocked query"
        );
    }

    #[tokio::test]
    async fn cancelling_an_item_drops_it_from_the_frontier() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-act-5");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "buy milk", "ready", 1_000).await;
        let id = host.frontier().items[0].item.id.clone();

        let response = host.act("seed-act-1", &id, "cancel", 2_000).await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.frontier().items.len(), 0);
    }
}

#[cfg(test)]
mod triage_tests {
    use super::*;

    #[test]
    fn triage_response_serializes_with_the_exact_keys_task_worker_ts_parses() {
        let ok = TriageResponse { kind: "ok", error: None };
        assert_eq!(serde_json::to_string(&ok).unwrap(), r#"{"kind":"ok","error":null}"#);
    }

    #[tokio::test]
    async fn triaging_with_an_unrecognised_destination_never_reaches_core_triage() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-triage-1");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "someday maybe", "triage", 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();

        let response = host
            .triage("seed-triage-1", &id, "backlog", TriageEdits::default(), 2_000)
            .await;

        assert_eq!(response.kind, "failed");
        assert!(response.error.is_some());
        assert_eq!(host.triage_inbox().items.len(), 1, "the item is untouched");
    }

    #[tokio::test]
    async fn triaging_with_an_unrecognised_size_never_reaches_core_triage() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-triage-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "someday maybe", "triage", 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();

        let response = host
            .triage(
                "seed-triage-1",
                &id,
                "ready",
                TriageEdits { size: Some("giant".to_string()), ..TriageEdits::default() },
                2_000,
            )
            .await;

        assert_eq!(response.kind, "failed");
        assert!(response.error.is_some());
        assert_eq!(host.triage_inbox().items.len(), 1, "the item is untouched");
    }

    #[tokio::test]
    async fn triaging_on_an_unknown_item_id_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-triage-3");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host
            .triage("seed-triage-1", "no-such-item", "ready", TriageEdits::default(), 1_000)
            .await;

        assert_eq!(response.kind, "not_found");
        assert!(response.error.is_some());
    }

    /// This issue's headline acceptance: a triaged item leaves the triage
    /// query and appears on the frontier — through the same `Core` overlay
    /// every other read here goes through.
    #[tokio::test]
    async fn promoting_to_ready_moves_the_item_from_triage_to_the_frontier() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-triage-4");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "someday maybe", "triage", 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();

        let response = host
            .triage(
                "seed-triage-1",
                &id,
                "ready",
                TriageEdits {
                    title: Some("buy milk".to_string()),
                    project_id: Some("project-1".to_string()),
                    size: Some("quick".to_string()),
                    energy: Some("low".to_string()),
                    context: Some("@errands".to_string()),
                },
                2_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.triage_inbox().items.len(), 0);
        let frontier = host.frontier();
        assert_eq!(frontier.items.len(), 1);
        let item = &frontier.items[0].item;
        assert_eq!(item.title, "buy milk");
        assert_eq!(item.project_id.as_deref(), Some("project-1"));
        assert!(item.size.is_some());
        assert!(item.energy.is_some());
        assert_eq!(item.context.as_deref(), Some("@errands"));
        assert!(frontier.items[0].pending, "an unconfirmed triage must read as pending");
    }

    #[tokio::test]
    async fn sending_to_grilling_leaves_the_triage_inbox_without_reaching_the_frontier() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-triage-5");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "someday maybe", "triage", 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();

        let response = host
            .triage("seed-triage-1", &id, "grilling", TriageEdits::default(), 2_000)
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.triage_inbox().items.len(), 0);
        assert_eq!(host.frontier().items.len(), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_fresh_host_has_no_snapshot_and_no_pending_items() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-1");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        assert_eq!(host.frontier(), ItemListResponse { kind: "ok", items: Vec::new() });
        assert_eq!(
            host.triage_inbox(),
            ItemListResponse { kind: "ok", items: Vec::new() }
        );
        assert_eq!(
            host.is_pending("some-id"),
            IsPendingResponse { kind: "ok", pending: false }
        );
    }

    #[tokio::test]
    async fn capturing_with_an_unrecognised_stage_never_reaches_core_capture() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host.capture("seed-1", "buy milk", "not-a-stage", 1_000).await;

        assert_eq!(response.kind, "failed");
        assert!(response.id.is_none());
        assert!(response.error.is_some());
        assert_eq!(host.frontier().items.len(), 0);
    }

    #[tokio::test]
    async fn a_capture_is_readable_from_the_frontier_and_marked_pending() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-3");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host.capture("seed-1", "buy milk", "ready", 1_000).await;

        assert_eq!(response.kind, "ok");
        let id = response.id.clone().unwrap();
        assert!(host.is_pending(&id).pending);
        let frontier = host.frontier();
        assert_eq!(frontier.items.len(), 1);
        assert_eq!(frontier.items[0].item.title, "buy milk");
        assert!(
            frontier.items[0].pending,
            "a still-queued capture must be marked pending on the frontier item itself, \
             not just answerable via a separate isPending request"
        );
        // A `Stage::Triage` (the default in `Core`, but here explicit via
        // "ready") capture is not on the triage inbox.
        assert_eq!(host.triage_inbox().items.len(), 0);
    }

    /// `Core::is_pending` itself (and the overlay-clearing behaviour once a
    /// sweep confirms or dead-letters a capture) is exhaustively covered at
    /// the `client/core` layer (`a_sweep_confirming_the_capture_removes_the_overlay_with_no_gap`,
    /// `a_dead_lettered_capture_removes_the_overlay_and_reverts_to_server_truth`).
    /// `with_pending` is a one-line pass-through with no branching logic of
    /// its own, so what this layer needs to pin is the wire shape it
    /// produces for both states — covered below by
    /// `frontier_item_dto_serializes_pending_alongside_the_flattened_item_fields`.

    #[tokio::test]
    async fn a_triage_stage_capture_lands_on_the_inbox_not_the_frontier() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-4");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        host.capture("seed-1", "someday maybe", "triage", 1_000).await;

        assert_eq!(host.frontier().items.len(), 0);
        assert_eq!(host.triage_inbox().items.len(), 1);
    }

    #[tokio::test]
    async fn a_fresh_host_reports_no_blocked_items_and_no_steps() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-blocked-1");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        assert_eq!(host.blocked(), BlockedListResponse { kind: "ok", entries: Vec::new() });
        assert_eq!(host.steps("some-id"), StepListResponse { kind: "ok", steps: Vec::new() });
        assert_eq!(host.projects(), ProjectListResponse { kind: "ok", projects: Vec::new() });
    }

    #[tokio::test]
    async fn no_api_key_ever_pushed_reports_no_credential_without_touching_the_network() {
        // `TaskHostCore::init`'s `api_key` argument is empty here — the
        // pre-#106 provisional value `core.worker.ts` starts every task
        // host with — so this is the real "device has never connected"
        // state as `Core::run` sees it... except `Core::init` always pushes
        // *something* (even ""), so this actually exercises the pull-failure
        // path below, not `CoreCycleOutcome::NoCredential` (which only
        // `Core::new` — never `Core::init` — can produce). See the finding
        // posted on #105.
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-5");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host.run(1_000, "user", true, 0.0).await;

        // An empty `base_url` builds a relative URL, which `reqwest` rejects
        // before ever opening a socket — deterministic and network-free.
        assert_eq!(response.kind, "pull_failed");
    }

    #[tokio::test]
    async fn clearing_the_key_reports_a_genuine_no_credential_without_touching_the_network() {
        // Unlike the fresh-init case above (`Core::init`'s `""` still counts
        // as *a* pushed key, so a fresh host actually exercises
        // `pull_failed`), `clear_api_key` removes the key outright — this is
        // the one path in this file that reaches a real
        // `CoreCycleOutcome::NoCredential`, network-free even though
        // `base_url` here is a real, well-formed relative path that would
        // otherwise be attempted.
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-clear-1");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "device-token")
            .await
            .unwrap();

        host.clear_api_key();
        let response = host.run(1_000, "user", true, 0.0).await;

        assert_eq!(response.kind, "no_credential");
    }

    #[tokio::test]
    async fn clearing_never_touches_a_pending_capture() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-clear-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "device-token")
            .await
            .unwrap();
        let response = host.capture("seed-1", "buy milk", "ready", 1_000).await;
        let id = response.id.unwrap();

        host.clear_api_key();

        assert!(host.is_pending(&id).pending);
        assert_eq!(host.frontier().items.len(), 1);
    }

    #[tokio::test]
    async fn a_timer_trigger_is_accepted_and_a_user_trigger_is_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-6");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        // Both trigger spellings reach `Core::run` rather than panicking on
        // an unrecognised string; the exact cycle outcome is `client/core`'s
        // own concern (263 tests already pin it), not this wrapper's.
        let timer = host.run(1_000, "timer", true, 0.0).await;
        let user = host.run(2_000, "anything-else", true, 0.0).await;
        assert_eq!(timer.kind, "pull_failed");
        assert_eq!(user.kind, "pull_failed");
    }

    // -------------------------------------------------- S9 sync-status reads

    #[tokio::test]
    async fn a_fresh_host_reports_zero_queue_depth_and_no_dead_letters() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-queue-1");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "device-token")
            .await
            .unwrap();

        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 0 });
        assert_eq!(
            host.dead_letters(),
            DeadLettersResponse { kind: "ok", entries: Vec::new() }
        );
    }

    #[tokio::test]
    async fn a_capture_raises_the_queue_depth() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-queue-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "device-token")
            .await
            .unwrap();

        host.capture("seed-1", "buy milk", "ready", 1_000).await;

        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
    }

    #[tokio::test]
    async fn a_fresh_host_serializes_a_readable_mirror_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-mirror-1");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "device-token")
            .await
            .unwrap();

        let response = host.mirror_snapshot();
        assert_eq!(response.kind, "ok");
        assert!(response.mirror.is_object());
    }

    #[tokio::test]
    async fn draining_events_twice_yields_nothing_new() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-7");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        assert_eq!(host.take_events(), Vec::new());
        assert_eq!(host.take_events(), Vec::new());
    }

    // ---------------------------------------------------- map_run_outcome
    //
    // `TaskHostCore::run` can only ever be driven, in this file's own
    // network-free tests, down the one branch an invalid `base_url` forces
    // (`"pull_failed"`, with a fixed drain outcome). That leaves the other
    // five `CoreCycleOutcome`/`CycleOutcome` branches, and most of
    // `RunResponse`'s payload fields, completely unexercised without these
    // — `map_run_outcome` is called directly, against hand-built
    // `CoreCycleOutcome` values, the same way `client/core`'s own tests
    // build `CycleOutcome` values without a real cycle.

    use hummingbird_core::sync::{CycleOutcome, DrainOutcome};

    #[test]
    fn maps_no_credential_and_held_with_every_payload_field_empty() {
        assert_eq!(
            map_run_outcome(CoreCycleOutcome::NoCredential),
            RunResponse {
                kind: "no_credential",
                retry_after_ms: None,
                active_item_count: None,
                was_full_sweep: None,
                dead_lettered: None,
            }
        );
        assert_eq!(
            map_run_outcome(CoreCycleOutcome::Held),
            RunResponse {
                kind: "held",
                retry_after_ms: None,
                active_item_count: None,
                was_full_sweep: None,
                dead_lettered: None,
            }
        );
    }

    #[test]
    fn maps_a_skipped_cycle_with_every_payload_field_empty() {
        assert_eq!(
            map_run_outcome(CoreCycleOutcome::Cycle(CycleOutcome::Skipped)),
            RunResponse {
                kind: "skipped",
                retry_after_ms: None,
                active_item_count: None,
                was_full_sweep: None,
                dead_lettered: None,
            }
        );
    }

    #[test]
    fn maps_a_blocked_cycle_carrying_its_dead_letter_count_and_retry_delay() {
        let outcome = CoreCycleOutcome::Cycle(CycleOutcome::Blocked {
            drain: DrainOutcome::Blocked { dead_lettered: 2 },
            retry_after_ms: 500,
        });

        assert_eq!(
            map_run_outcome(outcome),
            RunResponse {
                kind: "blocked",
                retry_after_ms: Some(500),
                active_item_count: None,
                was_full_sweep: None,
                dead_lettered: Some(2),
            }
        );
    }

    #[test]
    fn maps_a_credential_needed_cycle_carrying_its_dead_letter_count_but_no_retry_delay() {
        // Distinct from `Blocked`/`PullFailed`: a 401 is a hold, not a
        // backoff, so there is no `retry_after_ms` to carry — the field
        // must stay `None`, not accidentally default to some prior value.
        let outcome = CoreCycleOutcome::Cycle(CycleOutcome::CredentialNeeded {
            drain: DrainOutcome::CredentialNeeded { dead_lettered: 1 },
        });

        assert_eq!(
            map_run_outcome(outcome),
            RunResponse {
                kind: "credential_needed",
                retry_after_ms: None,
                active_item_count: None,
                was_full_sweep: None,
                dead_lettered: Some(1),
            }
        );
    }

    #[test]
    fn maps_a_persist_failed_cycle_carrying_only_its_retry_delay() {
        // The one variant with deliberately no `drain` (see `CycleOutcome`'s
        // own doc) — `dead_lettered` must stay `None`, not `Some(0)`.
        let outcome = CoreCycleOutcome::Cycle(CycleOutcome::PersistFailed {
            message: "disk full".to_string(),
            retry_after_ms: 750,
        });

        assert_eq!(
            map_run_outcome(outcome),
            RunResponse {
                kind: "persist_failed",
                retry_after_ms: Some(750),
                active_item_count: None,
                was_full_sweep: None,
                dead_lettered: None,
            }
        );
    }

    #[test]
    fn maps_a_pull_failed_cycle_carrying_its_dead_letter_count_and_retry_delay() {
        let outcome = CoreCycleOutcome::Cycle(CycleOutcome::PullFailed {
            drain: DrainOutcome::Completed { dead_lettered: 3 },
            retry_after_ms: 100,
        });

        assert_eq!(
            map_run_outcome(outcome),
            RunResponse {
                kind: "pull_failed",
                retry_after_ms: Some(100),
                active_item_count: None,
                was_full_sweep: None,
                dead_lettered: Some(3),
            }
        );
    }

    #[test]
    fn maps_a_completed_cycle_carrying_every_payload_field_but_no_retry_delay() {
        let outcome = CoreCycleOutcome::Cycle(CycleOutcome::Completed {
            drain: DrainOutcome::Completed { dead_lettered: 0 },
            active_item_count: 5,
            was_full_sweep: true,
        });

        assert_eq!(
            map_run_outcome(outcome),
            RunResponse {
                kind: "completed",
                retry_after_ms: None,
                active_item_count: Some(5),
                was_full_sweep: Some(true),
                dead_lettered: Some(0),
            }
        );
    }

    // ----------------------------------------------------- map_dead_letter
    //
    // Same reasoning as `map_run_outcome`'s tests above: this file's own
    // network-free `TaskHostCore::run` tests never actually reach a
    // `Conflict` dead-letter (that needs a real 409 rebase), so
    // `map_dead_letter` is exercised directly against hand-built
    // `DeadLetterEntry` values instead.

    use hummingbird_core::sync::queue::QueueEntry;

    #[test]
    fn a_permanent_dead_letter_carries_its_message_and_no_fields() {
        let entry = DeadLetterEntry {
            entry: QueueEntry {
                id: "item-1".to_string(),
                intent: MutationIntent::Create {
                    path: "/api/items".to_string(),
                    body: serde_json::json!({"title": "buy milk"}),
                },
            },
            reason: DeadLetterReason::Permanent("validation".to_string()),
            at_ms: 5_000,
        };

        assert_eq!(
            map_dead_letter(&entry),
            DeadLetterEntryDTO {
                id: "item-1".to_string(),
                reason: "permanent",
                message: Some("validation".to_string()),
                fields: Vec::new(),
                at_ms: 5_000,
            }
        );
    }

    #[test]
    fn a_conflict_dead_letter_pairs_each_named_field_with_its_local_and_server_value() {
        let entry = DeadLetterEntry {
            entry: QueueEntry {
                id: "item-1".to_string(),
                intent: MutationIntent::Patch {
                    path: "/api/items/item-1".to_string(),
                    method: hummingbird_core::sync::write::transport::HttpMethod::Patch,
                    base: serde_json::json!({"title": "buy milk", "version": 1}),
                    base_updated_at: 1_000,
                    patch_fields: serde_json::json!({"title": "buy oat milk"}),
                },
            },
            reason: DeadLetterReason::Conflict {
                fields: vec!["title".to_string()],
                current: serde_json::json!({"title": "someone else's", "version": 2}),
            },
            at_ms: 6_000,
        };

        assert_eq!(
            map_dead_letter(&entry),
            DeadLetterEntryDTO {
                id: "item-1".to_string(),
                reason: "conflict",
                message: None,
                fields: vec![DeadLetterFieldDTO {
                    field: "title".to_string(),
                    local: serde_json::json!("buy oat milk"),
                    server: serde_json::json!("someone else's"),
                }],
                at_ms: 6_000,
            }
        );
    }

    #[test]
    fn a_conflicting_field_absent_from_the_servers_current_entity_maps_to_null() {
        // Defensive: the server's `current` entity is whatever it chose to
        // send back on a 409, and this file has no control over that shape
        // — a field this queue thinks conflicted but that `current` simply
        // omits must render as an honest "no value", not panic or silently
        // drop the row.
        let entry = DeadLetterEntry {
            entry: QueueEntry {
                id: "item-1".to_string(),
                intent: MutationIntent::Patch {
                    path: "/api/items/item-1".to_string(),
                    method: hummingbird_core::sync::write::transport::HttpMethod::Patch,
                    base: serde_json::json!({"version": 1}),
                    base_updated_at: 1_000,
                    patch_fields: serde_json::json!({}),
                },
            },
            reason: DeadLetterReason::Conflict {
                fields: vec!["context".to_string()],
                current: serde_json::json!({"version": 2}),
            },
            at_ms: 7_000,
        };

        assert_eq!(
            map_dead_letter(&entry).fields,
            vec![DeadLetterFieldDTO {
                field: "context".to_string(),
                local: serde_json::Value::Null,
                server: serde_json::Value::Null,
            }]
        );
    }

    // ------------------------------------------------ wire shape pinning
    //
    // `task-worker.ts`'s hand-written `Raw*` TypeScript interfaces parse
    // these exact key names and `kind` string literals — nothing on that
    // side re-derives the shape from this crate's serde output, so a field
    // rename or a literal typo here would silently desync the two without
    // any test failing on either side unless the shape itself is pinned.

    #[test]
    fn capture_response_serializes_with_the_exact_keys_task_worker_ts_parses() {
        let ok = CaptureResponse {
            kind: "ok",
            id: Some("item-1".to_string()),
            error: None,
        };
        assert_eq!(
            serde_json::to_string(&ok).unwrap(),
            r#"{"kind":"ok","id":"item-1","error":null}"#
        );

        let failed = CaptureResponse {
            kind: "failed",
            id: None,
            error: Some("boom".to_string()),
        };
        assert_eq!(
            serde_json::to_string(&failed).unwrap(),
            r#"{"kind":"failed","id":null,"error":"boom"}"#
        );
    }

    #[test]
    fn item_list_response_serializes_with_the_exact_keys_task_worker_ts_parses() {
        let response = ItemListResponse {
            kind: "ok",
            items: Vec::new(),
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"kind":"ok","items":[]}"#
        );
    }

    fn fixture_item(id: &str) -> Item {
        Item {
            id: id.to_string(),
            seq: None,
            title: "buy milk".to_string(),
            description: None,
            stage: Stage::Ready,
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
            created_at: 1,
            updated_at: 1,
            version: 1,
        }
    }

    /// Pins the wire shape `task-worker.ts`'s `RawItem` parses: `pending`
    /// sits alongside the flattened `Item` fields, not nested under a
    /// separate `item` key — and this is asserted for both `true` and
    /// `false` so a regression that hard-codes one value would fail here.
    #[test]
    fn frontier_item_dto_serializes_pending_alongside_the_flattened_item_fields() {
        let pending = FrontierItemDTO {
            item: fixture_item("item-1"),
            pending: true,
        };
        let json = serde_json::to_string(&pending).unwrap();
        assert!(json.contains(r#""id":"item-1""#), "{json}");
        assert!(json.contains(r#""pending":true"#), "{json}");
        assert!(!json.contains("\"item\":{"), "pending must not nest under an `item` key: {json}");

        let confirmed = FrontierItemDTO {
            item: fixture_item("item-2"),
            pending: false,
        };
        assert!(serde_json::to_string(&confirmed).unwrap().contains(r#""pending":false"#));
    }

    #[test]
    fn blocked_entry_dto_carries_pending_on_both_the_item_and_its_blockers() {
        let entry = BlockedEntryDTO {
            item: FrontierItemDTO { item: fixture_item("blocked-1"), pending: true },
            blocked_by: vec![FrontierItemDTO { item: fixture_item("blocker-1"), pending: false }],
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""item":{"id":"blocked-1""#), "{json}");
        assert!(json.contains(r#""pending":true"#), "{json}");
        assert!(json.contains(r#""blocked_by":[{"id":"blocker-1""#), "{json}");
        assert!(json.contains(r#""pending":false"#), "{json}");
    }

    #[test]
    fn blocked_list_response_serializes_with_the_exact_keys_task_worker_ts_parses() {
        let response = BlockedListResponse {
            kind: "ok",
            entries: Vec::new(),
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"kind":"ok","entries":[]}"#
        );
    }

    #[test]
    fn step_list_response_serializes_with_the_exact_keys_task_worker_ts_parses() {
        let response = StepListResponse {
            kind: "ok",
            steps: Vec::new(),
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"kind":"ok","steps":[]}"#
        );
    }

    #[test]
    fn project_list_response_serializes_with_the_exact_keys_task_worker_ts_parses() {
        let response = ProjectListResponse {
            kind: "ok",
            projects: vec![Project {
                id: "p-1".to_string(),
                name: "Ship it".to_string(),
                archived_at: None,
                created_at: 1,
                updated_at: 1,
                version: 1,
            }],
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"kind":"ok","projects":[{"id":"p-1","name":"Ship it","archived_at":null,"created_at":1,"updated_at":1,"version":1}]}"#
        );
    }

    #[test]
    fn is_pending_response_serializes_with_the_exact_keys_task_worker_ts_parses() {
        let response = IsPendingResponse {
            kind: "ok",
            pending: true,
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"kind":"ok","pending":true}"#
        );
    }

    #[test]
    fn task_event_dto_serializes_with_the_exact_keys_task_worker_ts_parses() {
        let event = TaskEventDTO {
            kind: "credential_needed",
            at_ms: 5_000,
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"kind":"credential_needed","at_ms":5000}"#
        );
    }

    #[test]
    fn run_response_serializes_with_the_exact_keys_and_kind_literals_task_worker_ts_parses() {
        // One representative per `kind` literal `task-worker.ts` matches on
        // — the field *names* (asserted once, on the fully-populated
        // variant) are what a rename would silently desync; the `kind`
        // *literals* (asserted per variant) are what a typo in
        // `map_run_outcome`'s string constants would silently desync.
        let completed = RunResponse {
            kind: "completed",
            retry_after_ms: Some(100),
            active_item_count: Some(5),
            was_full_sweep: Some(true),
            dead_lettered: Some(0),
        };
        assert_eq!(
            serde_json::to_string(&completed).unwrap(),
            r#"{"kind":"completed","retry_after_ms":100,"active_item_count":5,"was_full_sweep":true,"dead_lettered":0}"#
        );

        for kind in [
            "no_credential",
            "held",
            "skipped",
            "blocked",
            "credential_needed",
            "persist_failed",
            "pull_failed",
        ] {
            let response = run_response(kind);
            let json = serde_json::to_string(&response).unwrap();
            assert_eq!(
                json,
                format!(
                    r#"{{"kind":"{kind}","retry_after_ms":null,"active_item_count":null,"was_full_sweep":null,"dead_lettered":null}}"#
                )
            );
        }
    }

    #[test]
    fn queue_depth_response_serializes_with_the_exact_keys_task_worker_ts_parses() {
        let response = QueueDepthResponse { kind: "ok", depth: 3 };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"kind":"ok","depth":3}"#
        );
    }

    #[test]
    fn dead_letters_response_serializes_with_the_exact_keys_task_worker_ts_parses() {
        let permanent = DeadLettersResponse {
            kind: "ok",
            entries: vec![DeadLetterEntryDTO {
                id: "item-1".to_string(),
                reason: "permanent",
                message: Some("validation".to_string()),
                fields: Vec::new(),
                at_ms: 5_000,
            }],
        };
        assert_eq!(
            serde_json::to_string(&permanent).unwrap(),
            r#"{"kind":"ok","entries":[{"id":"item-1","reason":"permanent","message":"validation","fields":[],"at_ms":5000}]}"#
        );

        let conflict = DeadLettersResponse {
            kind: "ok",
            entries: vec![DeadLetterEntryDTO {
                id: "item-2".to_string(),
                reason: "conflict",
                message: None,
                fields: vec![DeadLetterFieldDTO {
                    field: "title".to_string(),
                    local: serde_json::json!("buy oat milk"),
                    server: serde_json::json!("someone else's"),
                }],
                at_ms: 6_000,
            }],
        };
        assert_eq!(
            serde_json::to_string(&conflict).unwrap(),
            r#"{"kind":"ok","entries":[{"id":"item-2","reason":"conflict","message":null,"fields":[{"field":"title","local":"buy oat milk","server":"someone else's"}],"at_ms":6000}]}"#
        );
    }

    #[test]
    fn mirror_snapshot_response_serializes_with_the_exact_keys_task_worker_ts_parses() {
        let response = MirrorSnapshotResponse {
            kind: "ok",
            mirror: serde_json::json!({"version": 1}),
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"kind":"ok","mirror":{"version":1}}"#
        );
    }
}
