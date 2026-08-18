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
//! only, no capture-meta fields (out of scope until a later screen). M2
//! (#141, ADR-0012) adds the notification lane's client side:
//! [`MobileTaskHost::alerts`]/[`MobileTaskHost::alert`] as decided
//! [`AlertRecord`]s, [`MobileTaskHost::ack_alert`] (the Ack gesture — swipe
//! is not one), and [`MobileTaskHost::register_push_target`], the one
//! authority call here that bypasses the sync queue entirely.
//!
//! **Async runs under uniffi's tokio runtime** (`async_runtime = "tokio"`),
//! because `Core::run`'s reqwest transports need a reactor and the host
//! (Kotlin coroutines / Swift concurrency) must never be the thing
//! providing one. Interior state is a `tokio::sync::Mutex` for the same
//! reason: it is held across the cycle's awaits.
//!
//! **Android never calls a per-row decision function** (M1-6/#504) — this
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
//! [`AlertRecord`] is M2's instance of the same rule: it ships `is_live`
//! and `can_ack` as answers, so no Kotlin `dismissedAt == null` test can
//! disagree with ADR-0014's predicate.
//!
//! The rule is *per row*, and **free doors are per gesture**: the cost it
//! guards against is a JNI crossing multiplied by a list, which a gesture
//! taken once does not incur. [`can_submit_capture`] runs once per submit
//! and [`notification_tap_target`] once per notification tap — the latter
//! could not be a record at all, since the tap holds two payload strings
//! and no row to hang a decided answer on.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use hummingbird_core::decisions::{available_actions, frontier, urgency};
use hummingbird_core::storage::FsSnapshotStore;
use hummingbird_core::sync::write::transport::{
    HttpMethod, MutationRequest, MutationTransport,
};
use hummingbird_core::sync::write::ReqwestMutationTransport;
use hummingbird_core::sync::{ReqwestSyncTransport, Trigger};
use hummingbird_core::{CaptureOptions, Core, CoreCycleOutcome, CoreEvent, ItemAction};
use hummingbird_domain::{Alert, CreatePushTarget, Energy, Item, Platform, Size, Stage};

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

/// [`hummingbird_core::decisions::CaptureMetaProblems`], mirrored as a
/// `uniffi::Record` — what is wrong with a draft's two free-text date
/// fields, `None` per field meaning nothing is.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MetaProblems {
    pub deadline: Option<String>,
    pub scheduled_date: Option<String>,
}

/// The third free door on this seam, and the item edit form's first caller
/// (ADR-0027): the same rule the web's capture box and triage form both
/// read, so a malformed date is refused with the same words on every
/// client rather than being sent for the authority to 400 into the
/// dead-letter journal.
///
/// Only the free-text dates can be wrong here — every other editable field
/// is a closed vocabulary the form offers as choices, or the title, which
/// [`can_submit_capture`] already answers for.
#[uniffi::export]
pub fn capture_meta_problems(deadline: &str, scheduled_date: &str) -> MetaProblems {
    let problems = hummingbird_core::decisions::capture::capture_meta_problems(
        deadline,
        scheduled_date,
    );
    MetaProblems {
        deadline: problems.deadline,
        scheduled_date: problems.scheduled_date,
    }
}

/// [`decisions::TapTarget`], mirrored as a `uniffi::Enum` for
/// [`notification_tap_target`] — a second definition rather than an
/// annotation on the core type, for [`MobileUrgencyBand`]'s reason
/// (ADR-0003 keeps `hummingbird-core` binding-agnostic), and mapped
/// exhaustively with no wildcard arm so a third destination cannot reach
/// Kotlin unnoticed.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MobileTapTarget {
    Item { item_id: String },
    Alert,
}

/// Where a tapped notification lands, from the push payload's `source` and
/// `source_key` (ADR-0027) — the second free door on this seam, and the
/// second instance of the per-gesture carve-out the module header states.
///
/// `MainActivity`'s deep-link collector calls this **synchronously**,
/// before navigating. That is the whole reason it is a free function and
/// not a [`MobileTaskHost`] method: a method would take the interior mutex
/// and make the answer async, for a decision that reads no state. A Kotlin
/// `removePrefix("item:")` is banned here exactly as `isBlank()` is banned
/// for capture — the key recipe has one owner in
/// `hummingbird_domain`, and a hand-copy would keep compiling after the
/// recipe moved while silently routing every tap to the wrong place.
///
/// Kotlin passes empty strings for a payload that carried neither field
/// (an older server); that opens alert detail, which is the permanent
/// contract for every alert naming no item.
#[uniffi::export]
pub fn notification_tap_target(source: &str, source_key: &str) -> MobileTapTarget {
    match hummingbird_core::decisions::notification_tap_target(source, source_key) {
        hummingbird_core::decisions::TapTarget::Item { item_id } => {
            MobileTapTarget::Item { item_id }
        }
        hummingbird_core::decisions::TapTarget::Alert => MobileTapTarget::Alert,
    }
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

/// [`MobileTaskHost::ack_alert`] failed. `AlertNotFound` is the one that
/// matters operationally: a push payload names an alert this device has not
/// synced yet, so the ack has no `expected_version` to CAS against (the
/// payload carries none — `server/authority/src/fcm.rs`). The host's answer
/// is to run a cycle and retry, not to invent a version.
#[derive(Debug, uniffi::Error)]
pub enum MobileAlertError {
    AlertNotFound,
    AckFailed { detail: String },
}

impl std::fmt::Display for MobileAlertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobileAlertError::AlertNotFound => write!(f, "alert not found"),
            MobileAlertError::AckFailed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for MobileAlertError {}

/// [`MobileTaskHost::register_push_target`] failed (#141/#139). Split the
/// way the host's retry differs: `Unauthorized` means the device token is
/// wrong or missing and retrying the same call is pointless until a fresh
/// one is pushed, while `RegisterFailed` covers a transport failure or a
/// non-2xx the host should simply retry on next launch — registration is
/// idempotent by the client-supplied id, so a retry is free.
#[derive(Debug, uniffi::Error)]
pub enum MobilePushRegistrationError {
    Unauthorized,
    RegisterFailed { detail: String },
}

impl std::fmt::Display for MobilePushRegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobilePushRegistrationError::Unauthorized => write!(f, "unauthorized"),
            MobilePushRegistrationError::RegisterFailed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for MobilePushRegistrationError {}

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

// ----------------------------------------------------------------- M2 (#141)
// The notification lane's read side. Same asymmetry as the M1-6 section
// above: Kotlin receives decided records, never a predicate to apply.

/// One alert, decided — the row behind the alerts surface and the alert
/// detail screen (ADR-0012). Carries the wire columns the screens render
/// *plus* the two verdicts Kotlin must never re-derive:
///
/// - `is_live` is ADR-0014's three-clause predicate ([`Alert::is_live`])
///   already applied against the caller's clock. A Kotlin
///   `dismissedAt == null` test is the exact bug that predicate exists to
///   prevent — it cannot tell an expired-then-re-raised occurrence from an
///   acked one, and `expires_at` is never written back as a dismissal.
/// - `can_ack` is whether the Ack action should be offered at all. It is
///   `is_live` and nothing further, because ADR-0014's predicate already
///   contains the dismissal clause it is tempting to re-state here:
///   `raised_at > dismissed_at`. Re-stating it as `dismissed_at.is_none()`
///   is the same bug one line up in Kotlin dress — a re-raised occurrence
///   carries the *old* dismissal stamp (ADR-0014 keeps it deliberately;
///   nothing ever clears the column), so the column test hides the Ack on
///   the very row that most needs it. The two fields are kept separate
///   anyway: they answer different questions, and `is_live` is about to
///   grow display uses that have nothing to do with the action.
///   Acking a settled row is legal (the authority treats setting the
///   stored value as a version-preserving no-op) but is not a gesture
///   worth showing.
///
/// `version` rides along because the ack is CAS and the detail screen is
/// where a 409 retry re-reads from — the push payload carries no version.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AlertRecord {
    pub id: String,
    pub source: String,
    pub source_key: String,
    pub subject_key: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub url: Option<String>,
    pub severity: Option<String>,
    pub raised_at: i64,
    pub resolved_at: Option<i64>,
    pub dismissed_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub version: i64,
    pub is_live: bool,
    pub can_ack: bool,
}

fn to_alert_record(alert: &Alert, now_ms: i64) -> AlertRecord {
    let is_live = alert.is_live(now_ms);
    AlertRecord {
        id: alert.id.clone(),
        source: alert.source.clone(),
        source_key: alert.source_key.clone(),
        subject_key: alert.subject_key.clone(),
        title: alert.title.clone(),
        body: alert.body.clone(),
        url: alert.url.clone(),
        severity: alert.severity.clone(),
        raised_at: alert.raised_at,
        resolved_at: alert.resolved_at,
        dismissed_at: alert.dismissed_at,
        expires_at: alert.expires_at,
        version: alert.version,
        is_live,
        can_ack: is_live,
    }
}

// ----------------------------------------------------------- item detail
// ADR-0027's last slice. Mapping only, zero assembly: every answer here was
// decided by `hummingbird_core::Core::item_detail` — see
// `hummingbird_core::item_detail` for what each field means and why.

/// One checklist row. Read-only on this seam: no step mutation crosses it
/// yet, which is why [`hummingbird_core::item_detail::ItemDetail::steps`]
/// is deliberately un-overlaid on the other side.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ItemStepRecord {
    pub id: String,
    pub body: String,
    pub done: bool,
    pub position: i64,
}

/// One open blocker. `title` is `None` for an id this device has not
/// synced — the row is still listed, and the screen renders the bare id
/// rather than dropping a blocker the reader would never learn about.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OpenBlockerRecord {
    pub item_id: String,
    pub title: Option<String>,
}

/// One item, decided — the item screen's whole read (`CONTEXT.md`'s **Item
/// detail**). Carries the wire columns the screen renders plus the verdicts
/// Kotlin must never re-derive: `is_archived`, `is_editable` (Recall's
/// rule, #478) and `available_actions`, which is **empty for an archived
/// row** whatever `stage` still says.
///
/// `live_alert` reuses [`AlertRecord`] rather than restating it: that
/// record already ships `is_live` and `can_ack` as answers, and the item
/// screen offers exactly the same Ack the alert screen does.
///
/// `size` and `energy` cross as their wire vocabulary
/// ([`Size::as_str`]/[`Energy::as_str`]), the same closed-vocabulary
/// discipline the act strings already follow.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ItemDetailRecord {
    pub id: String,
    pub seq: Option<i64>,
    pub title: String,
    pub description: Option<String>,
    pub stage: String,
    pub size: Option<String>,
    pub energy: Option<String>,
    pub context: Option<String>,
    /// `CONTEXT.md`'s **Delegation axis**, the fourth axis alongside size,
    /// energy and context. Read-only here: it is set and cleared
    /// deliberately elsewhere, and `TriagePatch` carries no field for it,
    /// so [`MobileTaskHost::edit_item`] cannot touch it either.
    pub agent: bool,
    pub priority: i64,
    pub project_id: Option<String>,
    /// The project's name, or `None` when this device has not synced the
    /// project row — never a reason to hide `project_id`.
    pub project_name: Option<String>,
    pub deadline: Option<String>,
    pub scheduled_date: Option<String>,
    pub source_url: Option<String>,
    pub updated_at: i64,
    /// CAS target for the edit, exactly as [`AlertRecord::version`] is for
    /// the ack.
    pub version: i64,
    pub steps: Vec<ItemStepRecord>,
    pub open_blockers: Vec<OpenBlockerRecord>,
    pub live_alert: Option<AlertRecord>,
    pub is_archived: bool,
    pub is_editable: bool,
    pub available_actions: Vec<String>,
}

fn to_item_detail_record(
    detail: &hummingbird_core::item_detail::ItemDetail,
    now_ms: i64,
) -> ItemDetailRecord {
    let item = &detail.item;
    ItemDetailRecord {
        id: item.id.clone(),
        seq: item.seq,
        title: item.title.clone(),
        description: item.description.clone(),
        stage: item.stage.as_str().to_string(),
        size: item.size.map(|size| size.as_str().to_string()),
        energy: item.energy.map(|energy| energy.as_str().to_string()),
        context: item.context.clone(),
        agent: item.agent,
        priority: item.priority,
        project_id: detail.project.as_ref().map(|project| project.id.clone()),
        project_name: detail.project.as_ref().and_then(|project| project.name.clone()),
        deadline: item.deadline.clone(),
        scheduled_date: item.scheduled_date.clone(),
        source_url: item.source_url.clone(),
        updated_at: item.updated_at,
        version: item.version,
        steps: detail
            .steps
            .iter()
            .map(|step| ItemStepRecord {
                id: step.id.clone(),
                body: step.body.clone(),
                done: step.done,
                position: step.position,
            })
            .collect(),
        open_blockers: detail
            .open_blockers
            .iter()
            .map(|blocker| OpenBlockerRecord {
                item_id: blocker.item_id.clone(),
                title: blocker.title.clone(),
            })
            .collect(),
        live_alert: detail
            .live_alert
            .as_ref()
            .map(|alert| to_alert_record(alert, now_ms)),
        is_archived: detail.is_archived,
        is_editable: detail.is_editable,
        available_actions: detail
            .available_actions
            .iter()
            .map(|action| action.as_str().to_string())
            .collect(),
    }
}

/// What an edit does to one nullable field.
///
/// [`hummingbird_core::TriagePatch`] carries these as double-`Option`s —
/// outer `None` "leave it alone", `Some(None)` "clear it", `Some(Some(v))`
/// "set it" — and a double-`Option` does not cross UniFFI. Collapsing it to
/// a single `Option` would lose the distinction that matters most to an
/// editor: **"this deadline is now gone" is not the same as "I did not
/// touch the deadline"**, and a single `Option` can only ever add.
///
/// Every clearable field crosses as a string, including `size` and
/// `energy` in their wire vocabulary — a value the closed vocabulary does
/// not contain is refused at the seam, never sent.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum FieldPatch {
    Untouched,
    Clear,
    Set { value: String },
}

impl FieldPatch {
    fn to_text(&self) -> Option<Option<String>> {
        match self {
            FieldPatch::Untouched => None,
            FieldPatch::Clear => Some(None),
            FieldPatch::Set { value } => Some(Some(value.clone())),
        }
    }

    /// The same three answers over a closed vocabulary. `Err` is an
    /// unrecognised word — rejected before the seam, like every other
    /// vocabulary string crossing here.
    fn to_vocabulary<T>(&self, parse: impl Fn(&str) -> Option<T>) -> Result<Option<Option<T>>, String> {
        match self {
            FieldPatch::Untouched => Ok(None),
            FieldPatch::Clear => Ok(Some(None)),
            FieldPatch::Set { value } => parse(value)
                .map(|parsed| Some(Some(parsed)))
                .ok_or_else(|| format!("unrecognised value: {value}")),
        }
    }
}

/// One edit of an item's fields, as the item screen's edit mode collects
/// it. The mirror of [`hummingbird_core::TriagePatch`] across the seam —
/// minus `stage`, which is [`MobileTaskHost::act`]'s, and minus the
/// promotion, which [`MobileTaskHost::edit_item`] pins `false`.
///
/// `title` and `priority` are `NOT NULL` columns and so are plain
/// `Option`s: absent means untouched, and neither can be cleared. That is
/// the same asymmetry the authority enforces with a 400.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ItemEdit {
    pub title: Option<String>,
    pub priority: Option<i64>,
    pub description: FieldPatch,
    pub size: FieldPatch,
    pub energy: FieldPatch,
    pub context: FieldPatch,
    pub project_id: FieldPatch,
    pub deadline: FieldPatch,
    pub scheduled_date: FieldPatch,
}

/// [`MobileTaskHost::edit_item`] failed. `ItemNotFound` covers the archived
/// case too, and deliberately: the core's edit path reads the *live* view,
/// so history is unreachable from here by construction rather than by a
/// seam-side check Kotlin could be tempted to duplicate.
#[derive(Debug, uniffi::Error)]
pub enum MobileEditError {
    ItemNotFound,
    EditFailed { detail: String },
}

impl std::fmt::Display for MobileEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobileEditError::ItemNotFound => write!(f, "item not found"),
            MobileEditError::EditFailed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for MobileEditError {}

struct Inner {
    core: Core<FsSnapshotStore, FsSnapshotStore>,
    read_transport: ReqwestSyncTransport,
    write_transport: ReqwestMutationTransport,
    /// The device token, shadowing the one `Core` holds in memory (it
    /// exposes no getter, and never persists it — `Core::init`'s doc).
    ///
    /// It exists because [`MobileTaskHost::register_push_target`] is the
    /// one authority call here that does **not** go through the sync cycle:
    /// `push_targets` carry no `version` and never delta-pull (`api.rs`'s
    /// `ChangesResponse` doc), so registration is a direct one-shot POST on
    /// the write transport rather than a durable queue entry — and a direct
    /// send needs the bearer token in hand.
    ///
    /// Kept in lockstep with the core's copy under the same lock, by the
    /// same three methods that set it there ([`MobileTaskHost::push_api_key`],
    /// `rehydrate_api_key`, `clear_api_key`), so there is no second
    /// credential lifecycle to reason about. Memory only, like the core's.
    api_key: Option<String>,
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
        let shadow_key = (!empty_key).then(|| api_key.clone());
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
                api_key: shadow_key,
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
        let mut inner = self.inner.lock().await;
        inner.api_key = Some(api_key.clone());
        inner.core.push_api_key(api_key);
    }

    /// The host reloading a token it already had stored (app start), never
    /// resuming a hold — see [`Core::rehydrate_api_key`].
    pub async fn rehydrate_api_key(&self, api_key: String) {
        let mut inner = self.inner.lock().await;
        inner.api_key = Some(api_key.clone());
        inner.core.rehydrate_api_key(api_key);
    }

    /// "Forget token": clears the in-memory credential. Nothing durable to
    /// clean up — the core never persisted it.
    pub async fn clear_api_key(&self) {
        let mut inner = self.inner.lock().await;
        inner.api_key = None;
        inner.core.clear_api_key();
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
    ///
    /// Routed through [`hummingbird_core::Core::act_acking_alert`] rather
    /// than `Core::act` since ADR-0027: completing or cancelling an item
    /// acks the alert about it, and `CONTEXT.md`'s amended **Ack** makes
    /// that a property of the *gesture*, not of the screen it was taken
    /// from — so the Now list's checkmark silences the ring exactly as the
    /// item screen's does. With nothing ringing, or on an action that
    /// settles nothing, the composition is `Core::act` unchanged.
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
            .act_acking_alert(&seed, &item_id, parsed, now_ms)
            .await
            .map_err(|error| match error {
                hummingbird_core::ActError::ItemNotFound => MobileActError::ItemNotFound,
                other => MobileActError::ActFailed {
                    detail: other.to_string(),
                },
            })
    }

    /// One item's whole read (ADR-0027) —
    /// [`hummingbird_core::Core::item_detail`] verbatim, mapped. `None`
    /// means this device has not synced the item, which is a real state on
    /// a deep link from a push: the payload can arrive before the cycle
    /// carrying the row. The host runs a cycle and re-reads, exactly as
    /// [`MobileTaskHost::alert`]'s callers already do.
    ///
    /// Archived items answer here rather than `None` — history stays
    /// readable, and the record says so with `is_archived`/`is_editable`.
    pub async fn item_detail(&self, item_id: String, now_ms: i64) -> Option<ItemDetailRecord> {
        self.inner
            .lock()
            .await
            .core
            .item_detail(&item_id, now_ms)
            .map(|detail| to_item_detail_record(&detail, now_ms))
    }

    /// Edits an item's fields —
    /// [`hummingbird_core::Core::triage`] with `promote_to_ready` pinned
    /// **`false`** at this seam, because promotion is triage's gesture and
    /// this is not triage: `CONTEXT.md` reserves that word for promoting a
    /// captured item, and an edit made from item detail must never
    /// silently move an item's stage.
    ///
    /// One CAS `PATCH` for the whole edit, never one per field — see the
    /// core method's own doc for why fewer conflict surfaces is the point.
    /// Only fields the caller actually touched ride on it, and a cleared
    /// field is sent as an explicit `null` rather than omitted.
    pub async fn edit_item(
        &self,
        item_id: String,
        edit: ItemEdit,
        now_ms: i64,
    ) -> Result<(), MobileEditError> {
        let patch = hummingbird_core::TriagePatch {
            title: edit.title.clone(),
            priority: edit.priority,
            description: edit.description.to_text(),
            size: edit
                .size
                .to_vocabulary(Size::parse)
                .map_err(|detail| MobileEditError::EditFailed { detail })?,
            energy: edit
                .energy
                .to_vocabulary(Energy::parse)
                .map_err(|detail| MobileEditError::EditFailed { detail })?,
            context: edit.context.to_text(),
            project_id: edit.project_id.to_text(),
            deadline: edit.deadline.to_text(),
            scheduled_date: edit.scheduled_date.to_text(),
        };
        let seed = mint_mutation_seed("edit", now_ms);
        let mut inner = self.inner.lock().await;
        inner
            .core
            .triage(&seed, &item_id, false, patch, now_ms)
            .await
            .map_err(|error| match error {
                hummingbird_core::ActError::ItemNotFound => MobileEditError::ItemNotFound,
                other => MobileEditError::EditFailed {
                    detail: other.to_string(),
                },
            })
    }

    /// The alerts surface's whole read (M2/#141): every live alert across
    /// every source, newest raise first —
    /// [`hummingbird_core::Core::live_alerts`] verbatim, each row decided
    /// into an [`AlertRecord`]. `now_ms` is the host's wall clock in
    /// milliseconds (unlike [`MobileTaskHost::now_queue`]'s civil-time
    /// `now`: alert liveness is instants throughout, no civil date to
    /// resolve).
    pub async fn alerts(&self, now_ms: i64) -> Vec<AlertRecord> {
        self.inner
            .lock()
            .await
            .core
            .live_alerts(now_ms)
            .iter()
            .map(|alert| to_alert_record(alert, now_ms))
            .collect()
    }

    /// One alert by id, live or not — the alert-detail screen's read, and
    /// the read a notification's deep link lands on. Deliberately not
    /// liveness-filtered ([`hummingbird_core::Core::alert`]'s own doc): the
    /// row the human just acked must still render. `None` means this device
    /// has not synced that alert — which is a real state on a deep link
    /// from a push, since the payload can arrive before the cycle that
    /// carries the row.
    pub async fn alert(&self, alert_id: String, now_ms: i64) -> Option<AlertRecord> {
        self.inner
            .lock()
            .await
            .core
            .alert(&alert_id)
            .map(|alert| to_alert_record(&alert, now_ms))
    }

    /// Acks an alert (ADR-0012's Ack action): sync-then-CAS —
    /// [`hummingbird_core::Core::alert`] for the `expected_version` the
    /// push payload does not carry, then
    /// [`hummingbird_core::Core::dismiss_alert`] to enqueue the durable
    /// `PATCH /api/alerts/:id`. Durable before it returns, like every
    /// mutation here, so an ack tapped in the notification shade survives
    /// the process being killed a moment later.
    ///
    /// **Swipe must never reach this.** ADR-0012 is explicit that swiping a
    /// notification away is not a gesture — the delete-intent does nothing
    /// to the alert. Only the Ack action calls this.
    ///
    /// A row this device has not synced answers
    /// [`MobileAlertError::AlertNotFound`]; the host runs a cycle and
    /// retries rather than inventing a version.
    pub async fn ack_alert(&self, alert_id: String, now_ms: i64) -> Result<(), MobileAlertError> {
        let seed = mint_mutation_seed("ack", now_ms);
        let mut inner = self.inner.lock().await;
        let Some(current) = inner.core.alert(&alert_id) else {
            return Err(MobileAlertError::AlertNotFound);
        };
        inner
            .core
            .dismiss_alert(&seed, &current, Some(now_ms), now_ms)
            .await
            .map_err(|error| MobileAlertError::AckFailed {
                detail: error.to_string(),
            })
    }

    /// Registers this install for push (#139/#141): `POST /api/push_targets`,
    /// `device` scope, idempotent by the client-supplied `id`.
    ///
    /// **`id` is a device *slot*, not a token id** — mint one stable value
    /// per install, persist it, and re-send that same id from
    /// `onNewToken`. A replay with a changed `fcm_token` adopts the new
    /// token and clears `revoked_at`; a fresh id every call would instead
    /// accumulate dead slots that each get a copy of every alert.
    ///
    /// Sent directly on the write transport rather than through the sync
    /// queue: `push_targets` are server-side machinery with no `version`
    /// and no delta-pull, so they are not a mirror entity and a queue entry
    /// for one would have nothing to rebase against. Idempotency is what
    /// makes that safe — the host simply retries on next launch.
    pub async fn register_push_target(
        &self,
        id: String,
        name: String,
        fcm_token: String,
    ) -> Result<(), MobilePushRegistrationError> {
        let inner = self.inner.lock().await;
        let Some(api_key) = inner.api_key.clone() else {
            return Err(MobilePushRegistrationError::Unauthorized);
        };
        let body = serde_json::to_string(&CreatePushTarget {
            id,
            name,
            platform: Platform::Android,
            fcm_token,
        })
        .expect("CreatePushTarget always serializes");
        let response = inner
            .write_transport
            .send(
                &api_key,
                MutationRequest {
                    method: HttpMethod::Post,
                    path: "/api/push_targets".to_string(),
                    body,
                },
            )
            .await
            .map_err(|error| MobilePushRegistrationError::RegisterFailed {
                detail: error.to_string(),
            })?;
        match response.status {
            200..=299 => Ok(()),
            401 | 403 => Err(MobilePushRegistrationError::Unauthorized),
            status => Err(MobilePushRegistrationError::RegisterFailed {
                detail: format!("push target registration failed: {status} {}", response.body),
            }),
        }
    }
}

/// One `reqwest::Client` per host, cloned into both transports — connection
/// pooling shared across reads and writes, exactly `TaskHostCore::init`'s
/// arrangement.
fn reqwest_client() -> reqwest::Client {
    reqwest::Client::new()
}

// ------------------------------------------------------------- M4 (#538)
// The skills runner lane: `hummingbird_core::decisions::skills`, exposed as
// **applied results only**.
//
// Kotlin is never handed a `SkillLine`. It reports what happened to its own
// socket — a line arrived, the request never resolved, the response was a
// 401, the stream ended — and receives the new state. Terminality, ordering,
// heartbeat collapse and decline wording all stay in the core, on every
// platform at once; `SkillsLaneIsolationTest.kt` pins that no Kotlin file in
// the lane parses a runner line or spells a decline sentence.
//
// **Why a per-event door is allowed here.** This module's header states the
// rule: Kotlin never calls a *per-row* decision function, because the cost
// guarded against is a JNI crossing multiplied by a list. These are
// per-event on a stream that emits a line every few seconds at most (the
// runner's own heartbeat is 20s), and the alternative — a Kotlin copy of the
// reducer — is what ADR-0025 forbids outright. Same carve-out
// [`can_submit_capture`] took per submit and [`notification_tap_target`] per
// tap.
//
// **`answered` is observed at the response, never inferred from the
// terminal** — the web's `route-run.ts` states the same rule and reads it
// off whether `fetch` resolved. So [`grill_turn_response_failed`] and
// [`grill_turn_stream_ended`] carry `answered: true` (a response *did*
// arrive) while [`grill_turn_no_token`] and [`grill_turn_transport_failed`]
// carry `false`. A decline a backend answered is not evidence any backend
// is unreachable, and nothing downstream can recover the difference from
// the prose (`decline.rs` forbids matching on it).
//
// Both doors land now — the `grill_turn_*` family and the `skill_run_*`
// twins — even though nothing on Android calls the microtask one until
// #539: the binding surface is designed once, and #539 gets a transport and
// a reducer already proven together rather than half a seam. Every function
// here is covered by a test in this crate, so `dead_code` never fires and
// the door is not merely declared.

use hummingbird_core::decisions::skills;

/// [`hummingbird_domain::GrillVerdict`], mirrored as a `uniffi::Enum` for
/// the same reason [`MobileUrgencyBand`] is: `hummingbird-core` and
/// `hummingbird-domain` stay binding-agnostic (ADR-0003). An enum rather
/// than the wire string, so the two maps below are exhaustive and a third
/// verdict fails this crate's build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileGrillVerdict {
    Resolved,
    FogRemains,
}

fn map_verdict(verdict: hummingbird_domain::GrillVerdict) -> MobileGrillVerdict {
    match verdict {
        hummingbird_domain::GrillVerdict::Resolved => MobileGrillVerdict::Resolved,
        hummingbird_domain::GrillVerdict::FogRemains => MobileGrillVerdict::FogRemains,
    }
}

fn unmap_verdict(verdict: MobileGrillVerdict) -> hummingbird_domain::GrillVerdict {
    match verdict {
        MobileGrillVerdict::Resolved => hummingbird_domain::GrillVerdict::Resolved,
        MobileGrillVerdict::FogRemains => hummingbird_domain::GrillVerdict::FogRemains,
    }
}

/// `grill-me`'s question turn (ADR-0023) — 2-4 short `choices`, and free
/// text is always still a valid answer regardless of what they list.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileGrillQuestion {
    pub prompt: String,
    pub recommended_answer: String,
    pub choices: Vec<String>,
}

/// `grill-me`'s terminal proposal. **`patch_json` is opaque** — the raw
/// object text, carried whole to `Core::complete_grill`'s `applied_patch`
/// and never read into on this side of the boundary. A JSON string rather
/// than a map because uniffi has no `serde_json::Value`, and because a
/// typed mirror here would be a second schema for a field whose whole
/// contract is that nobody parses it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileGrillProposal {
    pub summary: String,
    pub verdict: MobileGrillVerdict,
    pub patch_json: String,
}

/// One completed round, threaded back on the next request — `grill-me` is
/// stateless and every request carries the whole conversation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileGrillTurn {
    pub question: MobileGrillQuestion,
    pub answer: String,
}

/// [`skills::GrillTurnState`], mirrored. The narration field is
/// **`messages`**, plural, and the line's own `message` never crosses at
/// all: a uniffi field named `message` generates a Kotlin `val message`
/// that collides with `kotlin.Exception.message` (see [`MobileInitError`]),
/// and this is the record that would have hit it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MobileGrillTurnState {
    Idle,
    Asking {
        messages: Vec<String>,
    },
    Question {
        messages: Vec<String>,
        question: MobileGrillQuestion,
        backend: Option<String>,
        model: Option<String>,
    },
    Proposal {
        messages: Vec<String>,
        proposal: MobileGrillProposal,
        backend: Option<String>,
        model: Option<String>,
    },
    Declined {
        messages: Vec<String>,
        reason: String,
        backend: Option<String>,
        model: Option<String>,
        answered: bool,
    },
}

/// [`skills::SkillRunState`], mirrored — `microtask`'s four phases.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MobileSkillRunState {
    Idle,
    Running {
        messages: Vec<String>,
    },
    Done {
        messages: Vec<String>,
        note: String,
        backend: Option<String>,
        model: Option<String>,
    },
    Declined {
        messages: Vec<String>,
        reason: String,
        backend: Option<String>,
        model: Option<String>,
        answered: bool,
    },
}

fn map_question(question: skills::GrillQuestion) -> MobileGrillQuestion {
    MobileGrillQuestion {
        prompt: question.prompt,
        recommended_answer: question.recommended_answer,
        choices: question.choices,
    }
}

fn unmap_question(question: MobileGrillQuestion) -> skills::GrillQuestion {
    skills::GrillQuestion {
        prompt: question.prompt,
        recommended_answer: question.recommended_answer,
        choices: question.choices,
    }
}

fn map_proposal(proposal: skills::GrillProposal) -> MobileGrillProposal {
    MobileGrillProposal {
        summary: proposal.summary,
        verdict: map_verdict(proposal.verdict),
        patch_json: serde_json::Value::Object(proposal.patch).to_string(),
    }
}

fn unmap_proposal(proposal: MobileGrillProposal) -> skills::GrillProposal {
    skills::GrillProposal {
        summary: proposal.summary,
        verdict: unmap_verdict(proposal.verdict),
        // Only reachable if a caller fabricated a state rather than passing
        // back one of ours; an empty patch is the reading that changes
        // nothing, which is the safe one for a value only ever carried.
        patch: serde_json::from_str(&proposal.patch_json).unwrap_or_default(),
    }
}

fn map_turn(turn: MobileGrillTurn) -> skills::GrillTurn {
    skills::GrillTurn { question: unmap_question(turn.question), answer: turn.answer }
}

fn map_grill_state(state: skills::GrillTurnState) -> MobileGrillTurnState {
    match state {
        skills::GrillTurnState::Idle => MobileGrillTurnState::Idle,
        skills::GrillTurnState::Asking { messages } => MobileGrillTurnState::Asking { messages },
        skills::GrillTurnState::Question { messages, question, backend, model } => {
            MobileGrillTurnState::Question {
                messages,
                question: map_question(question),
                backend,
                model,
            }
        }
        skills::GrillTurnState::Proposal { messages, proposal, backend, model } => {
            MobileGrillTurnState::Proposal {
                messages,
                proposal: map_proposal(proposal),
                backend,
                model,
            }
        }
        skills::GrillTurnState::Declined { messages, reason, backend, model, answered } => {
            MobileGrillTurnState::Declined { messages, reason, backend, model, answered }
        }
    }
}

fn unmap_grill_state(state: MobileGrillTurnState) -> skills::GrillTurnState {
    match state {
        MobileGrillTurnState::Idle => skills::GrillTurnState::Idle,
        MobileGrillTurnState::Asking { messages } => skills::GrillTurnState::Asking { messages },
        MobileGrillTurnState::Question { messages, question, backend, model } => {
            skills::GrillTurnState::Question {
                messages,
                question: unmap_question(question),
                backend,
                model,
            }
        }
        MobileGrillTurnState::Proposal { messages, proposal, backend, model } => {
            skills::GrillTurnState::Proposal {
                messages,
                proposal: unmap_proposal(proposal),
                backend,
                model,
            }
        }
        MobileGrillTurnState::Declined { messages, reason, backend, model, answered } => {
            skills::GrillTurnState::Declined { messages, reason, backend, model, answered }
        }
    }
}

fn map_run_state(state: skills::SkillRunState) -> MobileSkillRunState {
    match state {
        skills::SkillRunState::Idle => MobileSkillRunState::Idle,
        skills::SkillRunState::Running { messages } => MobileSkillRunState::Running { messages },
        skills::SkillRunState::Done { messages, note, backend, model } => {
            MobileSkillRunState::Done { messages, note, backend, model }
        }
        skills::SkillRunState::Declined { messages, reason, backend, model, answered } => {
            MobileSkillRunState::Declined { messages, reason, backend, model, answered }
        }
    }
}

fn unmap_run_state(state: MobileSkillRunState) -> skills::SkillRunState {
    match state {
        MobileSkillRunState::Idle => skills::SkillRunState::Idle,
        MobileSkillRunState::Running { messages } => skills::SkillRunState::Running { messages },
        MobileSkillRunState::Done { messages, note, backend, model } => {
            skills::SkillRunState::Done { messages, note, backend, model }
        }
        MobileSkillRunState::Declined { messages, reason, backend, model, answered } => {
            skills::SkillRunState::Declined { messages, reason, backend, model, answered }
        }
    }
}

/// The four transport reports, as core events. `answered` is set from which
/// report this is, never from the terminal's prose — see the section header.
fn no_token_event() -> skills::SkillEvent {
    skills::SkillEvent::Failed {
        error: skills::NO_TOKEN.to_string(),
        backend: None,
        model: None,
        answered: false,
    }
}

fn transport_failed_event(detail: &str) -> skills::SkillEvent {
    skills::SkillEvent::Failed {
        error: skills::decline_for_transport(detail),
        backend: None,
        model: None,
        answered: false,
    }
}

fn response_failed_event(status: u16) -> skills::SkillEvent {
    skills::SkillEvent::Failed {
        error: skills::decline_for_response(status),
        backend: None,
        model: None,
        answered: true,
    }
}

fn stream_ended_event() -> skills::SkillEvent {
    skills::SkillEvent::Failed {
        error: skills::NO_TERMINAL_LINE.to_string(),
        backend: None,
        model: None,
        answered: true,
    }
}

/// A line off the socket, classified and reduced in one crossing. A
/// terminal `failed` line carries `answered: true` — it arrived over a
/// response that resolved.
fn line_event(line: &str) -> skills::SkillEvent {
    match skills::classify_line(line) {
        skills::SkillLine::Failed { error, backend, model } => {
            skills::SkillEvent::Failed { error, backend, model, answered: true }
        }
        other => skills::SkillEvent::from(other),
    }
}

// ---- the Grill turn door (the #538 probe's own lane)

#[uniffi::export]
pub fn grill_turn_idle() -> MobileGrillTurnState {
    map_grill_state(skills::grill::IDLE)
}

/// The tap. A second one while already asking leaves the state untouched —
/// the duplicate-tap rule lives in the core's reducer, so no Kotlin
/// `isRunning` guard is the only thing holding it.
#[uniffi::export]
pub fn grill_turn_started(state: MobileGrillTurnState) -> MobileGrillTurnState {
    reduce_grill(state, skills::SkillEvent::Started)
}

/// One NDJSON line. Classification and the heartbeat collapse are the
/// core's; an unreadable line is dropped and is **not** terminal, so a
/// stream that emits garbage mid-flight has not ended.
#[uniffi::export]
pub fn grill_turn_line(state: MobileGrillTurnState, line: String) -> MobileGrillTurnState {
    reduce_grill(state, line_event(&line))
}

#[uniffi::export]
pub fn grill_turn_no_token(state: MobileGrillTurnState) -> MobileGrillTurnState {
    reduce_grill(state, no_token_event())
}

#[uniffi::export]
pub fn grill_turn_transport_failed(
    state: MobileGrillTurnState,
    detail: String,
) -> MobileGrillTurnState {
    reduce_grill(state, transport_failed_event(&detail))
}

#[uniffi::export]
pub fn grill_turn_response_failed(
    state: MobileGrillTurnState,
    status: u16,
) -> MobileGrillTurnState {
    reduce_grill(state, response_failed_event(status))
}

/// The socket closed. If the turn is still asking, that is a run with no
/// terminal line; if it already settled, this is a no-op — which is why the
/// transport can call it unconditionally at the end of every stream.
#[uniffi::export]
pub fn grill_turn_stream_ended(state: MobileGrillTurnState) -> MobileGrillTurnState {
    reduce_grill(state, stream_ended_event())
}

fn reduce_grill(state: MobileGrillTurnState, event: skills::SkillEvent) -> MobileGrillTurnState {
    map_grill_state(skills::reduce_grill_turn(&unmap_grill_state(state), &event))
}

/// The request body, byte for byte — pinned across Rust, TypeScript and
/// Kotlin by `client/core/tests/fixtures/skills-run-bodies.json`. The
/// transport posts this string verbatim and builds nothing of its own.
#[uniffi::export]
pub fn grill_run_body(reference: String, turns: Vec<MobileGrillTurn>) -> String {
    let turns: Vec<skills::GrillTurn> = turns.into_iter().map(map_turn).collect();
    skills::grill_run_body(&reference, &turns)
}

/// The plain-text transcript `Core::complete_grill` carries (ADR-0023
/// decision 2) — #539's caller, landed with the rest of the door.
#[uniffi::export]
pub fn format_grill_transcript(turns: Vec<MobileGrillTurn>) -> String {
    let turns: Vec<skills::GrillTurn> = turns.into_iter().map(map_turn).collect();
    skills::format_grill_transcript(&turns)
}

// ---- the microtask run door (#539's, landed now so the shape is proven)

#[uniffi::export]
pub fn skill_run_idle() -> MobileSkillRunState {
    map_run_state(skills::run::IDLE)
}

#[uniffi::export]
pub fn skill_run_started(state: MobileSkillRunState) -> MobileSkillRunState {
    reduce_run_state(state, skills::SkillEvent::Started)
}

#[uniffi::export]
pub fn skill_run_line(state: MobileSkillRunState, line: String) -> MobileSkillRunState {
    reduce_run_state(state, line_event(&line))
}

#[uniffi::export]
pub fn skill_run_no_token(state: MobileSkillRunState) -> MobileSkillRunState {
    reduce_run_state(state, no_token_event())
}

#[uniffi::export]
pub fn skill_run_transport_failed(
    state: MobileSkillRunState,
    detail: String,
) -> MobileSkillRunState {
    reduce_run_state(state, transport_failed_event(&detail))
}

#[uniffi::export]
pub fn skill_run_response_failed(state: MobileSkillRunState, status: u16) -> MobileSkillRunState {
    reduce_run_state(state, response_failed_event(status))
}

#[uniffi::export]
pub fn skill_run_stream_ended(state: MobileSkillRunState) -> MobileSkillRunState {
    reduce_run_state(state, stream_ended_event())
}

/// The `backend · model` stamp, or `None` when the envelope named no
/// backend. **There is no default name to fall back to** — the whole reason
/// #273's "not hardcoded at the render site" survives the port to a second
/// client is that no provider name exists anywhere in this lane to inherit.
#[uniffi::export]
pub fn skill_run_stamp_label(state: MobileSkillRunState) -> Option<String> {
    skills::stamp_label(&unmap_run_state(state))
}

fn reduce_run_state(state: MobileSkillRunState, event: skills::SkillEvent) -> MobileSkillRunState {
    map_run_state(skills::reduce_run(&unmap_run_state(state), &event))
}

/// The microtask body, byte-pinned by the same shared fixture. `replace` is
/// present-and-`true` or absent; `grain` and `model` are omitted when unset,
/// so the runner's defaults stay the defaults.
#[uniffi::export]
pub fn microtask_run_body(
    item_id: String,
    replace: bool,
    grain: Option<i64>,
    model: Option<String>,
) -> String {
    skills::microtask_run_body(&skills::MicrotaskRunInput { item_id, replace, grain, model })
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

    /// The tap-target binding, pinned the same way: the mapping is the
    /// only place the seam enum and the core enum may drift, so both arms
    /// are crossed here against the real key recipe.
    #[test]
    fn the_tap_target_binding_is_the_core_rule_verbatim() {
        let key = hummingbird_domain::item_threshold_v1_key("item-42");
        assert_eq!(
            notification_tap_target(hummingbird_domain::ITEM_THRESHOLD_V1, &key),
            MobileTapTarget::Item { item_id: "item-42".into() }
        );
        for (source, source_key) in [
            (hummingbird_domain::ITEM_THRESHOLD_V1, "not-an-item-key"),
            ("city-waste/v2", key.as_str()),
            ("", ""),
        ] {
            assert_eq!(
                notification_tap_target(source, source_key),
                MobileTapTarget::Alert,
                "({source:?}, {source_key:?}) should open the alert",
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

    // ------------------------------------------------------- M2 (#141)

    fn alert(id: &str, raised_at: i64) -> Alert {
        Alert {
            id: id.to_string(),
            source: "race/v1".to_string(),
            source_key: format!("occurrence:{id}"),
            subject_key: None,
            title: format!("alert {id}"),
            body: Some("body".to_string()),
            url: Some("https://example.invalid/a".to_string()),
            severity: Some("high".to_string()),
            raised_at,
            resolved_at: None,
            dismissed_at: None,
            expires_at: None,
            version: 3,
        }
    }

    async fn host_at(namespace: &str, api_key: &str) -> Arc<MobileTaskHost> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(namespace);
        // The tempdir outlives the host only within one test body; leaking
        // it here keeps the durable namespace alive for the whole test
        // without threading the guard through every call site.
        std::mem::forget(dir);
        MobileTaskHost::init(
            path.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            api_key.to_string(),
        )
        .await
        .unwrap()
    }

    /// The two verdicts this record exists to carry. A settled row is not
    /// live and cannot be acked; the same row under an earlier clock is
    /// both — which is exactly what a Kotlin `dismissedAt == null` test
    /// would get wrong for the expiry case below.
    #[test]
    fn the_record_decides_liveness_and_ackability_at_the_callers_clock() {
        let live = to_alert_record(&alert("a-1", 500), 1_000);
        assert!(live.is_live);
        assert!(live.can_ack);
        assert_eq!(live.version, 3, "the CAS version rides along for the retry");

        let mut expiring = alert("a-2", 500);
        expiring.expires_at = Some(2_000);
        assert!(to_alert_record(&expiring, 1_500).is_live);
        let expired = to_alert_record(&expiring, 3_000);
        assert!(!expired.is_live, "ADR-0014's expiry clause, not a column test");
        assert!(!expired.can_ack);
        assert_eq!(
            expired.dismissed_at, None,
            "expiry is never written back as a dismissal — the column stays clear"
        );

        let mut acked = alert("a-3", 500);
        acked.dismissed_at = Some(600);
        let acked = to_alert_record(&acked, 1_000);
        assert!(!acked.is_live);
        assert!(!acked.can_ack, "no second Ack action on an already-acked row");
    }

    /// The case a `dismissed_at` column test cannot see: the same row,
    /// acked once, then raised again by its source (ADR-0014's lifecycle
    /// axis — a state source re-enters live on the row it already has, and
    /// the old dismissal stamp is never cleared). It is live, so the Ack
    /// must be on offer; otherwise the new occurrence renders as one the
    /// human has already dealt with and can never be settled from a phone.
    #[test]
    fn a_re_raised_occurrence_carries_its_old_dismissal_and_is_still_ackable() {
        let mut re_raised = alert("a-4", 900);
        re_raised.dismissed_at = Some(600);

        let record = to_alert_record(&re_raised, 1_000);
        assert!(record.is_live, "raised since the dismissal — ADR-0014's clause");
        assert!(record.can_ack, "a live alert is ackable, whatever it settled as before");
        assert_eq!(
            record.dismissed_at,
            Some(600),
            "the stamp stays on the wire; it is history, not a verdict"
        );

        // The resolution half of the same axis (ADR-0014 corrected the
        // re-raise path for resolution as well as dismissal).
        let mut recovered = alert("a-5", 900);
        recovered.resolved_at = Some(700);
        assert!(to_alert_record(&recovered, 1_000).can_ack);
    }

    #[tokio::test]
    async fn a_device_that_has_synced_nothing_answers_the_alert_reads_empty() {
        let host = host_at("m2-empty-ns", "").await;
        assert!(host.alerts(1_000).await.is_empty());
        assert_eq!(host.alert("a-1".to_string(), 1_000).await, None);
    }

    /// The deep-link race: a push names an alert whose row has not been
    /// pulled yet. The ack refuses rather than inventing an
    /// `expected_version` — the payload carries none.
    #[tokio::test]
    async fn acking_an_unsynced_alert_refuses_instead_of_guessing_a_version() {
        let host = host_at("m2-ack-unknown-ns", "device-token").await;
        let result = host.ack_alert("a-not-synced".to_string(), 1_000).await;
        assert!(matches!(result, Err(MobileAlertError::AlertNotFound)));
        assert_eq!(host.queue_depth().await, 0, "nothing was enqueued");
    }

    #[tokio::test]
    async fn registering_without_a_credential_is_unauthorized_and_touches_no_network() {
        // `""` at init means "no token yet" — the same mapping `run`
        // makes. The base_url is unresolvable, so reaching the transport
        // at all would surface as `RegisterFailed`, not `Unauthorized`.
        let host = host_at("m2-register-nokey-ns", "").await;
        let result = host
            .register_push_target("slot-1".to_string(), "fold".to_string(), "fcm-1".to_string())
            .await;
        assert!(matches!(
            result,
            Err(MobilePushRegistrationError::Unauthorized)
        ));
    }

    /// The shadow token's whole job: a key pushed after init has to reach
    /// registration, which does not go through `Core`. Proven by the error
    /// *changing* — with a key in hand the call reaches the (unresolvable)
    /// transport and fails there instead of short-circuiting.
    #[tokio::test]
    async fn a_pushed_key_reaches_registration_and_clearing_it_takes_it_away() {
        let host = host_at("m2-register-key-ns", "").await;
        host.push_api_key("device-token".to_string()).await;
        let result = host
            .register_push_target("slot-1".to_string(), "fold".to_string(), "fcm-1".to_string())
            .await;
        assert!(
            matches!(result, Err(MobilePushRegistrationError::RegisterFailed { .. })),
            "with a credential the call reaches the transport, {result:?}"
        );

        host.clear_api_key().await;
        let after = host
            .register_push_target("slot-1".to_string(), "fold".to_string(), "fcm-1".to_string())
            .await;
        assert!(matches!(
            after,
            Err(MobilePushRegistrationError::Unauthorized)
        ));
    }

    /// The wire body the authority's `deny_unknown_fields` DTO will accept
    /// — pinned here because this is the one request body assembled at this
    /// seam rather than inside `hummingbird-core`.
    #[test]
    fn the_registration_body_is_exactly_the_authoritys_dto() {
        let body = serde_json::to_value(CreatePushTarget {
            id: "slot-1".to_string(),
            name: "fold".to_string(),
            platform: Platform::Android,
            fcm_token: "fcm-1".to_string(),
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "id": "slot-1",
                "name": "fold",
                "platform": "android",
                "fcm_token": "fcm-1",
            })
        );
    }

    // ------------------------------------------------- item detail (#141)

    fn fixture_detail(archived: bool) -> hummingbird_core::item_detail::ItemDetail {
        let mut item = Item {
            id: "a-1".into(),
            seq: Some(42),
            title: "ship the thing".into(),
            description: Some("with notes".into()),
            stage: Stage::Ready,
            size: Some(Size::Quick),
            energy: Some(Energy::Low),
            context: Some("@computer".into()),
            priority: 3,
            project_id: Some("p-1".into()),
            project_pos: None,
            deadline: Some("2026-08-20".into()),
            scheduled_date: None,
            source: None,
            source_key: None,
            source_url: Some("https://example.test/x".into()),
            archived_at: None,
            agent: false,
            created_at: 1,
            updated_at: 7,
            version: 4,
        };
        if archived {
            item.archived_at = Some(900);
        }
        hummingbird_core::item_detail::ItemDetail {
            item,
            project: Some(hummingbird_core::item_detail::ProjectRef {
                id: "p-1".into(),
                name: Some("Kitchen".into()),
            }),
            steps: vec![hummingbird_domain::Step {
                id: "s-1".into(),
                item_id: "a-1".into(),
                body: "first".into(),
                done: false,
                position: 1,
                deleted_at: None,
                version: 1,
            }],
            open_blockers: vec![hummingbird_core::item_detail::BlockerEntry {
                item_id: "b-unseen".into(),
                title: None,
            }],
            live_alert: None,
            is_archived: archived,
            is_editable: !archived,
            available_actions: if archived {
                vec![]
            } else {
                vec![ItemAction::Start, ItemAction::Complete]
            },
        }
    }

    /// The seam maps and decides nothing: every verdict on the record is
    /// the core's, carried across unchanged — including the empty action
    /// list an archived row gets, which Kotlin must never re-derive from
    /// `stage`.
    #[test]
    fn the_item_detail_record_carries_the_cores_verdicts_verbatim() {
        let record = to_item_detail_record(&fixture_detail(false), 1_000);
        assert_eq!(record.stage, "ready");
        assert_eq!(record.size.as_deref(), Some("quick"));
        assert_eq!(record.energy.as_deref(), Some("low"));
        assert_eq!(record.project_name.as_deref(), Some("Kitchen"));
        assert!(!record.agent, "the delegation axis is carried, absence and all");
        assert_eq!(record.available_actions, vec!["start", "complete"]);
        assert!(record.is_editable);
        assert_eq!(
            record.open_blockers,
            vec![OpenBlockerRecord { item_id: "b-unseen".into(), title: None }],
            "an unseen blocker crosses as a titleless row, never dropped"
        );

        let archived = to_item_detail_record(&fixture_detail(true), 1_000);
        assert!(archived.is_archived);
        assert!(!archived.is_editable);
        assert!(archived.available_actions.is_empty());
    }

    /// The double-`Option` that cannot cross UniFFI, pinned at the one
    /// place it is reconstructed: "clear it" must stay distinguishable
    /// from "I did not touch it", or an editor can only ever add.
    #[test]
    fn a_field_patch_keeps_clear_and_untouched_apart() {
        assert_eq!(FieldPatch::Untouched.to_text(), None);
        assert_eq!(FieldPatch::Clear.to_text(), Some(None));
        assert_eq!(
            FieldPatch::Set { value: "2026-08-20".into() }.to_text(),
            Some(Some("2026-08-20".to_string()))
        );
    }

    /// A vocabulary field is rejected before the seam, exactly as an
    /// unrecognised act string is — never sent for the authority to 400.
    #[test]
    fn an_unrecognised_vocabulary_word_never_reaches_the_queue() {
        assert_eq!(
            FieldPatch::Set { value: "quick".into() }.to_vocabulary(Size::parse),
            Ok(Some(Some(Size::Quick)))
        );
        assert_eq!(FieldPatch::Clear.to_vocabulary(Size::parse), Ok(Some(None)));
        assert!(FieldPatch::Set { value: "enormous".into() }
            .to_vocabulary(Size::parse)
            .is_err());
    }

    fn untouched_edit() -> ItemEdit {
        ItemEdit {
            title: None,
            priority: None,
            description: FieldPatch::Untouched,
            size: FieldPatch::Untouched,
            energy: FieldPatch::Untouched,
            context: FieldPatch::Untouched,
            project_id: FieldPatch::Untouched,
            deadline: FieldPatch::Untouched,
            scheduled_date: FieldPatch::Untouched,
        }
    }

    /// An edit is durable before any network is touched, like every other
    /// mutation on this seam — and it reads back through `item_detail`
    /// immediately, through the core's overlay.
    #[tokio::test]
    async fn an_edit_is_durable_and_reads_back_before_any_sync() {
        let dir = tempfile::tempdir().unwrap();
        let host = MobileTaskHost::init(
            dir.path().join("edit-ns").to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();
        let id = host.capture("buy milk".to_string(), 1_000).await.unwrap();

        host.edit_item(
            id.clone(),
            ItemEdit {
                title: Some("buy oat milk".into()),
                deadline: FieldPatch::Set { value: "2026-08-20".into() },
                ..untouched_edit()
            },
            2_000,
        )
        .await
        .unwrap();

        let detail = host.item_detail(id, 2_000).await.expect("captured item");
        assert_eq!(detail.title, "buy oat milk");
        assert_eq!(detail.deadline.as_deref(), Some("2026-08-20"));
        assert_eq!(
            host.queue_depth().await,
            2,
            "the capture and the edit are two durable entries"
        );
    }

    #[tokio::test]
    async fn editing_an_item_this_device_has_never_seen_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let host = MobileTaskHost::init(
            dir.path().join("edit-ns-2").to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        assert!(matches!(
            host.edit_item("nope".to_string(), untouched_edit(), 1_000).await,
            Err(MobileEditError::ItemNotFound)
        ));
        assert!(host.item_detail("nope".to_string(), 1_000).await.is_none());
    }
}

/// The M4 (#538) skills-lane doors. Every exported function above is
/// exercised here — `clippy -D warnings` fails on `dead_code`, so a door
/// that is declared but never reachable would fail the build rather than
/// ship as a promise.
#[cfg(test)]
mod skills_tests {
    use super::*;

    fn asking() -> MobileGrillTurnState {
        grill_turn_started(grill_turn_idle())
    }

    fn messages_of(state: &MobileGrillTurnState) -> Vec<String> {
        match state {
            MobileGrillTurnState::Idle => Vec::new(),
            MobileGrillTurnState::Asking { messages }
            | MobileGrillTurnState::Question { messages, .. }
            | MobileGrillTurnState::Proposal { messages, .. }
            | MobileGrillTurnState::Declined { messages, .. } => messages.clone(),
        }
    }

    #[test]
    fn a_tap_starts_asking_and_a_second_one_is_a_no_op() {
        assert_eq!(asking(), MobileGrillTurnState::Asking { messages: Vec::new() });
        let with_a_line = grill_turn_line(
            asking(),
            r#"{"type":"progress","message":"reading"}"#.to_string(),
        );
        assert_eq!(grill_turn_started(with_a_line.clone()), with_a_line);
    }

    /// The whole of heartbeat handling, across the boundary: the runner
    /// beats `"still running"` every 20s and the narration collapses them.
    /// No timer exists anywhere in this lane, on either side.
    #[test]
    fn consecutive_heartbeats_collapse_and_a_later_repeat_is_kept() {
        let beat = r#"{"type":"progress","message":"still running"}"#.to_string();
        let mut state = asking();
        for _ in 0..4 {
            state = grill_turn_line(state, beat.clone());
        }
        state = grill_turn_line(state, r#"{"type":"progress","message":"asking"}"#.to_string());
        state = grill_turn_line(state, beat.clone());
        assert_eq!(messages_of(&state), vec!["still running", "asking", "still running"]);
    }

    #[test]
    fn an_unreadable_line_is_dropped_and_is_not_terminal() {
        let state = grill_turn_line(asking(), "not json at all".to_string());
        assert_eq!(state, MobileGrillTurnState::Asking { messages: Vec::new() });
    }

    #[test]
    fn a_question_line_answers_the_question_phase_with_its_stamp() {
        let state = grill_turn_line(
            asking(),
            r#"{"ok":true,"result":{"kind":"question","question":{"prompt":"Which airport?","recommendedAnswer":"SEA","choices":["SEA","PDX"]}},"backend":"b","model":"m"}"#
                .to_string(),
        );
        let MobileGrillTurnState::Question { question, backend, model, .. } = state else {
            panic!("expected the question phase");
        };
        assert_eq!(question.prompt, "Which airport?");
        assert_eq!(question.choices, vec!["SEA".to_string(), "PDX".to_string()]);
        assert_eq!(backend.as_deref(), Some("b"));
        assert_eq!(model.as_deref(), Some("m"));
    }

    /// The patch crosses as opaque text and is never read into — a key this
    /// client has never heard of survives the round trip through both maps.
    #[test]
    fn a_proposal_carries_its_patch_verbatim_and_unread() {
        let state = grill_turn_line(
            asking(),
            r#"{"ok":true,"result":{"kind":"proposal","proposal":{"summary":"s","verdict":"fog_remains","patch":{"never_heard_of":[1,2]}}},"backend":null,"model":null}"#
                .to_string(),
        );
        let MobileGrillTurnState::Proposal { proposal, .. } = state.clone() else {
            panic!("expected the proposal phase");
        };
        assert_eq!(proposal.verdict, MobileGrillVerdict::FogRemains);
        assert!(proposal.patch_json.contains("never_heard_of"), "{}", proposal.patch_json);
        // Both maps, round trip: what Kotlin holds deserializes back to the
        // core state it came from.
        assert_eq!(map_grill_state(unmap_grill_state(state.clone())), state);
    }

    /// The decline the runner itself sends (a 200 whose terminal line says
    /// `ok:false`) is carried verbatim — never prefixed, never branched on
    /// — and counts as answered, because a response resolved.
    #[test]
    fn a_seam_decline_is_verbatim_and_answered() {
        let reason = "That item cannot be grilled: PROVISIONAL_TURN_CAP reached.";
        let state = grill_turn_line(
            asking(),
            format!(r#"{{"ok":false,"error":"{reason}","backend":"b","model":"m"}}"#),
        );
        assert_eq!(
            state,
            MobileGrillTurnState::Declined {
                messages: Vec::new(),
                reason: reason.to_string(),
                backend: Some("b".to_string()),
                model: Some("m".to_string()),
                answered: true,
            },
        );
    }

    /// `answered` is observed at the response, never inferred from the
    /// prose: the two transport-side reports are unanswered, the two
    /// response-side ones are answered.
    #[test]
    fn answered_tracks_whether_a_response_arrived() {
        let cases: Vec<(MobileGrillTurnState, bool)> = vec![
            (grill_turn_no_token(asking()), false),
            (grill_turn_transport_failed(asking(), "connection reset".to_string()), false),
            (grill_turn_response_failed(asking(), 401), true),
            (grill_turn_stream_ended(asking()), true),
        ];
        for (state, expected) in cases {
            let MobileGrillTurnState::Declined { answered, .. } = state else {
                panic!("expected a declined phase");
            };
            assert_eq!(answered, expected);
        }
    }

    #[test]
    fn the_four_transport_reports_carry_the_cores_words() {
        let words = |state: MobileGrillTurnState| match state {
            MobileGrillTurnState::Declined { reason, .. } => reason,
            other => panic!("expected a declined phase, got {other:?}"),
        };
        assert_eq!(words(grill_turn_no_token(asking())), skills::NO_TOKEN);
        assert_eq!(words(grill_turn_stream_ended(asking())), skills::NO_TERMINAL_LINE);
        assert_eq!(
            words(grill_turn_transport_failed(asking(), "  boom  ".to_string())),
            skills::decline_for_transport("boom"),
        );
        assert_eq!(
            words(grill_turn_response_failed(asking(), 403)),
            skills::decline_for_response(403),
        );
    }

    /// The transport calls `stream_ended` at the end of every stream,
    /// including the ones that answered — so it must be a no-op on a
    /// settled state.
    #[test]
    fn stream_ended_is_a_no_op_once_the_turn_has_settled() {
        let settled = grill_turn_line(
            asking(),
            r#"{"ok":true,"result":{"kind":"question","question":{"prompt":"p","recommendedAnswer":"r","choices":["a","b"]}},"backend":null,"model":null}"#
                .to_string(),
        );
        assert_eq!(grill_turn_stream_ended(settled.clone()), settled);
        assert_eq!(grill_turn_line(settled.clone(), "{\"ok\":false,\"error\":\"late\"}".to_string()), settled);
    }

    #[test]
    fn the_grill_body_is_the_shared_fixtures_bytes() {
        assert_eq!(
            grill_run_body("i".to_string(), Vec::new()),
            r#"{"skill":"grill-me","args":{"ref":"i","turns":[]}}"#,
        );
        let turn = MobileGrillTurn {
            question: MobileGrillQuestion {
                prompt: "Which airport?".to_string(),
                recommended_answer: "SEA".to_string(),
                choices: vec!["SEA".to_string(), "PDX".to_string()],
            },
            answer: "SEA".to_string(),
        };
        assert_eq!(
            grill_run_body("i".to_string(), vec![turn.clone()]),
            r#"{"skill":"grill-me","args":{"ref":"i","turns":[{"question":{"prompt":"Which airport?","recommendedAnswer":"SEA","choices":["SEA","PDX"]},"answer":"SEA"}]}}"#,
        );
        assert_eq!(format_grill_transcript(vec![turn]), "Q: Which airport?\nA: SEA");
    }

    // ---- the microtask twins

    #[test]
    fn a_microtask_run_narrates_then_completes_with_its_note_and_stamp() {
        let mut state = skill_run_started(skill_run_idle());
        state = skill_run_line(state, r#"{"type":"progress","message":"reading"}"#.to_string());
        state = skill_run_line(
            state,
            r#"{"ok":true,"result":{"steps":["a"],"note":"kept 2"},"backend":"b","model":"m"}"#
                .to_string(),
        );
        assert_eq!(
            state,
            MobileSkillRunState::Done {
                messages: vec!["reading".to_string()],
                note: "kept 2".to_string(),
                backend: Some("b".to_string()),
                model: Some("m".to_string()),
            },
        );
        assert_eq!(skill_run_stamp_label(state).as_deref(), Some("b · m"));
    }

    #[test]
    fn a_run_with_no_backend_named_has_no_stamp_to_render() {
        let state = skill_run_transport_failed(
            skill_run_started(skill_run_idle()),
            "offline".to_string(),
        );
        assert_eq!(skill_run_stamp_label(state), None);
    }

    #[test]
    fn the_microtask_run_doors_report_the_same_four_transport_events() {
        let started = || skill_run_started(skill_run_idle());
        let words = |state: MobileSkillRunState| match state {
            MobileSkillRunState::Declined { reason, answered, .. } => (reason, answered),
            other => panic!("expected a declined phase, got {other:?}"),
        };
        assert_eq!(words(skill_run_no_token(started())), (skills::NO_TOKEN.to_string(), false));
        assert_eq!(
            words(skill_run_stream_ended(started())),
            (skills::NO_TERMINAL_LINE.to_string(), true),
        );
        assert_eq!(
            words(skill_run_response_failed(started(), 500)),
            (skills::decline_for_response(500), true),
        );
        assert_eq!(
            words(skill_run_transport_failed(started(), String::new())),
            (skills::decline_for_transport(""), false),
        );
        // The duplicate-tap rule, on this reducer too.
        let running = started();
        assert_eq!(skill_run_started(running.clone()), running);
    }

    #[test]
    fn the_microtask_body_is_the_shared_fixtures_bytes() {
        assert_eq!(
            microtask_run_body("i".to_string(), false, None, None),
            r#"{"skill":"microtask","args":{"ref":"i"}}"#,
        );
        assert_eq!(
            microtask_run_body("i".to_string(), true, Some(3), Some("m".to_string())),
            r#"{"skill":"microtask","args":{"ref":"i","replace":true,"grain":3,"model":"m"}}"#,
        );
        // The web picker's "Default model" empty string omits the key.
        assert_eq!(
            microtask_run_body("i".to_string(), false, None, Some(String::new())),
            r#"{"skill":"microtask","args":{"ref":"i"}}"#,
        );
    }

    /// Both run-state maps round trip, the same way the grill ones do.
    #[test]
    fn the_run_state_maps_round_trip_every_phase() {
        let states = vec![
            skill_run_idle(),
            skill_run_started(skill_run_idle()),
            skill_run_line(
                skill_run_started(skill_run_idle()),
                r#"{"ok":true,"result":{"steps":[],"note":"n"},"backend":"b","model":null}"#
                    .to_string(),
            ),
            skill_run_no_token(skill_run_started(skill_run_idle())),
        ];
        for state in states {
            assert_eq!(map_run_state(unmap_run_state(state.clone())), state);
        }
    }

    /// **No provider or model name appears in this lane** — the stamp is
    /// read off the envelope or it is absent. The web pins the same absence
    /// over `ItemPanel.tsx`; this is the Rust side of that rule, and
    /// `SkillsLaneIsolationTest.kt` is the Kotlin side.
    #[test]
    fn no_decline_or_stamp_names_a_provider() {
        let stamped = skill_run_line(
            skill_run_started(skill_run_idle()),
            r#"{"ok":true,"result":null,"backend":"whatever-the-envelope-said","model":null}"#
                .to_string(),
        );
        assert_eq!(skill_run_stamp_label(stamped).as_deref(), Some("whatever-the-envelope-said"));
        let lowercase = format!(
            "{} {} {} {}",
            skills::NO_TOKEN,
            skills::NO_TERMINAL_LINE,
            skills::OUTSIDE_SCHEMA,
            skills::decline_for_response(500),
        )
        .to_lowercase();
        for name in ["anthropic", "claude-", "sonnet", "opus", "haiku", "moonshot"] {
            assert!(!lowercase.contains(name), "the decline prose names {name}");
        }
    }
}
