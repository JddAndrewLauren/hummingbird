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

use std::collections::{BTreeMap, BTreeSet};

pub mod calendar;
pub mod capture;
pub mod context;
pub mod storage;
pub mod sync;
pub mod task;

use hummingbird_domain::{CreateItem, Energy, Item, Project, Size, Stage};

use storage::{MemorySnapshotStore, SnapshotError, SnapshotStore};
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

#[cfg(not(target_arch = "wasm32"))]
fn queue_store(namespace: &str) -> CoreStore {
    CoreStore::new(std::path::Path::new(namespace).join("queue.json"))
}
#[cfg(not(target_arch = "wasm32"))]
fn mirror_store(namespace: &str) -> CoreStore {
    CoreStore::new(std::path::Path::new(namespace).join("mirror.json"))
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
            MutationIntent::Create { .. } | MutationIntent::Patch { .. } => {
                // Not an item mutation (a step/project/etc create, or a
                // patch on some other entity) — nothing this overlay
                // projects.
            }
        }
    }
    Ok(overlay)
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
}

/// S13/#111's triage destination — the only two stages triage itself may
/// promote an item into. Deliberately not a raw [`Stage`] the caller picks
/// (same "resolved by name from the vocabulary, never hardcoded" discipline
/// [`ItemAction::stage`] documents): `Grilling` is where a captured item
/// goes to have its fog worked before it can be minted, `Ready` is where it
/// goes once it is already startable outright. There is no `Backlog`
/// destination — the owned schema's six-stage vocabulary
/// (`hummingbird_domain::Stage`) has no such stage; a triaged item that
/// is not yet ready to promote simply stays in `Triage` (never calling
/// [`Core::triage`] at all) rather than moving to a stage the schema
/// cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriageDestination {
    Grilling,
    Ready,
}

impl TriageDestination {
    fn stage(self) -> Stage {
        match self {
            TriageDestination::Grilling => Stage::Grilling,
            TriageDestination::Ready => Stage::Ready,
        }
    }
}

/// S13/#111's multi-field triage edit — every field the triage form may set
/// alongside the destination stage, each `None` meaning "leave this field
/// alone" rather than "clear it". `None` on every field is a legal call: a
/// bare promotion with no other edit is still exactly one mutation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TriagePatch {
    pub title: Option<String>,
    pub project_id: Option<String>,
    pub size: Option<Size>,
    pub energy: Option<Energy>,
    pub context: Option<String>,
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
/// save_snapshot(&store, 1, 0, api_key).await.unwrap();
/// # }
/// ```
pub struct Core<QS = MemorySnapshotStore, MS = MemorySnapshotStore> {
    cycle: SyncCycle<QS, MS>,
    credential: AccessTokenSlot,
    overlay: BTreeMap<String, OverlayEntry>,
    events: Vec<CoreEvent>,
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
            events: Vec::new(),
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
        Ok(Self {
            cycle,
            credential,
            overlay,
            events: Vec::new(),
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
    pub async fn capture(
        &mut self,
        seed: &str,
        title: impl Into<String>,
        stage: Stage,
        now_ms: i64,
    ) -> Result<String, SnapshotError<QS::Error>> {
        let id = sync::write::deterministic_id(seed);
        let title = title.into();
        let _ = capture::parse_seam(&title);

        let create = CreateItem {
            id: id.clone(),
            title: title.clone(),
            description: None,
            stage: Some(stage),
            size: None,
            energy: None,
            context: None,
            priority: None,
            project_id: None,
            project_pos: None,
            deadline: None,
            scheduled_date: None,
            source: None,
            source_key: None,
            source_url: None,
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

    /// Triages an already-existing (captured) item (S13/#111): edits
    /// whatever fields `patch` sets, and promotes it to `destination`, as
    /// **one** CAS `PATCH` — enqueued the same way [`Core::act`] enqueues
    /// its own single-field patch (durably, via [`sync::SyncCycle::enqueue`],
    /// never [`sync::queue::OutboundQueue::enqueue`] directly), never four
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
    /// A triaged item leaves [`Core::triage_inbox`] and — for
    /// [`TriageDestination::Ready`] — appears on [`Core::frontier`] the
    /// instant this returns, through the same overlay every other read here
    /// goes through, never a separate local bookkeeping list.
    ///
    /// `seed` mints this mutation's own queue-entry id
    /// ([`sync::write::deterministic_id`]) — caller-supplied, same reasoning
    /// as [`Core::act`]'s `seed`.
    pub async fn triage(
        &mut self,
        seed: &str,
        item_id: &str,
        destination: TriageDestination,
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

        optimistic.stage = destination.stage();
        patch_fields.insert(
            "stage".to_string(),
            serde_json::to_value(destination.stage()).expect("Stage always serializes"),
        );

        if let Some(title) = &patch.title {
            optimistic.title = title.clone();
            patch_fields.insert("title".to_string(), serde_json::json!(title));
        }
        if let Some(project_id) = &patch.project_id {
            optimistic.project_id = Some(project_id.clone());
            patch_fields.insert("project_id".to_string(), serde_json::json!(project_id));
        }
        if let Some(size) = patch.size {
            optimistic.size = Some(size);
            patch_fields.insert(
                "size".to_string(),
                serde_json::to_value(size).expect("Size always serializes"),
            );
        }
        if let Some(energy) = patch.energy {
            optimistic.energy = Some(energy);
            patch_fields.insert(
                "energy".to_string(),
                serde_json::to_value(energy).expect("Energy always serializes"),
            );
        }
        if let Some(context) = &patch.context {
            optimistic.context = Some(context.clone());
            patch_fields.insert("context".to_string(), serde_json::json!(context));
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
        core.capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
        core.capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
            .await
            .unwrap();

        assert!(core.is_pending(&id));
        assert!(!core.is_pending("some-other-id"));
    }

    // ---------------------------------------------------------------- act

    /// This issue's headline acceptance: "Completing offline shows Done
    /// immediately". No transport is even wired up — proving the overlay
    /// needs no network call to appear.
    #[tokio::test]
    async fn completing_offline_shows_done_immediately() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "someday maybe", Stage::Triage, 1_000)
            .await
            .unwrap();
        assert_eq!(core.triage_inbox().len(), 1);
        assert_eq!(core.frontier().len(), 0);

        core.triage(
            "seed-triage-1",
            &id,
            TriageDestination::Ready,
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

    /// Grilling is pre-action too (`CONTEXT.md`) — a triage into Grilling
    /// leaves the triage inbox but never lands on the frontier.
    #[tokio::test]
    async fn sending_to_grilling_leaves_triage_without_reaching_the_frontier() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "someday maybe", Stage::Triage, 1_000)
            .await
            .unwrap();

        core.triage(
            "seed-triage-1",
            &id,
            TriageDestination::Grilling,
            TriagePatch::default(),
            2_000,
        )
        .await
        .unwrap();

        assert_eq!(core.triage_inbox().len(), 0);
        assert_eq!(core.frontier().len(), 0);
    }

    /// This issue's "a multi-field triage is one mutation, not four" —
    /// title, project, size, energy and context all land in the same
    /// enqueued `QueueEntry`.
    #[tokio::test]
    async fn a_multi_field_triage_is_exactly_one_queued_mutation() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "someday maybe", Stage::Triage, 1_000)
            .await
            .unwrap();
        assert_eq!(core.queue_depth(), 1, "capture itself is the first entry");

        core.triage(
            "seed-triage-1",
            &id,
            TriageDestination::Ready,
            TriagePatch {
                title: Some("buy milk".to_string()),
                project_id: Some("project-1".to_string()),
                size: Some(Size::Quick),
                energy: Some(Energy::Low),
                context: Some("@errands".to_string()),
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
        assert_eq!(item.project_id.as_deref(), Some("project-1"));
        assert_eq!(item.size, Some(Size::Quick));
        assert_eq!(item.energy, Some(Energy::Low));
        assert_eq!(item.context.as_deref(), Some("@errands"));
    }

    /// A field `TriagePatch` leaves `None` is untouched — triage never
    /// clears a field it was not asked to set.
    #[tokio::test]
    async fn an_unset_triage_patch_field_leaves_the_items_existing_value_untouched() {
        let mut core = Core::new();
        let id = core
            .capture("seed-1", "someday maybe", Stage::Triage, 1_000)
            .await
            .unwrap();
        core.triage(
            "seed-triage-1",
            &id,
            TriageDestination::Grilling,
            TriagePatch {
                context: Some("@computer".to_string()),
                ..TriagePatch::default()
            },
            2_000,
        )
        .await
        .unwrap();

        core.triage(
            "seed-triage-2",
            &id,
            TriageDestination::Ready,
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

    #[tokio::test]
    async fn triaging_an_unknown_item_id_is_item_not_found() {
        let mut core = Core::new();

        let error = core
            .triage(
                "seed-triage-1",
                "no-such-item",
                TriageDestination::Ready,
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
            .capture("seed-1", "someday maybe", Stage::Triage, 1_000)
            .await
            .unwrap();

        core.triage(
            "seed-triage-1",
            &id,
            TriageDestination::Ready,
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "someday maybe", Stage::Triage, 1_000)
            .await
            .unwrap();
        first
            .triage(
                "seed-triage-1",
                &id,
                TriageDestination::Ready,
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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

        core.capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
        core.capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
        core.capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            events: Vec::new(),
        };
        core.push_api_key("token-1");
        let id = core
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk v1", Stage::Ready, 1_000)
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
            .capture("seed-1", "buy milk v2", Stage::Ready, 2_000)
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
        let id = core.capture("seed-1", "buy milk", Stage::Ready, 1_000).await.unwrap();

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
        core.capture("seed-1", "someday maybe", Stage::Triage, 1_000)
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

        core.capture("seed-1", raw, Stage::Triage, 1_000).await.unwrap();

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
        let id1 = core.capture("seed-a", "buy milk", Stage::Triage, 1_000).await.unwrap();
        let id2 = core.capture("seed-b", "call dentist", Stage::Triage, 2_000).await.unwrap();
        let id3 = core.capture("seed-c", "water plants", Stage::Triage, 3_000).await.unwrap();
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
}
