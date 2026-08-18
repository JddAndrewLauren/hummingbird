//! `hummingbird-core`: the binding-agnostic sync engine core (ADR-0003).
//!
//! This crate has zero binding-macro dependencies (`uniffi`, `wasm_bindgen`)
//! by design — see the `cargo_toml_has_no_binding_macro_dependencies` test
//! below, which enforces that mechanically rather than by convention.
//!
//! [`Core`] is the one storage/sync public API that both `ffi-mobile` and
//! `ffi-web` surface verbatim (ADR-0001 seam rule 2). As of S6/#104 it is no
//! longer a stub: [`Core::init`] loads the durable [`sync::SyncCycle`] state
//! for a host-supplied storage namespace and holds the host-supplied API key
//! in memory only (never persisted — see the `compile_fail` proof on
//! [`Core`] itself); [`Core::frontier`] reads the mirror with every
//! not-yet-confirmed capture overlaid, so a capture is visible the instant
//! it is made and never flickers backwards mid-queue; and events (today,
//! just [`CoreEvent::CredentialNeeded`]) reach the host by
//! [`Core::take_events`] draining a queue, never by a host-implemented
//! callback (ADR-0003).
//!
//! **The credential hold this issue closes.** A 401 on the pull used to
//! record no backoff (`sync::cycle`'s
//! `a_401_on_the_pull_holds_the_cycle_too_after_the_queue_already_drained`
//! pins the gap), so a `Trigger::Timer` at ADR-0007's 60-second cadence
//! would re-attempt a doomed cycle every tick until a fresh token arrived.
//! [`Core::run`] closes it properly rather than papering over it with a
//! backoff: [`sync::CycleOutcome::CredentialNeeded`] holds every future
//! `Core::run` call outright — no transport is even reached — until the host
//! calls [`Core::push_api_key`] with a fresh one, exactly mirroring
//! [`context::ContextPoller`]'s push/hold contract for a context provider's
//! credential.
//!
//! **The calendar host handle is not folded into this interface.**
//! `ffi-web::calendar_host::CalendarHostCore` (issue #73) remains a second,
//! separate door into `core` for now. ADR-0001 rule 2 wants exactly one, but
//! folding it in is `ffi-web`-side wiring that issue #126 (SharedWorker /
//! ADR-0010) is landing concurrently with this one; forcing the merge here
//! would collide with that work rather than avoid it. This is a deliberate
//! deferral, not an oversight — tracked as issue #169, which does the actual
//! fold once #126 lands.
//!
//! Persistence lives in [`storage`] (#68).
//!
//! **The decisions every client has to agree on** live in [`decisions`]
//! (ADR-0025, #141): pure, clock-free predicates, reached from the web by a
//! second main-thread instantiation of the wasm module rather than through
//! [`Core`]'s SharedWorker protocol. That module's header states what may
//! and may not be added to it.

use std::collections::{BTreeMap, BTreeSet};

pub mod bindings;
pub mod calendar;
pub mod capture;
pub mod context;
pub mod decisions;
pub mod freshness;
pub mod item_detail;
pub mod pane;
pub mod rank;
pub mod search;
pub mod storage;
pub mod sync;
pub mod task;

use bindings::{Binding, BindingKey, BindingValue};
use hummingbird_domain::{
    resulting_stage, Alert, AlertPatch, Condition, CreateGrill, CreateItem, CreateRule, Energy,
    GrillVerdict, Item, Project, Rule, RulePatch, Setting, Size, Stage, Step, Tier,
};

use serde::{Deserialize, Serialize};
use storage::{
    load_snapshot_at_version, save_snapshot, MemorySnapshotStore, Persistable, PersistableSealed,
    SnapshotError, SnapshotStore,
};
use sync::queue::{MutationIntent, QueueEntry};
use sync::transport::ChangesTransport;
use sync::write::transport::{HttpMethod, MutationTransport};
use sync::{CycleOutcome, LoadError, SyncCycle, Trigger};

/// The public API version both FFI crates surface.
pub const API_VERSION: u32 = 1;

/// The real, target-specific [`storage::SnapshotStore`] [`Core::init`]
/// resolves to: IndexedDB in the browser, `std::fs` everywhere else — the
/// same per-target split [`storage`] already documents.
#[cfg(target_arch = "wasm32")]
type CoreStore = storage::IndexedDbSnapshotStore;
#[cfg(not(target_arch = "wasm32"))]
type CoreStore = storage::FsSnapshotStore;

#[cfg(target_arch = "wasm32")]
fn queue_store(namespace: &str) -> CoreStore {
    CoreStore::new(format!("{namespace}::queue"))
}
#[cfg(target_arch = "wasm32")]
fn mirror_store(namespace: &str) -> CoreStore {
    CoreStore::new(format!("{namespace}::mirror"))
}
/// #356's device-local Grill draft store — a THIRD core-owned snapshot,
/// alongside `queue_store`/`mirror_store`, never reached by
/// [`sync::SyncCycle`] and therefore never carried by a sweep or a delta
/// payload. Core-owned per ADR-0010 ("what only the core has": the
/// IndexedDB snapshot handle), not a per-view IndexedDB database — the
/// rejected alternative the issue's brief names — so this is an addition
/// alongside the existing two databases, not the "new IndexedDB database"
/// that brief's anti-goal rules out (that anti-goal is about a *view*
/// reaching into IndexedDB on its own, `client/web/src`'s own
/// `rg "indexedDB"` check).
#[cfg(target_arch = "wasm32")]
fn grill_draft_store(namespace: &str) -> CoreStore {
    CoreStore::new(format!("{namespace}::grill-drafts"))
}

#[cfg(not(target_arch = "wasm32"))]
fn queue_store(namespace: &str) -> CoreStore {
    CoreStore::new(std::path::Path::new(namespace).join("queue.json"))
}
#[cfg(not(target_arch = "wasm32"))]
fn mirror_store(namespace: &str) -> CoreStore {
    CoreStore::new(std::path::Path::new(namespace).join("mirror.json"))
}
#[cfg(not(target_arch = "wasm32"))]
fn grill_draft_store(namespace: &str) -> CoreStore {
    CoreStore::new(std::path::Path::new(namespace).join("grill-drafts.json"))
}

/// The `std::fs` leg stores the queue and mirror as sibling files under
/// `namespace`, treated as a directory — which must exist before either
/// store's first write. IndexedDB has no such precondition (opening a
/// database creates it), so this is a no-op there.
#[cfg(not(target_arch = "wasm32"))]
fn prepare_namespace(namespace: &str) -> Result<(), CoreInitError> {
    std::fs::create_dir_all(namespace).map_err(|error| CoreInitError(error.to_string()))
}
#[cfg(target_arch = "wasm32")]
fn prepare_namespace(_namespace: &str) -> Result<(), CoreInitError> {
    Ok(())
}

/// [`Core::init`] failed to load its durable state. Carries the underlying
/// [`LoadError`]'s message rather than the error itself, so callers on
/// either target see the same concrete type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreInitError(String);

impl std::fmt::Display for CoreInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CoreInitError {}

/// The host-facing credential slot: the API key [`Core::init`] receives, or
/// whatever [`Core::push_api_key`] most recently supplied. In memory only —
/// this type never derives `Serialize`/`Deserialize` and never implements
/// [`storage::Persistable`], and never will; see the `compile_fail` proof on
/// [`Core`] for the mechanical guarantee.
///
/// Mirrors [`context::ContextPoller`]'s own push/hold credential contract
/// (that type is private to `context`, so this is a second small copy
/// rather than a cross-module reuse) — a rejected credential holds every
/// future attempt until a fresh push, and a push unconditionally resumes.
#[derive(Debug, Clone, Default)]
struct AccessTokenSlot {
    key: Option<String>,
    held: bool,
}

impl AccessTokenSlot {
    fn push(&mut self, key: String) {
        self.key = Some(key);
        self.held = false;
    }

    /// Issue #196: rehydrates a key WITHOUT resuming a hold — see
    /// [`Core::rehydrate_api_key`] for the full contract this backs.
    fn rehydrate(&mut self, key: String) {
        self.key = Some(key);
    }

    fn token(&self) -> Option<&str> {
        if self.held {
            None
        } else {
            self.key.as_deref()
        }
    }

    fn hold(&mut self) {
        self.held = true;
    }

    fn is_held(&self) -> bool {
        self.held
    }

    /// "Forget token" (#106/S8): returns to the "no key has ever been
    /// pushed" state, distinct from [`AccessTokenSlot::hold`] — a hold
    /// means a previously-good key was just rejected and a fresh push is
    /// expected to resume it; a clear means the host discarded the key
    /// itself, so the next [`Core::run`] should report
    /// [`CoreCycleOutcome::NoCredential`], not stay [`CoreCycleOutcome::Held`].
    fn clear(&mut self) {
        self.key = None;
        self.held = false;
    }
}

/// One not-yet-confirmed capture: the mutation's own queue entry id (so a
/// dead-letter can be matched back to it) and the optimistic [`Item`] a
/// reader sees until the overlay clears.
struct OverlayEntry {
    entry_id: String,
    item: Item,
}

/// One not-yet-confirmed binding write (#118), the [`Setting`]-shaped twin
/// of [`OverlayEntry`]: the queue entry id (so a dead-letter reverts it, the
/// same match [`Core::run`] already makes for items) and the optimistic row
/// a reader sees until the overlay clears.
struct SettingOverlayEntry {
    entry_id: String,
    setting: Setting,
}

/// The capture box's optional field selections (#208), grouped in one small
/// struct rather than a long positional list on [`Core::capture`] — the same
/// "params struct once the positional list reads long" call `ffi-web`'s
/// `TriageEdits` already made for triage's own edit fields. `Default` is every
/// field `None`, which is what keeps a caller that never sets these producing a
/// capture with all of them absent.
///
/// **Plain `Option<T>`, not `TriageEdits`' double option.** These are
/// creation-time values on an item that does not exist yet, so there is no
/// stored value a `Some(None)` could be clearing: absent and "set to nothing"
/// are the same fact here, and the wire body simply omits the field.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CaptureOptions {
    pub size: Option<Size>,
    pub energy: Option<Energy>,
    pub context: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i64>,
    pub project_id: Option<String>,
    pub deadline: Option<String>,
    pub scheduled_date: Option<String>,
}

/// Builds the optimistic [`Item`] a reader sees for a still-queued create,
/// from the same [`CreateItem`] DTO the wire body holds — shared by
/// [`Core::capture`] (which has `now_ms` fresh) and the overlay this module
/// rebuilds from a reloaded queue at [`Core::init`] (which does not: a
/// capture's original timestamp is not recoverable from the DTO alone, so a
/// reloaded overlay entry's `created_at`/`updated_at` fall back to `0`
/// rather than inventing a wrong one — cosmetic only, since neither field
/// drives [`Core::frontier`]'s filter).
fn item_from_create(create: &CreateItem, now_ms: i64) -> Item {
    Item {
        id: create.id.clone(),
        seq: None,
        title: create.title.clone(),
        description: create.description.clone(),
        stage: create.stage.unwrap_or(Stage::Triage),
        size: create.size,
        energy: create.energy,
        context: create.context.clone(),
        priority: create.priority.unwrap_or(0),
        project_id: create.project_id.clone(),
        project_pos: create.project_pos,
        deadline: create.deadline.clone(),
        scheduled_date: create.scheduled_date.clone(),
        source: create.source.clone(),
        source_key: create.source_key.clone(),
        source_url: create.source_url.clone(),
        archived_at: None,
        agent: create.agent.unwrap_or(false),
        created_at: now_ms,
        updated_at: now_ms,
        version: 0,
    }
}

/// Applies `patch_fields` onto `base` (both an item's own JSON object, per
/// [`sync::queue::MutationIntent::Patch`]'s own doc — `base` is "the entity
/// as this client last knew it") field-by-field, the same absolute-value
/// overwrite [`sync::write::adapter::patch_with_rebase`] sends on the wire,
/// and deserialises the result as an [`Item`] — [`overlay_from_queue`]'s
/// patch-rebuild step, kept as its own function so that step is testable in
/// isolation from queue iteration. `None` if either side is not a JSON
/// object (never expected for an item's own `base`/`patch_fields`, but this
/// function does not assume it).
fn apply_item_patch(base: &serde_json::Value, patch_fields: &serde_json::Value) -> Option<Item> {
    let mut merged = base.as_object()?.clone();
    for (key, value) in patch_fields.as_object()? {
        merged.insert(key.clone(), value.clone());
    }
    serde_json::from_value(serde_json::Value::Object(merged)).ok()
}

/// The item-side projection of a still-queued
/// [`MutationIntent::CompleteGrill`], for the same reload-survival reason
/// [`apply_item_patch`] exists: the only field a completion ever moves on
/// the item is `stage`, and it moves it exactly the way
/// [`hummingbird_domain::resulting_stage`] says — never re-derived, never a
/// second inference of the verdict→stage table (`grill.rs`'s own "one
/// spelling" precedent). `None` for a malformed base, an unparseable
/// verdict, or a `Done` item (`resulting_stage`'s own rejection) — the same
/// "never silently overlay-blind" contract [`apply_item_patch`] documents.
fn apply_grill_completion(
    item_base: &serde_json::Value,
    grill_fields: &serde_json::Value,
) -> Option<Item> {
    let current: Item = serde_json::from_value(item_base.clone()).ok()?;
    let verdict: GrillVerdict = serde_json::from_value(grill_fields.get("verdict")?.clone()).ok()?;
    let resulting = resulting_stage(current.stage, verdict).ok()?;
    let patch_fields = serde_json::json!({ "stage": resulting });
    apply_item_patch(item_base, &patch_fields)
}

/// Rebuilds the overlay a previous session left mid-flight from whatever is
/// still queued — both item creates and item patches — so a capture or an
/// act (S11/#109) made offline, then reloaded before ever syncing, is still
/// readable from [`Core::frontier`] rather than silently vanishing until
/// the next successful cycle. [`Core::act`] enqueues exactly such a patch;
/// this is what keeps a completed/blocked/cancelled item's overlaid state
/// (and its [`Core::is_pending`] answer) surviving a reload while the
/// `PATCH` is still durably queued, not sent.
///
/// A create whose body no longer deserialises as [`CreateItem`], or a patch
/// whose `base`+`patch_fields` no longer merge into a valid [`Item`]
/// ([`apply_item_patch`]), is an `Err`, never a silently-dropped overlay
/// entry: the same never-silently-degrade rule [`sync::SyncCycle::load`]'s
/// own module docs state for the queue itself ("this function does not
/// special-case either table down to 'start fresh'") applies just as much
/// to a projection built from it — the durable queue entry is untouched
/// either way (drain can still retry it), but silently going
/// overlay-blind on it would tell a reader nothing is pending when
/// something still is.
///
/// Iterated in queue (FIFO) order and keyed by item id, so if more than one
/// still-queued entry targets the same item (e.g. an act queued on top of a
/// not-yet-confirmed capture), the later entry's rebuild wins — the same
/// "last enqueued is the client's current best knowledge" reasoning
/// [`Core::act`]'s own `base` (read from [`Core::overlaid_items`], the
/// overlay-if-present view) already applies when a fresh mutation is
/// enqueued mid-session.
/// One [`Core::ledger`] row: an item's current, derivable facts. Not a
/// history record — see the read's own doc for what "derive, don't record"
/// keeps out of it.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerEntry {
    pub item: Item,
    /// The mirror's retention stamp: `Some` when the row has gone absent
    /// (archived, or missing from a complete sweep) — [`task::Presence`]'s
    /// "first, not latest" `since_ms`, `None` for a live row.
    pub absent_since_ms: Option<i64>,
    /// A dead-lettered edit targets this item. Device-local by nature: the
    /// journal never syncs.
    pub dead_lettered: bool,
    /// A live alert's `source_key` names this item (`item:<id>`).
    pub has_live_alert: bool,
}

/// The item an intent targets, if it is an item mutation at all — the
/// dead-letter half of [`Core::ledger`]'s badge derivation, reading the same
/// two shapes [`overlay_from_queue`] projects (an items create's body id, an
/// items patch's path id) without insisting the body still deserialises:
/// a dead letter is already terminal, so a badge is owed on whatever id is
/// still legible, never an init-blocking `Err`.
///
/// The reading itself is [`MutationIntent::subject`]'s, narrowed to items —
/// this used to derive it again inline, and two derivations of "which row is
/// this change about" is exactly the pair that drifts once a path shape
/// changes.
fn item_id_of_intent(intent: &MutationIntent) -> Option<String> {
    let subject = intent.subject();
    if subject.entity == "items" {
        subject.id
    } else {
        None
    }
}

fn overlay_from_queue(
    queue: &sync::queue::OutboundQueue,
) -> Result<BTreeMap<String, OverlayEntry>, CoreInitError> {
    let mut overlay = BTreeMap::new();
    for entry in queue.entries() {
        match &entry.intent {
            MutationIntent::Create { path, body } if *path == sync::write::paths::items() => {
                let create: CreateItem = serde_json::from_value(body.clone()).map_err(|error| {
                    CoreInitError(format!(
                        "queue entry {} is a create for {path} whose body no longer \
                         deserialises as CreateItem: {error}",
                        entry.id
                    ))
                })?;
                let item = item_from_create(&create, 0);
                overlay.insert(
                    item.id.clone(),
                    OverlayEntry {
                        entry_id: entry.id.clone(),
                        item,
                    },
                );
            }
            MutationIntent::Patch {
                path,
                base,
                patch_fields,
                ..
            } if path.starts_with("/api/items/") => {
                let item = apply_item_patch(base, patch_fields).ok_or_else(|| {
                    CoreInitError(format!(
                        "queue entry {} is a patch for {path} whose base+patch_fields no \
                         longer merge into a valid Item",
                        entry.id
                    ))
                })?;
                overlay.insert(
                    item.id.clone(),
                    OverlayEntry {
                        entry_id: entry.id.clone(),
                        item,
                    },
                );
            }
            MutationIntent::CompleteGrill {
                item_base,
                grill_fields,
                ..
            } => {
                let item = apply_grill_completion(item_base, grill_fields).ok_or_else(|| {
                    CoreInitError(format!(
                        "queue entry {} is a grill completion whose item_base+grill_fields \
                         no longer resolve to a valid item stage move",
                        entry.id
                    ))
                })?;
                overlay.insert(
                    item.id.clone(),
                    OverlayEntry {
                        entry_id: entry.id.clone(),
                        item,
                    },
                );
            }
            MutationIntent::Create { .. } | MutationIntent::Patch { .. } => {
                // Not an item mutation (a step/project/etc create, or a
                // patch on some other entity) — nothing this overlay
                // projects.
            }
        }
    }
    Ok(overlay)
}

/// The `settings` write path — the prefix [`binding_overlay_from_queue`]
/// recognises a queued binding write by, and the one
/// [`sync::write::paths::setting`] mints.
const SETTINGS_PATH_PREFIX: &str = "/api/settings/";

/// Rebuilds the binding overlay from whatever binding writes the durable
/// queue still holds — [`overlay_from_queue`]'s exact twin for `settings`,
/// and for the same reason: a binding set offline and then reloaded before
/// ever syncing must still read back as set (and as `pending`), rather than
/// reverting on screen to whatever the mirror last pulled.
///
/// The rebuild reads the intent's `rebase_fields`, not its `patch_fields`:
/// those *are* the same intent in the entity's own encoding
/// ([`sync::queue::MutationIntent::Patch`]'s own doc), which is exactly what
/// merging onto a stored [`Setting`] needs — the wire body's typed `value`
/// would merge a bare JSON value into a column that stores its canonical
/// text. A binding entry carrying none, or one whose merge no longer
/// deserialises as a `Setting`, is an `Err` rather than a silently dropped
/// overlay entry, the same never-go-overlay-blind rule
/// [`overlay_from_queue`] documents.
fn binding_overlay_from_queue(
    queue: &sync::queue::OutboundQueue,
) -> Result<BTreeMap<String, SettingOverlayEntry>, CoreInitError> {
    let mut overlay = BTreeMap::new();
    for entry in queue.entries() {
        let MutationIntent::Patch {
            path,
            base,
            rebase_fields,
            ..
        } = &entry.intent
        else {
            continue;
        };
        if !path.starts_with(SETTINGS_PATH_PREFIX) {
            continue;
        }
        let setting = rebase_fields
            .as_ref()
            .and_then(|fields| apply_setting_patch(base, fields))
            .ok_or_else(|| {
                CoreInitError(format!(
                    "queue entry {} is a patch for {path} whose base+rebase_fields no \
                     longer merge into a valid Setting",
                    entry.id
                ))
            })?;
        overlay.insert(
            setting.key.clone(),
            SettingOverlayEntry {
                entry_id: entry.id.clone(),
                setting,
            },
        );
    }
    Ok(overlay)
}

/// [`apply_item_patch`]'s twin for a [`Setting`]: the same absolute-value
/// field overwrite, deserialised as the settings row rather than the item.
/// `None` if either side is not a JSON object, or the merge is not a valid
/// `Setting`.
fn apply_setting_patch(
    base: &serde_json::Value,
    fields: &serde_json::Value,
) -> Option<Setting> {
    let mut merged = base.as_object()?.clone();
    for (key, value) in fields.as_object()? {
        merged.insert(key.clone(), value.clone());
    }
    serde_json::from_value(serde_json::Value::Object(merged)).ok()
}

/// S11/#109's act vocabulary — every affordance the frontier/item-detail UI
/// offers on an already-existing item. Deliberately closed (never a raw
/// `Stage` the caller picks): [`ItemAction::stage`] is the one place a
/// UI action maps onto ADR-0009's stage vocabulary, so no caller ever sends
/// a hardcoded stage id of its own — the brief's "state ids are resolved by
/// name from the vocabulary, never hardcoded".
///
/// **`Blocked` means an external wait and nothing else** (`CONTEXT.md`):
/// there is no `ItemAction` for "depends on another item" — that is a
/// `blocked_by` relation edge (already covered by [`Core::blocked`]'s read
/// side), never this stage. Conflating the two here would let the UI
/// express an inter-item dependency as `Blocked`, which the brief
/// explicitly forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemAction {
    /// Ready/Triage/Grilling → `InProgress`.
    Start,
    /// → `Done`.
    Complete,
    /// → `Blocked` — an external wait, never an inter-item dependency.
    Block,
    /// Archives the item (`archived_at`), never a stage change — the owned
    /// schema's six-stage vocabulary has no "canceled" stage, and archiving
    /// is how every other entity in ADR-0009 is soft-removed.
    Cancel,
}

impl ItemAction {
    /// The stage this action sets, or `None` for [`ItemAction::Cancel`]
    /// (which touches `archived_at` instead of `stage`).
    fn stage(self) -> Option<Stage> {
        match self {
            ItemAction::Start => Some(Stage::InProgress),
            ItemAction::Complete => Some(Stage::Done),
            ItemAction::Block => Some(Stage::Blocked),
            ItemAction::Cancel => None,
        }
    }

    /// S11/#109's wire spelling of this action — `"start"`/`"complete"`/
    /// `"block"`/`"cancel"`. The mirror of [`ItemAction::parse`]; every
    /// `ItemAction -> &str` copy that used to live at each wasm boundary
    /// (`client/ffi-web/src/{task_host,decisions}.rs`) now calls this
    /// instead, so the wire spelling is spelled once for the whole
    /// workspace, not once per crossing.
    pub fn as_str(self) -> &'static str {
        match self {
            ItemAction::Start => "start",
            ItemAction::Complete => "complete",
            ItemAction::Block => "block",
            ItemAction::Cancel => "cancel",
        }
    }

    /// Maps S11/#109's wire action name to [`ItemAction`] — the one place a
    /// string crossing any client boundary becomes the closed act
    /// vocabulary, the same "reject before the seam" discipline this
    /// crate's `TriagePatch`/`Stage::parse` machinery already applies.
    /// Never a raw [`Stage`]: there is no wire action that lets a caller
    /// send an arbitrary stage id. Every wasm-facing crate that used to
    /// carry its own copy of this match (`client/ffi-web/src/task_host.rs`,
    /// `client/ffi-web/src/decisions.rs`) now calls this one instead.
    pub fn parse(action: &str) -> Option<Self> {
        match action {
            "start" => Some(ItemAction::Start),
            "complete" => Some(ItemAction::Complete),
            "block" => Some(ItemAction::Block),
            "cancel" => Some(ItemAction::Cancel),
            _ => None,
        }
    }
}

/// S13/#111's multi-field triage edit — every field of an item the triage
/// form may set alongside the destination stage. `None` on every field is a
/// legal call: a bare promotion with no other edit is still exactly one
/// mutation.
///
/// The nullable columns are double-`Option`, exactly as
/// [`hummingbird_domain::ItemPatch`] carries them on the wire: outer `None`
/// means "leave this field alone", `Some(None)` means "clear it", and
/// `Some(Some(v))` means "set it to `v`". That distinction is not decoration —
/// an editor that shows an item's real values needs a way to say "this
/// deadline is now gone", and a single `Option` can only ever add.
/// `NOT NULL` columns (`title`, `priority`) are single-`Option` and cannot be
/// cleared, the same asymmetry the authority enforces with a 400.
///
/// `scheduled_date` (#122) is the do-date affordance and follows the same
/// double-`Option` fidelity as every other nullable column here: outer
/// `None` leaves it untouched, `Some(None)` clears it, `Some(Some(date))`
/// sets it. A cleared date sent as an absent field would silently do
/// nothing, and a `null` spelled as an empty string would be an edit nobody
/// asked for — the same fidelity every other CAS write in this crate
/// already holds.
///
/// What is deliberately absent: `source`/`source_key`/`source_url` (owned by
/// whatever captured the item, never edited here), `stage` (that IS the
/// destination), `archived_at` (cancelling is [`Core::act`]'s), `project_pos`
/// (a Route's ordering, owned by the Route), and every server-stamped field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TriagePatch {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub size: Option<Option<Size>>,
    pub energy: Option<Option<Energy>>,
    pub context: Option<Option<String>>,
    pub priority: Option<i64>,
    pub project_id: Option<Option<String>>,
    pub deadline: Option<Option<String>>,
    pub scheduled_date: Option<Option<String>>,
}

/// [`Core::act`] failed before ever reaching the outbound queue, or while
/// durably enqueueing. Only [`Debug`](std::fmt::Debug) derives — same as
/// [`storage::SnapshotError`] itself, which this wraps and which carries no
/// `Clone`/`PartialEq` for its own store-error payload.
#[derive(Debug)]
pub enum ActError<E> {
    /// No live item with this id is known locally (mirror or overlay) —
    /// nothing to act on. A caller mistake, not a durability failure.
    ItemNotFound,
    /// [`sync::SyncCycle::enqueue`] itself failed to persist the candidate
    /// queue.
    Snapshot(SnapshotError<E>),
}

impl<E: std::fmt::Debug> std::fmt::Display for ActError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActError::ItemNotFound => write!(f, "item not found"),
            ActError::Snapshot(error) => write!(f, "{error}"),
        }
    }
}

impl<E: std::fmt::Debug> std::error::Error for ActError<E> {}

/// One reviewed Grill session's outcome, ready to submit as the atomic
/// completion mutation ([`Core::complete_grill`], #354, ADR-0023).
///
/// `transcript` is carried all the way into the queued
/// [`MutationIntent::CompleteGrill`] even though nothing else in this crate
/// ever reads it back — this issue's "a dead-lettered completion restores
/// reviewable state rather than discarding the session" acceptance is met
/// by never dropping it in the first place: a dead-lettered [`QueueEntry`]
/// still holds this session's full body, so whatever re-shows a
/// dead-lettered completion (a future UI, per `CONTEXT.md`'s **Dead-letter
/// journal**: "the words a person wrote are not" discarded) has everything
/// it needs, with no separate session store to keep in sync.
#[derive(Debug, Clone, PartialEq)]
pub struct GrillCompletion {
    pub transcript: String,
    pub summary: String,
    pub verdict: GrillVerdict,
    pub model_proposal: String,
    pub applied_patch: String,
    /// `CONTEXT.md`'s **Replace** gesture: ticked, explicit, default off.
    /// `false` unless a human actually ticked it in the review UI.
    pub delete_unticked_plan: bool,
}

/// [`Core::complete_grill`] failed before ever reaching the outbound queue.
#[derive(Debug)]
pub enum CompleteGrillError<E> {
    /// No live item with this id is known locally (mirror or overlay) —
    /// nothing to complete a Grill against.
    ItemNotFound,
    /// The item is already `Done` — out of scope for the whole Grill plan
    /// (ADR-0023), checked locally with the identical
    /// [`hummingbird_domain::resulting_stage`] the authority itself uses,
    /// rather than round-tripping to learn what a pure function of local
    /// state already answers.
    ItemDone,
    /// At least one unticked, undeleted Step this session captured no
    /// longer matches live state — a new unticked Step appeared, or an
    /// existing one's text changed, since the review was last shown. A
    /// Step *ticked or deleted* since then is not this — see
    /// [`unticked_steps_changed`]'s own doc for the full rule.
    NeedsReReview,
    /// [`sync::SyncCycle::enqueue`] itself failed to persist the candidate
    /// queue.
    Snapshot(SnapshotError<E>),
}

impl<E: std::fmt::Debug> std::fmt::Display for CompleteGrillError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompleteGrillError::ItemNotFound => write!(f, "item not found"),
            CompleteGrillError::ItemDone => write!(f, "item is done"),
            CompleteGrillError::NeedsReReview => {
                write!(f, "unticked steps changed since this review was last shown")
            }
            CompleteGrillError::Snapshot(error) => write!(f, "{error}"),
        }
    }
}

impl<E: std::fmt::Debug> std::error::Error for CompleteGrillError<E> {}

/// Whether the item's *unticked, undeleted* Steps have drifted since a
/// Grill review session captured `session_steps` — [`Core::complete_grill`]'s
/// "new or changed unticked Steps force re-review" acceptance criterion,
/// decided once here rather than inline.
///
/// A live unticked Step forces review when either: it has no matching id in
/// `session_steps` at all (a brand-new Step appeared since review), or its
/// body differs from what `session_steps` recorded for that id (an existing
/// Step's text changed) — including when `session_steps` had recorded that
/// id as already ticked or deleted (a Step *un*-ticked or resurrected since
/// review is exactly as much a surprise as a new one).
///
/// A Step `session_steps` recorded as unticked that is now ticked or
/// deleted *live* is deliberately **not** drift: `CONTEXT.md`'s **Replace**
/// only ever protects a still-*live* Plan, and by the time this runs that
/// Step has already left the live Plan by the human's own gesture,
/// elsewhere, mid-interview — the acceptance criterion's "Steps ticked or
/// deleted mid-interview survive," met by simply never comparing them.
fn unticked_steps_changed(session_steps: &[Step], live_steps: &[Step]) -> bool {
    let session_by_id: BTreeMap<&str, &Step> =
        session_steps.iter().map(|step| (step.id.as_str(), step)).collect();

    live_steps.iter().any(|live| {
        if live.done || live.deleted_at.is_some() {
            return false; // not live-unticked: never forces review
        }
        match session_by_id.get(live.id.as_str()) {
            None => true, // a brand-new unticked step
            Some(session_step) => {
                session_step.body != live.body
                    || session_step.done
                    || session_step.deleted_at.is_some()
            }
        }
    })
}

/// One host-visible signal, drained rather than delivered by callback
/// (ADR-0003 rules out a host-implemented callback into the core).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
    /// [`Core::run`] just held on a rejected credential — the host's cue to
    /// surface a re-auth prompt. Nothing further is attempted until
    /// [`Core::push_api_key`] supplies a fresh one.
    CredentialNeeded { at_ms: i64 },
}

/// What one [`Core::run`] call did — [`sync::CycleOutcome`] plus the two
/// credential-hold states that never reach [`sync::SyncCycle::run`] at all.
#[derive(Debug, Clone, PartialEq)]
pub enum CoreCycleOutcome {
    /// No API key has ever been pushed — a valid steady state, not an
    /// error (mirrors [`context::PollOutcome::NoCredential`]).
    NoCredential,
    /// Held on a previously rejected credential; nothing was attempted, not
    /// even a queue-empty no-op cycle.
    Held,
    /// An attempt was actually made.
    Cycle(CycleOutcome),
}

/// #356's schema version for [`GrillDrafts`] — the same bump discipline
/// [`sync::mirror::SYNC_MIRROR_SCHEMA_VERSION`] documents: a genuine shape
/// change to the persisted payload gets a new number, and
/// [`load_snapshot_at_version`] is what makes a stale envelope discarded
/// and rebuilt (an empty draft store) rather than a boot-time crash.
pub const GRILL_DRAFTS_SCHEMA_VERSION: u32 = 1;

/// One item's in-progress Grill interview, device-local (#356, ADR-0023):
/// every completed `{question, answer}` round so far, as the caller's own
/// opaque JSON — `Core` never parses or interprets a turn's shape, exactly
/// as it never interprets `GrillCompletion::transcript`. Threading it back
/// to the runner (`grill-me`'s own `turns` argument) and rendering it are
/// both `client/web`'s job; this crate only stores and returns it verbatim.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GrillDrafts(BTreeMap<String, serde_json::Value>);

impl PersistableSealed for GrillDrafts {}
impl Persistable for GrillDrafts {}

/// Handle for the sync engine.
///
/// Generic over the two [`storage::SnapshotStore`]s [`sync::SyncCycle`]
/// already is, with [`storage::MemorySnapshotStore`] as the default for
/// both — which is what keeps [`Core::new`] (used by the `ffi-web`/
/// `ffi-mobile` smoke tests, and by any caller that wants a fresh,
/// unpersisted handle) a synchronous, infallible constructor. A real host
/// calls [`Core::init`] instead, which resolves the per-target durable
/// store ([`storage::FsSnapshotStore`] / [`storage::IndexedDbSnapshotStore`])
/// from a storage namespace and loads whatever is already on disk.
///
/// ```compile_fail
/// # use hummingbird_core::storage::{save_snapshot, MemorySnapshotStore};
/// # async fn example() {
/// let store = MemorySnapshotStore::default();
/// // `Core::init` receives the API key as a `String` — the same type the
/// // storage module's own compile-fail proof already pins as never
/// // `Persistable`. This is that same unrepresentable-by-construction rule,
/// // named at the API key specifically: there is no way to route it
/// // through `save_snapshot`, so `Core` cannot accidentally persist it.
/// let api_key = "sk-super-secret".to_string();
/// save_snapshot(&store, 1, 0, &api_key).await.unwrap();
/// # }
/// ```
pub struct Core<QS = MemorySnapshotStore, MS = MemorySnapshotStore> {
    cycle: SyncCycle<QS, MS>,
    credential: AccessTokenSlot,
    overlay: BTreeMap<String, OverlayEntry>,
    /// #118's binding overlay — kept as its own map rather than folded into
    /// `overlay`: that one is keyed by *item* id and its entries project
    /// into [`Item`]s, and a `settings` row shares neither the key space nor
    /// the shape. Both follow the identical lifecycle in [`Core::run`].
    binding_overlay: BTreeMap<String, SettingOverlayEntry>,
    events: Vec<CoreEvent>,
    /// #356's device-local Grill draft store handle — a fresh instance of
    /// the SAME store type `MS` already is (real host: a third IndexedDB
    /// database beside queue/mirror; tests: another `MemorySnapshotStore`),
    /// never the mirror's own handle. See [`grill_draft_store`].
    grill_draft_store: MS,
    /// The in-memory cache [`grill_draft_store`] persists — loaded once at
    /// [`Core::init`], mutated and re-saved by
    /// [`Core::save_grill_draft`]/[`Core::discard_grill_draft`].
    grill_drafts: GrillDrafts,
}

impl Core<MemorySnapshotStore, MemorySnapshotStore> {
    /// A fresh, unpersisted core — no namespace, no API key, nothing
    /// durable. Exists for callers (the `ffi-web`/`ffi-mobile` API-version
    /// smoke tests, unit tests elsewhere) that want a `Core` handle without
    /// a real storage namespace; a real host calls [`Core::init`] instead.
    pub fn new() -> Self {
        Self {
            cycle: SyncCycle::new(MemorySnapshotStore::default(), MemorySnapshotStore::default()),
            credential: AccessTokenSlot::default(),
            overlay: BTreeMap::new(),
            binding_overlay: BTreeMap::new(),
            events: Vec::new(),
            grill_draft_store: MemorySnapshotStore::default(),
            grill_drafts: GrillDrafts::default(),
        }
    }
}

impl Default for Core<MemorySnapshotStore, MemorySnapshotStore> {
    fn default() -> Self {
        Self::new()
    }
}

impl Core<CoreStore, CoreStore> {
    /// Loads (or starts fresh, if nothing is durable yet) the sync state
    /// under `namespace`, and holds `api_key` in memory for the session.
    ///
    /// `namespace` is the one thing the host contributes at init (ADR-0003)
    /// — an IndexedDB database name prefix in the browser, a directory
    /// (created if it does not exist) everywhere else. `api_key` is never
    /// persisted — see the `compile_fail` proof on [`Core`].
    pub async fn init(
        namespace: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, CoreInitError> {
        let namespace = namespace.into();
        prepare_namespace(&namespace)?;
        let cycle = SyncCycle::load(queue_store(&namespace), mirror_store(&namespace))
            .await
            .map_err(|error: LoadError<_, _>| CoreInitError(error.to_string()))?;
        let mut credential = AccessTokenSlot::default();
        credential.push(api_key.into());
        let overlay = overlay_from_queue(cycle.queue())?;
        let binding_overlay = binding_overlay_from_queue(cycle.queue())?;
        let grill_draft_store = grill_draft_store(&namespace);
        let grill_drafts = load_snapshot_at_version::<GrillDrafts, _>(
            &grill_draft_store,
            GRILL_DRAFTS_SCHEMA_VERSION,
        )
        .await
        .map_err(|error| CoreInitError(error.to_string()))?
        .map(|envelope| envelope.payload)
        .unwrap_or_default();
        Ok(Self {
            cycle,
            credential,
            overlay,
            binding_overlay,
            events: Vec::new(),
            grill_draft_store,
            grill_drafts,
        })
    }
}

impl<QS, MS> Core<QS, MS>
where
    QS: SnapshotStore,
    MS: SnapshotStore,
{
    /// The public API version this core implements.
    pub fn api_version(&self) -> u32 {
        API_VERSION
    }

    /// The population ADR-0001's 250-issue watchline measures — passed
    /// through from [`sync::SyncCycle::active_item_count`].
    pub fn active_item_count(&self) -> usize {
        self.cycle.active_item_count()
    }

    /// Whether `item_id` currently has an unconfirmed capture overlaid on
    /// it — the affordance a UI uses to render a pending indicator.
    pub fn is_pending(&self, item_id: &str) -> bool {
        self.overlay.contains_key(item_id)
    }

    /// The outbound queue's current depth — how many mutations are enqueued
    /// and not yet drained. S9's sync-status indicator's "queued" figure.
    pub fn queue_depth(&self) -> usize {
        self.cycle.queue().len()
    }

    /// Every entry the outbound queue has permanently given up on
    /// (ADR-0007's dead-letter journal) — never pruned, so this always
    /// reflects the whole history, not just this session's. S9's "1 edit
    /// didn't apply" affordance reads this directly.
    pub fn dead_letters(&self) -> &[sync::queue::DeadLetterEntry] {
        self.cycle.queue().dead_letters()
    }

    /// The local mirror, serialized whole — S9's mirror download button.
    /// Always succeeds: every field [`sync::SyncMirror`] carries derives
    /// `Serialize`, the same guarantee its own persistence already relies
    /// on.
    pub fn mirror_snapshot(&self) -> serde_json::Value {
        serde_json::to_value(self.cycle.mirror()).expect("SyncMirror always serializes")
    }

    /// Every item the mirror knows about live, with every not-yet-confirmed
    /// capture overlaid on top — the shared read underneath both
    /// [`Core::frontier`] and [`Core::triage_inbox`], so an overlaid item is
    /// readable from *some* query whatever stage it captured into, not just
    /// the one [`Core::frontier`] filters to.
    ///
    /// A capture is present here the instant [`Core::capture`] returns —
    /// before [`Core::run`] has ever sent it — and stays present, unchanged,
    /// through every cycle attempt until either a completed cycle's pull
    /// confirms it (the overlay clears because drain-before-pull, ADR-0007,
    /// means this device's own write already landed by the time that pull
    /// asked for truth) or a dead-lettered entry reverts it to server
    /// truth. There is no cycle outcome that makes the item disappear in
    /// between: a queue-side send failure, a pull failure, or a credential
    /// hold all leave the overlay exactly as it was.
    fn overlaid_items(&self) -> BTreeMap<String, Item> {
        let mut items: BTreeMap<String, Item> = self
            .cycle
            .mirror()
            .all_items()
            .map(|item| (item.id.clone(), item.clone()))
            .collect();
        for overlay in self.overlay.values() {
            items.insert(overlay.item.id.clone(), overlay.item.clone());
        }
        items
    }

    /// [`Core::overlaid_items`] over the *retained* roster — every item the
    /// mirror has ever known, archived and swept-away rows included, with
    /// the same overlay on top. The read behind [`Core::item_detail`],
    /// which must answer for history: an archived row is
    /// [`task::Presence::Absent`], so the live view above cannot see one at
    /// all, and **Recall**'s rule is that history stays readable.
    fn overlaid_items_including_absent(&self) -> BTreeMap<String, Item> {
        let mut items: BTreeMap<String, Item> = self
            .cycle
            .mirror()
            .all_items_including_absent()
            .map(|(item, _)| (item.id.clone(), item.clone()))
            .collect();
        for overlay in self.overlay.values() {
            items.insert(overlay.item.id.clone(), overlay.item.clone());
        }
        items
    }

    /// What can actually be started right now — the owned-schema mirror's
    /// live `Ready`/`InProgress` items, not blocked on an open blocker,
    /// with the overlay from [`Core::overlaid_items`] applied.
    ///
    /// A freshly captured item defaults to `Stage::Triage`
    /// (`CreateItem::stage`'s own doc: "capture lands in the inbox") and so
    /// is *not* on this query — see [`Core::triage_inbox`] for the reader
    /// that actually makes such a capture visible anywhere.
    pub fn frontier(&self) -> Vec<Item> {
        let mirror = self.cycle.mirror();
        self.overlaid_items()
            .into_values()
            // A cancel (S11/#109) archives an overlaid item without
            // changing its stage, so this filter — not just the mirror's
            // own `live()` — is what makes a just-cancelled item drop off
            // the frontier immediately, offline or not.
            .filter(|item| item.archived_at.is_none())
            .filter(|item| matches!(item.stage, Stage::Ready | Stage::InProgress))
            .filter(|item| {
                !mirror.blockers_of(&item.id).any(|blocker_id| {
                    mirror
                        .item(blocker_id)
                        .is_some_and(|blocker| blocker.stage != Stage::Done)
                })
            })
            .collect()
    }

    /// Items awaiting triage: captured, not yet promoted to an actionable
    /// stage — the owned-schema counterpart to [`task::Mirror::triage_inbox`]'s
    /// S1/Linear-era twin, and, unlike [`Core::frontier`], the query a
    /// default (`Stage::Triage`) [`Core::capture`] is actually readable
    /// from immediately.
    pub fn triage_inbox(&self) -> Vec<Item> {
        self.overlaid_items()
            .into_values()
            .filter(|item| item.archived_at.is_none())
            .filter(|item| item.stage == Stage::Triage)
            .collect()
    }

    /// Items already grilled once and still foggy: CONTEXT.md's "triage
    /// process" is the pair of pre-action stages, Triage and Grilling,
    /// together, and this is the second half — [`Core::triage_inbox`]
    /// deliberately stays `Stage::Triage`-only rather than widening to cover
    /// both, so a caller that wants the combined queue reads both and
    /// combines them itself (#357). An item reaches `Grilling` exactly one
    /// way, a `fog_remains` verdict from a completed Grill
    /// ([`Core::complete_grill`]) — never through [`Core::triage`] (#360).
    pub fn grilling_items(&self) -> Vec<Item> {
        self.overlaid_items()
            .into_values()
            .filter(|item| item.archived_at.is_none())
            .filter(|item| item.stage == Stage::Grilling)
            .collect()
    }

    /// The items [`Core::frontier`] excludes because they carry an open
    /// relation blocker (ADR-0009 `blocked_by`), each paired with the
    /// blockers still open — S10's "relation-blocked items … marked and
    /// the reason visible" (issue #108), so a short frontier can be
    /// explained rather than merely being short.
    ///
    /// Distinct from [`Stage::Blocked`]: that stage means an *external*
    /// wait (`CONTEXT.md`), never a relation to another item, and — like
    /// [`Core::frontier`] — this query only ever considers `Ready`/
    /// `InProgress` items. A blocker counts while it is not `Stage::Done`;
    /// the owned schema's six-stage vocabulary has no separate "canceled"
    /// (unlike the S1/Linear mirror's `crate::task::Mirror::open_blockers`,
    /// which also treats `Canceled` as shut). An id this mirror has never
    /// seen, or one that has gone absent, does not block — [`SyncMirror::item`]
    /// already filters to live records only, so such an id is silently
    /// excluded from the blockers list precisely like [`Core::frontier`]'s
    /// own filter treats it.
    ///
    /// Sorted by item id — a stable order for a query with no `priority`/
    /// `deadline` ranking opinion of its own; ordering the *entries* is a
    /// display concern the caller applies the same way it orders the
    /// frontier.
    pub fn blocked(&self) -> Vec<(Item, Vec<Item>)> {
        let mirror = self.cycle.mirror();
        let mut result: Vec<(Item, Vec<Item>)> = self
            .overlaid_items()
            .into_values()
            .filter(|item| item.archived_at.is_none())
            .filter(|item| matches!(item.stage, Stage::Ready | Stage::InProgress))
            .filter_map(|item| {
                let blockers: Vec<Item> = mirror
                    .blockers_of(&item.id)
                    .filter_map(|blocker_id| mirror.item(blocker_id))
                    .filter(|blocker| blocker.stage != Stage::Done)
                    .cloned()
                    .collect();
                if blockers.is_empty() {
                    None
                } else {
                    Some((item, blockers))
                }
            })
            .collect();
        result.sort_by(|a, b| a.0.id.cmp(&b.0.id));
        result
    }

    /// Every live Step attached to `item_id`, position order — item
    /// detail's checklist (issue #96, S10). Read straight from the mirror,
    /// never overlaid: [`Core::capture`] never mints a Step, so there is no
    /// optimistic Step to overlay.
    pub fn steps_for(&self, item_id: &str) -> Vec<hummingbird_domain::Step> {
        let mut steps: Vec<hummingbird_domain::Step> = self
            .cycle
            .mirror()
            .steps_for_item(item_id)
            .cloned()
            .collect();
        steps.sort_by_key(|step| step.position);
        steps
    }

    /// Every live project — what the frontier's "grouped by project"
    /// display (issue #108) resolves a `TaskItemDTO.projectId` against to
    /// get the project's actual *name*, rather than rendering the raw id.
    /// Id order, for a stable list a caller can diff against its own.
    pub fn projects(&self) -> Vec<Project> {
        let mut projects: Vec<Project> = self.cycle.mirror().all_projects().cloned().collect();
        projects.sort_by(|a, b| a.id.cmp(&b.id));
        projects
    }

    /// The complete retained roster — every item this mirror has ever
    /// known, live, `Done` and archived alike, overlaid with any
    /// not-yet-confirmed mutation (the Ledger screen's read). Derive, don't
    /// record: nothing here is a stored history — no transition is
    /// reconstructible, only each item's current facts — so a row carries
    /// exactly what is derivable right now:
    ///
    /// - the item itself (overlay wins, as everywhere);
    /// - the mirror's retention stamp, for a row that has gone absent
    ///   ([`sync::SyncMirror::all_items_including_absent`] — this is the
    ///   first read to surface the retained history ADR-0007 keeps);
    /// - whether a dead-lettered edit targets it (honestly device-local:
    ///   the journal never syncs, so another device's Ledger won't show it);
    /// - whether a live alert names it (the key
    ///   [`hummingbird_domain::item_threshold_v1_key`] builds,
    ///   ADR-0014's item-threshold convention, joined across every source —
    ///   alerts *do* sync, so this badge agrees between devices).
    ///
    /// `now_ms` is caller-injected as everywhere in this crate, and resolves
    /// only alert liveness ([`hummingbird_domain::Alert::is_live`]). Id
    /// order; display order (last-touched) is the caller's, the same split
    /// [`Core::frontier`] leaves ranking to its consumers.
    pub fn ledger(&self, now_ms: i64) -> Vec<LedgerEntry> {
        let mirror = self.cycle.mirror();
        let mut rows: BTreeMap<String, (Item, Option<i64>)> = mirror
            .all_items_including_absent()
            .map(|(item, presence)| {
                let absent_since_ms = match presence {
                    task::Presence::Live => None,
                    task::Presence::Absent { since_ms } => Some(*since_ms),
                };
                (item.id.clone(), (item.clone(), absent_since_ms))
            })
            .collect();
        // Overlay content wins; the retention stamp is the mirror's own
        // fact and survives (a pending edit on an absent row does not make
        // the row less absent until a pull says so).
        for overlay in self.overlay.values() {
            let absent_since_ms = rows
                .get(&overlay.item.id)
                .and_then(|(_, absent)| *absent);
            rows.insert(
                overlay.item.id.clone(),
                (overlay.item.clone(), absent_since_ms),
            );
        }
        let dead_lettered: BTreeSet<String> = self
            .dead_letters()
            .iter()
            .filter_map(|entry| item_id_of_intent(&entry.entry.intent))
            .collect();
        let live_alert_keys: BTreeSet<String> = mirror
            .all_alerts()
            .filter(|alert| alert.is_live(now_ms))
            .map(|alert| alert.source_key.clone())
            .collect();
        rows.into_values()
            .map(|(item, absent_since_ms)| LedgerEntry {
                dead_lettered: dead_lettered.contains(&item.id),
                has_live_alert: live_alert_keys
                    .contains(&hummingbird_domain::item_threshold_v1_key(&item.id)),
                item,
                absent_since_ms,
            })
            .collect()
    }

    /// Everything known about one item, joined — `CONTEXT.md`'s **Item
    /// detail** and the read behind the phone's item screen (ADR-0027).
    /// `None` for an id this device's mirror has never seen, which the
    /// caller answers by running a cycle and re-reading rather than by
    /// rendering an empty shell.
    ///
    /// The first *per-id assembling* read on `Core`, and deliberately not a
    /// new mirror entity: every part of it is a read the mirror already
    /// served ([`Core::overlaid_items_including_absent`], [`Core::steps_for`],
    /// [`sync::SyncMirror::blockers_of`], [`sync::SyncMirror::project`],
    /// [`sync::SyncMirror::alerts_for_source`]), joined here so two clients
    /// cannot join them differently. See [`item_detail::ItemDetail`] for
    /// what each field decides and why.
    ///
    /// Archived rows are included, not filtered: **Recall** makes history
    /// readable, and a notification about an item cancelled since is
    /// exactly the tap that lands here. `now_ms` resolves only alert
    /// liveness, caller-injected as everywhere in this crate.
    pub fn item_detail(&self, item_id: &str, now_ms: i64) -> Option<item_detail::ItemDetail> {
        let items = self.overlaid_items_including_absent();
        let item = items.get(item_id)?.clone();
        let mirror = self.cycle.mirror();

        let project = item.project_id.as_ref().map(|id| item_detail::ProjectRef {
            id: id.clone(),
            name: mirror.project(id).map(|project| project.name.clone()),
        });

        let open_blockers = mirror
            .blockers_of(item_id)
            .filter_map(|blocker_id| match items.get(blocker_id) {
                // Settled: neither archived nor Done still holds anything
                // back, so it is not listed at all.
                Some(blocker)
                    if blocker.archived_at.is_some() || blocker.stage == Stage::Done =>
                {
                    None
                }
                Some(blocker) => Some(item_detail::BlockerEntry {
                    item_id: blocker.id.clone(),
                    title: Some(blocker.title.clone()),
                }),
                // Unseen, and listed anyway — see the field's own doc.
                None => Some(item_detail::BlockerEntry {
                    item_id: blocker_id.to_string(),
                    title: None,
                }),
            })
            .collect();

        // Constructed through the domain recipe, never parsed: the key
        // convention has one owner (ADR-0027 part 2), and this direction
        // never needs the inverse.
        let source_key = hummingbird_domain::item_threshold_v1_key(item_id);
        let live_alert = mirror
            .alerts_for_source(hummingbird_domain::ITEM_THRESHOLD_V1)
            .find(|alert| alert.source_key == source_key && alert.is_live(now_ms))
            .cloned();

        let is_archived = item.archived_at.is_some();
        let available_actions = if is_archived {
            Vec::new()
        } else {
            decisions::available_actions(item.stage).to_vec()
        };

        Some(item_detail::ItemDetail {
            project,
            steps: self.steps_for(item_id),
            open_blockers,
            live_alert,
            is_archived,
            is_editable: !is_archived,
            available_actions,
            item,
        })
    }

    /// Every live `Done` item — the Done screen's read, and the first query
    /// to surface completed work at all. Membership is the same
    /// live-presence rule every other screen uses (`archived_at` unset), so
    /// an item completed and *later* cancelled drops off here and remains
    /// visible only in [`Core::ledger`], labelled archived. Overlaid like
    /// [`Core::frontier`], so a `Complete` taken offline shows immediately.
    /// Id order; the caller orders by `updated_at` (with the documented
    /// caveat that any later edit re-sorts — the schema has no `done_at`).
    pub fn done(&self) -> Vec<Item> {
        self.overlaid_items()
            .into_values()
            .filter(|item| item.archived_at.is_none())
            .filter(|item| item.stage == Stage::Done)
            .collect()
    }

    /// **Recall** (#478, ADR-0025's core-first rule applied): re-find one
    /// known item across everything the mirror has ever known, by
    /// remembered words or by handle. Reads the exact same corpus
    /// [`Core::ledger`] does — live, `Done` and archived alike — and labels
    /// each row a [`search::Group`] the same way `ledger-order.ts`'s
    /// `ledgerRowState` already distinguishes them on the web side:
    /// [`search::Group::Archived`] when the item's own `archived_at` is
    /// `Some` **or** the row's `absent_since_ms` is (an explicit archive
    /// beats the mirror's retention stamp when both exist — `archived_at`
    /// is when the archive actually happened; a pending, not-yet-synced
    /// cancel sets it optimistically before any sweep updates the mirror's
    /// own presence flag), regardless of `stage`; [`search::Group::Done`]
    /// for a live `Stage::Done` row; [`search::Group::Live`] otherwise.
    /// Project names resolve through [`Core::projects`]'s own table
    /// (decision 11: searchable *through* an item, never a project result
    /// of its own) — an item whose project is itself archived resolves no
    /// name here, exactly the limit [`Core::projects`] already has.
    ///
    /// All decision logic — tokenizing, the handle shortcut, ordering, the
    /// cap — lives in [`search::search`], pure and unit-tested there; this
    /// method only assembles the corpus, the same split [`Core::ledger`]
    /// keeps from [`sync::mirror::SyncMirror::all_items_including_absent`].
    pub fn search(&self, query: &str, now_ms: i64) -> search::Outcome {
        let projects: std::collections::HashMap<String, String> = self
            .cycle
            .mirror()
            .all_projects()
            .map(|project| (project.id.clone(), project.name.clone()))
            .collect();

        let candidates: Vec<search::Candidate> = self
            .ledger(now_ms)
            .into_iter()
            .map(|entry| {
                let group = if entry.item.archived_at.is_some() || entry.absent_since_ms.is_some()
                {
                    search::Group::Archived
                } else if entry.item.stage == Stage::Done {
                    search::Group::Done
                } else {
                    search::Group::Live
                };
                let project_name = entry
                    .item
                    .project_id
                    .as_deref()
                    .and_then(|id| projects.get(id))
                    .cloned();
                search::Candidate {
                    item: entry.item,
                    group,
                    project_name,
                }
            })
            .collect();

        search::search(&candidates, query)
    }

    /// How old this device's answer to one standing question is (ADR-0015).
    ///
    /// **Computed here rather than assembled by the host.** The alternative
    /// — handing `fetched_at` and the declared cadence over the seam for TS
    /// to combine — would put the age subtraction back on the far side of
    /// the boundary, and the two prototypes that independently hand-rolled
    /// `Math.max(0, now - fetchedAt)` are the evidence for what happens
    /// next. [`freshness::Freshness::measure`]'s clock rule is only "once,
    /// in one place" if the finished value is what crosses.
    ///
    /// No row (never synced, or demoted by ADR-0003's absence rule) is
    /// [`freshness::Freshness::Unknown`] — not a zero age, and not a
    /// silently fresh answer. `now_ms` is caller-injected, as everywhere
    /// else in this crate.
    ///
    /// Deliberately *not* the pane read: this is the point lookup for one
    /// `(source, key)`. The generic "for a source, its snapshot rows and its
    /// live alerts" read is [`Core::pane_read`] (#245), which embeds the
    /// same freshness per row; nothing in either knows what a pane means by
    /// its answer.
    pub fn snapshot_freshness(&self, source: &str, key: &str, now_ms: i64) -> freshness::Freshness {
        match self.cycle.mirror().context_snapshot(source, key) {
            Some(snapshot) => freshness::Freshness::of_snapshot(now_ms, snapshot),
            None => freshness::Freshness::Unknown,
        }
    }

    /// Everything one standing question's pane reads from this device
    /// (#245, ADR-0015): every live `context_snapshots` row for `source`
    /// (key order, envelope parsed, age measured) and every alert that
    /// source has raised which is **live right now**.
    ///
    /// Per-source rather than per-pane, and `&self` with no `Result`: a
    /// device that has never synced answers with an empty read, which is an
    /// answer, not an error. `now_ms` is caller-injected like every other
    /// clock read in this crate.
    ///
    /// **No overlay.** The context lanes are server-written — no mutation
    /// entry point mints a snapshot or an alert — so there is nothing
    /// optimistic to overlay, exactly as [`Core::steps_for`] and
    /// [`Core::projects`] read the mirror directly.
    ///
    /// **Live-only alerts, decided here.** ADR-0014's predicate needs a
    /// clock, and it is one of the two things ADR-0015 carves into Rust so
    /// it cannot drift (freshness is the other); a pane that re-derived
    /// "still live" in TS would be the second implementation the carve-out
    /// exists to prevent. What is *not* decided here is the join:
    /// `subject_key` rides through untouched, because
    /// `(source, subject_key)` ↔ `(source, key)` is additive and the pane
    /// owns it.
    pub fn pane_read(&self, source: &str, now_ms: i64) -> pane::PaneRead {
        let mirror = self.cycle.mirror();
        pane::PaneRead {
            snapshots: mirror
                .context_snapshots_for(source)
                .map(|snapshot| pane::PaneSnapshot::of_row(now_ms, snapshot))
                .collect(),
            alerts: mirror
                .alerts_for_source(source)
                .filter(|alert| alert.is_live(now_ms))
                .cloned()
                .collect(),
        }
    }

    /// Every live alert across every source, newest raise first — the read
    /// behind M2's alerts surface (#141, ADR-0012). The third alert read
    /// here, and the first that is neither scoped to one source
    /// ([`Core::pane_read`]) nor joined onto an item (the Ledger badge).
    ///
    /// Liveness is ADR-0014's three-clause predicate applied with the
    /// caller's clock ([`Alert::is_live`]), never a naive `dismissed_at`
    /// column test: an expired-then-re-raised occurrence and an acked one
    /// differ only under the full predicate, and `expires_at` is never
    /// written back as a dismissal.
    ///
    /// The order is the decision this read sinks into core rather than
    /// leaving to each client (ADR-0025): `raised_at` descending, ties
    /// broken by id so two devices showing the same rows show them in the
    /// same order. No overlay — an in-flight ack is not projected
    /// optimistically here, the same contract [`Core::rules`] documents;
    /// the notification surface that acks is a fire-and-forget gesture, and
    /// the row leaves this list once the next completed cycle pulls the
    /// dismissal back.
    pub fn live_alerts(&self, now_ms: i64) -> Vec<Alert> {
        let mut alerts: Vec<Alert> = self
            .cycle
            .mirror()
            .all_alerts()
            .filter(|alert| alert.is_live(now_ms))
            .cloned()
            .collect();
        alerts.sort_by(|a, b| b.raised_at.cmp(&a.raised_at).then_with(|| a.id.cmp(&b.id)));
        alerts
    }

    /// One alert by id, live or not — the read behind M2's alert-detail
    /// screen, where a row the human just acked must still render rather
    /// than vanish mid-look. Callers that want the liveness verdict ask
    /// [`Alert::is_live`] themselves with their own clock.
    pub fn alert(&self, id: &str) -> Option<Alert> {
        self.cycle.mirror().alert(id).cloned()
    }

    /// Every standing-question binding (#118, ADR-0015): each
    /// [`BindingKey`] this build knows — set or not — in vocabulary order,
    /// then every other live `settings` row this device has pulled, in key
    /// order.
    ///
    /// The second group is why this is not simply "the three keys": a row a
    /// newer build wrote is really in the table, and an editor that showed
    /// only what it can write would misreport what is actually bound.
    /// [`Binding::known`] is how a reader tells the two apart — an unknown
    /// key is display-only, since minting arbitrary keys into a table with
    /// no DELETE is the failure ADR-0015's closed vocabulary exists to
    /// prevent.
    ///
    /// Reads through the same overlay-over-mirror view every other query
    /// here uses, so a binding set offline reads back set — and `pending` —
    /// immediately, without waiting for a cycle.
    pub fn bindings(&self) -> Vec<Binding> {
        let overlaid = self.overlaid_settings();
        let describe = |key: &str| {
            let value = match overlaid.get(key) {
                Some(setting) => BindingValue::from_stored(&setting.value),
                None => BindingValue::Unset,
            };
            Binding {
                key: key.to_string(),
                known: BindingKey::parse(key).is_some(),
                pending: self.binding_overlay.contains_key(key),
                value,
            }
        };

        let mut bindings: Vec<Binding> = BindingKey::ALL
            .iter()
            .map(|key| describe(key.as_str()))
            .collect();
        bindings.extend(
            overlaid
                .keys()
                .filter(|key| BindingKey::parse(key).is_none())
                .map(|key| describe(key)),
        );
        bindings
    }

    /// Every live `settings` row with every not-yet-confirmed binding write
    /// overlaid on top — [`Core::overlaid_items`]'s twin, and under the same
    /// contract: an overlaid row is present the instant
    /// [`Core::set_binding`] returns, survives every cycle outcome except a
    /// completed one (which supersedes it with server truth) or its own
    /// entry dead-lettering (which reverts it).
    fn overlaid_settings(&self) -> BTreeMap<String, Setting> {
        let mut settings: BTreeMap<String, Setting> = self
            .cycle
            .mirror()
            .all_settings()
            .map(|setting| (setting.key.clone(), setting.clone()))
            .collect();
        for overlay in self.binding_overlay.values() {
            settings.insert(overlay.setting.key.clone(), overlay.setting.clone());
        }
        settings
    }

    /// Sets one binding (#118): enqueues an absolute-value CAS
    /// `PUT /api/settings/:key` — durably, via [`sync::SyncCycle::enqueue`],
    /// never [`sync::queue::OutboundQueue::enqueue`] directly, the same rule
    /// [`Core::capture`]/[`Core::act`]/[`Core::triage`] follow — and
    /// overlays the row so a reader sees the new value immediately, offline
    /// or not.
    ///
    /// **No bespoke write path.** This is the ordinary entity-level CAS the
    /// whole owned-schema write vocabulary shares: `expected_version` is the
    /// version this device last knew (`0` when it knows no row at all, which
    /// is how `PUT` carries create semantics — see the authority's
    /// `handlers/settings.rs`), a stale write 409s and rebases per ADR-0008,
    /// and an unresolvable one dead-letters like any other.
    ///
    /// `key` is a resolved [`BindingKey`], never a raw string: the wire
    /// spelling is rejected by name at the seam
    /// (`ffi-web`'s `set_binding`), so no caller can mint a key into a table
    /// that has no DELETE. `value` is text — every binding this vocabulary
    /// carries is a name or an id — and is sent as a JSON string, which the
    /// authority stores as that value's canonical JSON.
    ///
    /// `seed` mints this mutation's queue-entry id
    /// ([`sync::write::deterministic_id`]) — caller-supplied, the same
    /// no-clock/no-RNG reasoning as every other mutation entry point here.
    pub async fn set_binding(
        &mut self,
        seed: &str,
        key: BindingKey,
        value: &str,
        now_ms: i64,
    ) -> Result<(), SnapshotError<QS::Error>> {
        let key = key.as_str();
        let current = self.overlaid_settings().get(key).cloned();

        // The wire's `value` is typed JSON (`PutSetting::value`); the stored
        // column is that JSON's canonical *text* (`Setting::value`). Both
        // encodings are carried: the first is what gets sent, the second is
        // what a 409 and the overlay are diffed and rebuilt in — see
        // `MutationIntent::Patch::rebase_fields`.
        let wire_value = serde_json::Value::String(value.to_string());
        let stored_value = wire_value.to_string();

        let (base, base_updated_at) = match &current {
            Some(setting) => (
                serde_json::to_value(setting).expect("Setting always serializes"),
                setting.updated_at,
            ),
            // No row this device knows of: version `0` is the create's
            // `expected_version`, and JSON `null` stands in for the value
            // there is not — never diffed (a create-shaped PUT cannot 409)
            // and overwritten by this very patch in the overlay below.
            None => (
                serde_json::json!({
                    "key": key,
                    "value": "null",
                    "updated_at": 0,
                    "version": 0,
                }),
                0,
            ),
        };

        let entry_id = sync::write::deterministic_id(seed);
        let entry = QueueEntry {
            id: entry_id.clone(),
            intent: MutationIntent::Patch {
                path: sync::write::paths::setting(key),
                method: HttpMethod::Put,
                base,
                base_updated_at,
                patch_fields: serde_json::json!({ "value": wire_value }),
                rebase_fields: Some(serde_json::json!({ "value": stored_value })),
            },
        };

        self.cycle.enqueue(entry, now_ms).await?;

        self.binding_overlay.insert(
            key.to_string(),
            SettingOverlayEntry {
                entry_id,
                setting: Setting {
                    key: key.to_string(),
                    value: stored_value,
                    updated_at: now_ms,
                    version: current.map(|setting| setting.version).unwrap_or(0),
                },
            },
        );

        Ok(())
    }

    /// Every rule (#140), id order — the read behind the rules screen. No
    /// overlay: unlike `bindings`/`overlaid_items`, a locally-created or
    /// -patched rule is not projected optimistically here — it becomes
    /// visible once the next completed cycle pulls it back, the same "read
    /// the mirror directly" contract [`Core::steps_for`]/[`Core::projects`]
    /// already follow for entities no mutation entry point overlays. A 409
    /// or a durability failure on the write still cannot be lost silently:
    /// it lands in the ordinary dead-letter journal
    /// ([`Core::dead_letters`]-equivalent host surface), the same generic
    /// per-field-diff affordance every other CAS write already uses — #140
    /// deliberately adds no bespoke conflict surface for rules.
    pub fn rules(&self) -> Vec<Rule> {
        self.cycle.mirror().all_rules().cloned().collect()
    }

    /// Creates a rule (#140): enqueues a `POST /api/rules` create, durably,
    /// via [`sync::SyncCycle::enqueue`] — the same rule every other mutation
    /// entry point here follows. `seed` mints the deterministic id
    /// ([`sync::write::deterministic_id`]), caller-supplied for the same
    /// no-clock/no-RNG reasoning as [`Core::capture`]. Returns the minted
    /// id. No optimistic overlay — see [`Core::rules`]'s own doc for why.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_rule(
        &mut self,
        seed: &str,
        name: impl Into<String>,
        event_kind: Option<String>,
        conditions: Vec<Condition>,
        severity: impl Into<String>,
        tier: Tier,
        enabled: bool,
        now_ms: i64,
    ) -> Result<String, SnapshotError<QS::Error>> {
        let id = sync::write::deterministic_id(seed);
        let create = CreateRule {
            id: id.clone(),
            name: name.into(),
            event_kind,
            conditions,
            severity: severity.into(),
            tier,
            enabled: Some(enabled),
        };
        let body = serde_json::to_value(&create).expect("CreateRule always serializes");
        let entry = QueueEntry {
            id: id.clone(),
            intent: MutationIntent::Create {
                path: sync::write::paths::rules(),
                body,
            },
        };
        self.cycle.enqueue(entry, now_ms).await?;
        Ok(id)
    }

    /// Patches a rule (#140) — the enable/disable toggle, and the rest of
    /// the editor's fields, share this one entry point: every `Some` field
    /// is touched, absolute-set, exactly [`RulePatch`]'s own contract, and
    /// `None` means "leave this field alone." `expected_version` is the
    /// version this device last knew — the ordinary CAS contract, so a
    /// stale write 409s and rebases or dead-letters like any other
    /// (`patch_with_rebase`, `sync::write::adapter`); this entry point never
    /// swallows that outcome, it only enqueues it durably. `base` is
    /// `current` verbatim (the caller's own last-known copy of the row,
    /// e.g. from [`Core::rules`]), for [`sync::write::rebase::decide`] to
    /// diff a 409 against.
    #[allow(clippy::too_many_arguments)]
    pub async fn patch_rule(
        &mut self,
        seed: &str,
        current: &Rule,
        name: Option<String>,
        event_kind: Option<Option<String>>,
        conditions: Option<Vec<Condition>>,
        severity: Option<String>,
        tier: Option<Tier>,
        enabled: Option<bool>,
        now_ms: i64,
    ) -> Result<(), SnapshotError<QS::Error>> {
        let base = serde_json::to_value(current).expect("Rule always serializes");
        let mut patch = RulePatch {
            expected_version: current.version,
            name: None,
            event_kind: None,
            conditions: None,
            severity: None,
            tier: None,
            enabled: None,
        };
        patch.name = name;
        patch.event_kind = event_kind;
        patch.conditions = conditions;
        patch.severity = severity;
        patch.tier = tier;
        patch.enabled = enabled;

        let mut patch_fields = serde_json::to_value(&patch).expect("RulePatch always serializes");
        // `expected_version` rides on the wire body but is not itself a
        // "touched field" for rebase purposes — `drain` refills it from
        // whatever version is live at send time (see `MutationIntent::Patch`
        // field doc), so it must not appear in `patch_fields`.
        if let serde_json::Value::Object(map) = &mut patch_fields {
            map.remove("expected_version");
        }

        let entry = QueueEntry {
            id: sync::write::deterministic_id(seed),
            intent: MutationIntent::Patch {
                path: sync::write::paths::rule(&current.id),
                method: HttpMethod::Patch,
                base,
                base_updated_at: current.updated_at,
                patch_fields,
                rebase_fields: None,
            },
        };
        self.cycle.enqueue(entry, now_ms).await?;
        Ok(())
    }

    /// Acks an alert (#141, ADR-0012): enqueues a `PATCH /api/alerts/:id`
    /// setting the human-owned `dismissed_at`, the one alert column a
    /// `device` token may write. Pass `dismissed_at: None` to un-dismiss —
    /// the DTO's double-`Option` sends an explicit JSON `null`, which the
    /// authority reads as "clear it", distinct from not touching the field.
    ///
    /// **Why this takes `current` rather than just an id.** A push payload
    /// carries no `version` (`server/authority/src/fcm.rs`), and the write
    /// is CAS: the `expected_version` has to come from the synced mirror,
    /// so an ack from a notification is sync-then-CAS. Reading the row
    /// through [`Core::alert`] is that read; a 409 lands in the ordinary
    /// dead-letter journal like every other CAS write here, and the caller
    /// re-reads and re-acks. Setting `dismissed_at` to the value already
    /// stored is a version-preserving no-op server-side, so a replayed ack
    /// is harmless.
    ///
    /// **Swipe is not a gesture** (ADR-0012): a dismissed notification
    /// must not reach this method. Only an explicit Ack does.
    ///
    /// `seed` mints the queue-entry id ([`sync::write::deterministic_id`]),
    /// caller-supplied for the same no-clock/no-RNG reasoning as every
    /// other mutation entry point here. No optimistic overlay — see
    /// [`Core::live_alerts`].
    pub async fn dismiss_alert(
        &mut self,
        seed: &str,
        current: &Alert,
        dismissed_at: Option<i64>,
        now_ms: i64,
    ) -> Result<(), SnapshotError<QS::Error>> {
        let base = serde_json::to_value(current).expect("Alert always serializes");
        let patch = AlertPatch {
            expected_version: current.version,
            dismissed_at: Some(dismissed_at),
        };

        let mut patch_fields = serde_json::to_value(&patch).expect("AlertPatch always serializes");
        // `expected_version` rides on the wire body but is not a "touched
        // field" for rebase purposes — `drain` refills it from whatever
        // version is live at send time (see `MutationIntent::Patch`).
        if let serde_json::Value::Object(map) = &mut patch_fields {
            map.remove("expected_version");
        }

        let entry = QueueEntry {
            id: sync::write::deterministic_id(seed),
            intent: MutationIntent::Patch {
                path: sync::write::paths::alert(&current.id),
                method: HttpMethod::Patch,
                base,
                // Alerts have no `updated_at` column (ADR-0009): `raised_at`
                // is the row's own last content stamp — a source-owned
                // change restamps it (`restamp_on_change`) — so it is what
                // this ADR-0007 S5 conflict-metadata slot carries here.
                base_updated_at: current.raised_at,
                patch_fields,
                rebase_fields: None,
            },
        };
        self.cycle.enqueue(entry, now_ms).await?;
        Ok(())
    }

    /// Captures a new item: enqueues a `POST /api/items` create (durably,
    /// via [`sync::SyncCycle::enqueue`] — never [`sync::queue::OutboundQueue::enqueue`]
    /// directly, per that module's own durability rule) and overlays an
    /// optimistic [`Item`] so [`Core::frontier`] sees it immediately.
    ///
    /// `seed` mints this capture's deterministic id
    /// ([`sync::write::deterministic_id`]) — caller-supplied rather than
    /// sampled, the same "no clock or RNG that does not panic on bare
    /// wasm32" reasoning every other caller-injected value in `sync`
    /// documents. Returns the minted id.
    ///
    /// Runs `title` through [`capture::parse_seam`] (issue #110/#42's named
    /// no-op) and discards the result: the `title` that reaches the
    /// mutation below is the caller's own string, verbatim, never the
    /// seam's output.
    ///
    /// `options` (#208) carries the capture box's own field selections onto
    /// the `CreateItem` — each field defaults to `None` in
    /// [`CaptureOptions::default`], so a caller that never touches the
    /// controls still produces a capture with all of them absent. Deciding
    /// stays optional at capture time; what changed is that a reader who has
    /// already decided need not wait for the mint to say so.
    pub async fn capture(
        &mut self,
        seed: &str,
        title: impl Into<String>,
        stage: Stage,
        now_ms: i64,
        options: CaptureOptions,
    ) -> Result<String, SnapshotError<QS::Error>> {
        let id = sync::write::deterministic_id(seed);
        let title = title.into();
        let _ = capture::parse_seam(&title);

        let create = CreateItem {
            id: id.clone(),
            title: title.clone(),
            description: options.description,
            stage: Some(stage),
            size: options.size,
            energy: options.energy,
            context: options.context,
            priority: options.priority,
            project_id: options.project_id,
            project_pos: None,
            deadline: options.deadline,
            scheduled_date: options.scheduled_date,
            source: None,
            source_key: None,
            source_url: None,
            // No client affordance sets the delegation axis (#115): the
            // skill is its only writer today, so a capture is the human's.
            agent: None,
        };
        let body = serde_json::to_value(&create).expect("CreateItem always serializes");
        let entry = QueueEntry {
            id: id.clone(),
            intent: MutationIntent::Create {
                path: sync::write::paths::items(),
                body,
            },
        };

        self.cycle.enqueue(entry, now_ms).await?;

        let optimistic = item_from_create(&create, now_ms);
        self.overlay.insert(
            id.clone(),
            OverlayEntry {
                entry_id: id.clone(),
                item: optimistic,
            },
        );

        Ok(id)
    }

    /// Acts on an already-existing item (S11/#109): enqueues a CAS `PATCH`
    /// (durably, via [`sync::SyncCycle::enqueue`] — never
    /// [`sync::queue::OutboundQueue::enqueue`] directly, the same rule
    /// [`Core::capture`] follows) and overlays the item's post-mutation
    /// value so a reader sees the change immediately, offline or not
    /// (this issue's "Completing offline shows Done immediately").
    ///
    /// `base` for the CAS write is [`Core::overlaid_items`]'s view of the
    /// item — the entity as this client last knew it, including any of its
    /// own still-queued create, exactly the contract
    /// [`sync::write::adapter::patch_with_rebase`] documents for `base`.
    ///
    /// If this entry is later dead-lettered (a same-field server-side
    /// change already landed), [`Core::run`]'s existing `entry_id`-matched
    /// overlay revert (unchanged by this method — it already generalises to
    /// any overlay entry, not just a capture's) reverts the overlay the same
    /// way it does for a dead-lettered capture — the UI falls back to
    /// mirror truth and [`Core::dead_letters`] carries the affordance,
    /// never a silent revert.
    ///
    /// **Known gap, not closed here:** the overlay map is keyed one entry
    /// per item id, so an `act` called while that same item's own create is
    /// still queued (unconfirmed) replaces the create's overlay entry
    /// outright — `base` above already reads the create's own optimistic
    /// item, so the *displayed* state is still correct, but the create's
    /// original `entry_id` is no longer what this item's overlay entry
    /// points at. If that create is later dead-lettered, `Core::run`'s
    /// `entry_id` match no longer finds it via this item's overlay entry
    /// (which now points at this patch's `entry_id` instead), so the
    /// overlay would not revert on that specific failure. Acting on a
    /// genuinely still-queued create is not a normal flow this UI drives
    /// today (S11's buttons only render for items already read from
    /// `Core::frontier`/`Core::blocked`, which excludes an unconfirmed
    /// create's target unless it already round-tripped once), so this is
    /// narrow — flagged rather than fixed, since closing it properly needs
    /// the overlay to track more than one pending mutation per item.
    ///
    /// `seed` mints this mutation's own queue-entry id
    /// ([`sync::write::deterministic_id`]) — caller-supplied, same
    /// reasoning as [`Core::capture`]'s `seed`.
    pub async fn act(
        &mut self,
        seed: &str,
        item_id: &str,
        action: ItemAction,
        now_ms: i64,
    ) -> Result<(), ActError<QS::Error>> {
        let items = self.overlaid_items();
        let Some(current) = items.get(item_id) else {
            return Err(ActError::ItemNotFound);
        };

        let base = serde_json::to_value(current).expect("Item always serializes");
        let mut optimistic = current.clone();
        let patch_fields = match action.stage() {
            Some(stage) => {
                optimistic.stage = stage;
                serde_json::json!({ "stage": stage })
            }
            None => {
                optimistic.archived_at = Some(now_ms);
                serde_json::json!({ "archived_at": now_ms })
            }
        };
        optimistic.updated_at = now_ms;

        let entry_id = sync::write::deterministic_id(seed);
        let entry = QueueEntry {
            id: entry_id.clone(),
            intent: MutationIntent::Patch {
                path: sync::write::paths::item(item_id),
                method: HttpMethod::Patch,
                base,
                base_updated_at: current.updated_at,
                patch_fields,
                rebase_fields: None,
            },
        };

        self.cycle
            .enqueue(entry, now_ms)
            .await
            .map_err(ActError::Snapshot)?;

        self.overlay.insert(
            item_id.to_string(),
            OverlayEntry {
                entry_id,
                item: optimistic,
            },
        );

        Ok(())
    }

    /// Acts on an item and, when the action *settles the matter*, acks the
    /// live alert about it in the same gesture (ADR-0027 part 3,
    /// `CONTEXT.md`'s amended **Ack**).
    ///
    /// **Which actions ack.** Completing or cancelling says the matter was
    /// dealt with, so the alert raised about it is dealt with too. Starting
    /// it or declaring an external wait are *not* acks: neither claims the
    /// matter is settled, and an alert that keeps ringing through a stalled
    /// start is the lane working, not failing. That rule is held here
    /// because two clients answering it differently would be a bug.
    ///
    /// **Two queue entries, deliberately not one.** ADR-0027 refuses to
    /// invent an authority-side combined mutation to serve a client-side
    /// gesture; what it forbids is that, not this helper. The `act` is
    /// enqueued **first** and the ack second, so the ordered, durable queue
    /// drains them in the order the human's intent happened — and a
    /// dead-letter on one and not the other lands in the journal, which
    /// names what each change was about.
    ///
    /// One `seed` mints both entry ids: the ack's is derived from it, so a
    /// caller cannot accidentally hand the two entries the same id, and a
    /// replay of the same gesture is idempotent across both.
    ///
    /// No live alert, or an action that does not settle the matter, makes
    /// this exactly [`Core::act`].
    pub async fn act_acking_alert(
        &mut self,
        seed: &str,
        item_id: &str,
        action: ItemAction,
        now_ms: i64,
    ) -> Result<(), ActError<QS::Error>> {
        let alert_to_ack = match action {
            ItemAction::Complete | ItemAction::Cancel => self
                .item_detail(item_id, now_ms)
                .and_then(|detail| detail.live_alert),
            ItemAction::Start | ItemAction::Block => None,
        };

        self.act(seed, item_id, action, now_ms).await?;

        if let Some(alert) = alert_to_ack {
            self.dismiss_alert(&format!("{seed}-ack"), &alert, Some(now_ms), now_ms)
                .await
                .map_err(ActError::Snapshot)?;
        }
        Ok(())
    }

    /// Triages an already-existing (captured) item (S13/#111): edits
    /// whatever fields `patch` sets, and — when `promote_to_ready` is `true`
    /// — promotes it to `Ready`, as **one** CAS `PATCH` — enqueued the same
    /// way [`Core::act`] enqueues its own single-field patch (durably, via
    /// [`sync::SyncCycle::enqueue`], never
    /// [`sync::queue::OutboundQueue::enqueue`] directly), never four
    /// separate mutations for four separate fields. Fewer conflict surfaces
    /// is the point: a 409 on this triage rebases (or dead-letters) the
    /// whole edit together, not one field at a time.
    ///
    /// `base` for the CAS write, the optimistic overlay this stamps, the
    /// dead-letter revert on [`Core::run`], and the "acting on a genuinely
    /// still-queued create" gap are all exactly [`Core::act`]'s own
    /// reasoning — see that method's doc; nothing about combining several
    /// fields into one patch changes it.
    ///
    /// A triaged item leaves [`Core::triage_inbox`] and — when promoted —
    /// appears on [`Core::frontier`] the instant this returns, through the
    /// same overlay every other read here goes through, never a separate
    /// local bookkeeping list. `Ready` is triage's only destination
    /// (#360): an item that reaches `Grilling` does so exactly one way — a
    /// `fog_remains` verdict from a completed Grill ([`Core::complete_grill`]),
    /// never through this entry point. An item not yet ready to promote
    /// simply stays in `Triage` — `promote_to_ready: false` — which is not a
    /// destination at all, only "leave `stage` alone".
    ///
    /// `promote_to_ready` is a plain `bool` rather than an enum of stage
    /// destinations because, with `Grilling` gone, there is only ever one
    /// stage this call may promote an item *into* — so the only remaining
    /// question is whether it does. `false` leaves `stage` off the patch
    /// entirely — the authority's `ItemPatch.stage` is already `Option`, so
    /// an absent field there is genuinely untouched, not defaulted — and the
    /// optimistic overlay keeps the item's current stage. This is what lets
    /// the same entry point carry a pure field edit — the weekend-plans
    /// pane's do-date chip — on an item that is not going through the triage
    /// promotion at all: `Core::frontier` only ever holds
    /// `Ready`/`InProgress` items, so a call that always promoted would
    /// demote an in-progress item back to Ready the moment its do-date
    /// changed. Every caller still enqueues through this same CAS `PATCH` —
    /// never a second mutation entry point.
    ///
    /// `seed` mints this mutation's own queue-entry id
    /// ([`sync::write::deterministic_id`]) — caller-supplied, same reasoning
    /// as [`Core::act`]'s `seed`.
    pub async fn triage(
        &mut self,
        seed: &str,
        item_id: &str,
        promote_to_ready: bool,
        patch: TriagePatch,
        now_ms: i64,
    ) -> Result<(), ActError<QS::Error>> {
        let items = self.overlaid_items();
        let Some(current) = items.get(item_id) else {
            return Err(ActError::ItemNotFound);
        };

        let base = serde_json::to_value(current).expect("Item always serializes");
        let mut optimistic = current.clone();
        let mut patch_fields = serde_json::Map::new();

        if promote_to_ready {
            optimistic.stage = Stage::Ready;
            patch_fields.insert(
                "stage".to_string(),
                serde_json::to_value(Stage::Ready).expect("Stage always serializes"),
            );
        }

        // Absolute-value sets, one per TOUCHED field — the outer `Option` is
        // the only thing that decides whether a field appears in
        // `patch_fields` at all, and a cleared field appears as a JSON `null`
        // rather than being left out. Leaving a clear out would send "leave
        // this alone", which is the opposite instruction.
        if let Some(title) = &patch.title {
            optimistic.title = title.clone();
            patch_fields.insert("title".to_string(), serde_json::json!(title));
        }
        if let Some(description) = &patch.description {
            optimistic.description = description.clone();
            patch_fields.insert("description".to_string(), serde_json::json!(description));
        }
        if let Some(size) = patch.size {
            optimistic.size = size;
            patch_fields.insert(
                "size".to_string(),
                serde_json::to_value(size).expect("Size always serializes"),
            );
        }
        if let Some(energy) = patch.energy {
            optimistic.energy = energy;
            patch_fields.insert(
                "energy".to_string(),
                serde_json::to_value(energy).expect("Energy always serializes"),
            );
        }
        if let Some(context) = &patch.context {
            optimistic.context = context.clone();
            patch_fields.insert("context".to_string(), serde_json::json!(context));
        }
        if let Some(priority) = patch.priority {
            optimistic.priority = priority;
            patch_fields.insert("priority".to_string(), serde_json::json!(priority));
        }
        if let Some(project_id) = &patch.project_id {
            optimistic.project_id = project_id.clone();
            patch_fields.insert("project_id".to_string(), serde_json::json!(project_id));
        }
        if let Some(deadline) = &patch.deadline {
            optimistic.deadline = deadline.clone();
            patch_fields.insert("deadline".to_string(), serde_json::json!(deadline));
        }
        // Three-state (#122): outer `None` is skipped entirely (untouched);
        // `Some(None)` clears — a real `null` on the wire, never an absent
        // key that would silently do nothing; `Some(Some(date))` sets it.
        if let Some(scheduled_date) = &patch.scheduled_date {
            optimistic.scheduled_date = scheduled_date.clone();
            patch_fields.insert("scheduled_date".to_string(), serde_json::json!(scheduled_date));
        }
        optimistic.updated_at = now_ms;

        let entry_id = sync::write::deterministic_id(seed);
        let entry = QueueEntry {
            id: entry_id.clone(),
            intent: MutationIntent::Patch {
                path: sync::write::paths::item(item_id),
                method: HttpMethod::Patch,
                base,
                base_updated_at: current.updated_at,
                patch_fields: serde_json::Value::Object(patch_fields),
                rebase_fields: None,
            },
        };

        self.cycle
            .enqueue(entry, now_ms)
            .await
            .map_err(ActError::Snapshot)?;

        self.overlay.insert(
            item_id.to_string(),
            OverlayEntry {
                entry_id,
                item: optimistic,
            },
        );

        Ok(())
    }

    /// Confirms a completed Grill interview (#354, ADR-0023): enqueues the
    /// one atomic authority mutation — the Grill record, the item's stage
    /// move, and (gated by `completion.delete_unticked_plan`) the unticked
    /// Plan's soft-delete, all one `POST /api/grills` — and overlays the
    /// item's new stage so a reader sees it immediately, offline or not.
    /// `base`/overlay/dead-letter-revert reasoning is exactly
    /// [`Core::act`]'s own (see that method's doc); nothing about a Grill
    /// completion changes any of it.
    ///
    /// `session_steps` is this review session's own captured snapshot of
    /// the item's Steps, compared against live mirror state via
    /// [`unticked_steps_changed`]. A drift there
    /// ([`CompleteGrillError::NeedsReReview`]) refuses to enqueue at all —
    /// see that function's doc for exactly what counts.
    ///
    /// The item's own eligibility is checked with the identical function
    /// the authority uses ([`hummingbird_domain::resulting_stage`]) before
    /// ever touching the queue: a `Done` item is refused locally, the same
    /// rejection the authority would otherwise answer with a 400.
    ///
    /// `seed` mints this Grill's own id
    /// ([`sync::write::deterministic_id`]) — the *entity*-minting flavor of
    /// the seed-minting rule ([`sync`]'s module doc), because a Grill is a
    /// brand-new record, not a CAS write against an existing one; exactly
    /// [`Core::capture`]'s own reasoning. Returns the minted Grill id.
    pub async fn complete_grill(
        &mut self,
        seed: &str,
        item_id: &str,
        session_steps: &[Step],
        completion: GrillCompletion,
        now_ms: i64,
    ) -> Result<String, CompleteGrillError<QS::Error>> {
        let items = self.overlaid_items();
        let Some(current) = items.get(item_id) else {
            return Err(CompleteGrillError::ItemNotFound);
        };
        let resulting = resulting_stage(current.stage, completion.verdict)
            .map_err(|_| CompleteGrillError::ItemDone)?;

        let live_steps = self.steps_for(item_id);
        if unticked_steps_changed(session_steps, &live_steps) {
            return Err(CompleteGrillError::NeedsReReview);
        }

        let grill_id = sync::write::deterministic_id(seed);
        let create = CreateGrill {
            id: grill_id.clone(),
            item_id: item_id.to_string(),
            expected_version: current.version,
            transcript: completion.transcript,
            summary: completion.summary,
            verdict: completion.verdict,
            model_proposal: completion.model_proposal,
            applied_patch: completion.applied_patch,
            delete_unticked_plan: completion.delete_unticked_plan,
        };
        // `expected_version` is `drain`'s to fill in per attempt (from
        // whichever version the item is rebased onto) — the same split
        // `MutationIntent::Patch::patch_fields` keeps from its own
        // `expected_version`, so it must not sit inside `grill_fields`.
        let mut grill_fields = serde_json::to_value(&create).expect("CreateGrill always serializes");
        if let serde_json::Value::Object(fields) = &mut grill_fields {
            fields.remove("expected_version");
        }
        let item_base = serde_json::to_value(current).expect("Item always serializes");

        let entry = QueueEntry {
            id: grill_id.clone(),
            intent: MutationIntent::CompleteGrill {
                path: sync::write::paths::grills(),
                grill_fields,
                item_base,
                item_base_updated_at: current.updated_at,
            },
        };

        self.cycle
            .enqueue(entry, now_ms)
            .await
            .map_err(CompleteGrillError::Snapshot)?;

        let mut optimistic = current.clone();
        optimistic.stage = resulting;
        optimistic.updated_at = now_ms;
        self.overlay.insert(
            item_id.to_string(),
            OverlayEntry {
                entry_id: grill_id.clone(),
                item: optimistic,
            },
        );

        Ok(grill_id)
    }

    /// #356's device-local draft read: this item's saved Grill turns, as the
    /// caller's own opaque JSON, or `None` when the item has no draft. Never
    /// touches the outbound queue or the mirror — a draft is device-local and
    /// never syncs (ADR-0023's own "device-local... never a Grill record").
    pub fn grill_draft(&self, item_id: &str) -> Option<&serde_json::Value> {
        self.grill_drafts.0.get(item_id)
    }

    /// Whether `item_id` has a saved draft — the one function a row's
    /// "Grill me"/"Resume grill" label is decided by (#356), the same "one
    /// deciding function" discipline `item-actions.ts`'s `canGrill` already
    /// documents for its own affordance.
    pub fn has_grill_draft(&self, item_id: &str) -> bool {
        self.grill_drafts.0.contains_key(item_id)
    }

    /// Every item id carrying a draft — the bulk read a Triage-inbox-wide
    /// "Resume grill" label needs without one request per row.
    pub fn grill_draft_item_ids(&self) -> Vec<String> {
        self.grill_drafts.0.keys().cloned().collect()
    }

    /// Saves (or replaces) `item_id`'s draft — #356's "Back or close saves
    /// automatically" contract: the caller re-saves after every completed
    /// turn, not just on Back, so the draft is never more than one round
    /// stale. Two tabs on one origin write through this ONE `Core` (ADR-0010)
    /// — the last save wins, with no lock and no arbitration, exactly the
    /// issue's own "so there is no cross-tab lock to build."
    pub async fn save_grill_draft(
        &mut self,
        item_id: &str,
        turns: serde_json::Value,
        now_ms: i64,
    ) -> Result<(), SnapshotError<MS::Error>> {
        self.grill_drafts.0.insert(item_id.to_string(), turns);
        save_snapshot(
            &self.grill_draft_store,
            GRILL_DRAFTS_SCHEMA_VERSION,
            now_ms as u64,
            &self.grill_drafts,
        )
        .await
    }

    /// Discards `item_id`'s draft — #356's explicit, confirmed "Discard"
    /// gesture, and the one place a completed Grill's `"ok"` clears the
    /// interview that produced it (the caller's job to call this only then,
    /// never on enqueue of a completion that might still dead-letter — see
    /// `GrillCompletion`'s own doc on why a dead-lettered completion still
    /// has everything it needs, separately from this draft). A no-op, not an
    /// error, when no draft exists.
    pub async fn discard_grill_draft(
        &mut self,
        item_id: &str,
        now_ms: i64,
    ) -> Result<(), SnapshotError<MS::Error>> {
        if self.grill_drafts.0.remove(item_id).is_none() {
            return Ok(());
        }
        save_snapshot(
            &self.grill_draft_store,
            GRILL_DRAFTS_SCHEMA_VERSION,
            now_ms as u64,
            &self.grill_drafts,
        )
        .await
    }

    /// The host calls this at init and on every credential rotation.
    /// Always resumes (a fresh push is the only way out of a hold), the
    /// same contract [`context::ContextPoller::push_token`] documents.
    ///
    /// Also drops any not-yet-drained [`CoreEvent::CredentialNeeded`]: once
    /// a fresh key is pushed, that prompt is moot — a host that had not
    /// gotten around to draining it yet must not see it fire after the
    /// rotation it is asking for already happened.
    pub fn push_api_key(&mut self, api_key: impl Into<String>) {
        self.credential.push(api_key.into());
        self.events
            .retain(|event| !matches!(event, CoreEvent::CredentialNeeded { .. }));
    }

    /// Issue #196 (shape 2): the REHYDRATION half of what used to be a
    /// single `push_api_key` call site — a host reloading a token it
    /// already had stored (core start, or a later view reaching `ready`
    /// under #126's one-shared-core-per-origin), not a genuinely new or
    /// re-entered one. Unlike [`Core::push_api_key`], never resumes a hold
    /// and never drops a pending [`CoreEvent::CredentialNeeded`]: those two
    /// side effects are the rotation contract, and a second view rehydrating
    /// the very token that just got rejected must not be able to trigger
    /// them, or the client silently retries a credential already known to
    /// be dead and Settings loses the prompt it was about to show.
    ///
    /// Still sets the token when nothing is held — a first-run device with
    /// a stored (never-yet-rejected) token still needs this to actually
    /// reach the credential slot, exactly as [`Core::push_api_key`] would.
    /// Only a genuinely fresh [`Core::push_api_key`] call resumes a hold.
    pub fn rehydrate_api_key(&mut self, api_key: impl Into<String>) {
        self.credential.rehydrate(api_key.into());
    }

    /// "Forget token" (#106/S8): discards the in-memory credential outright
    /// rather than holding it — the next [`Core::run`] reports
    /// [`CoreCycleOutcome::NoCredential`], the same steady state a device
    /// that has never pushed a key sees, not [`CoreCycleOutcome::Held`]
    /// (which would wrongly imply a key was rejected and is waiting to be
    /// retried). Never touches anything durable: the credential was never
    /// persisted in the first place (see the `compile_fail` proof on
    /// [`Core`]), so there is nothing here to clean up.
    ///
    /// Also drops any not-yet-drained [`CoreEvent::CredentialNeeded`], for
    /// the same reason [`Core::push_api_key`] does — forgetting the key is
    /// this host's own deliberate action, not something a stale prompt
    /// should re-litigate.
    pub fn clear_api_key(&mut self) {
        self.credential.clear();
        self.events
            .retain(|event| !matches!(event, CoreEvent::CredentialNeeded { .. }));
    }

    /// Drains every [`CoreEvent`] recorded since the last drain — poll-style
    /// rather than callback-delivered (ADR-0003).
    pub fn take_events(&mut self) -> Vec<CoreEvent> {
        std::mem::take(&mut self.events)
    }

    /// Runs one [`sync::SyncCycle::run`] cycle, held outright
    /// ([`CoreCycleOutcome::Held`], no transport ever reached — not even a
    /// [`sync::Backoff`]-gated no-op) if the credential is currently held,
    /// or reported as [`CoreCycleOutcome::NoCredential`] if none has ever
    /// been pushed.
    ///
    /// **Closes the pull-side 401 gap `sync::cycle`'s module docs name.** A
    /// [`sync::CycleOutcome::CredentialNeeded`] — from either half of the
    /// cycle — holds this credential here, so every subsequent `run` call,
    /// however triggered, short-circuits to `Held` until
    /// [`Core::push_api_key`] supplies a fresh key. That is a hold, not a
    /// backoff: unlike [`sync::Backoff`], nothing here ever expires on its
    /// own, because a dead credential is not a transient failure — no
    /// amount of waiting fixes it.
    #[allow(clippy::too_many_arguments)]
    pub async fn run(
        &mut self,
        read_transport: &impl ChangesTransport,
        write_transport: &impl MutationTransport,
        now_ms: i64,
        trigger: Trigger,
        force_full_sweep: bool,
        jitter_unit: f64,
    ) -> CoreCycleOutcome {
        if self.credential.is_held() {
            return CoreCycleOutcome::Held;
        }
        let Some(token) = self.credential.token().map(str::to_string) else {
            return CoreCycleOutcome::NoCredential;
        };

        // The dead-letter journal is append-only (`OutboundQueue` never
        // prunes it) and `deterministic_id` makes seed reuse the designed
        // retry pattern (its own doc: replaying the same seed after a
        // crash — or, here, after an earlier dead-letter — reuses the
        // identical id on purpose). So a *new* capture that reuses a
        // previously dead-lettered seed shares that old journal entry's id,
        // and matching against the whole journal below would wrongly strip
        // its overlay the moment any later cycle merely touches the queue
        // (`Blocked`/`PullFailed`/`CredentialNeeded`) — exactly the
        // "disappears mid-queue" state this overlay exists to prevent.
        // Snapshotting the length here and only looking at what grew past
        // it keeps the match scoped to entries *this* cycle dead-lettered.
        let dead_letters_before_this_cycle = self.cycle.queue().dead_letters().len();

        let outcome = self
            .cycle
            .run(
                read_transport,
                write_transport,
                &token,
                now_ms,
                trigger,
                force_full_sweep,
                jitter_unit,
            )
            .await;

        // A dead-lettered entry reverts to server truth regardless of what
        // the rest of this cycle did — matched by the queue entry id every
        // `OverlayEntry` carries for exactly this, but only among entries
        // *this* cycle dead-lettered (see above).
        let newly_dead_lettered_ids: BTreeSet<&str> = self
            .cycle
            .queue()
            .dead_letters()
            .iter()
            .skip(dead_letters_before_this_cycle)
            .map(|dead_letter| dead_letter.entry.id.as_str())
            .collect();
        self.overlay
            .retain(|_, overlay| !newly_dead_lettered_ids.contains(overlay.entry_id.as_str()));
        // #118's binding overlay follows the identical lifecycle — a
        // dead-lettered binding write reverts to server truth here, and
        // `Core::dead_letters` carries the affordance, exactly as for an
        // item. Kept in step deliberately: a binding that stayed overlaid
        // after its write was given up on would show a value this device
        // will never send again.
        self.binding_overlay
            .retain(|_, overlay| !newly_dead_lettered_ids.contains(overlay.entry_id.as_str()));

        // A completed cycle is drain-then-pull both having finished
        // (ADR-0007): every overlay entry either just dead-lettered
        // (removed above) or was sent and is now reflected by the pull
        // that followed it in the same cycle — nothing is left to track by
        // clearing the rest here. Any other outcome (blocked, a pull
        // failure, a credential hold) leaves the overlay exactly as it
        // was, which is what keeps a capture from ever disappearing
        // mid-queue.
        if matches!(outcome, CycleOutcome::Completed { .. }) {
            self.overlay.clear();
            self.binding_overlay.clear();
        }

        if matches!(outcome, CycleOutcome::CredentialNeeded { .. }) {
            self.credential.hold();
            self.events.push(CoreEvent::CredentialNeeded { at_ms: now_ms });
        }

        CoreCycleOutcome::Cycle(outcome)
    }
}

/// Performs an HTTP GET and returns the response status code.
///
/// A minimal placeholder proving the core owns HTTP via `reqwest` (ADR-0003):
/// one async HTTP path serves every client, including the browser via
/// `reqwest`'s Fetch-backed `wasm32` target.
#[cfg(feature = "reqwest-transport")]
pub async fn fetch_status(client: &reqwest::Client, url: &str) -> Result<u16, reqwest::Error> {
    let response = client.get(url).send().await?;
    Ok(response.status().as_u16())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use sync::transport::TransportError;
    use sync::write::transport::RawResponse;

    #[test]
    fn api_version_is_stable() {
        let core = Core::new();
        assert_eq!(core.api_version(), API_VERSION);
    }

    #[cfg(feature = "reqwest-transport")]
    #[tokio::test]
    async fn fetch_status_compiles_and_is_callable() {
        // No network call in CI: this only proves the async reqwest-backed
        // HTTP path type-checks against an unroutable address rather than
        // depending on network access being available in the sandbox.
        let client = reqwest::Client::new();
        let result = fetch_status(&client, "http://127.0.0.1:0/").await;
        assert!(result.is_err());
    }

    /// Mechanically enforces the binding-agnostic rule from ADR-0003: this
    /// crate must never gain a `uniffi` or `wasm_bindgen` dependency,
    /// checked against the crate's own manifest text rather than convention.
    #[test]
    fn cargo_toml_has_no_binding_macro_dependencies() {
        let manifest = include_str!("../Cargo.toml");
        for forbidden in ["uniffi", "wasm-bindgen", "wasm_bindgen"] {
            assert!(
                !manifest.contains(forbidden),
                "client/core/Cargo.toml must not depend on `{forbidden}` — \
                 core is binding-agnostic (ADR-0003)",
            );
        }
    }

    // ------------------------------------------------------ Core::init

    #[tokio::test]
    async fn init_creates_a_fresh_core_when_nothing_is_durable_yet() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-1");

        let core = Core::init(namespace.to_str().unwrap(), "api-key-1")
            .await
            .unwrap();

        assert_eq!(core.active_item_count(), 0);
        assert!(core.frontier().is_empty());
    }

    #[tokio::test]
    async fn init_loads_whatever_a_previous_session_left_durable() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-2");
        let ns = namespace.to_str().unwrap();

        let mut first = Core::init(ns, "api-key-1").await.unwrap();
        first
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        // Only the queue is durable at this point (no cycle ever ran) —
        // reloading must still see the captured mutation queued.
        drop(first);

        let second = Core::init(ns, "api-key-2").await.unwrap();
        assert_eq!(
            second.frontier().len(),
            1,
            "a previous session's still-queued capture must survive a reload"
        );
    }

    /// #104 acceptance: "No method returns or persists the API key" — the
    /// mechanical half beyond the `compile_fail` doctest on [`Core`] itself:
    /// the persisted bytes on disk must never contain the key a caller
    /// passed to `init`.
    #[tokio::test]
    async fn the_api_key_never_reaches_the_durable_snapshot_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-3");
        let ns = namespace.to_str().unwrap();
        let secret = "sk-do-not-persist-me";

        let mut core = Core::init(ns, secret).await.unwrap();
        core.capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        for entry in std::fs::read_dir(ns).unwrap() {
            let path = entry.unwrap().path();
            let bytes = std::fs::read_to_string(&path).unwrap();
            assert!(
                !bytes.contains(secret),
                "the API key must never appear in a persisted snapshot file: {path:?}"
            );
        }
    }

    // -------------------------------------------------- capture + overlay

    #[tokio::test]
    async fn a_capture_is_readable_from_the_frontier_immediately_after_enqueue() {
        let mut core = Core::new();

        // `capture` never touches a transport, and none is even wired up —
        // proving this needs no network call at all.
        core.capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        let frontier = core.frontier();
        assert_eq!(frontier.len(), 1);
        assert_eq!(frontier[0].title, "buy milk");
    }

    #[tokio::test]
    async fn an_overlaid_item_is_distinguishable_as_pending() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        assert!(core.is_pending(&id));
        assert!(!core.is_pending("some-other-id"));
    }

    /// #208's headline acceptance: setting Energy, Size and Context and
    /// submitting produces an item whose fields match the selections —
    /// asserted on the optimistic overlay [`Core::frontier`] returns before
    /// any network call, which is what proves this rides the one capture
    /// mutation rather than a follow-up patch.
    #[tokio::test]
    async fn capture_options_reach_the_optimistic_item() {
        let mut core = Core::new();
        core.capture(
            "seed-1",
            "buy milk",
            Stage::Ready,
            1_000,
            CaptureOptions {
                size: Some(Size::Deep),
                energy: Some(Energy::High),
                context: Some("@errands".to_string()),
                description: Some("the oat kind".to_string()),
                priority: Some(3),
                project_id: Some("proj-1".to_string()),
                deadline: Some("2026-09-01".to_string()),
                scheduled_date: Some("2026-08-30".to_string()),
            },
        )
        .await
        .unwrap();

        let frontier = core.frontier();
        assert_eq!(frontier[0].size, Some(Size::Deep));
        assert_eq!(frontier[0].energy, Some(Energy::High));
        assert_eq!(frontier[0].context.as_deref(), Some("@errands"));
        assert_eq!(frontier[0].description.as_deref(), Some("the oat kind"));
        assert_eq!(frontier[0].priority, 3);
        assert_eq!(frontier[0].project_id.as_deref(), Some("proj-1"));
        assert_eq!(frontier[0].deadline.as_deref(), Some("2026-09-01"));
        assert_eq!(frontier[0].scheduled_date.as_deref(), Some("2026-08-30"));
    }

    /// The same selections in the *wire body*, not merely in the overlay: an
    /// optimistic item assembled locally would agree with itself forever
    /// while the queued `POST /api/items` carried none of it.
    #[tokio::test]
    async fn capture_options_reach_the_queued_create_body() {
        let mut core = Core::new();
        core.capture(
            "seed-1",
            "buy milk",
            Stage::Ready,
            1_000,
            CaptureOptions {
                size: Some(Size::Quick),
                energy: None,
                context: None,
                description: Some("the oat kind".to_string()),
                priority: Some(2),
                project_id: Some("proj-1".to_string()),
                deadline: Some("2026-09-01T09:30".to_string()),
                scheduled_date: Some("2026-08-30".to_string()),
            },
        )
        .await
        .unwrap();

        let entries: Vec<&QueueEntry> = core.cycle.queue().entries().collect();
        let body = match &entries[0].intent {
            MutationIntent::Create { body, .. } => body.clone(),
            other => panic!("expected a create, got {other:?}"),
        };
        assert_eq!(body["description"], serde_json::json!("the oat kind"));
        assert_eq!(body["priority"], serde_json::json!(2));
        // Snake case on this wire: `CreateItem` carries no `rename_all`, so
        // the DTO's own field names are the keys.
        assert_eq!(body["project_id"], serde_json::json!("proj-1"));
        assert_eq!(body["deadline"], serde_json::json!("2026-09-01T09:30"));
        assert_eq!(body["scheduled_date"], serde_json::json!("2026-08-30"));
        // An unset field is *omitted*, never sent as null: the server's own
        // default is the resting state, and a null would be this client
        // asserting one.
        assert!(body.get("energy").is_none());
        assert!(body.get("context").is_none());
    }

    /// #208's other half: leaving every field at its resting state
    /// (`CaptureOptions::default()`) still produces a capture with all of
    /// them absent — the "optional, decided at mint time" contract must
    /// survive a caller that never touches the controls.
    #[tokio::test]
    async fn leaving_capture_options_unset_leaves_them_all_absent() {
        let mut core = Core::new();
        core.capture(
            "seed-1",
            "buy milk",
            Stage::Ready,
            1_000,
            CaptureOptions::default(),
        )
        .await
        .unwrap();

        let frontier = core.frontier();
        assert_eq!(frontier[0].size, None);
        assert_eq!(frontier[0].energy, None);
        assert_eq!(frontier[0].context, None);
        assert_eq!(frontier[0].description, None);
        assert_eq!(frontier[0].priority, 0);
        assert_eq!(frontier[0].project_id, None);
        assert_eq!(frontier[0].deadline, None);
        assert_eq!(frontier[0].scheduled_date, None);
    }

    /// Setting only one sends that one and leaves the rest absent — #208's
    /// third acceptance checkbox.
    #[tokio::test]
    async fn setting_only_one_capture_option_leaves_the_others_absent() {
        let mut core = Core::new();
        core.capture(
            "seed-1",
            "buy milk",
            Stage::Ready,
            1_000,
            CaptureOptions {
                size: Some(Size::Quick),
                ..CaptureOptions::default()
            },
        )
        .await
        .unwrap();

        let frontier = core.frontier();
        assert_eq!(frontier[0].size, Some(Size::Quick));
        assert_eq!(frontier[0].energy, None);
        assert_eq!(frontier[0].context, None);
    }

    // ---------------------------------------------------------------- act

    /// This issue's headline acceptance: "Completing offline shows Done
    /// immediately". No transport is even wired up — proving the overlay
    /// needs no network call to appear.
    #[tokio::test]
    async fn completing_offline_shows_done_immediately() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        core.act("seed-act-1", &id, ItemAction::Complete, 2_000)
            .await
            .unwrap();

        assert!(core.is_pending(&id));
        let frontier = core.frontier();
        assert!(
            frontier.is_empty(),
            "a Done item is no longer Ready/InProgress, so it drops off the frontier \
             immediately"
        );
    }

    #[tokio::test]
    async fn starting_an_item_moves_it_to_in_progress_immediately() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        core.act("seed-act-1", &id, ItemAction::Start, 2_000)
            .await
            .unwrap();

        let frontier = core.frontier();
        assert_eq!(frontier.len(), 1);
        assert_eq!(frontier[0].stage, Stage::InProgress);
    }

    #[tokio::test]
    async fn blocking_an_item_sets_the_blocked_stage_never_a_relation() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        core.act("seed-act-1", &id, ItemAction::Block, 2_000)
            .await
            .unwrap();

        let items = core.overlaid_items();
        assert_eq!(items.get(&id).unwrap().stage, Stage::Blocked);
        assert!(
            core.blocked().is_empty(),
            "Core::blocked is the relation-blocked query (blocked_by edges) — an item \
             carrying Stage::Blocked is a different fact and must never show up there"
        );
    }

    #[tokio::test]
    async fn cancelling_an_item_archives_it_rather_than_setting_a_stage() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        core.act("seed-act-1", &id, ItemAction::Cancel, 2_000)
            .await
            .unwrap();

        let items = core.overlaid_items();
        let item = items.get(&id).unwrap();
        assert_eq!(item.stage, Stage::Ready, "cancel never touches stage");
        assert_eq!(item.archived_at, Some(2_000));
        assert!(
            core.frontier().is_empty(),
            "an archived item must drop off the frontier immediately"
        );
    }

    #[tokio::test]
    async fn acting_on_an_unknown_item_id_is_item_not_found() {
        let mut core = Core::new();

        let error = core
            .act("seed-act-1", "no-such-item", ItemAction::Start, 1_000)
            .await
            .unwrap_err();

        assert!(matches!(error, ActError::ItemNotFound));
    }

    // -------------------------------------------------------------- triage

    /// This issue's headline acceptance: "A triaged item leaves the triage
    /// query and appears on the frontier through the mirror" — the overlay
    /// [`Core::triage`] shares with every other mutation here, never a
    /// separate local bookkeeping list.
    #[tokio::test]
    async fn promoting_to_ready_moves_the_item_from_triage_to_the_frontier_immediately() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "someday maybe", Stage::Triage, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        assert_eq!(core.triage_inbox().len(), 1);
        assert_eq!(core.frontier().len(), 0);

        core.triage(
            "seed-triage-1",
            &id,
            true,
            TriagePatch::default(),
            2_000,
        )
        .await
        .unwrap();

        assert_eq!(core.triage_inbox().len(), 0);
        let frontier = core.frontier();
        assert_eq!(frontier.len(), 1);
        assert_eq!(frontier[0].stage, Stage::Ready);
    }

    /// This issue's "a multi-field triage is one mutation, not four" —
    /// title, project, size, energy and context all land in the same
    /// enqueued `QueueEntry`.
    #[tokio::test]
    async fn a_multi_field_triage_is_exactly_one_queued_mutation() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "someday maybe", Stage::Triage, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        assert_eq!(core.queue_depth(), 1, "capture itself is the first entry");

        core.triage(
            "seed-triage-1",
            &id,
            true,
            TriagePatch {
                title: Some("buy milk".to_string()),
                description: Some(Some("oat, not dairy".to_string())),
                project_id: Some(Some("project-1".to_string())),
                size: Some(Some(Size::Quick)),
                energy: Some(Some(Energy::Low)),
                context: Some(Some("@errands".to_string())),
                priority: Some(2),
                deadline: Some(Some("2026-08-14".to_string())),
                scheduled_date: Some(Some("2026-08-12".to_string())),
            },
            2_000,
        )
        .await
        .unwrap();

        assert_eq!(
            core.queue_depth(),
            2,
            "one triage call must enqueue exactly one more entry, whatever fields it sets"
        );
        let frontier = core.frontier();
        assert_eq!(frontier.len(), 1);
        let item = &frontier[0];
        assert_eq!(item.title, "buy milk");
        assert_eq!(item.description.as_deref(), Some("oat, not dairy"));
        assert_eq!(item.project_id.as_deref(), Some("project-1"));
        assert_eq!(item.size, Some(Size::Quick));
        assert_eq!(item.energy, Some(Energy::Low));
        assert_eq!(item.context.as_deref(), Some("@errands"));
        assert_eq!(item.priority, 2);
        assert_eq!(item.deadline.as_deref(), Some("2026-08-14"));
        assert_eq!(item.scheduled_date.as_deref(), Some("2026-08-12"));
    }

    /// A field `TriagePatch` leaves `None` is untouched — triage never
    /// clears a field it was not asked to set.
    #[tokio::test]
    async fn an_unset_triage_patch_field_leaves_the_items_existing_value_untouched() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "someday maybe", Stage::Triage, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        core.triage(
            "seed-triage-1",
            &id,
            true,
            TriagePatch {
                context: Some(Some("@computer".to_string())),
                ..TriagePatch::default()
            },
            2_000,
        )
        .await
        .unwrap();

        core.triage(
            "seed-triage-2",
            &id,
            true,
            TriagePatch::default(),
            3_000,
        )
        .await
        .unwrap();

        let frontier = core.frontier();
        assert_eq!(
            frontier[0].context.as_deref(),
            Some("@computer"),
            "the second triage call set no context field, so the first call's value survives"
        );
    }

    /// The other half of that: a field the patch touches with `Some(None)` is
    /// CLEARED, and the difference between the two is visible in the enqueued
    /// mutation itself — untouched means absent from `patch_fields`, cleared
    /// means present as a JSON `null`. The authority reads absent as "leave
    /// alone" and null as "set to null" (`hummingbird_domain::ItemPatch`), so
    /// collapsing the two here would make an editor unable to remove a value
    /// it can set.
    #[tokio::test]
    async fn a_cleared_triage_field_is_sent_as_an_explicit_null_and_an_untouched_one_is_absent() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "someday maybe", Stage::Triage, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        core.triage(
            "seed-triage-1",
            &id,
            true,
            TriagePatch {
                context: Some(Some("@computer".to_string())),
                deadline: Some(Some("2026-08-14".to_string())),
                ..TriagePatch::default()
            },
            2_000,
        )
        .await
        .unwrap();
        assert_eq!(core.triage_inbox().len(), 0);

        core.triage(
            "seed-triage-2",
            &id,
            true,
            TriagePatch {
                deadline: Some(None),
                ..TriagePatch::default()
            },
            3_000,
        )
        .await
        .unwrap();

        let item = &core.frontier()[0];
        assert_eq!(item.deadline, None, "the cleared field is gone optimistically");
        assert_eq!(
            item.context.as_deref(),
            Some("@computer"),
            "the untouched field is not collateral damage of the clear"
        );

        let entries: Vec<&QueueEntry> = core.cycle.queue().entries().collect();
        let MutationIntent::Patch { patch_fields, .. } = &entries[2].intent else {
            panic!("a triage is a CAS patch, not a create");
        };
        let fields = patch_fields.as_object().expect("patch fields are an object");
        assert_eq!(
            fields.get("deadline"),
            Some(&serde_json::Value::Null),
            "a cleared field must be sent, as null — leaving it out would say `leave it alone`"
        );
        assert!(
            !fields.contains_key("context"),
            "an untouched field must not appear at all"
        );
    }

    #[tokio::test]
    async fn triaging_an_unknown_item_id_is_item_not_found() {
        let mut core = Core::new();

        let error = core
            .triage(
                "seed-triage-1",
                "no-such-item",
                true,
                TriagePatch::default(),
                1_000,
            )
            .await
            .unwrap_err();

        assert!(matches!(error, ActError::ItemNotFound));
    }

    /// This issue's "triaging offline queues correctly and reconciles on
    /// the next cycle" — no transport is even wired up here, proving the
    /// overlay needs no network call, exactly [`Core::act`]'s own
    /// "completing offline" proof.
    #[tokio::test]
    async fn triaging_offline_is_visible_immediately_with_no_transport_wired_up() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "someday maybe", Stage::Triage, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        core.triage(
            "seed-triage-1",
            &id,
            true,
            TriagePatch {
                title: Some("buy milk".to_string()),
                ..TriagePatch::default()
            },
            2_000,
        )
        .await
        .unwrap();

        assert!(core.is_pending(&id));
        assert_eq!(core.frontier()[0].title, "buy milk");
    }

    /// #122: `promote_to_ready: false` is a pure field edit — it must never
    /// touch `stage`, which is what lets the weekend-plans pane's do-date
    /// chip triage an `InProgress` item without demoting it back to `Ready`.
    #[tokio::test]
    async fn promote_to_ready_false_edits_fields_without_touching_stage() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "someday maybe", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        core.act("seed-act-1", &id, ItemAction::Start, 1_500).await.unwrap();
        assert_eq!(core.frontier()[0].stage, Stage::InProgress);

        core.triage(
            "seed-triage-1",
            &id,
            false,
            TriagePatch {
                scheduled_date: Some(Some("2026-08-15".to_string())),
                ..TriagePatch::default()
            },
            2_000,
        )
        .await
        .unwrap();

        let item = &core.frontier()[0];
        assert_eq!(item.stage, Stage::InProgress, "promote_to_ready: false must never change stage");
        assert_eq!(item.scheduled_date.as_deref(), Some("2026-08-15"));
    }

    /// #122's three-state `scheduled_date`: `Some(None)` clears an
    /// already-set do-date, distinguishable from the untouched `None` case
    /// pinned above.
    #[tokio::test]
    async fn clearing_scheduled_date_is_distinct_from_leaving_it_alone() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "someday maybe", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        core.triage(
            "seed-triage-1",
            &id,
            false,
            TriagePatch {
                scheduled_date: Some(Some("2026-08-15".to_string())),
                ..TriagePatch::default()
            },
            1_500,
        )
        .await
        .unwrap();
        assert_eq!(core.frontier()[0].scheduled_date.as_deref(), Some("2026-08-15"));

        // Leaving it alone: an unrelated edit must not disturb the do-date.
        core.triage(
            "seed-triage-2",
            &id,
            false,
            TriagePatch { title: Some("buy milk".to_string()), ..TriagePatch::default() },
            1_600,
        )
        .await
        .unwrap();
        assert_eq!(
            core.frontier()[0].scheduled_date.as_deref(),
            Some("2026-08-15"),
            "an untouched scheduled_date field must survive an unrelated edit"
        );

        // Clearing it: `Some(None)` is a real null, not a no-op.
        core.triage(
            "seed-triage-3",
            &id,
            false,
            TriagePatch { scheduled_date: Some(None), ..TriagePatch::default() },
            1_700,
        )
        .await
        .unwrap();
        assert_eq!(core.frontier()[0].scheduled_date, None);
    }

    /// Reviewer finding on PR #207: a queued act must survive the
    /// `SharedWorker` (and therefore the whole `Core`) terminating before
    /// the next cycle ever runs — routine whenever the last view closes.
    /// Before this fix, [`overlay_from_queue`] rebuilt overlay entries for
    /// still-queued creates only; a reload after an offline `act` silently
    /// dropped the overlay while the `PATCH` sat durably queued, so the
    /// item read back as its pre-mutation stage with `is_pending` false —
    /// exactly the "tells a reader nothing is pending when something still
    /// is" failure this module's own doc forbids.
    #[tokio::test]
    async fn a_queued_act_survives_a_reload_and_still_reads_as_pending() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-act-reload");
        let ns = namespace.to_str().unwrap();

        let mut first = Core::init(ns, "api-key-1").await.unwrap();
        let id = first
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        first
            .act("seed-act-1", &id, ItemAction::Complete, 2_000)
            .await
            .unwrap();
        // Only the queue is durable at this point (no cycle ever ran) — the
        // `SharedWorker`, and this `Core` with it, is dropped exactly as it
        // would be when the last view closes mid-queue.
        drop(first);

        let second = Core::init(ns, "api-key-2").await.unwrap();
        assert!(
            second.is_pending(&id),
            "a reload must not silently lose a still-queued act's pending state"
        );
        assert_eq!(
            second.frontier().len(),
            0,
            "the reloaded overlay must still show the acted-on state (Done), not the \
             pre-mutation Ready that would put it back on the frontier"
        );
    }

    /// S13/#111's "triaging offline queues correctly and reconciles on the
    /// next cycle", the reload half: [`overlay_from_queue`] is generic over
    /// any queued item patch, not `Core::act`'s single-field one, so a
    /// still-queued triage survives a reload exactly like a still-queued act
    /// does (`a_queued_act_survives_a_reload_and_still_reads_as_pending`).
    #[tokio::test]
    async fn a_queued_triage_survives_a_reload_and_still_reads_as_pending() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-triage-reload");
        let ns = namespace.to_str().unwrap();

        let mut first = Core::init(ns, "api-key-1").await.unwrap();
        let id = first
            .capture("seed-1", "someday maybe", Stage::Triage, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        first
            .triage(
                "seed-triage-1",
                &id,
                true,
                TriagePatch {
                    title: Some("buy milk".to_string()),
                    ..TriagePatch::default()
                },
                2_000,
            )
            .await
            .unwrap();
        // Only the queue is durable at this point (no cycle ever ran).
        drop(first);

        let second = Core::init(ns, "api-key-2").await.unwrap();
        assert!(
            second.is_pending(&id),
            "a reload must not silently lose a still-queued triage's pending state"
        );
        assert_eq!(second.triage_inbox().len(), 0);
        let frontier = second.frontier();
        assert_eq!(frontier.len(), 1);
        assert_eq!(frontier[0].title, "buy milk");
    }

    // ------------------------------------------------- fixtures for `run`

    #[derive(Default)]
    struct ScriptedRead {
        sweep: Mutex<Vec<Result<String, TransportError>>>,
    }

    impl ScriptedRead {
        fn sweep_only(results: Vec<Result<String, TransportError>>) -> Self {
            Self {
                sweep: Mutex::new(results),
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl ChangesTransport for ScriptedRead {
        async fn fetch_changes(
            &self,
            _access_token: &str,
            _since: i64,
        ) -> Result<String, TransportError> {
            panic!("this fixture is scripted for a full sweep only")
        }

        async fn fetch_sweep(&self, _access_token: &str) -> Result<String, TransportError> {
            self.sweep
                .lock()
                .unwrap()
                .pop()
                .expect("no more scripted sweep responses")
        }
    }

    #[derive(Default)]
    struct ScriptedWrite {
        responses: Mutex<std::collections::VecDeque<Result<RawResponse, TransportError>>>,
    }

    impl ScriptedWrite {
        fn new(responses: Vec<Result<RawResponse, TransportError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().collect()),
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl MutationTransport for ScriptedWrite {
        async fn send(
            &self,
            _access_token: &str,
            _request: sync::write::transport::MutationRequest,
        ) -> Result<RawResponse, TransportError> {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| panic!("no more scripted write responses"))
        }
    }

    fn ok(status: u16, body: impl Into<String>) -> Result<RawResponse, TransportError> {
        Ok(RawResponse {
            status,
            body: body.into(),
        })
    }

    fn empty_sweep_body(version: i64) -> String {
        serde_json::to_string(&hummingbird_domain::ChangesResponse::empty(version)).unwrap()
    }

    #[tokio::test]
    async fn a_sweep_confirming_the_capture_removes_the_overlay_with_no_gap() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        let confirmed_item = hummingbird_domain::Item {
            id: id.clone(),
            seq: Some(1),
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
            agent: false,
            created_at: 1_000,
            updated_at: 1_000,
            version: 1,
        };
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            items: vec![confirmed_item],
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        let write = ScriptedWrite::new(vec![ok(201, format!(r#"{{"id":"{id}","version":1}}"#))]);

        let outcome = core
            .run(&read, &write, 2_000, Trigger::User, true, 0.0)
            .await;

        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));
        assert!(
            !core.is_pending(&id),
            "a confirming sweep must clear the overlay"
        );
        let frontier = core.frontier();
        assert_eq!(
            frontier.len(),
            1,
            "the item must still be on the frontier, sourced from the mirror now, not the overlay"
        );
        assert_eq!(frontier[0].title, "buy milk");
    }

    #[tokio::test]
    async fn a_dead_lettered_capture_removes_the_overlay_and_reverts_to_server_truth() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        assert!(core.is_pending(&id));

        let read = ScriptedRead::sweep_only(vec![Ok(empty_sweep_body(1))]);
        // A permanent (non-retryable) failure dead-letters the create.
        let write = ScriptedWrite::new(vec![ok(400, r#"{"error":"validation"}"#)]);

        let outcome = core
            .run(&read, &write, 2_000, Trigger::User, true, 0.0)
            .await;

        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));
        assert!(
            !core.is_pending(&id),
            "a dead-lettered capture must clear the overlay"
        );
        assert!(
            core.frontier().is_empty(),
            "with no server-side item and no overlay, the frontier reverts to server truth"
        );
    }

    /// This issue's second acceptance: "A server-side change to the same
    /// field while the mutation is queued produces a dead-letter entry and
    /// the UI reverts with the affordance visible — never silently."
    #[tokio::test]
    async fn a_same_field_conflict_on_a_queued_act_dead_letters_and_reverts_the_overlay_visibly() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        // Cycle 1: the create lands and is confirmed by the sweep, so
        // `act` below is patching a real, mirror-backed item.
        let confirmed_item = hummingbird_domain::Item {
            id: id.clone(),
            seq: Some(1),
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
            agent: false,
            created_at: 1_000,
            updated_at: 1_000,
            version: 1,
        };
        let sweep1 = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            items: vec![confirmed_item.clone()],
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        let read1 = ScriptedRead::sweep_only(vec![Ok(sweep1)]);
        let write1 = ScriptedWrite::new(vec![ok(201, format!(r#"{{"id":"{id}","version":1}}"#))]);
        core.run(&read1, &write1, 2_000, Trigger::User, true, 0.0)
            .await;
        assert!(!core.is_pending(&id));

        // Complete it — queued, overlaid as Done immediately.
        core.act("seed-act-1", &id, ItemAction::Complete, 3_000)
            .await
            .unwrap();
        assert!(core.is_pending(&id));
        assert_eq!(core.frontier().len(), 0, "Done drops off the frontier");

        // Cycle 2: someone else already moved the same item's `stage` to
        // `blocked` server-side — a genuine same-field collision, reported
        // as a conflict on the first attempt (never retried).
        let conflicting_current = hummingbird_domain::Item {
            stage: Stage::Blocked,
            version: 2,
            ..confirmed_item.clone()
        };
        let conflict_body = serde_json::to_string(&serde_json::json!({
            "error": "version_conflict",
            "current": conflicting_current,
        }))
        .unwrap();
        let sweep2 = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 2,
            items: vec![conflicting_current],
            ..hummingbird_domain::ChangesResponse::empty(2)
        })
        .unwrap();
        let read2 = ScriptedRead::sweep_only(vec![Ok(sweep2)]);
        let write2 = ScriptedWrite::new(vec![ok(409, conflict_body)]);

        let outcome2 = core
            .run(&read2, &write2, 4_000, Trigger::User, true, 0.0)
            .await;

        assert!(matches!(
            outcome2,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));
        assert!(
            !core.is_pending(&id),
            "a dead-lettered act must clear the overlay, never leave a stale optimistic view"
        );
        let frontier = core.frontier();
        assert!(
            frontier.is_empty(),
            "the item reverts to server truth (Blocked), not the optimistic Done"
        );
        assert_eq!(
            core.dead_letters().len(),
            1,
            "the dead-letter journal is the never-silent affordance"
        );
    }

    // ------------------------------------------------- S9 sync-status reads

    #[tokio::test]
    async fn queue_depth_reflects_a_queued_capture_and_drops_once_it_is_sent() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        assert_eq!(core.queue_depth(), 0);

        core.capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        assert_eq!(core.queue_depth(), 1);

        let read = ScriptedRead::sweep_only(vec![Ok(empty_sweep_body(1))]);
        let write = ScriptedWrite::new(vec![ok(201, r#"{"id":"seed-1","version":1}"#)]);
        core.run(&read, &write, 2_000, Trigger::User, true, 0.0).await;

        assert_eq!(core.queue_depth(), 0, "a sent create leaves nothing queued");
    }

    #[tokio::test]
    async fn dead_letters_is_empty_on_a_fresh_core_and_carries_a_permanent_failure_after_one() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        assert!(core.dead_letters().is_empty());

        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(empty_sweep_body(1))]);
        let write = ScriptedWrite::new(vec![ok(400, r#"{"error":"validation"}"#)]);
        core.run(&read, &write, 2_000, Trigger::User, true, 0.0).await;

        let dead_letters = core.dead_letters();
        assert_eq!(dead_letters.len(), 1);
        assert_eq!(dead_letters[0].entry.id, id);
    }

    #[tokio::test]
    async fn mirror_snapshot_serializes_a_fresh_core_and_stays_readable_json() {
        let core = Core::new();
        let snapshot = core.mirror_snapshot();
        // A fresh mirror is a JSON object (not, say, a bare string or an
        // opaque encoding) — this is the shape a mirror-download button
        // writes to a file, so it must be independently readable.
        assert!(snapshot.is_object());
    }

    /// S9 round-1 review: `mirror_snapshot` and `dead_letters` are two new
    /// surfaces a whole mirror/journal crosses on its way out to a UI —
    /// the same "grep the bytes" proof
    /// `the_api_key_never_reaches_the_durable_snapshot_bytes` above already
    /// applies to persisted snapshots, extended to these two in-memory reads
    /// (`SyncMirror`/`DeadLetterEntry` carry no credential field by
    /// construction, but a mechanical proof outlives that fact staying
    /// true).
    #[tokio::test]
    async fn the_api_key_never_reaches_the_mirror_snapshot() {
        let mut core = Core::new();
        let secret = "sk-do-not-leak-into-the-mirror";
        core.push_api_key(secret);
        core.capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        let serialized = serde_json::to_string(&core.mirror_snapshot()).unwrap();
        assert!(!serialized.contains(secret));
    }

    #[tokio::test]
    async fn the_api_key_never_reaches_a_dead_lettered_entry() {
        let mut core = Core::new();
        let secret = "sk-do-not-leak-into-the-journal";
        core.push_api_key(secret);
        core.capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        let read = ScriptedRead::sweep_only(vec![Ok(empty_sweep_body(1))]);
        let write = ScriptedWrite::new(vec![ok(400, r#"{"error":"validation"}"#)]);
        core.run(&read, &write, 2_000, Trigger::User, true, 0.0).await;

        assert_eq!(core.dead_letters().len(), 1, "the run above must have dead-lettered exactly one entry");
        let serialized = serde_json::to_string(core.dead_letters()).unwrap();
        assert!(!serialized.contains(secret));
    }

    #[tokio::test]
    async fn a_pull_side_401_holds_every_subsequent_run_until_a_fresh_key_is_pushed() {
        let mut core = Core::new();
        core.push_api_key("stale-token");

        let read = ScriptedRead::sweep_only(vec![Err(TransportError::http(401, "token revoked"))]);
        let write = ScriptedWrite::new(vec![]);

        let outcome = core
            .run(&read, &write, 1_000, Trigger::User, true, 1.0)
            .await;
        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::CredentialNeeded { .. })
        ));

        // The host's cue to prompt for re-auth, drained rather than
        // delivered by callback.
        assert_eq!(
            core.take_events(),
            vec![CoreEvent::CredentialNeeded { at_ms: 1_000 }]
        );
        assert_eq!(core.take_events(), Vec::new(), "draining twice yields nothing new");

        // Every subsequent trigger — even a deliberate user gesture — is
        // held outright: neither scripted transport has any responses left,
        // so a call that actually reached them would panic.
        let held_read = ScriptedRead::default();
        let held_write = ScriptedWrite::default();
        let held = core
            .run(&held_read, &held_write, 2_000, Trigger::User, true, 1.0)
            .await;
        assert_eq!(held, CoreCycleOutcome::Held);

        // A fresh key is the only way out.
        core.push_api_key("fresh-token");
        let resumed_read = ScriptedRead::sweep_only(vec![Ok(empty_sweep_body(1))]);
        let resumed_write = ScriptedWrite::new(vec![]);
        let resumed = core
            .run(&resumed_read, &resumed_write, 3_000, Trigger::User, true, 1.0)
            .await;
        assert!(matches!(
            resumed,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));
    }

    #[tokio::test]
    async fn no_api_key_ever_pushed_reports_no_credential_without_touching_a_transport() {
        let mut core = Core::new();
        let read = ScriptedRead::default();
        let write = ScriptedWrite::default();

        let outcome = core
            .run(&read, &write, 1_000, Trigger::User, true, 0.0)
            .await;

        assert_eq!(outcome, CoreCycleOutcome::NoCredential);
    }

    // ---------------------------------- the overlay survives every non-clearing outcome

    /// Review finding on #168: only `CycleOutcome::Completed` (a full drain
    /// AND a confirming pull) and a fresh dead-letter may ever touch the
    /// overlay. Every other outcome — a blocked drain, a failed pull, a
    /// failed persist, a skipped timer tick, or a held credential — must
    /// leave it byte-for-byte as it was. This is the assertion that would
    /// have caught the dead-letter-scoping bug fixed alongside it: pinning
    /// every non-clearing branch, not just the two clearing ones.
    #[tokio::test]
    async fn a_blocked_drain_leaves_the_overlay_untouched() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        let read = ScriptedRead::default(); // must never be reached
        let write = ScriptedWrite::new(vec![ok(503, "")]); // retryable: blocks

        let outcome = core
            .run(&read, &write, 2_000, Trigger::User, true, 1.0)
            .await;

        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::Blocked { .. })
        ));
        assert!(core.is_pending(&id), "a blocked drain must not clear the overlay");
        assert_eq!(core.frontier().len(), 1);
    }

    #[tokio::test]
    async fn a_failed_pull_leaves_the_overlay_untouched() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        // The drain succeeds (the create is actually sent) but the pull
        // that follows it fails outright — the overlay must still not
        // clear, because nothing has confirmed this device's write yet.
        let read = ScriptedRead::sweep_only(vec![Err(TransportError::new("connection reset"))]);
        let write = ScriptedWrite::new(vec![ok(201, format!(r#"{{"id":"{id}","version":1}}"#))]);

        let outcome = core
            .run(&read, &write, 2_000, Trigger::User, true, 1.0)
            .await;

        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::PullFailed { .. })
        ));
        assert!(core.is_pending(&id), "a failed pull must not clear the overlay");
    }

    #[tokio::test]
    async fn a_failed_persist_leaves_the_overlay_untouched() {
        let mut core = Core {
            cycle: SyncCycle::new(
                storage::InstrumentedSnapshotStore::new(),
                storage::InstrumentedSnapshotStore::new(),
            ),
            credential: AccessTokenSlot::default(),
            overlay: BTreeMap::new(),
            binding_overlay: BTreeMap::new(),
            events: Vec::new(),
            grill_draft_store: storage::InstrumentedSnapshotStore::new(),
            grill_drafts: GrillDrafts::default(),
        };
        core.push_api_key("token-1");
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        // Fail the queue store's writes only after the capture's own
        // enqueue (which must itself succeed) durably landed.
        core.cycle.queue_store().set_failing(true);

        // Drain still attempts the send — the queue's own persist (which
        // is what this test fails) happens right after, per
        // `SyncCycle::run`'s module docs — so the write transport is
        // reached even though the pull never will be.
        let read = ScriptedRead::default(); // must never be reached
        let write = ScriptedWrite::new(vec![ok(201, format!(r#"{{"id":"{id}","version":1}}"#))]);

        let outcome = core
            .run(&read, &write, 2_000, Trigger::User, true, 1.0)
            .await;

        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::PersistFailed { .. })
        ));
        assert!(core.is_pending(&id), "a failed persist must not clear the overlay");
    }

    #[tokio::test]
    async fn a_skipped_timer_tick_leaves_the_overlay_untouched() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        // First, a real attempt that records a backoff failure.
        let blocking_read = ScriptedRead::default();
        let blocking_write = ScriptedWrite::new(vec![ok(503, "")]);
        let blocked = core
            .run(&blocking_read, &blocking_write, 1_000, Trigger::User, true, 1.0)
            .await;
        assert!(matches!(
            blocked,
            CoreCycleOutcome::Cycle(CycleOutcome::Blocked { .. })
        ));

        // A `Trigger::Timer` tick before backoff's delay elapses must skip
        // outright — neither transport reached, since both would panic if
        // called.
        let unreachable_read = ScriptedRead::default();
        let unreachable_write = ScriptedWrite::default();
        let outcome = core
            .run(
                &unreachable_read,
                &unreachable_write,
                1_100,
                Trigger::Timer,
                false,
                1.0,
            )
            .await;

        assert_eq!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::Skipped)
        );
        assert!(core.is_pending(&id), "a skipped tick must not clear the overlay");
    }

    #[tokio::test]
    async fn a_held_credential_leaves_the_overlay_untouched() {
        let mut core = Core::new();
        core.push_api_key("stale-token");
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        // The drain succeeds first (the create is actually sent); the pull
        // that follows it is what hits the 401.
        let read = ScriptedRead::sweep_only(vec![Err(TransportError::http(401, "revoked"))]);
        let write = ScriptedWrite::new(vec![ok(201, format!(r#"{{"id":"{id}","version":1}}"#))]);
        let holding = core
            .run(&read, &write, 1_000, Trigger::User, true, 1.0)
            .await;
        assert!(matches!(
            holding,
            CoreCycleOutcome::Cycle(CycleOutcome::CredentialNeeded { .. })
        ));

        let held_read = ScriptedRead::default();
        let held_write = ScriptedWrite::default();
        let outcome = core
            .run(&held_read, &held_write, 2_000, Trigger::User, true, 1.0)
            .await;

        assert_eq!(outcome, CoreCycleOutcome::Held);
        assert!(core.is_pending(&id), "a held credential must not clear the overlay");
    }

    /// The regression this review round exists to pin: the dead-letter
    /// journal is append-only (`OutboundQueue` never prunes it), and
    /// `deterministic_id` makes seed reuse the designed retry pattern, so a
    /// *new* capture that reuses a previously dead-lettered seed shares
    /// that old journal entry's id. Matching a dead-letter clear against
    /// the *whole* journal (rather than just what this cycle added) would
    /// wrongly strip the new capture's overlay the moment any later cycle
    /// merely blocks or fails to pull — exactly the "disappears mid-queue"
    /// state the overlay exists to prevent.
    #[tokio::test]
    async fn a_recapture_reusing_a_previously_dead_lettered_seed_keeps_its_overlay_while_still_queued(
    ) {
        let mut core = Core::new();
        core.push_api_key("token-1");

        // Cycle 1: this capture is permanently rejected and dead-lettered.
        // The cycle still *completes* (the drain finishes, the pull
        // succeeds), which is the one outcome that legitimately clears the
        // whole overlay — so this alone would not have caught the bug.
        let id = core
            .capture("seed-1", "buy milk v1", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        let read1 = ScriptedRead::sweep_only(vec![Ok(empty_sweep_body(1))]);
        let write1 = ScriptedWrite::new(vec![ok(400, r#"{"error":"validation"}"#)]);
        let outcome1 = core
            .run(&read1, &write1, 1_000, Trigger::User, true, 0.0)
            .await;
        assert!(matches!(
            outcome1,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));
        assert!(!core.is_pending(&id));

        // A fresh capture reuses the same seed on purpose (the designed
        // retry pattern) — same deterministic id, back in the queue and
        // the overlay.
        let id2 = core
            .capture("seed-1", "buy milk v2", Stage::Ready, 2_000, CaptureOptions::default())
            .await
            .unwrap();
        assert_eq!(id2, id, "reusing the seed must mint the identical id");
        assert!(core.is_pending(&id));

        // Cycle 2 merely blocks — it dead-letters nothing new. The bug: a
        // journal-wide match would still find `id` from cycle 1's
        // dead-letter and strip the fresh overlay entry that shares it.
        let read2 = ScriptedRead::default();
        let write2 = ScriptedWrite::new(vec![ok(503, "")]);
        let outcome2 = core
            .run(&read2, &write2, 3_000, Trigger::User, true, 1.0)
            .await;

        assert!(matches!(
            outcome2,
            CoreCycleOutcome::Cycle(CycleOutcome::Blocked { .. })
        ));
        assert!(
            core.is_pending(&id),
            "a fresh capture reusing a previously dead-lettered seed must keep its overlay \
             while it is still genuinely queued"
        );
        assert_eq!(core.frontier().len(), 1);
    }

    // -------------------------------------------------------- push_api_key

    #[tokio::test]
    async fn pushing_a_fresh_key_drops_an_undrained_credential_needed_event() {
        let mut core = Core::new();
        core.push_api_key("stale-token");

        let read = ScriptedRead::sweep_only(vec![Err(TransportError::http(401, "revoked"))]);
        let write = ScriptedWrite::new(vec![]);
        core.run(&read, &write, 1_000, Trigger::User, true, 1.0)
            .await;

        // The event is recorded but deliberately never drained here.
        core.push_api_key("fresh-token");

        assert_eq!(
            core.take_events(),
            Vec::new(),
            "a fresh push must retract a prompt for a hold it just resolved"
        );
    }

    // --------------------------------------------------- rehydrate_api_key
    //
    // Issue #196: under one shared core (#126), every view that reaches
    // `ready` reloads its stored token and re-supplies it. These pin that a
    // SECOND (or later) view's rehydration can never do what only a real
    // `push_api_key` may — resume a hold or retract its prompt — while a
    // first-run device still gets its stored token into the credential slot.

    #[tokio::test]
    async fn rehydrating_a_held_credential_does_not_resume_it_or_retract_its_prompt() {
        let mut core = Core::new();
        core.push_api_key("stale-token");

        let read = ScriptedRead::sweep_only(vec![Err(TransportError::http(401, "revoked"))]);
        let write = ScriptedWrite::new(vec![]);
        core.run(&read, &write, 1_000, Trigger::User, true, 1.0)
            .await;

        // A second view connecting reloads and re-supplies the very token
        // that was just rejected — this must not resume anything.
        core.rehydrate_api_key("stale-token");

        let held_read = ScriptedRead::default();
        let held_write = ScriptedWrite::default();
        let held = core
            .run(&held_read, &held_write, 2_000, Trigger::User, true, 1.0)
            .await;
        assert_eq!(held, CoreCycleOutcome::Held);

        // Nor does it retract the pending prompt — Settings must still see
        // it on whichever view drains next.
        assert_eq!(
            core.take_events(),
            vec![CoreEvent::CredentialNeeded { at_ms: 1_000 }],
            "a rehydration must never retract a hold prompt it did not resolve"
        );
    }

    #[tokio::test]
    async fn rehydrating_repeatedly_while_held_still_never_resumes_only_a_real_push_does() {
        let mut core = Core::new();
        core.push_api_key("stale-token");

        let read = ScriptedRead::sweep_only(vec![Err(TransportError::http(401, "revoked"))]);
        let write = ScriptedWrite::new(vec![]);
        core.run(&read, &write, 1_000, Trigger::User, true, 1.0)
            .await;

        // Three more views connecting in turn, all reloading the same
        // stored (rejected) token.
        core.rehydrate_api_key("stale-token");
        core.rehydrate_api_key("stale-token");
        core.rehydrate_api_key("stale-token");

        let held_read = ScriptedRead::default();
        let held_write = ScriptedWrite::default();
        let held = core
            .run(&held_read, &held_write, 2_000, Trigger::User, true, 1.0)
            .await;
        assert_eq!(held, CoreCycleOutcome::Held);

        // Only a genuine push resumes.
        core.push_api_key("fresh-token");
        let resumed_read = ScriptedRead::sweep_only(vec![Ok(empty_sweep_body(1))]);
        let resumed_write = ScriptedWrite::new(vec![]);
        let resumed = core
            .run(&resumed_read, &resumed_write, 3_000, Trigger::User, true, 1.0)
            .await;
        assert!(matches!(
            resumed,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));
    }

    #[tokio::test]
    async fn a_first_run_devices_rehydration_still_reaches_the_credential_slot() {
        // No `push_api_key` call at all here — this is the never-held,
        // never-pushed case a first-run device's core-start rehydration
        // hits, and it must still make the token usable.
        let mut core = Core::new();
        core.rehydrate_api_key("device-token");

        let read = ScriptedRead::sweep_only(vec![Ok(empty_sweep_body(1))]);
        let write = ScriptedWrite::new(vec![]);
        let outcome = core.run(&read, &write, 1_000, Trigger::User, true, 1.0).await;

        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));
    }

    // ------------------------------------------------------- clear_api_key

    #[tokio::test]
    async fn clearing_a_never_pushed_key_stays_no_credential() {
        let mut core = Core::new();
        core.clear_api_key();

        let read = ScriptedRead::sweep_only(vec![]);
        let write = ScriptedWrite::new(vec![]);
        let outcome = core.run(&read, &write, 1_000, Trigger::User, true, 1.0).await;

        assert_eq!(outcome, CoreCycleOutcome::NoCredential);
    }

    #[tokio::test]
    async fn clearing_a_working_key_reports_no_credential_not_held() {
        let mut core = Core::new();
        core.push_api_key("device-token");
        core.clear_api_key();

        let read = ScriptedRead::sweep_only(vec![]);
        let write = ScriptedWrite::new(vec![]);
        let outcome = core.run(&read, &write, 1_000, Trigger::User, true, 1.0).await;

        // Distinct from `CoreCycleOutcome::Held`: nothing was rejected here,
        // the host simply forgot a key that was working fine — the "no
        // credential" steady state is the honest one, not "waiting for a
        // retry of a bad token".
        assert_eq!(outcome, CoreCycleOutcome::NoCredential);
    }

    #[tokio::test]
    async fn clearing_after_a_401_hold_also_reports_no_credential_not_held() {
        let mut core = Core::new();
        core.push_api_key("stale-token");
        let read = ScriptedRead::sweep_only(vec![Err(TransportError::http(401, "revoked"))]);
        let write = ScriptedWrite::new(vec![]);
        // The first run actually attempts the cycle and hits the 401,
        // which is what sets `held` — see
        // `a_pull_side_401_holds_every_subsequent_run_until_a_fresh_key_is_pushed`.
        core.run(&read, &write, 1_000, Trigger::User, true, 1.0).await;
        let held_read = ScriptedRead::default();
        let held_write = ScriptedWrite::default();
        let held = core
            .run(&held_read, &held_write, 2_000, Trigger::User, true, 1.0)
            .await;
        assert_eq!(held, CoreCycleOutcome::Held);

        core.clear_api_key();

        let outcome = core
            .run(&held_read, &held_write, 3_000, Trigger::User, true, 1.0)
            .await;
        assert_eq!(outcome, CoreCycleOutcome::NoCredential);
    }

    #[tokio::test]
    async fn clearing_the_key_drops_an_undrained_credential_needed_event() {
        let mut core = Core::new();
        core.push_api_key("stale-token");
        let read = ScriptedRead::sweep_only(vec![Err(TransportError::http(401, "revoked"))]);
        let write = ScriptedWrite::new(vec![]);
        core.run(&read, &write, 1_000, Trigger::User, true, 1.0)
            .await;

        // The event is recorded but deliberately never drained here.
        core.clear_api_key();

        assert_eq!(
            core.take_events(),
            Vec::new(),
            "forgetting the key is this host's own action, not something a stale re-prompt \
             should re-litigate"
        );
    }

    #[tokio::test]
    async fn clearing_never_touches_the_overlay() {
        let mut core = Core::new();
        core.push_api_key("device-token");
        let id = core.capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default()).await.unwrap();

        core.clear_api_key();

        assert!(
            core.is_pending(&id),
            "clearing the credential must not touch a queued capture's overlay"
        );
        assert_eq!(core.frontier().len(), 1);
    }

    // -------------------------------------------------------------- triage_inbox

    #[tokio::test]
    async fn a_default_stage_triage_capture_is_readable_from_the_triage_inbox_not_the_frontier() {
        let mut core = Core::new();
        core.capture("seed-1", "someday maybe", Stage::Triage, 1_000, CaptureOptions::default())
            .await
            .unwrap();

        assert!(
            core.frontier().is_empty(),
            "a Triage-stage capture must not appear on the frontier"
        );
        let inbox = core.triage_inbox();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].title, "someday maybe");
    }

    /// #110 acceptance: "The raw string reaches the mutation unmodified."
    /// Deliberately pathological — leading/trailing padding, internal
    /// whitespace, mixed case — none of which a "helpful" parser would
    /// leave alone, so this fails loudly if `capture::parse_seam`'s output
    /// is ever wired in instead of being discarded.
    #[tokio::test]
    async fn the_raw_capture_string_reaches_the_mutation_unmodified() {
        let raw = "  Buy   OAT milk\tand   eggs  ";
        let mut core = Core::new();

        core.capture("seed-1", raw, Stage::Triage, 1_000, CaptureOptions::default()).await.unwrap();

        let inbox = core.triage_inbox();
        assert_eq!(inbox.len(), 1);
        assert_eq!(
            inbox[0].title, raw,
            "the title an item overlays with must be byte-for-byte what was typed"
        );
    }

    /// #110 acceptance: "Offline, three captures then reconnecting produces
    /// three Triage items in order, with no duplicates." Three captures are
    /// enqueued with no credential ever reaching the transport (offline);
    /// reconnecting is one `run` whose write transport accepts all three
    /// creates and whose read transport's sweep echoes them back as
    /// confirmed server truth. `capture`'s deterministic id (from each
    /// distinct seed) is what rules out a duplicate: the same three seeds
    /// enqueued would collapse to fewer than three ids, and this test would
    /// fail on the `len()` assertion below.
    #[tokio::test]
    async fn three_offline_captures_then_reconnecting_produce_three_distinct_triage_items(
    ) {
        let mut core = Core::new();
        // No `push_api_key` call yet — every capture below is enqueued
        // durably (`SyncCycle::enqueue`) with no transport ever touched,
        // which is offline capture end to end.
        let id1 = core.capture("seed-a", "buy milk", Stage::Triage, 1_000, CaptureOptions::default()).await.unwrap();
        let id2 = core.capture("seed-b", "call dentist", Stage::Triage, 2_000, CaptureOptions::default()).await.unwrap();
        let id3 = core.capture("seed-c", "water plants", Stage::Triage, 3_000, CaptureOptions::default()).await.unwrap();
        assert_eq!(core.queue_depth(), 3, "all three must be durably queued before any network call");
        assert_eq!(core.triage_inbox().len(), 3, "a capture is visible in the list before any network call");

        // Reconnecting: a credential arrives and one cycle drains the queue,
        // then pulls a sweep reflecting all three now-confirmed items.
        core.push_api_key("device-token");
        let sweep = hummingbird_domain::ChangesResponse {
            version: 1,
            items: vec![
                fixture_item(&id1, Stage::Triage),
                fixture_item(&id2, Stage::Triage),
                fixture_item(&id3, Stage::Triage),
            ],
            ..hummingbird_domain::ChangesResponse::empty(1)
        };
        let read = ScriptedRead::sweep_only(vec![Ok(serde_json::to_string(&sweep).unwrap())]);
        let write = ScriptedWrite::new(vec![
            ok(201, format!(r#"{{"id":"{id1}","version":1}}"#)),
            ok(201, format!(r#"{{"id":"{id2}","version":1}}"#)),
            ok(201, format!(r#"{{"id":"{id3}","version":1}}"#)),
        ]);
        let outcome = core.run(&read, &write, 10_000, Trigger::User, true, 0.0).await;

        assert!(
            matches!(outcome, CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })),
            "expected a completed reconnect cycle, got {outcome:?}"
        );
        assert_eq!(core.queue_depth(), 0, "every queued capture must have drained");

        let inbox = core.triage_inbox();
        let mut ids: Vec<&str> = inbox.iter().map(|item| item.id.as_str()).collect();
        ids.sort();
        let mut expected = vec![id1.as_str(), id2.as_str(), id3.as_str()];
        expected.sort();
        assert_eq!(
            ids, expected,
            "reconnecting must produce exactly the three captured items, no duplicates and none lost"
        );
        assert_eq!(
            inbox.len(),
            3,
            "no duplicates: three offline captures must never collapse into or expand past three items"
        );
    }

    // ------------------------------------------------ grilling_items (#357)

    #[tokio::test]
    async fn a_grilling_stage_item_is_readable_from_grilling_items_not_triage_inbox() {
        let mut core = Core::new();
        core.push_api_key("device-token");
        let sweep = hummingbird_domain::ChangesResponse {
            version: 1,
            items: vec![fixture_item("item-1", Stage::Grilling)],
            ..hummingbird_domain::ChangesResponse::empty(1)
        };
        let read = ScriptedRead::sweep_only(vec![Ok(serde_json::to_string(&sweep).unwrap())]);
        let write = ScriptedWrite::new(vec![]);
        core.run(&read, &write, 10_000, Trigger::User, true, 0.0).await;

        assert!(
            core.triage_inbox().is_empty(),
            "a Grilling-stage item must not appear in the triage inbox"
        );
        let grilling = core.grilling_items();
        assert_eq!(grilling.len(), 1);
        assert_eq!(grilling[0].stage, Stage::Grilling);
    }

    // ------------------------------------------------- blocked() (S10, issue #108)

    fn fixture_item(id: &str, stage: Stage) -> hummingbird_domain::Item {
        hummingbird_domain::Item {
            id: id.to_string(),
            seq: Some(1),
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
            created_at: 1,
            updated_at: 1,
            version: 1,
        }
    }

    fn fixture_blocked_by(item_id: &str, blocker_id: &str) -> hummingbird_domain::BlockedBy {
        hummingbird_domain::BlockedBy {
            item_id: item_id.to_string(),
            blocker_id: blocker_id.to_string(),
            version: 1,
            removed_at: None,
        }
    }

    /// Runs one full-sweep cycle seeding `items`/`blocked_by`, against a
    /// fresh `Core` — the same `ScriptedRead`/`ScriptedWrite` fixture shape
    /// the capture-confirmation tests above use, reused here so `blocked()`
    /// and `steps_for` are exercised over a real mirror rather than a
    /// hand-built one this file has no other legal way to construct
    /// (`SyncCycle` exposes no `mirror_mut`).
    async fn seeded_core(
        items: Vec<hummingbird_domain::Item>,
        blocked_by: Vec<hummingbird_domain::BlockedBy>,
    ) -> Core {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            items,
            blocked_by,
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        let write = ScriptedWrite::new(vec![]);
        let outcome = core
            .run(&read, &write, 1_000, Trigger::User, true, 0.0)
            .await;
        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));
        core
    }

    #[tokio::test]
    async fn an_open_blocker_keeps_an_item_off_the_frontier_and_explains_why_in_blocked() {
        let core = seeded_core(
            vec![fixture_item("a-1", Stage::Ready), fixture_item("a-2", Stage::Ready)],
            vec![fixture_blocked_by("a-2", "a-1")],
        )
        .await;

        assert_eq!(
            core.frontier().iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
            vec!["a-1"]
        );

        let blocked = core.blocked();
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].0.id, "a-2");
        assert_eq!(
            blocked[0].1.iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
            vec!["a-1"]
        );
    }

    /// Acceptance criterion: "Blocked-by direction is proven by a fixture:
    /// a closed blocker does not block."
    #[tokio::test]
    async fn a_closed_blocker_does_not_block() {
        let core = seeded_core(
            vec![fixture_item("a-1", Stage::Done), fixture_item("a-2", Stage::Ready)],
            vec![fixture_blocked_by("a-2", "a-1")],
        )
        .await;

        assert_eq!(
            core.frontier().iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
            vec!["a-2"],
            "a-2's only blocker is Done, so a-2 must be on the frontier"
        );
        assert!(
            core.blocked().is_empty(),
            "no item is explained as blocked once its only blocker is Done"
        );
    }

    #[tokio::test]
    async fn an_externally_blocked_stage_item_never_appears_in_blocked_either() {
        // Stage::Blocked means an external wait (CONTEXT.md) — a different
        // fact from a relation blocker, and `blocked()` must not conflate
        // the two: it only considers Ready/InProgress, same as `frontier()`.
        let core = seeded_core(vec![fixture_item("a-1", Stage::Blocked)], vec![]).await;

        assert!(core.frontier().is_empty());
        assert!(core.blocked().is_empty());
    }

    /// Acceptance criterion: "Absent items never appear." An archived
    /// (soft-deleted) blocker must stop blocking, exactly like a Done one —
    /// and an archived item must never itself appear on the frontier or in
    /// the blocked explanation.
    #[tokio::test]
    async fn an_absent_item_never_appears_in_the_frontier_or_blocked() {
        let mut core = seeded_core(
            vec![fixture_item("a-1", Stage::Ready), fixture_item("a-2", Stage::Ready)],
            vec![fixture_blocked_by("a-2", "a-1")],
        )
        .await;
        assert_eq!(core.blocked().len(), 1, "a-2 starts out blocked by a-1");

        // A full sweep that omits a-1 demotes it to absent.
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 2,
            items: vec![fixture_item("a-2", Stage::Ready)],
            blocked_by: vec![fixture_blocked_by("a-2", "a-1")],
            ..hummingbird_domain::ChangesResponse::empty(2)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        let write = ScriptedWrite::new(vec![]);
        core.run(&read, &write, 3_000, Trigger::User, true, 0.0).await;

        assert!(
            core.frontier().iter().all(|item| item.id != "a-1"),
            "an absent item must never appear in the frontier"
        );
        assert_eq!(
            core.frontier().iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
            vec!["a-2"],
            "a-1 going absent must clear it as a-2's blocker, same as it going Done"
        );
        assert!(
            core.blocked().is_empty(),
            "an absent blocker no longer explains anything as blocked"
        );
    }

    // ------------------------------------------------- ledger + done

    /// The Ledger's membership rule: *everything* — live, Done, archived —
    /// where every other read filters. An archived row is shown labelled
    /// (its retention stamp), never hidden; "complete" is the point.
    #[tokio::test]
    async fn the_ledger_shows_live_done_and_archived_items_each_labelled() {
        let mut archived = fixture_item("a-3", Stage::Ready);
        archived.archived_at = Some(500);
        let core = seeded_core(
            vec![
                fixture_item("a-1", Stage::Ready),
                fixture_item("a-2", Stage::Done),
                archived,
            ],
            vec![],
        )
        .await;

        let ledger = core.ledger(2_000);
        assert_eq!(
            ledger.iter().map(|row| row.item.id.clone()).collect::<Vec<_>>(),
            vec!["a-1", "a-2", "a-3"],
            "every item the mirror has ever known is a ledger row"
        );
        assert_eq!(ledger[0].absent_since_ms, None);
        assert_eq!(ledger[1].absent_since_ms, None);
        assert_eq!(
            ledger[2].absent_since_ms,
            Some(500),
            "an archived row carries its own archived_at as the retention stamp"
        );
    }

    /// Done's membership is the same live-presence rule as every other
    /// screen: an item completed and later cancelled drops off Done and
    /// stays visible only in the ledger.
    #[tokio::test]
    async fn done_lists_live_done_items_and_excludes_a_done_then_archived_one() {
        let mut done_then_archived = fixture_item("a-2", Stage::Done);
        done_then_archived.archived_at = Some(900);
        let core = seeded_core(
            vec![
                fixture_item("a-1", Stage::Done),
                done_then_archived,
                fixture_item("a-3", Stage::Ready),
            ],
            vec![],
        )
        .await;

        assert_eq!(
            core.done().iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
            vec!["a-1"],
            "only live Done items belong on the Done screen"
        );
        assert!(
            core.ledger(2_000).iter().any(|row| row.item.id == "a-2"),
            "the done-then-archived item is still a ledger row"
        );
    }

    // ------------------------------------------------- search() (#478, Recall)

    /// [`Core::search`] assembles the same corpus [`Core::ledger`] reads —
    /// live, `Done` and archived alike — and labels each row's
    /// [`search::Group`] the way this module's doc promises: an archived
    /// row groups `Archived` even when its own `stage` is `Done`.
    #[tokio::test]
    async fn search_groups_the_ledgers_own_corpus_live_done_and_archived() {
        let mut done_then_archived = fixture_item("a-2", Stage::Done);
        done_then_archived.title = "widget project".to_string();
        done_then_archived.archived_at = Some(900);
        let mut live_done = fixture_item("a-3", Stage::Done);
        live_done.title = "widget report".to_string();
        let mut live = fixture_item("a-1", Stage::Ready);
        live.title = "widget spec".to_string();

        let core = seeded_core(vec![live, live_done, done_then_archived], vec![]).await;

        let outcome = core.search("widget", 2_000);
        let groups: std::collections::HashMap<String, search::Group> = outcome
            .rows
            .iter()
            .map(|row| (row.item.id.clone(), row.group))
            .collect();
        assert_eq!(groups.get("a-1"), Some(&search::Group::Live));
        assert_eq!(groups.get("a-3"), Some(&search::Group::Done));
        assert_eq!(
            groups.get("a-2"),
            Some(&search::Group::Archived),
            "a Done item that was later archived groups Archived, not Done"
        );
    }

    /// A pending, not-yet-synced cancel sets `archived_at` optimistically
    /// before any sweep updates the mirror's own presence flag —
    /// [`Core::search`] must group it `Archived` from that flag alone, the
    /// same "the item's own flag wins" rule `ledger-order.ts`'s
    /// `ledgerRowState` documents on the web side.
    #[tokio::test]
    async fn search_groups_a_pending_cancel_as_archived_before_any_sync() {
        let mut core = seeded_core(vec![fixture_item("a-1", Stage::Ready)], vec![]).await;
        core.act("seed-1", "a-1", ItemAction::Cancel, 1_500)
            .await
            .unwrap();

        let outcome = core.search("item a-1", 2_000);
        assert_eq!(outcome.rows.len(), 1);
        assert_eq!(outcome.rows[0].group, search::Group::Archived);
    }

    /// Decision 11: a project's name is searchable through its items.
    #[tokio::test]
    async fn search_matches_a_query_token_against_the_items_own_project_name() {
        fn fixture_project(id: &str, name: &str) -> hummingbird_domain::Project {
            hummingbird_domain::Project {
                id: id.to_string(),
                name: name.to_string(),
                archived_at: None,
                created_at: 1,
                updated_at: 1,
                version: 1,
            }
        }

        let mut in_project = fixture_item("a-1", Stage::Ready);
        in_project.title = "buy stamps".to_string();
        in_project.project_id = Some("p-1".to_string());
        let mut not_in_project = fixture_item("a-2", Stage::Ready);
        not_in_project.title = "buy stamps".to_string();

        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            items: vec![in_project, not_in_project],
            projects: vec![fixture_project("p-1", "House move")],
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        let write = ScriptedWrite::new(vec![]);
        core.run(&read, &write, 1_000, Trigger::User, true, 0.0).await;

        let outcome = core.search("move", 2_000);
        assert_eq!(
            outcome.rows.iter().map(|row| row.item.id.clone()).collect::<Vec<_>>(),
            vec!["a-1"]
        );
    }

    /// A handle-shaped query finds one item by `seq`, ranging over the whole
    /// retained roster like every other Recall query.
    #[tokio::test]
    async fn search_finds_an_archived_item_by_its_handle() {
        let mut archived = fixture_item("a-1", Stage::Ready);
        archived.seq = Some(42);
        archived.title = "unrelated title".to_string();
        archived.archived_at = Some(500);
        let core = seeded_core(vec![archived], vec![]).await;

        let outcome = core.search("hb-42", 2_000);
        assert_eq!(
            outcome.rows.iter().map(|row| row.item.id.clone()).collect::<Vec<_>>(),
            vec!["a-1"]
        );
        assert_eq!(outcome.rows[0].group, search::Group::Archived);
    }

    /// The ledger is overlaid like every other item read: a mutation taken
    /// offline shows immediately, and a complete shows on `done()` too.
    #[tokio::test]
    async fn a_pending_capture_and_an_offline_complete_show_in_ledger_and_done() {
        let mut core = seeded_core(vec![fixture_item("a-1", Stage::InProgress)], vec![]).await;
        core.capture("seed-1", "buy milk", Stage::Triage, 1_500, CaptureOptions::default())
            .await
            .unwrap();
        core.act("seed-2", "a-1", ItemAction::Complete, 1_600)
            .await
            .unwrap();

        let ledger = core.ledger(2_000);
        assert_eq!(ledger.len(), 2, "the capture is a ledger row before any cycle ran");
        assert!(ledger.iter().any(|row| row.item.title == "buy milk"));
        assert_eq!(
            core.done().iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
            vec!["a-1"],
            "an offline complete is on Done immediately"
        );
    }

    /// The dead-letter badge: a permanently-rejected edit marks its item's
    /// row — device-local by nature, and derived from the journal entry's
    /// own intent, so it needs no extra bookkeeping anywhere.
    #[tokio::test]
    async fn a_dead_lettered_edit_badges_its_items_ledger_row() {
        let mut core = seeded_core(vec![fixture_item("a-1", Stage::Ready)], vec![]).await;
        core.act("seed-1", "a-1", ItemAction::Start, 1_500)
            .await
            .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(empty_sweep_body(2))]);
        let write = ScriptedWrite::new(vec![ok(400, r#"{"error":"validation"}"#)]);
        core.run(&read, &write, 2_000, Trigger::User, true, 0.0).await;
        assert_eq!(core.dead_letters().len(), 1);

        let ledger = core.ledger(3_000);
        let row = ledger.iter().find(|row| row.item.id == "a-1").unwrap();
        assert!(row.dead_lettered, "the rejected edit's item carries the badge");
    }

    /// The alert badge joins on ADR-0014's `item:<id>` convention across
    /// every source, and only while the alert is live *now* — a resolved
    /// alert badges nothing.
    #[tokio::test]
    async fn a_live_alert_naming_an_item_badges_its_row_and_a_resolved_one_does_not() {
        let mut live_alert = fixture_alert("al-1", "item-threshold/v1", None, 1_000);
        live_alert.source_key = "item:a-1".to_string();
        let mut resolved_alert = fixture_alert("al-2", "item-threshold/v1", None, 1_000);
        resolved_alert.source_key = "item:a-2".to_string();
        resolved_alert.resolved_at = Some(1_500);

        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            items: vec![
                fixture_item("a-1", Stage::Ready),
                fixture_item("a-2", Stage::Ready),
            ],
            alerts: vec![live_alert, resolved_alert],
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        core.run(&read, &ScriptedWrite::new(vec![]), 1_000, Trigger::User, true, 0.0)
            .await;

        let ledger = core.ledger(2_000);
        let by_id = |id: &str| ledger.iter().find(|row| row.item.id == id).unwrap();
        assert!(by_id("a-1").has_live_alert);
        assert!(!by_id("a-2").has_live_alert);
    }

    // ------------------------------------------------- steps_for (S10, #96)

    #[tokio::test]
    async fn steps_for_an_item_come_back_in_position_order() {
        fn fixture_step(id: &str, item_id: &str, position: i64) -> hummingbird_domain::Step {
            hummingbird_domain::Step {
                id: id.to_string(),
                item_id: item_id.to_string(),
                body: format!("step {id}"),
                done: false,
                position,
                deleted_at: None,
                version: 1,
            }
        }

        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            items: vec![fixture_item("a-1", Stage::Ready)],
            steps: vec![
                fixture_step("s-2", "a-1", 2),
                fixture_step("s-1", "a-1", 1),
                fixture_step("other", "a-2", 1),
            ],
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        let write = ScriptedWrite::new(vec![]);
        core.run(&read, &write, 1_000, Trigger::User, true, 0.0).await;

        let steps = core.steps_for("a-1");
        assert_eq!(
            steps.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
            vec!["s-1", "s-2"],
            "steps come back in position order, not insertion order"
        );
        assert_eq!(
            core.steps_for("a-2")
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>(),
            vec!["other"]
        );
        assert!(core.steps_for("nonexistent").is_empty());
    }

    // ------------------------------------------- complete_grill (#354, ADR-0023)

    fn fixture_step(id: &str, item_id: &str, body: &str, done: bool) -> hummingbird_domain::Step {
        hummingbird_domain::Step {
            id: id.to_string(),
            item_id: item_id.to_string(),
            body: body.to_string(),
            done,
            position: 1,
            deleted_at: None,
            version: 1,
        }
    }

    fn sample_completion(delete_unticked_plan: bool) -> GrillCompletion {
        GrillCompletion {
            transcript: "turn 1\nturn 2".to_string(),
            summary: "clarified the destination".to_string(),
            verdict: GrillVerdict::Resolved,
            model_proposal: "{}".to_string(),
            applied_patch: "{}".to_string(),
            delete_unticked_plan,
        }
    }

    /// The heart of #354: one call enqueues the atomic completion and
    /// overlays the item's new stage immediately, offline or not — the
    /// same "readable the instant it is made" contract [`Core::act`] and
    /// [`Core::triage`] already hold.
    #[tokio::test]
    async fn completing_a_grill_overlays_the_items_new_stage_immediately() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            items: vec![fixture_item("a-1", Stage::Triage)],
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        core.run(&ScriptedRead::sweep_only(vec![Ok(sweep_body)]), &ScriptedWrite::new(vec![]), 1_000, Trigger::User, true, 0.0).await;

        let grill_id = core
            .complete_grill("seed-1", "a-1", &[], sample_completion(false), 2_000)
            .await
            .unwrap();
        assert!(!grill_id.is_empty());

        let frontier = core.frontier();
        assert_eq!(frontier.len(), 1, "triage + resolved moves straight onto the frontier");
        assert_eq!(frontier[0].stage, Stage::Ready);
        assert_eq!(core.queue_depth(), 1);
    }

    #[tokio::test]
    async fn completing_a_grill_on_an_unknown_item_is_item_not_found() {
        let mut core = Core::new();
        let error = core
            .complete_grill("seed-1", "no-such-item", &[], sample_completion(false), 1_000)
            .await
            .unwrap_err();
        assert!(matches!(error, CompleteGrillError::ItemNotFound));
    }

    /// ADR-0023: Done is out of scope for the whole Grill plan — checked
    /// locally with the identical function the authority uses, before ever
    /// touching the queue.
    #[tokio::test]
    async fn completing_a_grill_on_a_done_item_is_refused_locally() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            items: vec![fixture_item("a-1", Stage::Done)],
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        core.run(&ScriptedRead::sweep_only(vec![Ok(sweep_body)]), &ScriptedWrite::new(vec![]), 1_000, Trigger::User, true, 0.0).await;

        let error = core
            .complete_grill("seed-1", "a-1", &[], sample_completion(false), 2_000)
            .await
            .unwrap_err();
        assert!(matches!(error, CompleteGrillError::ItemDone));
        assert_eq!(core.queue_depth(), 0, "nothing was enqueued");
    }

    /// A brand-new unticked Step that appeared since the review session
    /// captured its snapshot forces re-review rather than silently
    /// committing against a Plan the human never saw.
    #[tokio::test]
    async fn a_new_unticked_step_since_review_forces_re_review() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            items: vec![fixture_item("a-1", Stage::Triage)],
            steps: vec![fixture_step("s-1", "a-1", "call the vet", false)],
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        core.run(&ScriptedRead::sweep_only(vec![Ok(sweep_body)]), &ScriptedWrite::new(vec![]), 1_000, Trigger::User, true, 0.0).await;

        // The session opened before "s-1" existed.
        let error = core
            .complete_grill("seed-1", "a-1", &[], sample_completion(false), 2_000)
            .await
            .unwrap_err();
        assert!(matches!(error, CompleteGrillError::NeedsReReview));
        assert_eq!(core.queue_depth(), 0, "a forced re-review enqueues nothing");
    }

    /// An unticked Step whose text changed since the session's snapshot
    /// also forces re-review.
    #[tokio::test]
    async fn a_changed_unticked_step_since_review_forces_re_review() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            items: vec![fixture_item("a-1", Stage::Triage)],
            steps: vec![fixture_step("s-1", "a-1", "call the vet", false)],
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        core.run(&ScriptedRead::sweep_only(vec![Ok(sweep_body)]), &ScriptedWrite::new(vec![]), 1_000, Trigger::User, true, 0.0).await;

        let session = vec![fixture_step("s-1", "a-1", "call the groomer", false)];
        let error = core
            .complete_grill("seed-1", "a-1", &session, sample_completion(false), 2_000)
            .await
            .unwrap_err();
        assert!(matches!(error, CompleteGrillError::NeedsReReview));
    }

    /// The anti-goal, asserted directly: a Step ticked or deleted *during*
    /// the interview — after the session's own snapshot was captured —
    /// never forces re-review. It simply survives, exactly as the
    /// authority's own live re-evaluation (never a stale snapshot) does.
    #[tokio::test]
    async fn a_step_ticked_or_deleted_mid_interview_never_forces_re_review() {
        for (live_done, live_deleted_at) in [(true, None), (false, Some(1_500))] {
            let mut core = Core::new();
            core.push_api_key("token-1");
            let mut live_step = fixture_step("s-1", "a-1", "call the vet", false);
            live_step.done = live_done;
            live_step.deleted_at = live_deleted_at;
            let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
                version: 1,
                items: vec![fixture_item("a-1", Stage::Triage)],
                steps: vec![live_step],
                ..hummingbird_domain::ChangesResponse::empty(1)
            })
            .unwrap();
            core.run(&ScriptedRead::sweep_only(vec![Ok(sweep_body)]), &ScriptedWrite::new(vec![]), 1_000, Trigger::User, true, 0.0).await;

            // The session's own snapshot still shows it live and unticked —
            // it was ticked/deleted by the human *after* the session opened.
            let session = vec![fixture_step("s-1", "a-1", "call the vet", false)];
            core.complete_grill("seed-1", "a-1", &session, sample_completion(false), 2_000)
                .await
                .expect("ticked/deleted-since-review must never force re-review");
        }
    }

    /// The pure decision function, tested directly for the three cases the
    /// issue names by name: new, changed, and ticked/deleted-since.
    #[test]
    fn unticked_steps_changed_covers_the_three_named_cases() {
        let base = fixture_step("s-1", "a-1", "call the vet", false);
        let base_slice = std::slice::from_ref(&base);

        // Identical: no drift.
        assert!(!unticked_steps_changed(base_slice, base_slice));

        // New unticked step live that the session never saw.
        assert!(unticked_steps_changed(&[], base_slice));

        // Changed body.
        let mut changed = base.clone();
        changed.body = "call the groomer".to_string();
        assert!(unticked_steps_changed(base_slice, std::slice::from_ref(&changed)));

        // Ticked since the session's snapshot: survives, no drift.
        let mut ticked = base.clone();
        ticked.done = true;
        assert!(!unticked_steps_changed(base_slice, std::slice::from_ref(&ticked)));

        // Deleted since the session's snapshot: survives, no drift.
        let mut deleted = base.clone();
        deleted.deleted_at = Some(9_999);
        assert!(!unticked_steps_changed(base_slice, std::slice::from_ref(&deleted)));
    }

    /// `delete_unticked_plan: true` reaches the wire body untouched — the
    /// explicit **Replace** gesture (`CONTEXT.md`), never inferred.
    #[tokio::test]
    async fn delete_unticked_plan_reaches_the_queued_mutation_body() {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            items: vec![fixture_item("a-1", Stage::Triage)],
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        core.run(&ScriptedRead::sweep_only(vec![Ok(sweep_body)]), &ScriptedWrite::new(vec![]), 1_000, Trigger::User, true, 0.0).await;

        core.complete_grill("seed-1", "a-1", &[], sample_completion(true), 2_000)
            .await
            .unwrap();

        let entries: Vec<&QueueEntry> = core.cycle.queue().entries().collect();
        let MutationIntent::CompleteGrill { grill_fields, item_base, .. } = &entries.last().unwrap().intent
        else {
            panic!("a grill completion is a CompleteGrill intent");
        };
        assert_eq!(grill_fields["delete_unticked_plan"], serde_json::json!(true));
        assert_eq!(grill_fields["item_id"], serde_json::json!("a-1"));
        assert!(
            grill_fields.get("expected_version").is_none(),
            "expected_version is drain's to fill in per attempt, not enqueued here"
        );
        assert_eq!(item_base["version"], serde_json::json!(1));
    }

    // ------------------------------------------------- grill drafts (#356, ADR-0023)

    fn sample_turns() -> serde_json::Value {
        serde_json::json!([
            {"question": {"prompt": "what's blocking it?", "recommendedAnswer": "nothing", "choices": []}, "answer": "waiting on a callback"},
        ])
    }

    #[test]
    fn a_fresh_core_has_no_grill_draft_for_any_item() {
        let core = Core::new();
        assert!(!core.has_grill_draft("a-1"));
        assert_eq!(core.grill_draft("a-1"), None);
        assert!(core.grill_draft_item_ids().is_empty());
    }

    #[tokio::test]
    async fn saving_a_draft_makes_it_readable_and_lists_the_item() {
        let mut core = Core::new();
        core.save_grill_draft("a-1", sample_turns(), 1_000).await.unwrap();

        assert!(core.has_grill_draft("a-1"));
        assert_eq!(core.grill_draft("a-1"), Some(&sample_turns()));
        assert_eq!(core.grill_draft_item_ids(), vec!["a-1".to_string()]);
    }

    #[tokio::test]
    async fn saving_a_second_time_replaces_rather_than_appends() {
        let mut core = Core::new();
        core.save_grill_draft("a-1", sample_turns(), 1_000).await.unwrap();
        core.save_grill_draft("a-1", serde_json::json!([]), 2_000).await.unwrap();

        assert_eq!(core.grill_draft("a-1"), Some(&serde_json::json!([])));
    }

    #[tokio::test]
    async fn discarding_a_draft_removes_it_and_is_a_no_op_when_absent() {
        let mut core = Core::new();
        core.save_grill_draft("a-1", sample_turns(), 1_000).await.unwrap();

        core.discard_grill_draft("a-1", 2_000).await.unwrap();
        assert!(!core.has_grill_draft("a-1"));
        assert_eq!(core.grill_draft("a-1"), None);

        // Discarding an item with no draft is a no-op, not an error.
        core.discard_grill_draft("no-such-item", 3_000).await.unwrap();
    }

    #[tokio::test]
    async fn two_items_hold_two_independent_drafts() {
        let mut core = Core::new();
        core.save_grill_draft("a-1", serde_json::json!(["one"]), 1_000).await.unwrap();
        core.save_grill_draft("a-2", serde_json::json!(["two"]), 1_000).await.unwrap();

        assert_eq!(core.grill_draft("a-1"), Some(&serde_json::json!(["one"])));
        assert_eq!(core.grill_draft("a-2"), Some(&serde_json::json!(["two"])));

        core.discard_grill_draft("a-1", 2_000).await.unwrap();
        assert!(!core.has_grill_draft("a-1"));
        assert!(core.has_grill_draft("a-2"), "discarding one item's draft must not touch another's");
    }

    /// #356's whole point: a draft survives the `SharedWorker` (and this
    /// `Core` with it) terminating and restarting — the same "survives a
    /// reload" contract `a_queued_act_survives_a_reload_and_still_reads_as_pending`
    /// already proves for the queue.
    #[tokio::test]
    async fn a_saved_draft_survives_a_core_reload() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-grill-draft-reload");
        let ns = namespace.to_str().unwrap();

        let mut first = Core::init(ns, "api-key-1").await.unwrap();
        first.save_grill_draft("a-1", sample_turns(), 1_000).await.unwrap();
        drop(first);

        let second = Core::init(ns, "api-key-2").await.unwrap();
        assert!(second.has_grill_draft("a-1"), "reopening must resume the same draft");
        assert_eq!(second.grill_draft("a-1"), Some(&sample_turns()));
    }

    /// A discard is itself durable — a reload after discarding must not
    /// resurrect the draft it removed.
    #[tokio::test]
    async fn a_discarded_draft_stays_gone_after_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-grill-draft-discard-reload");
        let ns = namespace.to_str().unwrap();

        let mut first = Core::init(ns, "api-key-1").await.unwrap();
        first.save_grill_draft("a-1", sample_turns(), 1_000).await.unwrap();
        first.discard_grill_draft("a-1", 2_000).await.unwrap();
        drop(first);

        let second = Core::init(ns, "api-key-2").await.unwrap();
        assert!(!second.has_grill_draft("a-1"));
    }

    /// A draft is device-local: it must never ride along on
    /// [`Core::mirror_snapshot`], the debug export a sweep/delta payload's
    /// own shape is drawn from — #356's "the draft never appears in any
    /// sweep or delta payload" acceptance.
    #[tokio::test]
    async fn a_grill_draft_never_appears_in_the_mirror_snapshot() {
        let mut core = Core::new();
        core.save_grill_draft("a-1", sample_turns(), 1_000).await.unwrap();

        let snapshot = serde_json::to_string(&core.mirror_snapshot()).unwrap();
        assert!(
            !snapshot.contains("waiting on a callback"),
            "a draft's content must never reach the mirror snapshot: {snapshot}"
        );
    }

    // ------------------------------------------------- projects() (S10, #108 review)

    #[tokio::test]
    async fn projects_resolves_names_in_id_order_and_excludes_absent_ones() {
        fn fixture_project(id: &str, name: &str) -> hummingbird_domain::Project {
            hummingbird_domain::Project {
                id: id.to_string(),
                name: name.to_string(),
                archived_at: None,
                created_at: 1,
                updated_at: 1,
                version: 1,
            }
        }

        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            projects: vec![fixture_project("p-2", "Second"), fixture_project("p-1", "First")],
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        let write = ScriptedWrite::new(vec![]);
        core.run(&read, &write, 1_000, Trigger::User, true, 0.0).await;

        let projects = core.projects();
        assert_eq!(
            projects.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            vec!["First", "Second"],
            "projects come back in id order, not sweep order"
        );

        // A sweep that omits p-1 demotes it to absent.
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 2,
            projects: vec![fixture_project("p-2", "Second")],
            ..hummingbird_domain::ChangesResponse::empty(2)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        let write = ScriptedWrite::new(vec![]);
        core.run(&read, &write, 2_000, Trigger::User, true, 0.0).await;

        assert_eq!(
            core.projects().iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
            vec!["p-2"],
            "an absent project must never appear"
        );
    }

    #[tokio::test]
    async fn a_fresh_core_reports_no_projects() {
        let core = Core::new();
        assert!(core.projects().is_empty());
    }

    // ------------------------------------------- snapshot_freshness() (ADR-0015)

    #[tokio::test]
    async fn snapshot_freshness_reads_the_row_and_demotion_returns_it_to_unknown() {
        use freshness::Freshness;

        fn gauge(fetched_at: i64, version: i64) -> hummingbird_domain::ContextSnapshot {
            hummingbird_domain::ContextSnapshot {
                source: "city-waste/v2".to_string(),
                key: "next_collection".to_string(),
                payload: r#"{"schema":"city-waste/v2","polled_every_ms":86400000,"body":{}}"#
                    .to_string(),
                fetched_at,
                version,
            }
        }

        let mut core = Core::new();
        core.push_api_key("token-1");

        // Nothing synced yet: unknown, and unknown is stale against every
        // threshold — never a zero age standing in for "no answer".
        let before = core.snapshot_freshness("city-waste/v2", "next_collection", 100_000);
        assert_eq!(before, Freshness::Unknown);
        assert!(before.is_stale_beyond(i64::MAX));

        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            context_snapshots: vec![gauge(60_000, 1)],
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        core.run(&read, &ScriptedWrite::new(vec![]), 1_000, Trigger::User, true, 0.0)
            .await;

        assert_eq!(
            core.snapshot_freshness("city-waste/v2", "next_collection", 100_000),
            Freshness::Age { age_ms: 40_000, declared_cadence_ms: Some(86_400_000) },
        );
        // Identity is `(source, key)` — a different key is a different
        // question, not the same answer.
        assert_eq!(
            core.snapshot_freshness("city-waste/v2", "some_other_metric", 100_000),
            Freshness::Unknown,
        );

        // A sweep that omits the row demotes it (ADR-0003). A demoted gauge
        // must read as unknown, not as the last answer it happened to hold.
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse::empty(2))
            .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        core.run(&read, &ScriptedWrite::new(vec![]), 2_000, Trigger::User, true, 0.0)
            .await;

        assert_eq!(
            core.snapshot_freshness("city-waste/v2", "next_collection", 100_000),
            Freshness::Unknown,
        );
    }

    // ------------------------------------------- pane_read() (#245, ADR-0015)

    fn fixture_snapshot(
        source: &str,
        key: &str,
        payload: &str,
        fetched_at: i64,
    ) -> hummingbird_domain::ContextSnapshot {
        hummingbird_domain::ContextSnapshot {
            source: source.to_string(),
            key: key.to_string(),
            payload: payload.to_string(),
            fetched_at,
            version: 1,
        }
    }

    fn fixture_alert(
        id: &str,
        source: &str,
        subject_key: Option<&str>,
        raised_at: i64,
    ) -> hummingbird_domain::Alert {
        hummingbird_domain::Alert {
            id: id.to_string(),
            source: source.to_string(),
            source_key: format!("occurrence:{id}"),
            subject_key: subject_key.map(|key| key.to_string()),
            title: format!("alert {id}"),
            body: None,
            url: None,
            severity: None,
            raised_at,
            resolved_at: None,
            dismissed_at: None,
            expires_at: None,
            version: 1,
        }
    }

    /// Runs one full-sweep cycle seeding the two context lanes — the same
    /// shape [`core_with_settings`] uses, and for the same reason:
    /// `SyncCycle` exposes no `mirror_mut`, so a server-written row must
    /// reach the mirror the ordinary way or not at all.
    async fn core_with_context(
        snapshots: Vec<hummingbird_domain::ContextSnapshot>,
        alerts: Vec<hummingbird_domain::Alert>,
    ) -> Core {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            context_snapshots: snapshots,
            alerts,
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        let outcome = core
            .run(&read, &ScriptedWrite::new(vec![]), 1_000, Trigger::User, true, 0.0)
            .await;
        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));
        core
    }

    const WASTE_PAYLOAD: &str =
        r#"{"schema":"city-waste/v2","polled_every_ms":86400000,"body":{"zone":"Europe/London"}}"#;

    #[tokio::test]
    async fn a_fresh_core_answers_the_pane_read_empty_rather_than_failing() {
        // An answer, not an error: a device that has never synced has
        // nothing to say about this question, and saying so is the answer.
        let read = Core::new().pane_read("city-waste/v2", 1_000);
        assert!(read.snapshots.is_empty());
        assert!(read.alerts.is_empty());
    }

    #[tokio::test]
    async fn the_read_carries_only_the_named_sources_rows_and_alerts() {
        let core = core_with_context(
            vec![
                fixture_snapshot("city-waste/v2", "collection", WASTE_PAYLOAD, 500),
                fixture_snapshot("race/v1", "next", WASTE_PAYLOAD, 500),
            ],
            vec![
                fixture_alert("a-1", "city-waste/v2", Some("collection"), 500),
                fixture_alert("a-2", "race/v1", None, 500),
            ],
        )
        .await;

        let read = core.pane_read("city-waste/v2", 1_000);
        assert_eq!(
            read.snapshots.iter().map(|s| s.key.clone()).collect::<Vec<_>>(),
            vec!["collection"]
        );
        assert_eq!(
            read.alerts.iter().map(|a| a.id.clone()).collect::<Vec<_>>(),
            vec!["a-1"]
        );
        // And a source nothing was ever written for is empty, not an error.
        assert!(core.pane_read("no-such-source/v1", 1_000).snapshots.is_empty());
    }

    #[tokio::test]
    async fn only_alerts_live_at_the_callers_clock_ride_the_read() {
        // The first client-side end-to-end test of ADR-0014's predicate:
        // liveness is decided here, against the injected clock, so no pane
        // can re-derive it in TS and drift.
        let mut resolved = fixture_alert("a-resolved", "city-waste/v2", None, 500);
        resolved.resolved_at = Some(600);
        let mut dismissed = fixture_alert("a-dismissed", "city-waste/v2", None, 500);
        dismissed.dismissed_at = Some(600);
        let mut expiring = fixture_alert("a-expiring", "city-waste/v2", None, 500);
        expiring.expires_at = Some(2_000);

        let core = core_with_context(
            vec![],
            vec![
                resolved,
                dismissed,
                expiring,
                fixture_alert("a-live", "city-waste/v2", None, 500),
            ],
        )
        .await;

        assert_eq!(
            core.pane_read("city-waste/v2", 3_000)
                .alerts
                .iter()
                .map(|a| a.id.clone())
                .collect::<Vec<_>>(),
            vec!["a-live"],
            "resolved, dismissed and expired alerts are all settled"
        );
        // The same alert, the same mirror, an earlier clock: still live.
        assert_eq!(
            core.pane_read("city-waste/v2", 1_500)
                .alerts
                .iter()
                .map(|a| a.id.clone())
                .collect::<Vec<_>>(),
            vec!["a-expiring", "a-live"],
        );
    }

    #[tokio::test]
    async fn a_demoted_snapshot_never_appears_in_the_pane_read() {
        let mut core = core_with_context(
            vec![fixture_snapshot("city-waste/v2", "collection", WASTE_PAYLOAD, 500)],
            vec![fixture_alert("a-1", "city-waste/v2", None, 500)],
        )
        .await;
        assert_eq!(core.pane_read("city-waste/v2", 1_000).snapshots.len(), 1);

        // A complete sweep that omits both rows demotes them (ADR-0003).
        let sweep_body =
            serde_json::to_string(&hummingbird_domain::ChangesResponse::empty(2)).unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        core.run(&read, &ScriptedWrite::new(vec![]), 2_000, Trigger::User, true, 0.0)
            .await;

        let read = core.pane_read("city-waste/v2", 2_000);
        assert!(read.snapshots.is_empty(), "a demoted gauge is not a current answer");
        assert!(read.alerts.is_empty());
    }

    #[tokio::test]
    async fn a_broken_envelope_is_malformed_with_a_reason_and_still_carries_its_age() {
        let core = core_with_context(
            vec![fixture_snapshot("city-waste/v2", "collection", "not json at all", 1_000)],
            vec![],
        )
        .await;

        let read = core.pane_read("city-waste/v2", 61_000);
        let row = &read.snapshots[0];
        // Visibly broken, never quietly empty.
        match &row.envelope {
            pane::PaneEnvelope::Malformed { reason } => {
                assert!(reason.contains("not JSON"), "{reason}")
            }
            other => panic!("expected malformed, got {other:?}"),
        }
        // The row was really fetched: the break costs the cadence, not the
        // age, and never collapses to `Unknown` (which never renders fresh).
        assert_eq!(
            row.freshness,
            freshness::Freshness::Age { age_ms: 60_000, declared_cadence_ms: None }
        );
    }

    #[tokio::test]
    async fn an_unrecognised_schema_rides_through_untouched() {
        // ADR-0015: never a registry check. A source this build has not
        // heard of is a fact about the build, and the pane says so itself.
        let core = core_with_context(
            vec![fixture_snapshot(
                "city-waste/v2",
                "collection",
                r#"{"schema":"city-waste/v9","body":{}}"#,
                1_000,
            )],
            vec![],
        )
        .await;

        match &core.pane_read("city-waste/v2", 1_000).snapshots[0].envelope {
            pane::PaneEnvelope::Parsed { schema, polled_every_ms, body } => {
                assert_eq!(schema, "city-waste/v9");
                assert_eq!(*polled_every_ms, None);
                assert_eq!(body, "{}");
            }
            other => panic!("expected parsed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn per_row_freshness_agrees_with_the_snapshot_freshness_point_lookup() {
        // Two parses of the same payload must not drift: whatever
        // `Freshness::of_snapshot` says for a row, the embedded value says
        // too — including for the malformed one.
        let core = core_with_context(
            vec![
                fixture_snapshot("city-waste/v2", "collection", WASTE_PAYLOAD, 500),
                fixture_snapshot("city-waste/v2", "broken", "[]", 700),
            ],
            vec![],
        )
        .await;

        for row in core.pane_read("city-waste/v2", 90_000).snapshots {
            assert_eq!(
                row.freshness,
                core.snapshot_freshness("city-waste/v2", &row.key, 90_000),
                "{}",
                row.key
            );
        }
    }

    #[tokio::test]
    async fn an_alert_naming_no_subject_still_rides_the_read() {
        // The pane join is additive: `subject_key: None` is a legitimate
        // alert, carried untouched for the pane (or `AlertsScreen`) to read.
        let core = core_with_context(
            vec![],
            vec![
                fixture_alert("a-none", "city-waste/v2", None, 500),
                fixture_alert("a-subject", "city-waste/v2", Some("collection"), 500),
            ],
        )
        .await;

        let read = core.pane_read("city-waste/v2", 1_000);
        assert_eq!(
            read.alerts.iter().map(|a| a.subject_key.clone()).collect::<Vec<_>>(),
            vec![None, Some("collection".to_string())],
        );
    }

    // ------------------------------------------- #118: settings bindings

    fn fixture_setting(key: &str, value: &str, version: i64) -> hummingbird_domain::Setting {
        hummingbird_domain::Setting {
            key: key.to_string(),
            value: value.to_string(),
            updated_at: 1,
            version,
        }
    }

    /// Runs one full-sweep cycle seeding `settings` — the same shape
    /// [`seeded_core`] uses for items, since `SyncCycle` exposes no
    /// `mirror_mut` and a binding pulled from the authority must reach the
    /// mirror the ordinary way or not at all.
    async fn core_with_settings(settings: Vec<hummingbird_domain::Setting>) -> Core {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            settings,
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        let outcome = core
            .run(&read, &ScriptedWrite::new(vec![]), 1_000, Trigger::User, true, 0.0)
            .await;
        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));
        core
    }

    fn binding<'a>(bindings: &'a [Binding], key: &str) -> &'a Binding {
        bindings
            .iter()
            .find(|binding| binding.key == key)
            .unwrap_or_else(|| panic!("no binding for {key}"))
    }

    /// #118 acceptance: "a binding edited on one client is visible on a
    /// second client after its next pull" — the receiving half. The row
    /// arrives through the ordinary pull, with no binding-specific sync path
    /// anywhere: this device never wrote it.
    #[tokio::test]
    async fn a_binding_written_elsewhere_arrives_through_the_ordinary_pull() {
        let core = core_with_settings(vec![fixture_setting("race-series", "\"f1\"", 3)]).await;

        let bindings = core.bindings();
        let race = binding(&bindings, "race-series");
        assert_eq!(race.value, BindingValue::Text { text: "f1".to_string() });
        assert!(race.known);
        assert!(!race.pending, "nothing local is queued for it");
    }

    /// Every known key is listed whether or not it is set — an editor that
    /// only showed rows that already exist could never set the first one.
    #[tokio::test]
    async fn every_known_binding_is_listed_in_vocabulary_order_set_or_not() {
        let core = core_with_settings(vec![fixture_setting("trips-calendar", "\"cal-1\"", 1)]).await;

        let bindings = core.bindings();
        assert_eq!(
            bindings.iter().map(|b| b.key.as_str()).collect::<Vec<_>>(),
            vec!["race-series", "trips-calendar", "city-waste-page"],
        );
        assert_eq!(binding(&bindings, "race-series").value, BindingValue::Unset);
        assert_eq!(binding(&bindings, "city-waste-page").value, BindingValue::Unset);
    }

    /// A key this build has never heard of is still in the table, so it is
    /// still shown — flagged as unwritable rather than hidden, the same
    /// reading ADR-0015 gives an unrecognised snapshot `schema`.
    #[tokio::test]
    async fn a_settings_row_this_build_does_not_know_is_listed_but_not_writable() {
        let core = core_with_settings(vec![
            fixture_setting("some-future-binding", "\"whatever\"", 1),
            fixture_setting("a-non-string-one", "7", 1),
        ])
        .await;

        let bindings = core.bindings();
        let future = binding(&bindings, "some-future-binding");
        assert!(!future.known, "this build cannot write a key it cannot name");
        assert_eq!(
            future.value,
            BindingValue::Text { text: "whatever".to_string() },
            "unknown to this build is not unreadable"
        );
        assert_eq!(
            binding(&bindings, "a-non-string-one").value,
            BindingValue::Other { raw: "7".to_string() },
            "a non-text value is shown as what it is, never as unset"
        );
        // The unknown keys sort after every known one, so the editor's own
        // rows stay put.
        assert_eq!(
            bindings.iter().map(|b| b.key.as_str()).collect::<Vec<_>>(),
            vec![
                "race-series",
                "trips-calendar",
                "city-waste-page",
                "a-non-string-one",
                "some-future-binding",
            ],
        );
    }

    /// #118 acceptance: "writing a binding is an absolute-value CAS set" —
    /// and the sending half of "visible on a second client after its next
    /// pull": what this device enqueues is one ordinary CAS `PUT`, at the
    /// version it last knew, with no bespoke path of any kind.
    #[tokio::test]
    async fn setting_a_binding_enqueues_one_absolute_value_cas_put_at_the_known_version() {
        let mut core = core_with_settings(vec![fixture_setting("race-series", "\"f1\"", 3)]).await;

        core.set_binding("seed-1", BindingKey::RaceSeries, "motogp", 2_000)
            .await
            .unwrap();

        let entries: Vec<&QueueEntry> = core.cycle.queue().entries().collect();
        assert_eq!(entries.len(), 1, "one binding write is one mutation");
        let MutationIntent::Patch {
            path,
            method,
            base,
            patch_fields,
            rebase_fields,
            ..
        } = &entries[0].intent
        else {
            panic!("a binding write is a CAS patch, not a create");
        };
        assert_eq!(path, "/api/settings/race-series");
        assert_eq!(*method, HttpMethod::Put);
        assert_eq!(
            base.get("version").and_then(serde_json::Value::as_i64),
            Some(3),
            "the CAS expects the version this device last knew"
        );
        assert_eq!(
            patch_fields,
            &serde_json::json!({"value": "motogp"}),
            "the wire carries typed JSON — PutSetting::value"
        );
        assert_eq!(
            rebase_fields,
            &Some(serde_json::json!({"value": "\"motogp\""})),
            "the 409 diff and the overlay work in the column's own encoding"
        );
    }

    /// A key with no row yet is the same write, at `expected_version` 0 —
    /// which is exactly how `PUT /api/settings/:key` carries create
    /// semantics (the authority's `handlers/settings.rs`), so there is no
    /// separate create path here either.
    #[tokio::test]
    async fn setting_a_binding_that_has_no_row_yet_is_the_same_write_at_version_zero() {
        let mut core = Core::new();

        core.set_binding("seed-1", BindingKey::CityWastePage, "https://city/waste", 2_000)
            .await
            .unwrap();

        let entries: Vec<&QueueEntry> = core.cycle.queue().entries().collect();
        let MutationIntent::Patch { base, .. } = &entries[0].intent else {
            panic!("expected a patch");
        };
        assert_eq!(base.get("version").and_then(serde_json::Value::as_i64), Some(0));
        assert_eq!(
            binding(&core.bindings(), "city-waste-page").value,
            BindingValue::Text { text: "https://city/waste".to_string() },
        );
    }

    /// The overlay half: a binding set with no network at all reads back
    /// immediately, and says so.
    #[tokio::test]
    async fn a_binding_set_offline_reads_back_immediately_and_reads_as_pending() {
        let mut core = core_with_settings(vec![fixture_setting("race-series", "\"f1\"", 3)]).await;

        core.set_binding("seed-1", BindingKey::RaceSeries, "motogp", 2_000)
            .await
            .unwrap();

        let bindings = core.bindings();
        let race = binding(&bindings, "race-series");
        assert_eq!(race.value, BindingValue::Text { text: "motogp".to_string() });
        assert!(race.pending, "an unconfirmed write must say so");
    }

    /// The reload half, exactly [`overlay_from_queue`]'s own guarantee for
    /// items: a binding set offline and then reloaded before ever syncing is
    /// still readable, rather than reverting on screen to the last pulled
    /// value while the write is still durably queued.
    #[tokio::test]
    async fn a_queued_binding_write_survives_a_reload_and_still_reads_as_pending() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-binding-reload");
        let ns = namespace.to_str().unwrap();

        let mut first = Core::init(ns, "api-key-1").await.unwrap();
        first
            .set_binding("seed-1", BindingKey::TripsCalendar, "cal-trips", 1_000)
            .await
            .unwrap();
        drop(first);

        let second = Core::init(ns, "api-key-2").await.unwrap();
        let bindings = second.bindings();
        let trips = binding(&bindings, "trips-calendar");
        assert_eq!(trips.value, BindingValue::Text { text: "cal-trips".to_string() });
        assert!(trips.pending);
    }

    /// A binding write that is permanently rejected reverts to server truth,
    /// matched by its own queue entry id — the identical lifecycle
    /// `Core::run` already gives a dead-lettered item mutation. A binding
    /// left overlaid after its write was given up on would show a value this
    /// device is never going to send again.
    #[tokio::test]
    async fn a_dead_lettered_binding_write_reverts_the_overlay_to_server_truth() {
        let mut core = core_with_settings(vec![fixture_setting("race-series", "\"f1\"", 3)]).await;
        core.set_binding("seed-1", BindingKey::RaceSeries, "motogp", 2_000)
            .await
            .unwrap();

        // A 400 is permanent: no retry fixes it, so the entry dead-letters.
        let write = ScriptedWrite::new(vec![ok(400, r#"{"error":"bad_request"}"#)]);
        // The pull that follows the drain still carries the server's own
        // value — the write never landed, so nothing about it changed.
        let unchanged = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 2,
            settings: vec![fixture_setting("race-series", "\"f1\"", 3)],
            ..hummingbird_domain::ChangesResponse::empty(2)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(unchanged)]);
        core.run(&read, &write, 3_000, Trigger::User, true, 0.0).await;

        let bindings = core.bindings();
        let race = binding(&bindings, "race-series");
        assert_eq!(
            race.value,
            BindingValue::Text { text: "f1".to_string() },
            "the overlay must revert to what the server actually holds"
        );
        assert!(!race.pending);
        assert_eq!(core.dead_letters().len(), 1, "and the affordance carries it");
    }

    /// A completed cycle supersedes the overlay with server truth, the same
    /// as for items: drain-then-pull means this device's own write already
    /// landed by the time that pull asked.
    #[tokio::test]
    async fn a_completed_cycle_clears_the_binding_overlay_in_favour_of_the_pull() {
        let mut core = core_with_settings(vec![fixture_setting("race-series", "\"f1\"", 3)]).await;
        core.set_binding("seed-1", BindingKey::RaceSeries, "motogp", 2_000)
            .await
            .unwrap();

        let confirmed = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 2,
            settings: vec![fixture_setting("race-series", "\"motogp\"", 4)],
            ..hummingbird_domain::ChangesResponse::empty(2)
        })
        .unwrap();
        let write = ScriptedWrite::new(vec![ok(
            200,
            r#"{"key":"race-series","value":"\"motogp\"","updated_at":3000,"version":4}"#,
        )]);
        let read = ScriptedRead::sweep_only(vec![Ok(confirmed)]);
        let outcome = core.run(&read, &write, 3_000, Trigger::User, true, 0.0).await;
        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));

        let bindings = core.bindings();
        let race = binding(&bindings, "race-series");
        assert_eq!(race.value, BindingValue::Text { text: "motogp".to_string() });
        assert!(!race.pending, "confirmed by the pull — nothing is queued now");
        assert_eq!(core.queue_depth(), 0);
    }

    // -- #140: rules -------------------------------------------------------

    fn fixture_rule(id: &str, enabled: bool, version: i64) -> Rule {
        Rule {
            id: id.to_string(),
            name: format!("rule {id}"),
            event_kind: Some("email".to_string()),
            conditions: vec![Condition {
                field: "subject".to_string(),
                op: "contains".to_string(),
                value: serde_json::json!("urgent"),
                negate: false,
            }],
            severity: "high".to_string(),
            tier: Tier::Urgent,
            enabled,
            updated_at: 1,
            version,
        }
    }

    /// Runs one full-sweep cycle seeding `rules` — [`core_with_settings`]'s
    /// own shape, since a rule pulled from the authority must reach the
    /// mirror the ordinary way or not at all.
    async fn core_with_rules(rules: Vec<Rule>) -> Core {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            rules,
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        let outcome = core
            .run(&read, &ScriptedWrite::new(vec![]), 1_000, Trigger::User, true, 0.0)
            .await;
        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));
        core
    }

    /// #140 acceptance: the rules screen reads whatever the mirror pulled,
    /// no bespoke path.
    #[tokio::test]
    async fn rules_reads_every_synced_rule() {
        let core = core_with_rules(vec![fixture_rule("r-1", true, 1), fixture_rule("r-2", false, 1)]).await;
        let rules = core.rules();
        assert_eq!(rules.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["r-1", "r-2"]);
    }

    /// #140 acceptance: creating a rule enqueues one `POST /api/rules`
    /// create, idempotent by this device's own minted id — the same
    /// contract [`Core::capture`] follows for items.
    #[tokio::test]
    async fn create_rule_enqueues_one_post_create() {
        let mut core = Core::new();
        core.push_api_key("token-1");

        let id = core
            .create_rule(
                "seed-1",
                "trash slide",
                None,
                vec![Condition {
                    field: "source".to_string(),
                    op: "eq".to_string(),
                    value: serde_json::json!("city-waste/v2"),
                    negate: false,
                }],
                "high",
                Tier::Urgent,
                true,
                2_000,
            )
            .await
            .unwrap();

        let entries: Vec<&QueueEntry> = core.cycle.queue().entries().collect();
        assert_eq!(entries.len(), 1);
        let MutationIntent::Create { path, body } = &entries[0].intent else {
            panic!("a rule create is a POST create, not a patch");
        };
        assert_eq!(path, "/api/rules");
        assert_eq!(body.get("id").and_then(|v| v.as_str()), Some(id.as_str()));
        assert_eq!(body.get("name").and_then(|v| v.as_str()), Some("trash slide"));
        assert_eq!(body.get("event_kind"), None, "any kind is the absent field, not null");
    }

    /// #140 acceptance: "the enable/disable toggle is one CAS field,
    /// following the authority's absolute-set + `expected_version`
    /// contract." Only `enabled` is touched; every other field stays out of
    /// `patch_fields` entirely, so a 409's rebase diff never has to reason
    /// about fields this write never meant to change.
    #[tokio::test]
    async fn toggling_enabled_is_one_cas_patch_touching_only_that_field() {
        let mut core = core_with_rules(vec![fixture_rule("r-1", true, 5)]).await;
        let current = core.rules().into_iter().find(|r| r.id == "r-1").unwrap();

        core.patch_rule(
            "seed-1",
            &current,
            None,
            None,
            None,
            None,
            None,
            Some(false),
            3_000,
        )
        .await
        .unwrap();

        let entries: Vec<&QueueEntry> = core.cycle.queue().entries().collect();
        assert_eq!(entries.len(), 1);
        let MutationIntent::Patch {
            path,
            method,
            base,
            patch_fields,
            ..
        } = &entries[0].intent
        else {
            panic!("a toggle is a CAS patch, not a create");
        };
        assert_eq!(path, "/api/rules/r-1");
        assert_eq!(*method, HttpMethod::Patch);
        assert_eq!(base.get("version").and_then(serde_json::Value::as_i64), Some(5));
        assert_eq!(patch_fields, &serde_json::json!({"enabled": false}));
        assert!(
            patch_fields.get("expected_version").is_none(),
            "expected_version is not a touched field — drain fills it at send time"
        );
    }

    // ---- M2's alert surface (#141, ADR-0012) ----

    #[tokio::test]
    async fn live_alerts_crosses_every_source_and_applies_adr_0014s_predicate() {
        // The whole-table read `pane_read` is not: the notification surface
        // shows what rang, whichever source raised it.
        let mut resolved = fixture_alert("a-resolved", "race/v1", None, 500);
        resolved.resolved_at = Some(600);
        let mut dismissed = fixture_alert("a-dismissed", "city-waste/v2", None, 500);
        dismissed.dismissed_at = Some(600);
        let mut expired = fixture_alert("a-expired", "race/v1", None, 500);
        expired.expires_at = Some(2_000);

        let core = core_with_context(
            vec![],
            vec![
                resolved,
                dismissed,
                expired,
                fixture_alert("a-waste", "city-waste/v2", None, 500),
                fixture_alert("a-race", "race/v1", None, 700),
            ],
        )
        .await;

        assert_eq!(
            core.live_alerts(3_000).iter().map(|a| a.id.clone()).collect::<Vec<_>>(),
            vec!["a-race", "a-waste"],
            "both sources ride this read, newest raise first; settled rows do not"
        );
        // The same mirror, an earlier clock: the expiring one is still live,
        // and still ordered by `raised_at` descending.
        assert_eq!(
            core.live_alerts(1_500).iter().map(|a| a.id.clone()).collect::<Vec<_>>(),
            vec!["a-race", "a-expired", "a-waste"]
        );
    }

    #[tokio::test]
    async fn equal_raises_break_ties_by_id_so_two_devices_agree() {
        let core = core_with_context(
            vec![],
            vec![
                fixture_alert("a-zzz", "race/v1", None, 500),
                fixture_alert("a-aaa", "city-waste/v2", None, 500),
            ],
        )
        .await;
        assert_eq!(
            core.live_alerts(1_000).iter().map(|a| a.id.clone()).collect::<Vec<_>>(),
            vec!["a-aaa", "a-zzz"]
        );
    }

    #[tokio::test]
    async fn a_fresh_core_answers_the_alerts_reads_empty_rather_than_failing() {
        let core = Core::new();
        assert!(core.live_alerts(1_000).is_empty());
        assert_eq!(core.alert("a-1"), None);
    }

    /// The detail read is deliberately *not* liveness-filtered: acking an
    /// alert from its own detail screen must not make the screen's subject
    /// disappear out from under the reader.
    #[tokio::test]
    async fn the_by_id_read_still_answers_for_a_settled_alert() {
        let mut dismissed = fixture_alert("a-1", "city-waste/v2", None, 500);
        dismissed.dismissed_at = Some(600);
        let core = core_with_context(vec![], vec![dismissed]).await;

        assert!(core.live_alerts(1_000).is_empty());
        let found = core.alert("a-1").expect("the row is still in the mirror");
        assert_eq!(found.dismissed_at, Some(600));
        assert!(!found.is_live(1_000), "the caller applies the predicate");
        assert_eq!(core.alert("a-nope"), None);
    }

    /// ADR-0012's Ack: one CAS patch on the one human-owned column, at the
    /// `device`-scope by-id route. `dismissed_at` is the only touched field
    /// — the client never writes `resolved_at` or `expires_at`, which are
    /// the source's and ADR-0014's respectively.
    #[tokio::test]
    async fn acking_an_alert_is_one_cas_patch_touching_only_dismissed_at() {
        let mut core = core_with_context(vec![], vec![fixture_alert("a-1", "race/v1", None, 500)]).await;
        let current = core.alert("a-1").unwrap();

        core.dismiss_alert("seed-1", &current, Some(3_000), 3_000).await.unwrap();

        let entries: Vec<&QueueEntry> = core.cycle.queue().entries().collect();
        assert_eq!(entries.len(), 1);
        let MutationIntent::Patch {
            path,
            method,
            base,
            base_updated_at,
            patch_fields,
            ..
        } = &entries[0].intent
        else {
            panic!("an ack is a CAS patch, not a create");
        };
        assert_eq!(path, "/api/alerts/a-1");
        assert_eq!(*method, HttpMethod::Patch);
        assert_eq!(base.get("version").and_then(serde_json::Value::as_i64), Some(1));
        assert_eq!(*base_updated_at, 500, "alerts have no updated_at; raised_at stands in");
        assert_eq!(patch_fields, &serde_json::json!({"dismissed_at": 3_000}));
        assert!(
            patch_fields.get("expected_version").is_none(),
            "expected_version is not a touched field — drain fills it at send time"
        );
    }

    /// The double-`Option` earns its keep: un-dismissing sends an explicit
    /// JSON `null`, which the authority reads as "clear it" — distinct from
    /// omitting the field, which means "don't touch it".
    #[tokio::test]
    async fn un_dismissing_sends_an_explicit_null_not_an_absent_field() {
        let mut acked = fixture_alert("a-1", "race/v1", None, 500);
        acked.dismissed_at = Some(600);
        let mut core = core_with_context(vec![], vec![acked]).await;
        let current = core.alert("a-1").unwrap();

        core.dismiss_alert("seed-1", &current, None, 3_000).await.unwrap();

        let entries: Vec<&QueueEntry> = core.cycle.queue().entries().collect();
        let MutationIntent::Patch { patch_fields, .. } = &entries[0].intent else {
            panic!("an un-ack is a CAS patch");
        };
        assert_eq!(patch_fields, &serde_json::json!({"dismissed_at": serde_json::Value::Null}));
    }

    /// The ack path is the same durable queue every other mutation uses, so
    /// a push-triggered ack survives the process dying before the send.
    #[tokio::test]
    async fn the_ack_lands_in_the_durable_queue_like_every_other_mutation() {
        let mut core = core_with_context(vec![], vec![fixture_alert("a-1", "race/v1", None, 500)]).await;
        let current = core.alert("a-1").unwrap();
        core.dismiss_alert("seed-1", &current, Some(3_000), 3_000).await.unwrap();

        assert_eq!(
            core.cycle.queue().entries().next().unwrap().id,
            sync::write::deterministic_id("seed-1"),
            "the entry id is the caller's seed, so a replayed ack is the same entry"
        );
    }

    // ------------------------------------------------- item detail (#141)
    // ADR-0027's assembled read. Seeded through a scripted sweep like every
    // other mirror-backed test here — `SyncCycle` exposes no `mirror_mut`.

    async fn core_with_item_detail_fixtures(
        items: Vec<hummingbird_domain::Item>,
        projects: Vec<hummingbird_domain::Project>,
        steps: Vec<hummingbird_domain::Step>,
        blocked_by: Vec<hummingbird_domain::BlockedBy>,
        alerts: Vec<hummingbird_domain::Alert>,
    ) -> Core {
        let mut core = Core::new();
        core.push_api_key("token-1");
        let sweep_body = serde_json::to_string(&hummingbird_domain::ChangesResponse {
            version: 1,
            items,
            projects,
            steps,
            blocked_by,
            alerts,
            ..hummingbird_domain::ChangesResponse::empty(1)
        })
        .unwrap();
        let read = ScriptedRead::sweep_only(vec![Ok(sweep_body)]);
        let outcome = core
            .run(&read, &ScriptedWrite::new(vec![]), 1_000, Trigger::User, true, 0.0)
            .await;
        assert!(matches!(
            outcome,
            CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })
        ));
        core
    }

    /// The alert an item-threshold row raises about `item_id`, keyed
    /// through the domain recipe — never a literal, so a recipe move
    /// breaks the join here instead of only on a device.
    fn fixture_item_alert(id: &str, item_id: &str, raised_at: i64) -> hummingbird_domain::Alert {
        hummingbird_domain::Alert {
            source: hummingbird_domain::ITEM_THRESHOLD_V1.to_string(),
            source_key: hummingbird_domain::item_threshold_v1_key(item_id),
            ..fixture_alert(id, hummingbird_domain::ITEM_THRESHOLD_V1, None, raised_at)
        }
    }

    fn fixture_detail_step(id: &str, item_id: &str, position: i64) -> hummingbird_domain::Step {
        hummingbird_domain::Step {
            id: id.to_string(),
            item_id: item_id.to_string(),
            body: format!("step {id}"),
            done: false,
            position,
            deleted_at: None,
            version: 1,
        }
    }

    #[tokio::test]
    async fn an_id_the_mirror_has_never_seen_has_no_detail() {
        let core = core_with_item_detail_fixtures(vec![], vec![], vec![], vec![], vec![]).await;
        assert!(core.item_detail("nope", 1_000).is_none());
    }

    #[tokio::test]
    async fn item_detail_joins_the_project_steps_blockers_and_live_alert() {
        let mut item = fixture_item("a-1", Stage::Ready);
        item.project_id = Some("p-1".to_string());
        let core = core_with_item_detail_fixtures(
            vec![item, fixture_item("b-1", Stage::Ready)],
            vec![hummingbird_domain::Project {
                id: "p-1".to_string(),
                name: "Kitchen".to_string(),
                archived_at: None,
                created_at: 1,
                updated_at: 1,
                version: 1,
            }],
            vec![
                fixture_detail_step("s-2", "a-1", 2),
                fixture_detail_step("s-1", "a-1", 1),
                fixture_detail_step("elsewhere", "b-1", 1),
            ],
            vec![fixture_blocked_by("a-1", "b-1")],
            vec![fixture_item_alert("al-1", "a-1", 500)],
        )
        .await;

        let detail = core.item_detail("a-1", 1_000).expect("seeded item");
        assert_eq!(detail.item.id, "a-1");
        assert_eq!(
            detail.project,
            Some(item_detail::ProjectRef {
                id: "p-1".to_string(),
                name: Some("Kitchen".to_string()),
            })
        );
        assert_eq!(
            detail.steps.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec!["s-1", "s-2"],
            "position order, and only this item's steps"
        );
        assert_eq!(
            detail.open_blockers,
            vec![item_detail::BlockerEntry {
                item_id: "b-1".to_string(),
                title: Some("item b-1".to_string()),
            }]
        );
        assert_eq!(
            detail.live_alert.map(|alert| alert.id),
            Some("al-1".to_string()),
            "the state-source alert keyed on this item, joined by the built key"
        );
        assert!(!detail.is_archived);
        assert!(detail.is_editable);
    }

    /// The one-directional narrowness of the join: an alert keyed on a
    /// *different* item never reaches this record, and a dismissed one is
    /// no longer live.
    #[tokio::test]
    async fn only_a_live_alert_about_this_item_is_carried() {
        let mut acked = fixture_item_alert("al-mine", "a-1", 500);
        acked.dismissed_at = Some(600);
        let core = core_with_item_detail_fixtures(
            vec![fixture_item("a-1", Stage::Ready)],
            vec![],
            vec![],
            vec![],
            vec![acked, fixture_item_alert("al-theirs", "a-2", 500)],
        )
        .await;

        assert_eq!(core.item_detail("a-1", 1_000).unwrap().live_alert, None);
    }

    /// The count must not understate what holds an item back: a blocker id
    /// this device has never synced is listed with no title, never dropped.
    /// A settled blocker (Done, or archived) is not a blocker at all.
    #[tokio::test]
    async fn an_unseen_blocker_is_listed_and_a_settled_one_is_not() {
        let mut archived = fixture_item("b-archived", Stage::Ready);
        archived.archived_at = Some(900);
        let core = core_with_item_detail_fixtures(
            vec![
                fixture_item("a-1", Stage::Ready),
                fixture_item("b-done", Stage::Done),
                archived,
            ],
            vec![],
            vec![],
            vec![
                fixture_blocked_by("a-1", "b-done"),
                fixture_blocked_by("a-1", "b-archived"),
                fixture_blocked_by("a-1", "b-unseen"),
            ],
            vec![],
        )
        .await;

        assert_eq!(
            core.item_detail("a-1", 1_000).unwrap().open_blockers,
            vec![item_detail::BlockerEntry {
                item_id: "b-unseen".to_string(),
                title: None,
            }]
        );
    }

    /// A project id the mirror has no row for names itself rather than
    /// disappearing — the same "never understate" rule the blockers get.
    #[tokio::test]
    async fn an_unseen_project_keeps_its_id_and_loses_only_its_name() {
        let mut item = fixture_item("a-1", Stage::Ready);
        item.project_id = Some("p-unseen".to_string());
        let core =
            core_with_item_detail_fixtures(vec![item], vec![], vec![], vec![], vec![]).await;

        assert_eq!(
            core.item_detail("a-1", 1_000).unwrap().project,
            Some(item_detail::ProjectRef {
                id: "p-unseen".to_string(),
                name: None,
            })
        );
    }

    /// `CONTEXT.md`'s Recall rule (#478) through the second door: history
    /// is readable and ackable, never editable, and offers no act
    /// vocabulary whatever stage its row still carries.
    #[tokio::test]
    async fn an_archived_item_is_readable_and_ackable_but_not_editable() {
        let mut archived = fixture_item("a-1", Stage::Ready);
        archived.archived_at = Some(900);
        let core = core_with_item_detail_fixtures(
            vec![archived],
            vec![],
            vec![],
            vec![],
            vec![fixture_item_alert("al-1", "a-1", 500)],
        )
        .await;

        let detail = core.item_detail("a-1", 1_000).expect("archived rows stay readable");
        assert!(detail.is_archived);
        assert!(!detail.is_editable);
        assert!(
            detail.available_actions.is_empty(),
            "an archived row is not a live item in a stage"
        );
        assert!(
            detail.live_alert.is_some(),
            "silencing something still ringing is not editing history"
        );
    }

    /// The one invariant that ties [`Core::frontier`] and
    /// [`Core::item_detail`] together (#545), pinned as intended
    /// behaviour rather than made red: the overlay can carry
    /// `archived_at` for an item whose mirror row is still Live and
    /// `Ready` — that is exactly what a cancel taken offline (S11/#109)
    /// is — and the two reads then answer differently about it.
    ///
    /// `CONTEXT.md` adjudicates, and it says the divergence is correct.
    /// They must never disagree about *what the item is*: both lay the
    /// same overlay over the mirror, so both see the archive stamp the
    /// instant it is enqueued. They differ only on membership, and each
    /// membership rule is the term's own: the **Frontier** is "what can
    /// be started right now", so a cancelled item leaves it immediately,
    /// offline or not; **Item detail** keeps it, on **Recall**'s rule
    /// that history stays readable and only what is still live is
    /// editable. The failures this pins are the reads *agreeing* wrongly
    /// — a cancelled item still offered as startable, or detail
    /// presenting one as editable.
    #[tokio::test]
    async fn a_pending_cancel_leaves_the_frontier_while_detail_still_reads_it_as_archived() {
        let mut core = core_with_item_detail_fixtures(
            vec![fixture_item("a-1", Stage::Ready)],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await;
        assert_eq!(
            core.frontier().iter().map(|item| item.id.clone()).collect::<Vec<_>>(),
            vec!["a-1"],
            "a Live Ready row is on the frontier before the cancel"
        );

        core.act("seed-1", "a-1", ItemAction::Cancel, 2_000).await.unwrap();

        // The premise: nothing about the mirror's own row changed. The
        // contradiction lives in the overlay alone until a cycle confirms it.
        let mirrored = core.cycle.mirror().item("a-1").expect("the mirror row is still Live");
        assert_eq!(mirrored.stage, Stage::Ready);
        assert_eq!(mirrored.archived_at, None);

        assert!(
            core.frontier().is_empty(),
            "the frontier is what can be started right now — a cancelled item is not"
        );
        let detail = core
            .item_detail("a-1", 2_000)
            .expect("history stays readable, so detail still answers for the row");
        assert_eq!(
            detail.item.archived_at,
            Some(2_000),
            "both reads see the same overlay: detail must not show the pre-cancel row"
        );
        assert!(detail.is_archived);
        assert!(!detail.is_editable);
        assert!(
            detail.available_actions.is_empty(),
            "an item cancelled offline offers no act vocabulary, pending or not"
        );
    }

    /// The same pair after a reload, because the state is durable: the
    /// overlay is never persisted, [`overlay_from_queue`] rebuilds it from
    /// the queue entry, and the queue entry outlives the process. A reload
    /// that dropped the archive stamp would put a cancelled item back on
    /// the frontier and make it editable again from detail.
    #[tokio::test]
    async fn a_pending_cancel_still_splits_the_two_reads_after_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-cancel-reload");
        let ns = namespace.to_str().unwrap();

        let mut first = Core::init(ns, "api-key-1").await.unwrap();
        let id = first
            .capture("seed-1", "buy milk", Stage::Ready, 1_000, CaptureOptions::default())
            .await
            .unwrap();
        first
            .act("seed-act-1", &id, ItemAction::Cancel, 2_000)
            .await
            .unwrap();
        drop(first);

        let second = Core::init(ns, "api-key-2").await.unwrap();
        assert!(
            second.frontier().is_empty(),
            "a reload must not resurrect a cancelled item onto the frontier"
        );
        let detail = second.item_detail(&id, 3_000).expect("history stays readable");
        assert!(detail.is_archived);
        assert!(!detail.is_editable);
    }

    /// The overlay wins here exactly as it does on the frontier: an act
    /// taken offline shows on the detail the instant it is enqueued.
    #[tokio::test]
    async fn a_pending_act_shows_on_the_detail_before_it_is_confirmed() {
        let mut core = core_with_item_detail_fixtures(
            vec![fixture_item("a-1", Stage::Ready)],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await;
        core.act("seed-1", "a-1", ItemAction::Start, 2_000).await.unwrap();

        assert_eq!(core.item_detail("a-1", 2_000).unwrap().item.stage, Stage::InProgress);
    }

    /// ADR-0027 part 3: completing the item acks the alert about it, as
    /// two ordered queue entries with the act first — never one invented
    /// authority-side mutation.
    #[tokio::test]
    async fn completing_an_item_acks_its_alert_act_first() {
        let mut core = core_with_item_detail_fixtures(
            vec![fixture_item("a-1", Stage::Ready)],
            vec![],
            vec![],
            vec![],
            vec![fixture_item_alert("al-1", "a-1", 500)],
        )
        .await;

        core.act_acking_alert("seed-1", "a-1", ItemAction::Complete, 3_000)
            .await
            .unwrap();

        let paths: Vec<String> = core
            .cycle
            .queue()
            .entries()
            .map(|entry| match &entry.intent {
                MutationIntent::Patch { path, .. } => path.clone(),
                other => panic!("expected two patches, got {other:?}"),
            })
            .collect();
        assert_eq!(
            paths,
            vec!["/api/items/a-1".to_string(), "/api/alerts/al-1".to_string()],
            "the act is enqueued first, in the order the intent happened"
        );
        assert_eq!(core.item_detail("a-1", 3_000).unwrap().item.stage, Stage::Done);
    }

    /// Starting an item, or declaring an external wait, is not an ack:
    /// neither claims the matter is settled, so the alert keeps ringing.
    #[tokio::test]
    async fn starting_or_blocking_an_item_does_not_ack_its_alert() {
        for action in [ItemAction::Start, ItemAction::Block] {
            let mut core = core_with_item_detail_fixtures(
                vec![fixture_item("a-1", Stage::Ready)],
                vec![],
                vec![],
                vec![],
                vec![fixture_item_alert("al-1", "a-1", 500)],
            )
            .await;

            core.act_acking_alert("seed-1", "a-1", action, 3_000).await.unwrap();

            assert_eq!(
                core.cycle.queue().entries().count(),
                1,
                "{action:?} enqueues the act alone"
            );
        }
    }

    /// With nothing ringing, the composition is plain `act` — no empty
    /// second entry, no special case at the call site.
    #[tokio::test]
    async fn acting_with_no_live_alert_enqueues_only_the_act() {
        let mut core = core_with_item_detail_fixtures(
            vec![fixture_item("a-1", Stage::Ready)],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await;

        core.act_acking_alert("seed-1", "a-1", ItemAction::Complete, 3_000)
            .await
            .unwrap();

        assert_eq!(core.cycle.queue().entries().count(), 1);
    }
}
