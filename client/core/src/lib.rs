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
//! deferral, not an oversight — a follow-up issue should do the actual fold
//! once #126 lands.
//!
//! Persistence lives in [`storage`] (#68).

use std::collections::{BTreeMap, BTreeSet};

pub mod calendar;
pub mod context;
pub mod storage;
pub mod sync;
pub mod task;

use hummingbird_domain::{CreateItem, Item, Stage};

use storage::{MemorySnapshotStore, SnapshotError, SnapshotStore};
use sync::queue::{MutationIntent, QueueEntry};
use sync::transport::ChangesTransport;
use sync::write::transport::MutationTransport;
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
        due_date: create.due_date.clone(),
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

/// Rebuilds the overlay a previous session left mid-flight from whatever
/// still-queued item creates [`sync::SyncCycle::load`] just loaded — so a
/// capture made offline, then reloaded before ever syncing, is still
/// readable from [`Core::frontier`] rather than silently vanishing until
/// the next successful cycle. Patches are not overlaid (out of this
/// issue's scope — only [`Core::capture`] writes to the overlay today, and
/// it never enqueues one).
fn overlay_from_queue(queue: &sync::queue::OutboundQueue) -> BTreeMap<String, OverlayEntry> {
    let mut overlay = BTreeMap::new();
    for entry in queue.entries() {
        if let MutationIntent::Create { path, body } = &entry.intent {
            if *path == sync::write::paths::items() {
                if let Ok(create) = serde_json::from_value::<CreateItem>(body.clone()) {
                    let item = item_from_create(&create, 0);
                    overlay.insert(
                        item.id.clone(),
                        OverlayEntry {
                            entry_id: entry.id.clone(),
                            item,
                        },
                    );
                }
            }
        }
    }
    overlay
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
        let overlay = overlay_from_queue(cycle.queue());
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

    /// What can actually be started right now — the owned-schema mirror's
    /// live `Ready`/`InProgress` items, not blocked on an open blocker,
    /// **with every not-yet-confirmed capture overlaid on top.**
    ///
    /// A capture is readable here the instant [`Core::capture`] returns —
    /// before [`Core::run`] has ever sent it — and stays readable,
    /// unchanged, through every cycle attempt until either a completed
    /// cycle's pull confirms it (the overlay clears because drain-before-
    /// pull, ADR-0007, means this device's own write already landed by the
    /// time that pull asked for truth) or a dead-lettered entry reverts it
    /// to server truth. There is no cycle outcome that makes the item
    /// disappear in between: a queue-side send failure, a pull failure, or
    /// a credential hold all leave the overlay exactly as it was.
    pub fn frontier(&self) -> Vec<Item> {
        let mirror = self.cycle.mirror();
        let mut items: BTreeMap<String, Item> = mirror
            .all_items()
            .map(|item| (item.id.clone(), item.clone()))
            .collect();
        for overlay in self.overlay.values() {
            items.insert(overlay.item.id.clone(), overlay.item.clone());
        }

        items
            .into_values()
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
    pub async fn capture(
        &mut self,
        seed: &str,
        title: impl Into<String>,
        stage: Stage,
        now_ms: i64,
    ) -> Result<String, SnapshotError<QS::Error>> {
        let id = sync::write::deterministic_id(seed);
        let title = title.into();

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
            due_date: None,
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

    /// The host calls this at init and on every credential rotation.
    /// Always resumes (a fresh push is the only way out of a hold), the
    /// same contract [`context::ContextPoller::push_token`] documents.
    pub fn push_api_key(&mut self, api_key: impl Into<String>) {
        self.credential.push(api_key.into());
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
        // `OverlayEntry` carries for exactly this.
        let dead_lettered_ids: BTreeSet<&str> = self
            .cycle
            .queue()
            .dead_letters()
            .iter()
            .map(|dead_letter| dead_letter.entry.id.as_str())
            .collect();
        self.overlay
            .retain(|_, overlay| !dead_lettered_ids.contains(overlay.entry_id.as_str()));

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
            due_date: None,
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
}
