//! `hummingbird-ffi-mobile`: `#[uniffi::export]` wrappers over
//! `hummingbird-core` for Kotlin (Android, Wear OS) and Swift (iPad).
//!
//! [`MobileTaskHost`] is the native hosts' one door into `Core` — the same
//! wrapper-plus-live-transports split `ffi-web::task_host::TaskHostCore`
//! is, surfaced through UniFFI instead of `wasm_bindgen` (ADR-0001 seam
//! rule 2: both FFI crates surface `Core` verbatim, neither reimplements
//! it). The surface grows one screen at a time with the Android build
//! (#141), each screen's decision modules arriving in `core` first
//! (ADR-0025); M0 carried init, credential push, one sync cycle, and the
//! counters the proof screen shows. M1-5 (#503) added the free
//! [`can_submit_capture`] door onto [`hummingbird_core::decisions`] and
//! [`MobileTaskHost::capture`] — `CaptureActivity`'s whole surface, title
//! only, no capture-meta fields (out of scope until a later screen).
//!
//! **Async runs under uniffi's tokio runtime** (`async_runtime = "tokio"`),
//! because `Core::run`'s reqwest transports need a reactor and the host
//! (Kotlin coroutines / Swift concurrency) must never be the thing
//! providing one. Interior state is a `tokio::sync::Mutex` for the same
//! reason: it is held across the cycle's awaits.
//!
//! **Android never calls a per-item decision function** (M1-6/#504) — this
//! is the designed asymmetry with the web seam. `client/ffi-web/src/decisions.rs`
//! exposes `hummingbird_core::decisions` as a free-function door a *second*,
//! main-thread wasm instance calls per keystroke/per row (that module's own
//! header explains why a second stateless instance is safe there). Android
//! has no such second instance and no in-process wasm boundary to cross
//! cheaply from Kotlin — every crossing here is a JNI call — so
//! [`MobileTaskHost::now_queue`] does the *decided* work once, on the Rust
//! side, and hands Kotlin one ordered [`Vec<NowItemRecord>`], each record
//! already carrying its [`MobileUrgencyBand`] and wire-vocabulary action
//! list. `NowScreen` never calls `by_priority_then_due`, `compute_urgency`
//! or `available_actions` itself, and never could: those functions are not
//! exported to Kotlin at all, only their already-applied results are.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use hummingbird_core::decisions::{available_actions, frontier, urgency};
use hummingbird_core::storage::FsSnapshotStore;
use hummingbird_core::sync::write::ReqwestMutationTransport;
use hummingbird_core::sync::{ReqwestSyncTransport, Trigger};
use hummingbird_core::{CaptureOptions, Core, CoreCycleOutcome, CoreEvent, ItemAction};
use hummingbird_domain::{Item, Stage};

/// Whether `draft` is worth submitting — the free door onto
/// [`hummingbird_core::decisions::can_submit_capture`] (ADR-0025), the
/// mobile twin of `ffi-web::decisions::can_submit_capture`. `CaptureActivity`
/// calls this before ever calling [`MobileTaskHost::capture`], and it is the
/// *only* place the refusal may be decided: a Kotlin `isBlank()` copy of
/// this rule is banned (M1-5/#503's own trap) precisely because it disagrees
/// with the real rule on a BOM-only draft — see
/// `hummingbird_core::decisions::capture`'s doc for that case.
#[uniffi::export]
pub fn can_submit_capture(draft: &str) -> bool {
    hummingbird_core::decisions::can_submit_capture(draft)
}

/// Mints a fresh seed for one durable mutation ([`MobileTaskHost::capture`]
/// or [`MobileTaskHost::act`]). Only uniqueness matters here, never
/// unpredictability — but uniqueness has to hold **across processes**, not
/// just within one, because the seed becomes an entity id and the
/// authority's create path is idempotent on it: a second capture that mints
/// an id an earlier one already used is not a duplicate, it is a *lost*
/// capture, silently answered with the old item. `now_ms` plus a
/// process-local counter cannot carry that weight — a restart resets the
/// counter, so a repeated wall-clock millisecond (a clock step back, an NTP
/// correction, a fast relaunch) reproduces a live id. So the seed leads
/// with OS randomness, exactly as the web's equivalent already does
/// (`useCaptureWiring.ts`'s `mintSeed`, `crypto.randomUUID()`).
///
/// The counter stays, behind the random bytes, for the one thing randomness
/// does not give for free: two mutations in the same process are ordered
/// and distinct even if the RNG were to repeat itself. `kind` and `now_ms`
/// remain for legibility of a raw id in a log.
///
/// This crate never builds for `wasm32` (that is `ffi-web`, which takes a
/// caller-supplied seed from JS precisely so it needs no RNG of its own),
/// so `getrandom` here does not reopen the "no RNG that panics on bare
/// wasm32" constraint `sync::write::deterministic_id`'s callers work under.
/// A failing RNG is not a reason to refuse a capture, so it degrades to the
/// old counter-only shape rather than propagating — the narrow cross-process
/// window is strictly better than dropping the user's text.
static MUTATION_SEQ: AtomicU64 = AtomicU64::new(0);

fn mint_mutation_seed(kind: &str, now_ms: i64) -> String {
    let seq = MUTATION_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut random = [0u8; 16];
    match getrandom::getrandom(&mut random) {
        Ok(()) => {
            let hex: String = random.iter().map(|b| format!("{b:02x}")).collect();
            format!("mobile-{kind}:{now_ms}:{seq}:{hex}")
        }
        Err(_) => format!("mobile-{kind}:{now_ms}:{seq}"),
    }
}

uniffi::setup_scaffolding!();

/// The public API version of the wrapped `hummingbird-core`, surfaced
/// verbatim to Kotlin/Swift (ADR-0001 seam rule 2). Predates
/// [`MobileTaskHost`] as the stub's smoke test; kept exported because it is
/// the cheapest end-to-end proof a host has that the generated binding and
/// the loaded `.so` agree — the Android instrumented test asserts it.
#[uniffi::export]
pub fn core_api_version() -> u32 {
    hummingbird_core::Core::new().api_version()
}

/// [`MobileTaskHost::init`] failed to load durable state under the given
/// namespace. One flat arm: the caller's only recovery is showing the
/// message (`CoreInitError` is already a message-carrying wrapper for the
/// same reason — see its doc).
#[derive(Debug, uniffi::Error)]
pub enum MobileInitError {
    // `detail`, not `message`: a UniFFI error variant field named
    // `message` generates a Kotlin `val message` that collides with
    // `kotlin.Exception.message` and fails to compile.
    InitFailed { detail: String },
}

impl std::fmt::Display for MobileInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobileInitError::InitFailed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for MobileInitError {}

/// [`MobileTaskHost::capture`] failed to enqueue — a [`hummingbird_core`]
/// `SnapshotError`, surfaced the same message-wrapping way
/// [`MobileInitError`] is (see its doc for why `detail`, not `message`).
#[derive(Debug, uniffi::Error)]
pub enum MobileCaptureError {
    CaptureFailed { detail: String },
}

impl std::fmt::Display for MobileCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobileCaptureError::CaptureFailed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for MobileCaptureError {}

/// [`MobileTaskHost::act`] failed — either the item is unknown locally
/// ([`hummingbird_core::ActError::ItemNotFound`], a caller mistake, not a
/// durability failure) or the mutation failed to enqueue durably (wrapped
/// the same message-only way [`MobileCaptureError`] wraps a `SnapshotError`
/// — see [`MobileInitError`]'s doc for why `detail`, not `message`).
#[derive(Debug, uniffi::Error)]
pub enum MobileActError {
    ItemNotFound,
    ActFailed { detail: String },
}

impl std::fmt::Display for MobileActError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobileActError::ItemNotFound => write!(f, "item not found"),
            MobileActError::ActFailed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for MobileActError {}

/// One drained [`CoreEvent`], as the mobile hosts' shape — the same
/// `kind`/`at_ms` pair `ffi-web`'s `TaskEventDTO` carries, so the two FFI
/// surfaces stay nameable in one breath.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileEvent {
    pub kind: String,
    pub at_ms: i64,
}

fn map_event(event: CoreEvent) -> MobileEvent {
    match event {
        CoreEvent::CredentialNeeded { at_ms } => MobileEvent {
            kind: "credential_needed".to_string(),
            at_ms,
        },
    }
}

/// What one [`MobileTaskHost::run`] cycle did — `ffi-web`'s `RunResponse`
/// shape with the identical `kind` strings, so a host on either side of
/// the seam badges sync state from the same vocabulary.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RunOutcome {
    pub kind: String,
    pub retry_after_ms: Option<i64>,
    pub active_item_count: Option<u32>,
    pub was_full_sweep: Option<bool>,
    pub dead_lettered: Option<u32>,
}

fn run_outcome(kind: &str) -> RunOutcome {
    RunOutcome {
        kind: kind.to_string(),
        retry_after_ms: None,
        active_item_count: None,
        was_full_sweep: None,
        dead_lettered: None,
    }
}

fn map_run_outcome(outcome: CoreCycleOutcome) -> RunOutcome {
    use hummingbird_core::sync::CycleOutcome;
    match outcome {
        CoreCycleOutcome::NoCredential => run_outcome("no_credential"),
        CoreCycleOutcome::Held => run_outcome("held"),
        CoreCycleOutcome::Cycle(cycle) => match cycle {
            CycleOutcome::Skipped => run_outcome("skipped"),
            CycleOutcome::Blocked {
                drain,
                retry_after_ms,
            } => RunOutcome {
                dead_lettered: Some(drain.dead_lettered() as u32),
                retry_after_ms: Some(retry_after_ms),
                ..run_outcome("blocked")
            },
            CycleOutcome::CredentialNeeded { drain } => RunOutcome {
                dead_lettered: Some(drain.dead_lettered() as u32),
                ..run_outcome("credential_needed")
            },
            CycleOutcome::PersistFailed { retry_after_ms, .. } => RunOutcome {
                retry_after_ms: Some(retry_after_ms),
                ..run_outcome("persist_failed")
            },
            CycleOutcome::PullFailed {
                drain,
                retry_after_ms,
            } => RunOutcome {
                dead_lettered: Some(drain.dead_lettered() as u32),
                retry_after_ms: Some(retry_after_ms),
                ..run_outcome("pull_failed")
            },
            CycleOutcome::Completed {
                drain,
                active_item_count,
                was_full_sweep,
            } => RunOutcome {
                dead_lettered: Some(drain.dead_lettered() as u32),
                active_item_count: Some(active_item_count as u32),
                was_full_sweep: Some(was_full_sweep),
                ..run_outcome("completed")
            },
        },
    }
}

// ----------------------------------------------------------------- M1-6 (#504)
// `NowScreen`'s whole read: the frontier, decided. See the module header for
// the Android-never-calls-per-item-decision-functions asymmetry this section
// exists to keep.

/// [`urgency::UrgencyBand`], mirrored as a `uniffi::Enum` — the exposure
/// `urgency.rs`'s own doc names as "M1-6's job (#504), not this module's":
/// `hummingbird-core` stays binding-agnostic (ADR-0003), so this is a
/// second, uniffi-derived definition of the same four bands rather than an
/// annotation added to the core type. [`map_urgency_band`] is the only place
/// the two are allowed to drift apart from, and it is exhaustive with no
/// wildcard arm for exactly that reason. Kotlin's `when (band)` over this
/// type is the compile-time drift gate the brief names: a fifth band added
/// to `urgency::UrgencyBand` without a matching arm here fails this crate's
/// build before it ever reaches Kotlin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileUrgencyBand {
    Calm,
    Soon,
    Now,
    Overdue,
}

fn map_urgency_band(band: urgency::UrgencyBand) -> MobileUrgencyBand {
    match band {
        urgency::UrgencyBand::Calm => MobileUrgencyBand::Calm,
        urgency::UrgencyBand::Soon => MobileUrgencyBand::Soon,
        urgency::UrgencyBand::Now => MobileUrgencyBand::Now,
        urgency::UrgencyBand::Overdue => MobileUrgencyBand::Overdue,
    }
}

/// One `NowScreen` row: an already-decided [`Item`] — ordered by the
/// caller ([`MobileTaskHost::now_queue`] returns these pre-ordered, so
/// `ids`/index order *is* display order), its [`MobileUrgencyBand`] and its
/// S11/#109 act vocabulary strings (`"start"|"complete"|"block"|"cancel"`,
/// [`ItemAction::as_str`]) ready for [`MobileTaskHost::act`]. Carries only
/// the fields `NowScreen` actually renders — the same "hand the boundary
/// only what a decision needs" discipline [`frontier::FrontierItem`]'s own
/// doc states, applied on the way *out* instead of in.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NowItemRecord {
    pub id: String,
    pub title: String,
    pub deadline: Option<String>,
    pub urgency: MobileUrgencyBand,
    /// The raw wire priority (0..=4) — never re-derive its display rank in
    /// Kotlin; [`frontier::priority_rank`] already decided the order this
    /// record arrives in.
    pub priority: i64,
    pub context: Option<String>,
    pub available_actions: Vec<String>,
}

fn to_frontier_item(item: &Item) -> frontier::FrontierItem {
    frontier::FrontierItem {
        id: item.id.clone(),
        priority: item.priority,
        deadline: item.deadline.clone(),
        context: item.context.clone(),
        size: item.size.map(|size| size.as_str().to_string()),
        energy: item.energy.map(|energy| energy.as_str().to_string()),
        project_id: item.project_id.clone(),
    }
}

fn to_now_item_record(item: &Item, now: &str) -> NowItemRecord {
    let band = urgency::compute_urgency(item.deadline.as_deref(), now);
    let actions: Vec<String> = available_actions(item.stage)
        .iter()
        .map(|action| action.as_str().to_string())
        .collect();
    NowItemRecord {
        id: item.id.clone(),
        title: item.title.clone(),
        deadline: item.deadline.clone(),
        urgency: map_urgency_band(band),
        priority: item.priority,
        context: item.context.clone(),
        available_actions: actions,
    }
}

/// [`MobileTaskHost::now_queue`]'s pure core: `hummingbird_core::Core::frontier`'s
/// `Ready`/`InProgress` items, ordered by [`frontier::by_priority_then_due`]
/// (ADR-0021 decision 1's one spelling) and decided into [`NowItemRecord`]s.
/// Free of `Core`/`MobileTaskHost` entirely so the ordering pin below can
/// exercise it directly, against the identical fixture shapes
/// `hummingbird_core::decisions::frontier`'s own tests use, with no async
/// runtime or durable store in the loop.
fn build_now_queue(items: &[Item], now: &str) -> Vec<NowItemRecord> {
    let by_id: HashMap<&str, &Item> = items.iter().map(|item| (item.id.as_str(), item)).collect();
    let entries: Vec<frontier::FrontierItem> = items.iter().map(to_frontier_item).collect();
    frontier::by_priority_then_due(&entries)
        .into_iter()
        .filter_map(|id| by_id.get(id.as_str()).map(|item| to_now_item_record(item, now)))
        .collect()
}

struct Inner {
    core: Core<FsSnapshotStore, FsSnapshotStore>,
    read_transport: ReqwestSyncTransport,
    write_transport: ReqwestMutationTransport,
}

/// The native hosts' handle on one durable `Core` — `TaskHostCore`'s
/// mobile twin (see the module doc for the seam-rule symmetry).
#[derive(uniffi::Object)]
pub struct MobileTaskHost {
    inner: tokio::sync::Mutex<Inner>,
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileTaskHost {
    /// Loads (or starts fresh) the durable sync state under `namespace` —
    /// a directory path on mobile targets; the host supplies its app-local
    /// files dir (ADR-0003: the namespace is the one thing the host
    /// contributes at init). `base_url` is the authority's origin,
    /// host-supplied for the same reason. `api_key` is whatever device
    /// token the host already holds — often empty at first launch; it is
    /// never persisted by the core ([`Core::init`]'s own doc), and
    /// [`MobileTaskHost::push_api_key`] /
    /// [`MobileTaskHost::rehydrate_api_key`] supply it afterwards.
    #[uniffi::constructor]
    pub async fn init(
        namespace: String,
        base_url: String,
        api_key: String,
    ) -> Result<Arc<Self>, MobileInitError> {
        let empty_key = api_key.is_empty();
        let mut core = Core::init(namespace, api_key)
            .await
            .map_err(|error| MobileInitError::InitFailed {
                detail: error.to_string(),
            })?;
        // `Core::init` holds whatever string it is given — including `""`,
        // which would then be *sent* as a bearer token. An empty key from
        // the host means "no token yet", so map it to the core's explicit
        // no-credential state: `run` answers `no_credential` and reaches no
        // transport, instead of `pull_failed` after a doomed request.
        if empty_key {
            core.clear_api_key();
        }
        let client = reqwest_client();
        let read_transport = ReqwestSyncTransport::new(client.clone(), base_url.clone());
        let write_transport = ReqwestMutationTransport::new(client, base_url);
        Ok(Arc::new(Self {
            inner: tokio::sync::Mutex::new(Inner {
                core,
                read_transport,
                write_transport,
            }),
        }))
    }

    /// The wrapped core's public API version — same value as the free
    /// [`core_api_version`], surfaced on the object so a host holding only
    /// the handle can show it.
    pub async fn api_version(&self) -> u32 {
        self.inner.lock().await.core.api_version()
    }

    /// The mirror's active-item population (ADR-0001's watchline figure) —
    /// the M0 proof screen's number.
    pub async fn active_item_count(&self) -> u32 {
        self.inner.lock().await.core.active_item_count() as u32
    }

    /// The outbound queue's current depth — the "queued" sync-status
    /// figure.
    pub async fn queue_depth(&self) -> u32 {
        self.inner.lock().await.core.queue_depth() as u32
    }

    /// A fresh device token from the person (first entry, or rotation
    /// after a `credential_needed` event). Always resumes a hold — see
    /// [`Core::push_api_key`].
    pub async fn push_api_key(&self, api_key: String) {
        self.inner.lock().await.core.push_api_key(api_key);
    }

    /// The host reloading a token it already had stored (app start), never
    /// resuming a hold — see [`Core::rehydrate_api_key`].
    pub async fn rehydrate_api_key(&self, api_key: String) {
        self.inner.lock().await.core.rehydrate_api_key(api_key);
    }

    /// "Forget token": clears the in-memory credential. Nothing durable to
    /// clean up — the core never persisted it.
    pub async fn clear_api_key(&self) {
        self.inner.lock().await.core.clear_api_key();
    }

    /// Runs one sync cycle against the live transports. `trigger` is the
    /// web protocol's string pair (`"timer"` gates on backoff, anything
    /// else is a deliberate `"user"` gesture); `now_ms` and `jitter_unit`
    /// come from the host clock and RNG, injected here exactly as on the
    /// web side so the core stays deterministic under test.
    pub async fn run(
        &self,
        now_ms: i64,
        trigger: String,
        force_full_sweep: bool,
        jitter_unit: f64,
    ) -> RunOutcome {
        let trigger = match trigger.as_str() {
            "timer" => Trigger::Timer,
            _ => Trigger::User,
        };
        let inner = &mut *self.inner.lock().await;
        let outcome = inner
            .core
            .run(
                &inner.read_transport,
                &inner.write_transport,
                now_ms,
                trigger,
                force_full_sweep,
                jitter_unit,
            )
            .await;
        map_run_outcome(outcome)
    }

    /// Captures `title` — `CaptureActivity`'s whole surface (M1-5/#503).
    /// **Local-first** (#128's own criterion): reaches
    /// [`hummingbird_core::Core::capture`], which durably enqueues via
    /// `SyncCycle::enqueue` and overlays the optimistic [`Item`] before any
    /// transport is ever touched, so the item exists in the local mirror the
    /// instant this returns — sync or no sync, online or offline. Always
    /// captures into `Stage::Triage` with every [`CaptureOptions`] field
    /// absent: raw text only, no capture-meta surface in M1 (say so at every
    /// call site rather than only here). Mints its own seed
    /// ([`mint_mutation_seed`]) since the host supplies only `title`/`now_ms`.
    pub async fn capture(&self, title: String, now_ms: i64) -> Result<String, MobileCaptureError> {
        let seed = mint_mutation_seed("capture", now_ms);
        let mut inner = self.inner.lock().await;
        inner
            .core
            .capture(&seed, title, Stage::Triage, now_ms, CaptureOptions::default())
            .await
            .map_err(|error| MobileCaptureError::CaptureFailed {
                detail: error.to_string(),
            })
    }

    /// Drains queued [`CoreEvent`]s (today: `credential_needed`) — a
    /// pull-based drain, never a host-implemented callback (ADR-0003).
    pub async fn take_events(&self) -> Vec<MobileEvent> {
        self.inner
            .lock()
            .await
            .core
            .take_events()
            .into_iter()
            .map(map_event)
            .collect()
    }

    /// `NowScreen`'s whole read (M1-6/#504): the frontier
    /// ([`hummingbird_core::Core::frontier`]), ordered exactly
    /// [`frontier::by_priority_then_due`] orders it, each item already
    /// decided into a [`NowItemRecord`] — its [`MobileUrgencyBand`]
    /// ([`urgency::compute_urgency`]) and its wire-vocabulary
    /// [`available_actions`] ([`hummingbird_core::decisions::available_actions`]).
    /// See the module header for why this crosses decided records rather
    /// than exposing the decision functions themselves.
    ///
    /// `now` is deadline-shaped (`YYYY-MM-DDTHH:MM`), the host's own local
    /// wall clock already rendered into that shape — the same convention
    /// `urgency.rs`'s module header states for [`urgency::compute_urgency`]:
    /// `hummingbird-core` resolves no civil date to an instant, so Android,
    /// like the web seam, is the reader that does.
    pub async fn now_queue(&self, now: String) -> Vec<NowItemRecord> {
        let inner = self.inner.lock().await;
        let items = inner.core.frontier();
        build_now_queue(&items, &now)
    }

    /// One S11/#109 act (`start`/`complete`/`block`/`cancel`, S11's closed
    /// wire vocabulary — [`ItemAction::parse`]) against `item_id` —
    /// [`hummingbird_core::Core::act`] verbatim, the same seed-minting and
    /// error-wrapping convention [`MobileTaskHost::capture`] already uses.
    /// An unrecognised `action` string answers [`MobileActError::ActFailed`]
    /// rather than panicking — the same "reject before the seam" discipline
    /// [`ItemAction::parse`]'s own doc states, since uniffi crosses the
    /// action as a plain string and Kotlin has no closed enum of its own to
    /// enforce it first.
    pub async fn act(
        &self,
        item_id: String,
        action: String,
        now_ms: i64,
    ) -> Result<(), MobileActError> {
        let Some(parsed) = ItemAction::parse(&action) else {
            return Err(MobileActError::ActFailed {
                detail: format!("unrecognised action: {action}"),
            });
        };
        let seed = mint_mutation_seed("act", now_ms);
        let mut inner = self.inner.lock().await;
        inner
            .core
            .act(&seed, &item_id, parsed, now_ms)
            .await
            .map_err(|error| match error {
                hummingbird_core::ActError::ItemNotFound => MobileActError::ItemNotFound,
                other => MobileActError::ActFailed {
                    detail: other.to_string(),
                },
            })
    }
}

/// One `reqwest::Client` per host, cloned into both transports — connection
/// pooling shared across reads and writes, exactly `TaskHostCore::init`'s
/// arrangement.
fn reqwest_client() -> reqwest::Client {
    reqwest::Client::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_stub_matches_core_api_version() {
        assert_eq!(core_api_version(), hummingbird_core::API_VERSION);
    }

    #[tokio::test]
    async fn init_creates_the_namespace_and_answers_the_proof_pair() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m0-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://authority.example".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        assert_eq!(host.api_version().await, hummingbird_core::API_VERSION);
        assert_eq!(host.active_item_count().await, 0);
        assert_eq!(host.queue_depth().await, 0);
    }

    #[tokio::test]
    async fn run_without_a_credential_reports_no_credential_and_touches_no_network() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m0-ns-2");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            // A base URL that would fail DNS if any request were attempted:
            // `no_credential` short-circuits before the transports.
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let outcome = host.run(1_000, "user".to_string(), false, 0.0).await;
        assert_eq!(outcome.kind, "no_credential");
        assert!(host.take_events().await.is_empty());
    }

    // ------------------------------------------------------- M1-5 (#503)

    /// The exposure is a pass-through and nothing more — the rule itself is
    /// tested in `hummingbird_core::decisions::capture`. Mirrors
    /// `ffi-web::decisions`' identical pin, so the two FFI surfaces are
    /// proven to agree, not just each proven separately.
    #[test]
    fn the_capture_binding_is_the_core_rule_verbatim() {
        for draft in [
            "",
            "   ",
            "\t\n",
            "\u{feff}",
            "buy milk",
            "  buy milk  ",
            "\u{feff}buy milk",
        ] {
            assert_eq!(
                can_submit_capture(draft),
                hummingbird_core::decisions::can_submit_capture(draft),
                "{draft:?} disagreed across the binding",
            );
        }
    }

    /// #128's local-first criterion, proved the same way the M0 test above
    /// proves "no network reached": a base URL that would fail DNS, yet the
    /// capture still succeeds and is durably enqueued (`queue_depth`) — the
    /// mutation exists before any transport is ever reached.
    /// `active_item_count`/`frontier` stay at zero on purpose: a capture is
    /// born into `Stage::Triage`, which `Core::frontier`'s own
    /// Ready/InProgress filter never surfaces — `Core::triage_inbox` is the
    /// query that would show it, not yet exposed to a mobile host (deferred
    /// to the Now-list screen, M1-6).
    #[tokio::test]
    async fn capture_is_durable_before_any_network_is_touched() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m1-5-capture-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let id = host.capture("buy milk".to_string(), 1_000).await.unwrap();
        assert!(!id.is_empty());
        assert_eq!(host.queue_depth().await, 1);
    }

    /// Two captures at the identical `now_ms` must mint distinct ids — the
    /// whole reason [`mint_mutation_seed`] carries its own counter rather
    /// than deriving the seed from `now_ms` alone.
    #[tokio::test]
    async fn two_captures_in_the_same_millisecond_mint_distinct_ids() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m1-5-capture-ns-2");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let a = host.capture("first".to_string(), 5_000).await.unwrap();
        let b = host.capture("second".to_string(), 5_000).await.unwrap();
        assert_ne!(a, b);
        assert_eq!(host.queue_depth().await, 2);
    }

    /// The counter alone cannot carry uniqueness across a process restart
    /// (it starts over), so the seed leads with OS randomness — see
    /// [`mint_mutation_seed`]'s doc for why a repeated id is a *lost*
    /// capture rather than a duplicate one. Held identical `kind` and
    /// `now_ms` here so only the random field can be doing the work; the
    /// counter is deliberately not what this observes.
    #[test]
    fn the_mutation_seed_carries_randomness_not_just_the_counter() {
        let random_field = |seed: String| seed.rsplit(':').next().unwrap().to_string();
        let a = random_field(mint_mutation_seed("capture", 1_000));
        let b = random_field(mint_mutation_seed("capture", 1_000));
        assert_eq!(a.len(), 32, "expected 16 random bytes, hex-encoded");
        assert_ne!(a, b);
    }

    /// `Core::capture` still has no opinion of its own on an empty title
    /// (`hummingbird_core::decisions::capture`'s own doc) — the refusal is
    /// entirely `CaptureActivity`'s job, reached through
    /// [`can_submit_capture`] before this is ever called. Pinned here so
    /// nobody "fixes" that by adding a check inside this method instead.
    #[tokio::test]
    async fn capture_itself_has_no_opinion_on_a_blank_title() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m1-5-capture-ns-3");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        assert!(host.capture("   ".to_string(), 1_000).await.is_ok());
    }

    // ------------------------------------------------------- M1-6 (#504)

    fn item(id: &str, priority: i64, deadline: Option<&str>) -> Item {
        Item {
            id: id.to_string(),
            seq: None,
            title: format!("item {id}"),
            description: None,
            stage: Stage::Ready,
            size: None,
            energy: None,
            context: None,
            priority,
            project_id: None,
            project_pos: None,
            deadline: deadline.map(str::to_string),
            scheduled_date: None,
            source: None,
            source_key: None,
            source_url: None,
            archived_at: None,
            agent: false,
            created_at: 0,
            updated_at: 0,
            version: 0,
        }
    }

    /// The shared-fixture ordering pin: the exact fixture shapes
    /// `hummingbird_core::decisions::frontier`'s own
    /// `ranks_by_priority_label_never_the_raw_wire_number` and
    /// `within_the_same_priority_orders_by_deadline_chronologically` tests
    /// use, proving [`build_now_queue`] — `now_queue`'s pure core — orders
    /// identically to the web frontier's own decision-sink pin, not a
    /// second, independently-drifting copy of the rule.
    #[test]
    fn now_queue_orders_exactly_like_the_shared_frontier_fixtures() {
        let none = item("none", 0, None);
        let urgent = item("urgent", 1, None);
        let low = item("low", 4, None);

        let ids: Vec<String> = build_now_queue(&[none, low, urgent], "2026-08-15T12:00")
            .into_iter()
            .map(|record| record.id)
            .collect();
        assert_eq!(ids, vec!["urgent", "low", "none"]);

        let soon = item("soon", 1, Some("2026-08-15"));
        let later = item("later", 1, Some("2026-08-20"));
        let none_deadline = item("none-deadline", 1, None);

        let ids: Vec<String> =
            build_now_queue(&[none_deadline, later, soon], "2026-08-13T12:00")
                .into_iter()
                .map(|record| record.id)
                .collect();
        assert_eq!(ids, vec!["soon", "later", "none-deadline"]);
    }

    #[test]
    fn now_queue_records_carry_urgency_priority_and_available_actions() {
        let overdue = item("overdue-id", 2, Some("2020-01-01"));

        let records = build_now_queue(&[overdue], "2026-08-15T12:00");

        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.urgency, MobileUrgencyBand::Overdue);
        assert_eq!(record.priority, 2);
        assert_eq!(
            record.available_actions,
            vec!["start", "complete", "block", "cancel"],
        );
    }

    #[test]
    fn now_queue_is_a_pure_function_returning_the_same_order_every_call() {
        let items = vec![item("a", 3, None), item("b", 1, None)];
        assert_eq!(
            build_now_queue(&items, "2026-08-15T12:00"),
            build_now_queue(&items, "2026-08-15T12:00"),
        );
    }

    /// End-to-end proof that [`MobileTaskHost::now_queue`] wires the real
    /// `Core::frontier` read through [`build_now_queue`] — captures two
    /// items, promotes both to `Ready` with `Core::triage` (bypassing the
    /// FFI surface directly, same-crate access to `MobileTaskHost::inner`;
    /// triage itself is out of M1-6's scope) at different priorities, and
    /// asserts `now_queue` returns them in the decided order.
    #[tokio::test]
    async fn now_queue_reads_the_live_frontier_in_priority_order() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m1-6-now-queue-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let low_id = host.capture("low priority".to_string(), 1_000).await.unwrap();
        let urgent_id = host.capture("urgent thing".to_string(), 1_000).await.unwrap();

        {
            let mut inner = host.inner.lock().await;
            inner
                .core
                .triage(
                    "seed-low",
                    &low_id,
                    true,
                    hummingbird_core::TriagePatch {
                        priority: Some(4),
                        ..Default::default()
                    },
                    2_000,
                )
                .await
                .unwrap();
            inner
                .core
                .triage(
                    "seed-urgent",
                    &urgent_id,
                    true,
                    hummingbird_core::TriagePatch {
                        priority: Some(1),
                        ..Default::default()
                    },
                    2_000,
                )
                .await
                .unwrap();
        }

        let queue = host.now_queue("2026-08-15T12:00".to_string()).await;
        assert_eq!(
            queue.iter().map(|record| record.id.clone()).collect::<Vec<_>>(),
            vec![urgent_id, low_id],
        );
    }

    #[tokio::test]
    async fn act_start_moves_a_ready_item_to_in_progress_in_the_local_mirror() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m1-6-act-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let id = host.capture("do it".to_string(), 1_000).await.unwrap();
        {
            let mut inner = host.inner.lock().await;
            inner
                .core
                .triage("seed", &id, true, hummingbird_core::TriagePatch::default(), 2_000)
                .await
                .unwrap();
        }

        host.act(id.clone(), "start".to_string(), 3_000).await.unwrap();

        let queue = host.now_queue("2026-08-15T12:00".to_string()).await;
        let record = queue.iter().find(|r| r.id == id).expect("item still on the frontier");
        assert_eq!(record.available_actions, vec!["complete", "block", "cancel"]);
    }

    #[tokio::test]
    async fn act_refuses_an_unrecognised_action_string() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m1-6-act-ns-2");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let id = host.capture("whatever".to_string(), 1_000).await.unwrap();
        let result = host.act(id, "not-a-real-action".to_string(), 1_000).await;
        assert!(matches!(result, Err(MobileActError::ActFailed { .. })));
    }

    #[tokio::test]
    async fn act_refuses_an_unknown_item_id() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m1-6-act-ns-3");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let result = host.act("no-such-id".to_string(), "start".to_string(), 1_000).await;
        assert!(matches!(result, Err(MobileActError::ItemNotFound)));
    }
}
