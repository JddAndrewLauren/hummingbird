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
//! only, no capture-meta fields (out of scope until a later screen). M3/#529
//! widened [`MobileTaskHost::capture`] to take a whole [`CaptureDraft`] and
//! added the free [`capture_form_meta`] door, a pure expose over decisions
//! already sunk at M1-2/#500: no new core logic, just the rest of the
//! capture box's field set reaching the same seam. M2
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
//! **Android never calls a per-row decision function** (M1-6/#504, widened
//! by M3/#530) — this is the designed asymmetry with the web seam.
//! `client/ffi-web/src/decisions.rs` exposes `hummingbird_core::decisions`
//! as a free-function door a *second*, main-thread wasm instance calls per
//! keystroke/per row (that module's own header explains why a second
//! stateless instance is safe there). Android has no such second instance
//! and no in-process wasm boundary to cross cheaply from Kotlin — every
//! crossing here is a JNI call — so [`MobileTaskHost::now_board`] does the
//! *decided* work once, on the Rust side, and hands Kotlin one whole board
//! — every column pre-grouped, pre-filtered and pre-ordered, plus the
//! blocked section — in a single crossing, each row already carrying its
//! [`MobileUrgencyBand`] and wire-vocabulary action list. `NowScreen` never
//! calls `by_priority_then_due`, `group_frontier`, `apply_facets`,
//! `compute_urgency` or `available_actions` itself, and never could: those
//! functions are not exported to Kotlin at all, only their already-applied
//! results are.
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

use hummingbird_core::decisions::{available_actions, can_mark_done, frontier, queue, urgency};
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

/// [`hummingbird_core::decisions::vocabulary::VocabOption`], mirrored as a
/// `uniffi::Record` — one `<select>`-equivalent option's wire value and
/// display label, crossed to Kotlin so no size/energy word is ever a
/// literal in `CaptureScreen`'s source (M3/#529's own structural-test
/// criterion).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VocabOption {
    pub value: String,
    pub label: String,
}

fn to_vocab_option(option: hummingbird_core::decisions::vocabulary::VocabOption) -> VocabOption {
    VocabOption {
        value: option.value,
        label: option.label,
    }
}

/// The capture form's whole metadata bundle — sizes, energies and the
/// suggested (open-vocabulary) contexts — as one applied result
/// (M3/#529). This is a *per-gesture* free door, not a per-row one (the
/// module doc's asymmetry rule): `CaptureScreen` reads it once, on mount,
/// never once per row or per keystroke, so a single JNI crossing here is
/// the whole cost this ever incurs.
///
/// `suggestedContexts` is not a closed vocabulary — CONTEXT.md: "the set of
/// places a person works is theirs" — so it is offered only as suggestions;
/// [`CaptureDraft::context`] accepts any string, including one absent from
/// this list.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CaptureFormMeta {
    pub sizes: Vec<VocabOption>,
    pub energies: Vec<VocabOption>,
    pub suggested_contexts: Vec<String>,
}

/// One project, as the details disclosure's Project picker needs it —
/// [`MobileTaskHost::projects`]'s row, carrying only what a picker reads
/// (see that method's own doc for why not the whole [`hummingbird_domain::Project`]).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileProject {
    pub id: String,
    pub name: String,
}

/// [`MobileTaskHost::capture`]'s form metadata read — the vocabulary and
/// context suggestions `CaptureScreen` needs to render its fields, with no
/// vocabulary literal of its own. Free rather than a method on
/// [`MobileTaskHost`]: it needs no durable state, the same reason
/// [`can_submit_capture`] and [`capture_meta_problems`] are free functions.
#[uniffi::export]
pub fn capture_form_meta() -> CaptureFormMeta {
    CaptureFormMeta {
        sizes: hummingbird_core::decisions::vocabulary::size_options()
            .into_iter()
            .map(to_vocab_option)
            .collect(),
        energies: hummingbird_core::decisions::vocabulary::energy_options()
            .into_iter()
            .map(to_vocab_option)
            .collect(),
        suggested_contexts: hummingbird_core::decisions::vocabulary::CONTEXTS
            .iter()
            .map(|context| context.to_string())
            .collect(),
    }
}

/// The capture box's destination choice — Triage (the default funnel entry)
/// or Ready (skip the funnel because the item is already decided). A closed
/// two-value vocabulary, so a `uniffi::Enum` rather than a string: unlike
/// size/energy/context, this one is never open, and there is no third
/// destination capture can name (`Stage`'s other four values are reachable
/// only by later action, never at mint time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CaptureDestination {
    Triage,
    Ready,
}

/// The capture box's whole draft, as the form collects it (M3/#529) — the
/// mobile twin of [`hummingbird_core::CaptureOptions`] plus the title and
/// the destination [`MobileTaskHost::capture`] needs
/// [`hummingbird_core::Core::capture`]'s own `stage` argument for.
///
/// Every optional field crosses as a plain `String`, `""` meaning "not
/// set" — the form's own resting state — never a nested `Option`: capture
/// is a create, so there is no stored value a "clear" could mean anything
/// different from "absent" for (the same reasoning
/// [`hummingbird_core::CaptureOptions`]'s own doc gives, carried across the
/// seam). `size`/`energy` cross as their wire vocabulary word; an
/// unrecognised, non-empty value is refused at
/// [`MobileTaskHost::capture`] rather than silently treated as unset, the
/// same "reject before the seam" rule [`FieldPatch::to_vocabulary`] already
/// applies to the item-edit form.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CaptureDraft {
    pub title: String,
    pub destination: CaptureDestination,
    pub size: String,
    pub energy: String,
    pub context: String,
    pub description: String,
    pub project_id: String,
    /// The priority `Select`'s raw value (`""`..`"4"`) — resolved through
    /// [`hummingbird_core::decisions::capture::priority_from_select`],
    /// never re-derived here.
    pub priority: String,
    pub deadline: String,
    pub scheduled_date: String,
}

/// Resolves one optional vocabulary field: `""` is "not set", anything else
/// must parse or the whole capture is refused before it ever reaches the
/// core — mirrors [`FieldPatch::to_vocabulary`]'s "reject before the seam"
/// rule for the item-edit form.
fn parse_optional_vocabulary<T>(
    raw: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Option<T>, String> {
    if raw.is_empty() {
        return Ok(None);
    }
    parse(raw)
        .map(Some)
        .ok_or_else(|| format!("unrecognised value: {raw}"))
}

/// A free-text optional field: `""` is "not set", anything else is sent
/// verbatim.
fn some_if_present(raw: &str) -> Option<String> {
    (!raw.is_empty()).then(|| raw.to_string())
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
/// caller ([`MobileTaskHost::now_board`] returns every row pre-ordered
/// within its column, so `items`/index order *is* display order), its
/// [`MobileUrgencyBand`] and its S11/#109 act vocabulary strings
/// (`"start"|"complete"|"block"|"cancel"`, [`ItemAction::as_str`]) ready
/// for [`MobileTaskHost::act`]. Carries only the fields `NowScreen`
/// actually renders — the same "hand the boundary only what a decision
/// needs" discipline [`frontier::FrontierItem`]'s own doc states, applied
/// on the way *out* instead of in.
///
/// `stage` is `hummingbird_domain::Stage::as_str`'s own wire spelling
/// (`"triage"`, `"grilling"`, `"ready"`, `"in_progress"`) — the M3/#530
/// addition that lets an inline triage-process card carry its own stage
/// chip exactly as `ItemRow`'s `item.stage === "ready" ? null : <StageBadge>`
/// does on web: a `Ready`/`InProgress` frontier row still stages, it is
/// only never rendered for those two, and that rendering choice belongs to
/// `NowScreen`, not to this record.
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
    pub stage: String,
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

fn to_queue_item(item: &Item) -> queue::QueueItem {
    queue::QueueItem { id: item.id.clone(), created_at: item.created_at }
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
        stage: item.stage.as_str().to_string(),
    }
}

/// [`to_now_item_record`] for a **relation-blocked** row, whose act
/// vocabulary is narrower than its stage alone would say: only the
/// core-decided mark-done gesture ([`hummingbird_core::decisions::
/// can_mark_done`]), never `start`/`block`/`cancel`.
///
/// [`available_actions`] answers from [`Item::stage`] and nothing else, and
/// a relation-blocked item's stage is usually `Ready` — so reusing the
/// frontier record here offered `Start` on an item with an open blocker,
/// and taking it minted an In Progress item that was still blocked. The web
/// blocked row has always passed `onComplete` alone, gated on `canMarkDone`
/// (`client/web/src/screens/NowScreen.tsx`); this is that rule, decided on
/// this side of the seam so `NowScreen.kt` still renders whatever list it
/// is handed rather than filtering one itself (the module doc's
/// Android-decides-no-affordance rule).
fn to_blocked_item_record(item: &Item, now: &str) -> NowItemRecord {
    let record = to_now_item_record(item, now);
    let actions = if can_mark_done(item.stage, item.archived_at.is_some()) {
        vec![ItemAction::Complete.as_str().to_string()]
    } else {
        Vec::new()
    };
    NowItemRecord { available_actions: actions, ..record }
}

/// [`FrontierAxis`], mirrored as a `uniffi::Enum` — the grouping axis
/// switch (M3/#530). A second, uniffi-derived definition of the same four
/// axes rather than an annotation on the core type (ADR-0003, the same
/// reasoning [`MobileUrgencyBand`] states); [`map_frontier_axis`] is the
/// only place the two are allowed to drift apart from, and it is
/// exhaustive with no wildcard arm for exactly that reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileFrontierAxis {
    Context,
    Project,
    Size,
    Energy,
}

fn map_frontier_axis(axis: MobileFrontierAxis) -> frontier::FrontierAxis {
    match axis {
        MobileFrontierAxis::Context => frontier::FrontierAxis::Context,
        MobileFrontierAxis::Project => frontier::FrontierAxis::Project,
        MobileFrontierAxis::Size => frontier::FrontierAxis::Size,
        MobileFrontierAxis::Energy => frontier::FrontierAxis::Energy,
    }
}

/// [`frontier::FacetSelection`], mirrored as a `uniffi::Record` — a
/// `HashSet<String>` per facet crosses as a `Vec<String>` (uniffi has no
/// `Set` type), so [`to_facet_selection`] is where the two are reconciled.
/// Kotlin owns the actual `Set` shape and toggling; this record only ever
/// carries the picked values *to* [`MobileTaskHost::now_board`].
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct NowFacetSelectionRecord {
    pub context: Vec<String>,
    pub size: Vec<String>,
    pub energy: Vec<String>,
    pub urgency: Vec<String>,
}

fn to_facet_selection(record: &NowFacetSelectionRecord) -> frontier::FacetSelection {
    frontier::FacetSelection {
        context: record.context.iter().cloned().collect(),
        size: record.size.iter().cloned().collect(),
        energy: record.energy.iter().cloned().collect(),
        urgency: record.urgency.iter().cloned().collect(),
    }
}

/// One [`frontier::group_frontier`] column, decided: `value`/`label`
/// exactly that function's own doc (`None` for the axis's no-value
/// bucket), `items` the column's rows in display order — already capped to
/// nothing; the six-card-and-"N more" cap is `NowScreen`'s own rendering
/// choice over this full list, not a second crossing.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NowColumnRecord {
    pub value: Option<String>,
    pub label: Option<String>,
    pub items: Vec<NowItemRecord>,
}

/// One [`hummingbird_core::Core::blocked`] entry, decided: the blocked row
/// itself, plus its still-open blockers' titles — `blockedReasonLabel`'s
/// own web-side input shape (`client/web/src/screens/blocked-reason.ts`),
/// so `NowScreen`'s blocked-section label reads the identical words.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NowBlockedEntryRecord {
    pub item: NowItemRecord,
    pub blocked_by_titles: Vec<String>,
}

/// [`MobileTaskHost::now_board`]'s whole read, decided: every frontier
/// column (with the triage-process queue's rows riding inline into
/// whichever column their axis value lands them in — see
/// [`build_now_board`]'s own doc) plus the blocked section [`Core::blocked`]
/// carries separately, since a relation-blocked item is not on the
/// frontier at all and groups by no axis.
///
/// `contexts` is [`frontier::contexts_of`] over the ordered, **pre-facet**
/// list (`FrontierColumns.tsx`'s own `contextsOf(ordered)`, not `shown`) —
/// the Context facet's live chip vocabulary: contexts actually present on
/// the board, suggested vocabulary first, [`frontier::NO_CONTEXT`] last
/// only when something on the board actually lacks one. Reusing `shown`
/// instead would make a picked chip remove its own option from the list
/// the moment it narrowed the board to nothing but itself.
///
/// `live_column_keys` is every column key [`frontier::group_frontier`]
/// would produce for the *given axis* over that same pre-facet list —
/// `FrontierColumns.tsx`'s own `liveKeys` (`groupFrontier(ordered, axis,
/// projects)`, not `shown`): the set a caller prunes its persisted
/// collapse set against before writing, so a column the live filter
/// merely hides is never mistaken for one that no longer exists (a
/// pruned-then-reappearing column would otherwise come back collapsed for
/// a reason the reader cannot see — ADR-0021 decision 5's key-accretion
/// argument).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NowBoardRecord {
    pub columns: Vec<NowColumnRecord>,
    pub blocked: Vec<NowBlockedEntryRecord>,
    pub contexts: Vec<String>,
    pub live_column_keys: Vec<String>,
}

/// [`MobileTaskHost::now_board`]'s pure core: the frontier
/// ([`hummingbird_core::Core::frontier`]) ordered by
/// [`frontier::by_priority_then_due`], with the triage-process queue
/// ([`queue::triage_process_queue`] over `triage_items`/`grilling_items`/
/// `draft_item_ids`) appended after it — exactly the web board's own
/// `[...orderFrontier(frontier), ...triageProcessQueue(...).items]`
/// concatenation (`FrontierColumns.tsx`): whichever column a triage/
/// grilling row's axis value lands it in, it sorts under that column's
/// startable actions, because [`frontier::group_frontier`] preserves input
/// order inside every bucket. Facets apply to that combined, ordered list
/// before grouping ([`frontier::apply_facets`]), and grouping
/// ([`frontier::group_frontier`]) never re-sorts. `blocked` is entirely
/// separate: [`hummingbird_core::Core::blocked`]'s relation-blocked items
/// never join the frontier or the triage-process queue, so they never
/// enter the grouping pass at all.
///
/// Free of `Core`/`MobileTaskHost` entirely so a fixture-driven test can
/// exercise it directly, with no async runtime or durable store in the
/// loop — `build_now_queue`'s own reason, before it retired into this.
#[allow(clippy::too_many_arguments)]
fn build_now_board(
    frontier_items: &[Item],
    triage_items: &[Item],
    grilling_items: &[Item],
    draft_item_ids: &[String],
    blocked: &[(Item, Vec<Item>)],
    projects: &[frontier::ProjectName],
    axis: frontier::FrontierAxis,
    facets: &frontier::FacetSelection,
    now: &str,
) -> NowBoardRecord {
    let by_id: HashMap<&str, &Item> = frontier_items
        .iter()
        .chain(triage_items.iter())
        .chain(grilling_items.iter())
        .map(|item| (item.id.as_str(), item))
        .collect();

    let frontier_entries: Vec<frontier::FrontierItem> =
        frontier_items.iter().map(to_frontier_item).collect();
    let frontier_ordered = frontier::by_priority_then_due(&frontier_entries);

    let triage_queue: Vec<queue::QueueItem> = triage_items.iter().map(to_queue_item).collect();
    let grilling_queue: Vec<queue::QueueItem> = grilling_items.iter().map(to_queue_item).collect();
    let triage_process = queue::triage_process_queue(&triage_queue, &grilling_queue, draft_item_ids);

    let ordered_ids: Vec<String> =
        frontier_ordered.into_iter().chain(triage_process.ids).collect();
    let ordered_entries: Vec<frontier::FrontierItem> = ordered_ids
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).map(|item| to_frontier_item(item)))
        .collect();

    let contexts = frontier::contexts_of(&ordered_entries);
    let live_column_keys: Vec<String> = frontier::group_frontier(&ordered_entries, axis, projects)
        .into_iter()
        .map(|column| column.value.unwrap_or_default())
        .collect();

    let shown_ids = frontier::apply_facets(&ordered_entries, facets, now);
    let shown_entries: Vec<frontier::FrontierItem> = shown_ids
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).map(|item| to_frontier_item(item)))
        .collect();

    let columns = frontier::group_frontier(&shown_entries, axis, projects)
        .into_iter()
        .map(|column| NowColumnRecord {
            value: column.value,
            label: column.label,
            items: column
                .ids
                .iter()
                .filter_map(|id| by_id.get(id.as_str()).map(|item| to_now_item_record(item, now)))
                .collect(),
        })
        .collect();

    let blocked = blocked
        .iter()
        .map(|(item, blockers)| NowBlockedEntryRecord {
            item: to_blocked_item_record(item, now),
            blocked_by_titles: blockers.iter().map(|blocker| blocker.title.clone()).collect(),
        })
        .collect();

    NowBoardRecord { columns, blocked, contexts, live_column_keys }
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

    /// Captures a whole [`CaptureDraft`] — `CaptureScreen`'s full field set
    /// (M3/#529), widened from M1-5/#503's title-only surface. **Still
    /// local-first** (#128's own criterion): reaches
    /// [`hummingbird_core::Core::capture`], which durably enqueues via
    /// `SyncCycle::enqueue` and overlays the optimistic [`Item`] before any
    /// transport is ever touched, so the item exists in the local mirror the
    /// instant this returns — sync or no sync, online or offline.
    ///
    /// `draft.destination` picks [`hummingbird_core::Core::capture`]'s
    /// `stage` argument; every other field resolves into a
    /// [`CaptureOptions`] the same way [`MobileTaskHost::edit_item`]
    /// resolves an [`ItemEdit`] into a `TriagePatch` — an unrecognised
    /// `size`/`energy` word is refused here, before the core is ever
    /// reached, rather than silently treated as absent. Mints its own seed
    /// ([`mint_mutation_seed`]) since the host supplies only
    /// `draft`/`now_ms`.
    ///
    /// This is a pure expose (#529's own framing): every decision here —
    /// what counts as a valid date, what a priority `"0"` means, which
    /// words are valid size/energy — already lives in
    /// `hummingbird_core::decisions`; this method only assembles their
    /// answers into the one [`CaptureOptions`] the core's own `capture`
    /// takes. `Core::capture` itself still refuses no title (that stays
    /// [`can_submit_capture`]'s job, called before this ever is).
    pub async fn capture(
        &self,
        draft: CaptureDraft,
        now_ms: i64,
    ) -> Result<String, MobileCaptureError> {
        let stage = match draft.destination {
            CaptureDestination::Triage => Stage::Triage,
            CaptureDestination::Ready => Stage::Ready,
        };
        let size = parse_optional_vocabulary(&draft.size, Size::parse)
            .map_err(|detail| MobileCaptureError::CaptureFailed { detail })?;
        let energy = parse_optional_vocabulary(&draft.energy, Energy::parse)
            .map_err(|detail| MobileCaptureError::CaptureFailed { detail })?;
        let options = CaptureOptions {
            size,
            energy,
            context: some_if_present(&draft.context),
            description: some_if_present(&draft.description),
            priority: hummingbird_core::decisions::capture::priority_from_select(&draft.priority),
            project_id: some_if_present(&draft.project_id),
            deadline: some_if_present(&draft.deadline),
            scheduled_date: some_if_present(&draft.scheduled_date),
        };

        let seed = mint_mutation_seed("capture", now_ms);
        let mut inner = self.inner.lock().await;
        inner
            .core
            .capture(&seed, draft.title, stage, now_ms, options)
            .await
            .map_err(|error| MobileCaptureError::CaptureFailed {
                detail: error.to_string(),
            })
    }

    /// Every live project — the details disclosure's Project picker's read
    /// (review finding on #529's own PR): the brief's "one new door" clause
    /// governs [`capture_form_meta`]'s vocabulary/context bundle, not this
    /// one, and this read needs the live mirror ([`hummingbird_core::Core::projects`]),
    /// which a free function cannot reach. Shaped like [`MobileTaskHost::alerts`]
    /// — a plain state-bearing read, not a `"busy"`-wrapped one, since a
    /// checked-out mid-poll host still answers a synchronous method call
    /// truthfully (`Core::projects`'s own doc: "id order, for a stable list
    /// a caller can diff against its own").
    ///
    /// Only `id`/`name` cross, not the whole [`hummingbird_domain::Project`] row: a picker reads
    /// nothing else, and `ffi-web::task_host::TaskHostCore::projects`
    /// crosses the full row only because the web's `ProjectDTO` already
    /// carries the rest for other callers. This seam does have the frontier's
    /// "grouped by project" display now ([`MobileFrontierAxis::Project`],
    /// M3/#530), but it feeds off its own mapping of the same
    /// `Core::projects()` read — [`MobileTaskHost::now_board`]'s
    /// `frontier::ProjectName`, built where the board is built — and that is
    /// two mappings of one read at two layers, not a duplicate (#530's
    /// FINAL-GATE review): the picker needs `MobileProject` records crossing
    /// the seam to Kotlin, the grouping axis needs names inside Rust and
    /// never crosses at all.
    pub async fn projects(&self) -> Vec<MobileProject> {
        self.inner
            .lock()
            .await
            .core
            .projects()
            .into_iter()
            .map(|project| MobileProject { id: project.id, name: project.name })
            .collect()
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

    /// `NowScreen`'s whole read (M1-6/#504, widened to the frontier board
    /// by M3/#530): the frontier ([`hummingbird_core::Core::frontier`])
    /// plus the triage-process queue ([`Core::triage_inbox`],
    /// [`Core::grilling_items`], [`Core::grill_draft_item_ids`]), ordered,
    /// faceted and grouped by `axis` — [`build_now_board`]'s doc has the
    /// full recipe — plus [`Core::blocked`]'s relation-blocked section.
    /// Each row is already decided into a [`NowItemRecord`] — its
    /// [`MobileUrgencyBand`] ([`urgency::compute_urgency`]) and its
    /// wire-vocabulary [`available_actions`]
    /// ([`hummingbird_core::decisions::available_actions`]). See the
    /// module header for why this crosses one decided board rather than
    /// exposing the decision functions themselves — a render of the whole
    /// screen costs this one crossing, however many columns or rows it
    /// holds.
    ///
    /// This is the one seam door onto the frontier board: the flat
    /// [`Vec<NowItemRecord>`] `now_queue` door M1-6 shipped retired into
    /// this on M3/#530, rather than living alongside it as a second read
    /// path.
    ///
    /// `now` is deadline-shaped (`YYYY-MM-DDTHH:MM`), the host's own local
    /// wall clock already rendered into that shape — the same convention
    /// `urgency.rs`'s module header states for [`urgency::compute_urgency`]:
    /// `hummingbird-core` resolves no civil date to an instant, so Android,
    /// like the web seam, is the reader that does.
    pub async fn now_board(
        &self,
        axis: MobileFrontierAxis,
        facets: NowFacetSelectionRecord,
        now: String,
    ) -> NowBoardRecord {
        let inner = self.inner.lock().await;
        let frontier_items = inner.core.frontier();
        let triage_items = inner.core.triage_inbox();
        let grilling_items = inner.core.grilling_items();
        let draft_item_ids = inner.core.grill_draft_item_ids();
        let blocked = inner.core.blocked();
        let projects: Vec<frontier::ProjectName> = inner
            .core
            .projects()
            .into_iter()
            .map(|project| frontier::ProjectName { id: project.id, name: project.name })
            .collect();
        build_now_board(
            &frontier_items,
            &triage_items,
            &grilling_items,
            &draft_item_ids,
            &blocked,
            &projects,
            map_frontier_axis(axis),
            &to_facet_selection(&facets),
            &now,
        )
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
    /// milliseconds (unlike [`MobileTaskHost::now_board`]'s civil-time
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A [`CaptureDraft`] with only `title` set — every M1-5-era test below
    /// pins behaviour that predates the rest of the field set, so this
    /// keeps those cases unchanged rather than growing nine-field literals
    /// that have nothing to do with what each test actually asserts.
    fn title_only_draft(title: &str) -> CaptureDraft {
        CaptureDraft {
            title: title.to_string(),
            destination: CaptureDestination::Triage,
            size: String::new(),
            energy: String::new(),
            context: String::new(),
            description: String::new(),
            project_id: String::new(),
            priority: String::new(),
            deadline: String::new(),
            scheduled_date: String::new(),
        }
    }

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

        let id = host.capture(title_only_draft("buy milk"), 1_000).await.unwrap();
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

        let a = host.capture(title_only_draft("first"), 5_000).await.unwrap();
        let b = host.capture(title_only_draft("second"), 5_000).await.unwrap();
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

        assert!(host.capture(title_only_draft("   "), 1_000).await.is_ok());
    }

    // ------------------------------------------------------------- M3 (#529)

    /// [`capture_form_meta`] is a pure expose over
    /// `hummingbird_core::decisions::vocabulary` (#500) — this pins that it
    /// hands across the real values rather than some other list, so a
    /// vocabulary rename (ADR-0024's own history: `short` -> `normal`)
    /// cannot silently stop reaching Kotlin.
    #[test]
    fn capture_form_meta_carries_the_real_vocabulary_values() {
        let meta = capture_form_meta();
        assert_eq!(
            meta.sizes,
            vec![
                VocabOption { value: "quick".to_string(), label: "Quick".to_string() },
                VocabOption { value: "normal".to_string(), label: "Normal".to_string() },
                VocabOption { value: "deep".to_string(), label: "Deep".to_string() },
            ],
        );
        assert_eq!(
            meta.energies,
            vec![
                VocabOption { value: "low".to_string(), label: "Low".to_string() },
                VocabOption { value: "medium".to_string(), label: "Medium".to_string() },
                VocabOption { value: "high".to_string(), label: "High".to_string() },
            ],
        );
        assert_eq!(
            meta.suggested_contexts,
            vec!["@home", "@computer", "@phone", "@errands", "@garden"],
        );
    }

    /// [`MobileTaskHost::projects`] on a mirror that has never synced
    /// anything — the same "empty is a real, honest answer" case
    /// `ffi-web::task_host`'s own
    /// `a_fresh_host_reports_no_blocked_items_and_no_steps`-adjacent
    /// `projects()` test pins on the web seam.
    #[tokio::test]
    async fn projects_on_a_fresh_mirror_is_empty_not_busy() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m3-capture-projects-empty");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        assert_eq!(host.projects().await, Vec::<MobileProject>::new());
    }

    /// `CaptureActivity.kt`'s `PriorityRow` hardcodes its display order as
    /// `1, 2, 3, 4, 0` (Urgent..Low, then No priority last) because a plain
    /// JVM test cannot call a generated JNI binding directly
    /// (`CaptureSubmitRefusalTest`'s own doc — no host-arch `.so` in that
    /// process), so the pin lives here, on the Rust side of the seam: if
    /// `decisions::frontier::priority_rank`'s ordering ever changes, this
    /// test breaks and names the Kotlin literal (`PriorityRow`,
    /// `CaptureActivity.kt`) that must change to match.
    ///
    /// It reads the core rule **directly**, never through a
    /// `#[uniffi::export]`ed pass-through: exporting one would put a
    /// per-item decision function on the mobile seam that no Kotlin caller
    /// wants (the module doc's own asymmetry — Android reads applied
    /// results), to buy a test nothing but the test would use.
    #[test]
    fn the_priority_row_order_matches_priority_rank() {
        let mut wire_values = vec![0i64, 1, 2, 3, 4];
        wire_values.sort_by_key(|raw| frontier::priority_rank(*raw));
        assert_eq!(
            wire_values,
            vec![1, 2, 3, 4, 0],
            "PriorityRow's hardcoded Kotlin order (CaptureActivity.kt) must match this",
        );
    }

    /// `NowScreen.kt`'s facet chips hold the size, energy and axis
    /// vocabularies as Kotlin literals — the same shape `seam.ts` keeps on
    /// the web side, where `seam.test.ts` pins them against the core
    /// ("the one surviving unpinned vocabulary copy", the M1-2 review).
    /// The mobile half of that pin lives here for `priority_rank`'s own
    /// reason: a plain JVM test cannot call the generated JNI binding. If
    /// any of these vocabularies moves, this test breaks and names the
    /// Kotlin literal that must move with it.
    ///
    /// `URGENCY_VALUES` is deliberately absent: the facet's three words are
    /// `UrgencyBand` minus `calm` (a facet for "nothing pressing" is a
    /// facet for "everything"), which is a filter-vocabulary decision no
    /// core constant states — web's `URGENCIES` is unpinned for the same
    /// reason. The `when (band)` exhaustiveness gate in
    /// `NowScreenStructuralTest` is what catches a fifth band.
    #[test]
    fn the_now_screen_facet_vocabularies_match_the_core() {
        let sizes: Vec<String> = hummingbird_core::decisions::vocabulary::size_options()
            .into_iter()
            .map(|option| option.value)
            .collect();
        assert_eq!(
            sizes,
            vec!["quick", "normal", "deep"],
            "NowScreen.kt's SIZE_VALUES must match this",
        );

        let energies: Vec<String> = hummingbird_core::decisions::vocabulary::energy_options()
            .into_iter()
            .map(|option| option.value)
            .collect();
        assert_eq!(
            energies,
            vec!["low", "medium", "high"],
            "NowScreen.kt's ENERGY_VALUES must match this",
        );

        let axes: Vec<&str> = frontier::FRONTIER_GROUP_AXES
            .into_iter()
            .map(|axis| axis.as_str())
            .collect();
        assert_eq!(
            axes,
            vec!["context", "project", "size", "energy"],
            "NowScreen.kt's FRONTIER_AXES (and AXIS_LABEL/NO_VALUE_LABEL) must match this order",
        );
    }

    /// The whole field set reaches the captured [`Item`] in one mutation —
    /// the mobile twin of `ffi-web::task_host`'s
    /// `everything_the_capture_box_can_set_reaches_the_item_in_one_capture_mutation`.
    #[tokio::test]
    async fn a_full_draft_reaches_the_captured_item_in_one_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m3-capture-full-draft");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let draft = CaptureDraft {
            title: "renew the passport".to_string(),
            destination: CaptureDestination::Ready,
            size: "deep".to_string(),
            energy: "high".to_string(),
            context: "@computer".to_string(),
            description: "before the trip".to_string(),
            project_id: "proj-1".to_string(),
            priority: "2".to_string(),
            deadline: "2026-09-01".to_string(),
            scheduled_date: "2026-08-30".to_string(),
        };
        let id = host.capture(draft, 1_000).await.unwrap();

        let inner = host.inner.lock().await;
        let item = inner.core.frontier().into_iter().find(|item| item.id == id).unwrap();
        assert_eq!(item.stage, Stage::Ready);
        assert_eq!(item.size, Some(Size::Deep));
        assert_eq!(item.energy, Some(Energy::High));
        assert_eq!(item.context, Some("@computer".to_string()));
        assert_eq!(item.description, Some("before the trip".to_string()));
        assert_eq!(item.project_id, Some("proj-1".to_string()));
        assert_eq!(item.priority, 2);
        assert_eq!(item.deadline, Some("2026-09-01".to_string()));
        assert_eq!(item.scheduled_date, Some("2026-08-30".to_string()));
    }

    /// `destination: Triage` never reaches [`Core::frontier`] (Triage is
    /// filtered out of it, the same fact
    /// `capture_is_durable_before_any_network_is_touched` already leans on)
    /// — proved instead through the queue depth plus a `Ready` capture
    /// alongside it landing on the frontier, so both arms of the enum are
    /// pinned in one test.
    #[tokio::test]
    async fn destination_picks_the_captured_items_stage() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m3-capture-destination");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        host.capture(title_only_draft("stays in triage"), 1_000).await.unwrap();
        let ready_id = host
            .capture(
                CaptureDraft { destination: CaptureDestination::Ready, ..title_only_draft("goes to ready") },
                1_000,
            )
            .await
            .unwrap();

        let inner = host.inner.lock().await;
        let frontier = inner.core.frontier();
        assert_eq!(frontier.len(), 1, "only the Ready capture should be on the frontier");
        assert_eq!(frontier[0].id, ready_id);
        drop(inner);
        assert_eq!(host.queue_depth().await, 2, "the Triage capture is still durably queued");
    }

    /// An unrecognised size/energy word is refused before
    /// [`hummingbird_core::Core::capture`] is ever reached — the same
    /// "reject before the seam" discipline `FieldPatch::to_vocabulary`
    /// already applies to the item-edit form, proved here by the queue
    /// depth staying at zero.
    #[tokio::test]
    async fn an_unrecognised_size_or_energy_word_is_refused_before_the_core_is_reached() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m3-capture-bad-vocab");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let bad_size = CaptureDraft { size: "gigantic".to_string(), ..title_only_draft("x") };
        assert!(host.capture(bad_size, 1_000).await.is_err());

        let bad_energy = CaptureDraft { energy: "extreme".to_string(), ..title_only_draft("y") };
        assert!(host.capture(bad_energy, 1_000).await.is_err());

        assert_eq!(host.queue_depth().await, 0);
    }

    /// `"0"` is the priority `Select`'s own resting value — "not sent",
    /// never a sent zero (`priority_from_select`'s own doc) — pinned here
    /// at the seam so a capture with no priority chosen never asserts one.
    #[tokio::test]
    async fn priority_zero_is_not_sent_through_the_seam() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m3-capture-priority-zero");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let draft = CaptureDraft {
            destination: CaptureDestination::Ready,
            priority: "0".to_string(),
            ..title_only_draft("no priority chosen")
        };
        let id = host.capture(draft, 1_000).await.unwrap();

        let inner = host.inner.lock().await;
        let item = inner.core.frontier().into_iter().find(|item| item.id == id).unwrap();
        // The server's own default, never an asserted zero — same check
        // `hummingbird_core::decisions::capture`'s own doc gives.
        assert_eq!(item.priority, 0);
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

    /// Every frontier column's rows, in column-then-row order — the flat
    /// shape most of these tests want, over the whole board's own nested
    /// one.
    fn board_ids(board: &NowBoardRecord) -> Vec<String> {
        board
            .columns
            .iter()
            .flat_map(|column| column.items.iter().map(|record| record.id.clone()))
            .collect()
    }

    fn no_facets() -> frontier::FacetSelection {
        frontier::FacetSelection::default()
    }

    /// The shared-fixture ordering pin: the exact fixture shapes
    /// `hummingbird_core::decisions::frontier`'s own
    /// `ranks_by_priority_label_never_the_raw_wire_number` and
    /// `within_the_same_priority_orders_by_deadline_chronologically` tests
    /// use, proving [`build_now_board`] — `now_board`'s pure core — orders
    /// identically to the web frontier's own decision-sink pin, not a
    /// second, independently-drifting copy of the rule.
    #[test]
    fn now_board_orders_exactly_like_the_shared_frontier_fixtures() {
        let none = item("none", 0, None);
        let urgent = item("urgent", 1, None);
        let low = item("low", 4, None);

        let board = build_now_board(
            &[none, low, urgent],
            &[],
            &[],
            &[],
            &[],
            &[],
            frontier::FrontierAxis::Context,
            &no_facets(),
            "2026-08-15T12:00",
        );
        assert_eq!(board_ids(&board), vec!["urgent", "low", "none"]);

        let soon = item("soon", 1, Some("2026-08-15"));
        let later = item("later", 1, Some("2026-08-20"));
        let none_deadline = item("none-deadline", 1, None);

        let board = build_now_board(
            &[none_deadline, later, soon],
            &[],
            &[],
            &[],
            &[],
            &[],
            frontier::FrontierAxis::Context,
            &no_facets(),
            "2026-08-13T12:00",
        );
        assert_eq!(board_ids(&board), vec!["soon", "later", "none-deadline"]);
    }

    #[test]
    fn now_board_records_carry_urgency_priority_available_actions_and_stage() {
        let overdue = item("overdue-id", 2, Some("2020-01-01"));

        let board = build_now_board(
            &[overdue],
            &[],
            &[],
            &[],
            &[],
            &[],
            frontier::FrontierAxis::Context,
            &no_facets(),
            "2026-08-15T12:00",
        );

        assert_eq!(board.columns.len(), 1);
        let record = &board.columns[0].items[0];
        assert_eq!(record.urgency, MobileUrgencyBand::Overdue);
        assert_eq!(record.priority, 2);
        assert_eq!(
            record.available_actions,
            vec!["start", "complete", "block", "cancel"],
        );
        assert_eq!(record.stage, "ready");
    }

    #[test]
    fn now_board_is_a_pure_function_returning_the_same_output_every_call() {
        let items = vec![item("a", 3, None), item("b", 1, None)];
        let build = || {
            build_now_board(
                &items,
                &[],
                &[],
                &[],
                &[],
                &[],
                frontier::FrontierAxis::Context,
                &no_facets(),
                "2026-08-15T12:00",
            )
        };
        assert_eq!(build(), build());
    }

    /// Grouping: every column's own axis value and label, fullest first
    /// (the same rule [`frontier::group_frontier`]'s own tests already
    /// pin), decided all the way down to [`NowItemRecord`]s.
    #[test]
    fn now_board_groups_by_the_given_axis() {
        let phone_a = Item { context: Some("@phone".to_string()), ..item("a", 0, None) };
        let phone_b = Item { context: Some("@phone".to_string()), ..item("b", 0, None) };
        let computer = Item { context: Some("@computer".to_string()), ..item("c", 0, None) };

        let board = build_now_board(
            &[phone_a, phone_b, computer],
            &[],
            &[],
            &[],
            &[],
            &[],
            frontier::FrontierAxis::Context,
            &no_facets(),
            "2026-08-15T12:00",
        );

        assert_eq!(board.columns[0].value.as_deref(), Some("@phone"));
        assert_eq!(
            board.columns[0].items.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
            vec!["a", "b"],
        );
        assert_eq!(board.columns[1].value.as_deref(), Some("@computer"));
    }

    /// `live_column_keys` names every column the given axis produces over
    /// the pre-facet list — including one the active facet selection has
    /// just filtered every item out of, so a caller pruning a persisted
    /// collapse set against it never mistakes "hidden by the live filter"
    /// for "no longer exists".
    #[test]
    fn now_board_live_column_keys_survive_a_facet_that_empties_a_column() {
        let phone = Item { context: Some("@phone".to_string()), ..item("a", 0, None) };
        let computer = Item { context: Some("@computer".to_string()), ..item("b", 0, None) };
        let picked = frontier::toggle_facet(&no_facets(), frontier::Facet::Context, "@phone");

        let board = build_now_board(
            &[phone, computer],
            &[],
            &[],
            &[],
            &[],
            &[],
            frontier::FrontierAxis::Context,
            &picked,
            "2026-08-15T12:00",
        );

        // @computer has no columns left once the filter is applied...
        assert_eq!(board.columns.len(), 1);
        assert_eq!(board.columns[0].value.as_deref(), Some("@phone"));
        // ...but its key is still live, because the filter hid it rather
        // than it ceasing to exist.
        let mut keys = board.live_column_keys.clone();
        keys.sort();
        assert_eq!(keys, vec!["@computer", "@phone"]);
    }

    /// Facets narrow the shown set before grouping, exactly
    /// [`frontier::apply_facets`]'s own contract.
    #[test]
    fn now_board_applies_the_given_facet_selection() {
        let phone = Item { context: Some("@phone".to_string()), ..item("a", 0, None) };
        let computer = Item { context: Some("@computer".to_string()), ..item("b", 0, None) };
        let picked = frontier::toggle_facet(&no_facets(), frontier::Facet::Context, "@phone");

        let board = build_now_board(
            &[phone, computer],
            &[],
            &[],
            &[],
            &[],
            &[],
            frontier::FrontierAxis::Context,
            &picked,
            "2026-08-15T12:00",
        );

        assert_eq!(board_ids(&board), vec!["a"]);
    }

    /// `contexts` is computed over the ordered **pre-facet** list
    /// (`frontier::contexts_of`, mirroring `FrontierColumns.tsx`'s own
    /// `contextsOf(ordered)`), so a picked chip that narrows the board to
    /// nothing but itself does not also remove every other chip from the
    /// panel — the reviewer's own reason for pinning this to `ordered`
    /// rather than `shown`.
    #[test]
    fn now_board_carries_the_live_context_vocabulary_from_the_pre_facet_list() {
        let phone = Item { context: Some("@phone".to_string()), ..item("a", 0, None) };
        let computer = Item { context: Some("@computer".to_string()), ..item("b", 0, None) };
        let unjudged = item("c", 0, None);
        let picked = frontier::toggle_facet(&no_facets(), frontier::Facet::Context, "@phone");

        let board = build_now_board(
            &[phone, computer, unjudged],
            &[],
            &[],
            &[],
            &[],
            &[],
            frontier::FrontierAxis::Context,
            &picked,
            "2026-08-15T12:00",
        );

        // The board is narrowed to @phone alone, but the chip vocabulary
        // still names every context actually on the (unfiltered) board.
        assert_eq!(board_ids(&board), vec!["a"]);
        assert_eq!(board.contexts, vec!["@computer", "@phone", frontier::NO_CONTEXT]);
    }

    /// The triage-process queue rides inline, after the ordered frontier —
    /// the web board's own concatenation (`FrontierColumns.tsx`), so a
    /// capture with no context lands in the axis's no-value column,
    /// beneath any already-startable frontier item there.
    #[test]
    fn now_board_appends_the_triage_process_queue_after_the_ordered_frontier() {
        let ready = Item { context: Some("@phone".to_string()), ..item("ready-1", 1, None) };
        let triage_capture = Item {
            context: Some("@phone".to_string()),
            stage: Stage::Triage,
            ..item("triage-1", 0, None)
        };

        let board = build_now_board(
            &[ready],
            &[triage_capture],
            &[],
            &[],
            &[],
            &[],
            frontier::FrontierAxis::Context,
            &no_facets(),
            "2026-08-15T12:00",
        );

        assert_eq!(
            board.columns[0].items.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
            vec!["ready-1", "triage-1"],
        );
    }

    /// Blocked entries never join the frontier columns, and carry their
    /// still-open blockers' titles.
    #[test]
    fn now_board_carries_a_separate_blocked_section() {
        let blocker = item("blocker", 0, None);
        let blocked_item = item("blocked-1", 0, None);

        let board = build_now_board(
            &[],
            &[],
            &[],
            &[],
            &[(blocked_item, vec![blocker])],
            &[],
            frontier::FrontierAxis::Context,
            &no_facets(),
            "2026-08-15T12:00",
        );

        assert!(board.columns.is_empty());
        assert_eq!(board.blocked.len(), 1);
        assert_eq!(board.blocked[0].item.id, "blocked-1");
        assert_eq!(board.blocked[0].blocked_by_titles, vec!["item blocker"]);
    }

    /// A relation-blocked row offers the mark-done gesture and nothing
    /// else, whatever its stage says — `Start` on an item with an open
    /// blocker would mint an In Progress item that is still blocked, and
    /// the web blocked row has never offered it (`NowScreen.tsx`'s
    /// `canMarkDone`-gated `onComplete`).
    #[test]
    fn a_relation_blocked_row_offers_only_the_mark_done_gesture() {
        for stage in [Stage::Ready, Stage::InProgress, Stage::Blocked, Stage::Triage] {
            let blocked_item = Item { stage, ..item("blocked-1", 0, None) };
            let board = build_now_board(
                &[],
                &[],
                &[],
                &[],
                &[(blocked_item, vec![item("blocker", 0, None)])],
                &[],
                frontier::FrontierAxis::Context,
                &no_facets(),
                "2026-08-15T12:00",
            );

            assert_eq!(
                board.blocked[0].item.available_actions,
                vec!["complete"],
                "a {stage:?} blocked row must offer mark-done and nothing else",
            );
        }
    }

    /// Done and archived blocked rows offer nothing at all — the same
    /// `can_mark_done` answer, not a second rule.
    #[test]
    fn a_finished_or_archived_blocked_row_offers_no_action() {
        for blocked_item in [
            Item { stage: Stage::Done, ..item("blocked-done", 0, None) },
            Item { archived_at: Some(1), ..item("blocked-archived", 0, None) },
        ] {
            let board = build_now_board(
                &[],
                &[],
                &[],
                &[],
                &[(blocked_item, vec![item("blocker", 0, None)])],
                &[],
                frontier::FrontierAxis::Context,
                &no_facets(),
                "2026-08-15T12:00",
            );

            assert!(board.blocked[0].item.available_actions.is_empty());
        }
    }

    /// End-to-end proof that [`MobileTaskHost::now_board`] wires the real
    /// `Core::frontier` read through [`build_now_board`] — captures two
    /// items, promotes both to `Ready` with `Core::triage` (bypassing the
    /// FFI surface directly, same-crate access to `MobileTaskHost::inner`;
    /// triage itself is out of scope here) at different priorities, and
    /// asserts `now_board` returns them in the decided order.
    #[tokio::test]
    async fn now_board_reads_the_live_frontier_in_priority_order() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m3-now-board-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let low_id = host.capture(title_only_draft("low priority"), 1_000).await.unwrap();
        let urgent_id = host.capture(title_only_draft("urgent thing"), 1_000).await.unwrap();

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

        let board = host
            .now_board(
                MobileFrontierAxis::Context,
                NowFacetSelectionRecord::default(),
                "2026-08-15T12:00".to_string(),
            )
            .await;
        assert_eq!(board_ids(&board), vec![urgent_id, low_id]);
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

        let id = host.capture(title_only_draft("do it"), 1_000).await.unwrap();
        {
            let mut inner = host.inner.lock().await;
            inner
                .core
                .triage("seed", &id, true, hummingbird_core::TriagePatch::default(), 2_000)
                .await
                .unwrap();
        }

        host.act(id.clone(), "start".to_string(), 3_000).await.unwrap();

        let board = host
            .now_board(
                MobileFrontierAxis::Context,
                NowFacetSelectionRecord::default(),
                "2026-08-15T12:00".to_string(),
            )
            .await;
        let record = board
            .columns
            .iter()
            .flat_map(|column| column.items.iter())
            .find(|r| r.id == id)
            .expect("item still on the frontier");
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

        let id = host.capture(title_only_draft("whatever"), 1_000).await.unwrap();
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
        let id = host.capture(title_only_draft("buy milk"), 1_000).await.unwrap();

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
