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
//!
//! **Pane facts ride the one `rank_panes` crossing** (the pane-facts
//! slice): every [`MobileRankedPane`] carries its own question's decided
//! [`MobilePaneFacts`] — the same per-question fact sets the web reads
//! through its per-question wasm exports (`waste_facts_json` and kin).
//! Deliberately not a per-pane door: a surface's collapsed rows need every
//! pane's facts on every load to compose their headlines, so N doors would
//! be exactly the JNI-crossing-times-a-list cost this header bans, where
//! the record they already receive crosses once. Facts arrive as
//! structured data and gaps as *kinds*, never sentences (ADR-0025's cut);
//! the headline words stay in Kotlin.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

mod calendar_token;
mod core_lock;
mod diagnostics;

use calendar_token::{
    connection_state, mint_calendar_token, CalendarState, MintOutcome, ROTATION_MARGIN_MS,
};

use hummingbird_core::bindings::{Binding, BindingKey, BindingValue};
use hummingbird_core::calendar::{
    effective_selection, outcome_name, CalendarEventsResponse, CalendarHorizon, CalendarHostCore,
    CalendarSelection, EventRecord, EventStatus, EventWhen, CALENDAR_POLL_INTERVAL_MS,
};
use hummingbird_core::decisions::{
    available_actions, can_grill, can_mark_done, frontier, panes, queue, roster, rules, urgency,
};
use hummingbird_core::decisions::panes::contract::{
    AnswerState, Band, PaneAnswerCore, StandingQuestion, Surface, QUESTION_ORDER,
};
use hummingbird_core::decisions::panes::inputs::{
    BindingFact, BindingValueFact, CalendarEventFacts, CalendarEventStatusFact,
    CalendarEventWhenFacts, CalendarReadFacts, FreshnessFact, PaneInputs, PaneItemFacts,
    PaneReadFacts, SyncFacts,
};
use hummingbird_core::freshness::Freshness;
use hummingbird_core::decisions::panes::zone::{ZoneFact, ZoneFacts, ZoneQuery};
use hummingbird_core::decisions::panes::{
    github, homework, kimi, poller, race, reachability, scps, uptime, vacation, waste, weekend,
};
use hummingbird_core::pane::PaneEnvelope;
use hummingbird_core::sync::queue::{DeadLetterEntry, DeadLetterReason, MutationIntent};
use hummingbird_core::storage::FsSnapshotStore;
use hummingbird_core::sync::write::transport::{
    HttpMethod, MutationRequest, MutationTransport,
};
use hummingbird_core::sync::write::ReqwestMutationTransport;
use hummingbird_core::sync::{ReqwestSyncTransport, Trigger};
use hummingbird_core::{search, CaptureOptions, Core, CoreCycleOutcome, CoreEvent, ItemAction};
use hummingbird_domain::{
    Alert, Condition, CreatePushTarget, Energy, FieldType, Item, Platform, Rule, Size, Stage, Tier,
};
use hummingbird_rules_engine::Operator;

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

/// [`urgency::DeadlineParts`], mirrored as a `uniffi::Record` — a deadline
/// split into the two controls that edit it. `time` is `None` when the
/// deadline names a whole civil day, which is the resting shape:
/// `server/domain/src/deadline.rs` reads a date-only deadline as *end of
/// day*, never midnight, so "no time" and `T00:00` are different facts and
/// the `Option` is what keeps them apart across this seam.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DeadlineParts {
    pub date: String,
    pub time: Option<String>,
}

/// [`urgency::split_deadline`] — the mobile twin of
/// `ffi-web::decisions::split_deadline`, which had been this rule's only
/// seam since M1-2 sank it. Android's deadline control needs the same split
/// the web's `DeadlineField.tsx` has always used, and ADR-0025's
/// sink-as-you-go step is what makes crossing the sunk function the way to
/// get it: a Kotlin `substringBefore("T")` would be a second copy of a
/// grammar that already has an owner, and `CaptureFieldSetStructuralTest`'s
/// date-regex ban exists to catch exactly that.
///
/// A value in neither shape crosses back whole, as `date`, with no `time` —
/// [`urgency::split_deadline`]'s own doc says why, and the Android field
/// leans on it: a legacy free-text deadline stays visible rather than being
/// emptied the moment a form loads it.
#[uniffi::export]
pub fn split_deadline(value: &str) -> DeadlineParts {
    let parts = urgency::split_deadline(value);
    DeadlineParts { date: parts.date, time: parts.time }
}

/// [`urgency::join_deadline`], the inverse. `Option<String>` rather than
/// `Option<&str>` because uniffi crosses an optional argument as an owned
/// value; the core function takes the borrow, so this hands it one.
#[uniffi::export]
pub fn join_deadline(date: &str, time: Option<String>) -> String {
    urgency::join_deadline(date, time.as_deref())
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
    /// The item's judged size/energy, raw wire words (`quick`..., `low`...)
    /// or `None` when unjudged — the card's word-free glyphs (#558,
    /// ADR-0024) draw only a judged dimension and omit an absent one
    /// entirely, so `None` must survive the seam as `None`, never a
    /// default.
    pub size: Option<String>,
    pub energy: Option<String>,
    pub available_actions: Vec<String>,
    pub stage: String,
    /// `item-actions.ts`'s widened one-click rule ([`can_mark_done`]: any
    /// live, unarchived stage but Done), decided here exactly as
    /// [`MobileLedgerRowRecord`] carries it — the card's trailing check
    /// renders from this, never from `available_actions`, whose vocabulary
    /// a blocked row narrows.
    pub can_mark_done: bool,
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
        size: item.size.map(|size| size.as_str().to_string()),
        energy: item.energy.map(|energy| energy.as_str().to_string()),
        available_actions: actions,
        stage: item.stage.as_str().to_string(),
        can_mark_done: can_mark_done(item.stage, item.archived_at.is_some()),
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
///
/// `shown_count`/`total_count` are the filter disclosure's "N of M shown"
/// (`FrontierColumns.tsx`'s own meta line): post-facet and pre-facet sizes
/// of the same ordered list the columns are grouped from — blocked rows
/// count in neither, since facets never apply to them. Computed here
/// because Kotlin never holds the pre-facet list at all (ADR-0025: an
/// applied result crosses the seam, not the inputs to re-derive it).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NowBoardRecord {
    pub columns: Vec<NowColumnRecord>,
    pub blocked: Vec<NowBlockedEntryRecord>,
    pub contexts: Vec<String>,
    pub live_column_keys: Vec<String>,
    pub shown_count: u32,
    pub total_count: u32,
}

// -------------------------------------------------------------- M3 (#532)
// Done and the Ledger — the bottom nav's More sheet's first two screens.
// [`MobileTaskHost::done_items`]/[`MobileTaskHost::ledger_rows`] hand
// Kotlin a pre-ordered, pre-decided read exactly as [`MobileTaskHost::
// now_board`]'s own module-header rule states: `hummingbird_core::
// decisions::roster::{order_done, order_ledger, ledger_row_state,
// last_touched_ms}` run once here, never per row in Kotlin.

/// [`roster::LedgerRowState`], mirrored as a `uniffi::Enum` — Live or
/// Archived-with-its-instant, exactly as `ledger-order.ts`'s own
/// `LedgerRowState` union crosses on the web. [`map_ledger_row_state`] is
/// the only place the two are allowed to drift apart from, and it is
/// exhaustive with no wildcard arm for exactly that reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileLedgerRowState {
    Live,
    Archived { since_ms: i64 },
}

fn map_ledger_row_state(state: roster::LedgerRowState) -> MobileLedgerRowState {
    match state {
        roster::LedgerRowState::Live => MobileLedgerRowState::Live,
        roster::LedgerRowState::Archived { since_ms } => {
            MobileLedgerRowState::Archived { since_ms }
        }
    }
}

/// One Done screen row: an already-ordered, already-decided [`Item`] —
/// index order *is* display order ([`roster::order_done`], most recently
/// touched first). Read-only by decision: Done offers no act vocabulary
/// (`DoneScreen.tsx`'s own decision — there is no reopen), so this record
/// carries nothing to act on.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileDoneRecord {
    pub id: String,
    pub title: String,
    pub updated_at: i64,
    pub pending: bool,
}

fn to_roster_item(item: &Item) -> roster::RosterItem {
    roster::RosterItem { id: item.id.clone(), updated_at: item.updated_at }
}

/// One Ledger row: an item plus its already-decided [`MobileLedgerRowState`]
/// and last-touched instant ([`roster::ledger_row_state`]/
/// [`roster::last_touched_ms`]), the three badges `LedgerScreen.tsx`
/// renders, and `can_mark_done` — `item-actions.ts`'s own widened one-click
/// rule (any live, unarchived stage but Done), so the row checkmark never
/// offers what [`MobileTaskHost::act`] would refuse. Rows arrive
/// pre-ordered ([`roster::order_ledger`], last touched first); Kotlin does
/// no ordering.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileLedgerRowRecord {
    pub id: String,
    pub title: String,
    pub stage: String,
    pub state: MobileLedgerRowState,
    pub last_touched_ms: i64,
    pub pending: bool,
    pub dead_lettered: bool,
    pub has_live_alert: bool,
    pub can_mark_done: bool,
}

fn to_ledger_roster_item(entry: &hummingbird_core::LedgerEntry) -> roster::LedgerRosterItem {
    roster::LedgerRosterItem {
        id: entry.item.id.clone(),
        updated_at: entry.item.updated_at,
        archived_at: entry.item.archived_at,
        absent_since_ms: entry.absent_since_ms,
    }
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

    let shown_count = shown_ids.len() as u32;
    let total_count = ordered_entries.len() as u32;

    NowBoardRecord { columns, blocked, contexts, live_column_keys, shown_count, total_count }
}

// ------------------------------------------------------------- M4 (#542)
// Recall: `hummingbird_core::search` handles matching, grouping and
// ordering entirely core-side (ADR-0025) — this door only maps the
// already-capped, already-ordered rows to the wire shape and stamps
// `pending`, the same "no per-row decision in Kotlin" rule the ledger door
// above follows. `RecallScreen`/`RecallViewModel` re-derive none of it —
// see `RecallScreenStructuralTest`.

/// [`search::Group`], mirrored as a `uniffi::Enum` — Live, Done or Archived,
/// exactly as `RecallGroup` crosses on the web (`protocol.ts`).
/// [`map_recall_group`] is the only place the two are allowed to drift
/// apart from, and it is exhaustive with no wildcard arm for exactly that
/// reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileRecallGroup {
    Live,
    Done,
    Archived,
}

fn map_recall_group(group: search::Group) -> MobileRecallGroup {
    match group {
        search::Group::Live => MobileRecallGroup::Live,
        search::Group::Done => MobileRecallGroup::Done,
        search::Group::Archived => MobileRecallGroup::Archived,
    }
}

/// One Recall result row: an item's display fields flat at the top level
/// (`LedgerRowRecord`'s own shape), plus the [`MobileRecallGroup`] it
/// matched in and the same per-item `pending` stamp every other item read
/// carries. Never carries the resolved project name — the query matched
/// against it core-side (decision 11), and Recall's output is read-only in
/// this slice with no reason to duplicate a lookup `MobileTaskHost` already
/// answers elsewhere.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileRecallRowRecord {
    pub id: String,
    pub title: String,
    pub stage: String,
    pub group: MobileRecallGroup,
    pub updated_at: i64,
    pub pending: bool,
}

/// [`MobileTaskHost::search`]'s whole answer: the capped, ordered rows
/// (`hummingbird_core::search::CAP`), plus the un-capped `total` match count
/// the "N more" line reads — core-decided, never a UI invention (decision
/// 8), the same contract `ffi-web`'s own `SearchResponse` keeps.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileRecallOutcome {
    pub rows: Vec<MobileRecallRowRecord>,
    pub total: u32,
}

// ----------------------------------------------------------------- M3 (#531)
// The Triage screen's own door — the "triage process" queue
// (`hummingbird_core::decisions::queue`), already sunk from web and already
// riding inline into `build_now_board`'s combined list, exposed here as its
// own decided read so a dedicated screen never has to pick triage/grilling
// rows back out of the frontier board's columns.

/// One Triage-screen row: an already-decided [`Item`], carrying every field
/// [`hummingbird_core::TriagePatch`] can touch — the seeded editor's whole
/// starting draft — plus `stage` (a combined queue can hold a Grilling row
/// beside a Triage one, so the badge reads the item's own stage rather than
/// assuming "triage") and `can_mark_done`.
///
/// `can_mark_done` rides along rather than `available_actions`
/// ([`NowItemRecord`]'s own field): [`available_actions`] answers `&[]` for
/// both Triage and Grilling (`hummingbird_core::decisions::actions`'s own
/// doc — "neither is action yet"), so a row built the frontier board's way
/// would never offer the checkmark the web's `TriageRow` renders today.
/// [`can_mark_done`] is the wider, deliberately-separate rule that answers
/// this instead — the same one [`to_blocked_item_record`] already reaches
/// for.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TriageItemRecord {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub stage: String,
    pub size: Option<String>,
    pub energy: Option<String>,
    pub context: Option<String>,
    pub priority: i64,
    pub project_id: Option<String>,
    pub deadline: Option<String>,
    /// The deadline's decided urgency band against the caller's device-local
    /// `now` (#617's Triage-parity slice): the Triage rows render through the
    /// same card the frontier board's rows do, and that card's swatch/word
    /// read a decided band — never a Kotlin date comparison.
    pub urgency: MobileUrgencyBand,
    pub scheduled_date: Option<String>,
    pub source: Option<String>,
    pub created_at: i64,
    pub can_mark_done: bool,
    /// Whether the row's Grill button is live — `hummingbird_core::
    /// decisions::can_grill` verbatim (#539). The Triage board's own rows
    /// are always Triage or Grilling, so this reads `true` today, but the
    /// field crosses decided rather than assumed.
    pub can_grill: bool,
    /// Whether this item already carries a saved Grill draft (#356) — the
    /// button's own "Grill me"/"Resume grill" label source.
    pub has_grill_draft: bool,
}

fn to_triage_item_record(item: &Item, has_grill_draft: bool, now: &str) -> TriageItemRecord {
    TriageItemRecord {
        id: item.id.clone(),
        title: item.title.clone(),
        description: item.description.clone(),
        stage: item.stage.as_str().to_string(),
        size: item.size.map(|size| size.as_str().to_string()),
        energy: item.energy.map(|energy| energy.as_str().to_string()),
        context: item.context.clone(),
        priority: item.priority,
        project_id: item.project_id.clone(),
        deadline: item.deadline.clone(),
        urgency: map_urgency_band(urgency::compute_urgency(item.deadline.as_deref(), now)),
        scheduled_date: item.scheduled_date.clone(),
        source: item.source.clone(),
        created_at: item.created_at,
        can_mark_done: can_mark_done(item.stage, item.archived_at.is_some()),
        can_grill: can_grill(item.stage),
        has_grill_draft,
    }
}

/// [`MobileTaskHost::triage_board`]'s whole read, decided: [`queue::
/// triage_process_queue`]'s own ordered id list, resolved back to full rows
/// and its two record-field counts carried verbatim — never recomputed from
/// `items.len()` in Kotlin, which is exactly the "N captured · M grilling"
/// header's own acceptance criterion.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TriageBoardRecord {
    pub items: Vec<TriageItemRecord>,
    pub captured_count: u32,
    pub grilling_count: u32,
}

/// [`MobileTaskHost::triage_board`]'s pure core, free of `Core`/
/// `MobileTaskHost` for the same fixture-driven-test reason
/// [`build_now_board`]'s own doc gives.
fn build_triage_board(
    triage_items: &[Item],
    grilling_items: &[Item],
    draft_item_ids: &[String],
    now: &str,
) -> TriageBoardRecord {
    let by_id: HashMap<&str, &Item> = triage_items
        .iter()
        .chain(grilling_items.iter())
        .map(|item| (item.id.as_str(), item))
        .collect();

    let triage_queue: Vec<queue::QueueItem> = triage_items.iter().map(to_queue_item).collect();
    let grilling_queue: Vec<queue::QueueItem> = grilling_items.iter().map(to_queue_item).collect();
    let process = queue::triage_process_queue(&triage_queue, &grilling_queue, draft_item_ids);

    let draft_ids: std::collections::HashSet<&str> =
        draft_item_ids.iter().map(String::as_str).collect();
    let items = process
        .ids
        .iter()
        .filter_map(|id| {
            by_id
                .get(id.as_str())
                .map(|item| to_triage_item_record(item, draft_ids.contains(id.as_str()), now))
        })
        .collect();

    TriageBoardRecord {
        items,
        captured_count: process.captured_count as u32,
        grilling_count: process.grilling_count as u32,
    }
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
    /// ms epoch, or `None` for a live step — #539's addition, carried so a
    /// Grill confirm's `session_steps` argument can be rebuilt from this
    /// same record rather than a second, deleted-at-blind read of the
    /// checklist.
    pub deleted_at: Option<i64>,
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
    /// #771's vault-relative Obsidian path, carried so the record round-trips
    /// the whole item. **Nothing on Android draws it**: a gap stays silent,
    /// the same idiom the nav alarm follows — the column syncs, the phone
    /// renders nothing, and no affordance claims a capability the platform
    /// does not have.
    pub vault_path: Option<String>,
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
    /// Whether the pane draws its one-click done checkmark —
    /// [`hummingbird_core::item_detail::ItemDetail::can_mark_done`]
    /// verbatim.
    ///
    /// It rides separately from `available_actions` for the same reason it
    /// does on [`TriageItemRecord`]: [`available_actions`] answers `&[]`
    /// for Triage and Grilling ("neither is action yet"), so a pane that
    /// derived the checkmark from that vocabulary would lose it on exactly
    /// the two stages the Triage host opens. The consequence for the
    /// caller: render `available_actions` *minus* `complete`, or the
    /// affordance is drawn twice.
    pub can_mark_done: bool,
    /// [`MobileMicrotaskAffordance`], mirrored — #539's applied result.
    /// `None` for a non-editable (archived) item, exactly matching
    /// [`ItemDetailRecord::is_editable`]. Kotlin renders this; it decides
    /// nothing.
    pub microtask_affordance: Option<MobileMicrotaskAffordance>,
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
        vault_path: item.vault_path.clone(),
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
                deleted_at: step.deleted_at,
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
        can_mark_done: detail.can_mark_done,
        microtask_affordance: detail.microtask_affordance.map(to_mobile_microtask_affordance),
    }
}

/// [`hummingbird_core::decisions::skills::MicrotaskAffordance`], mirrored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileMicrotaskAffordance {
    Break,
    Rewrite { undone_count: u32 },
}

fn to_mobile_microtask_affordance(
    affordance: hummingbird_core::decisions::skills::MicrotaskAffordance,
) -> MobileMicrotaskAffordance {
    match affordance {
        hummingbird_core::decisions::skills::MicrotaskAffordance::Break => MobileMicrotaskAffordance::Break,
        hummingbird_core::decisions::skills::MicrotaskAffordance::Rewrite { undone_count } => {
            MobileMicrotaskAffordance::Rewrite { undone_count }
        }
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

/// [`ItemEdit`] → [`hummingbird_core::TriagePatch`], the one conversion
/// [`MobileTaskHost::edit_item`] and [`MobileTaskHost::triage_item`] both
/// need (review finding on #531's own PR — the two calls carried the
/// identical nine-field literal before this was pulled out). `Err` is an
/// unrecognised vocabulary word, rejected before the seam exactly as every
/// other closed-vocabulary string crossing here is.
fn to_triage_patch(edit: &ItemEdit) -> Result<hummingbird_core::TriagePatch, String> {
    Ok(hummingbird_core::TriagePatch {
        title: edit.title.clone(),
        priority: edit.priority,
        description: edit.description.to_text(),
        size: edit.size.to_vocabulary(Size::parse)?,
        energy: edit.energy.to_vocabulary(Energy::parse)?,
        context: edit.context.to_text(),
        project_id: edit.project_id.to_text(),
        deadline: edit.deadline.to_text(),
        scheduled_date: edit.scheduled_date.to_text(),
        // #771: the phone renders no vault-path affordance — Obsidian's
        // custom scheme is a desktop gesture — so this seam never touches
        // the column. `None` is "leave it alone", which is what keeps a
        // path set on the web from being cleared by an edit made here.
        vault_path: None,
    })
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


// ----------------------------------------------------------------- M4 (#540)
// The rules surface. Same asymmetry as every section above: Kotlin receives
// decided records, never a predicate to apply. Every verdict a rules screen
// renders — `is_valid`, the legal operator list, the value widget, the
// sub-alarm-interval warning, the backtest count — is decided in
// `hummingbird_core::decisions::rules` and arrives applied. The Compose
// screen holds no operator table, no duration regex, no `23:59`, and no
// notion of which fields a kind declares.

/// [`hummingbird_rules_engine::Operator`], mirrored as a `uniffi::Enum` for
/// the same reason [`MobileUrgencyBand`] mirrors its band: the core stays
/// binding-agnostic (ADR-0003), so this is a second uniffi-derived
/// definition rather than an annotation on someone else's type.
/// [`map_operator`] is the only place the two may drift apart from, and it
/// is exhaustive with no wildcard arm for exactly that reason — an eighth
/// operator added to ADR-0013's vocabulary fails this crate's build before
/// it ever reaches Kotlin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileOperator {
    Eq,
    Contains,
    Gt,
    Lt,
    Is,
    WithinNext,
    WithinLast,
}

fn map_operator(op: Operator) -> MobileOperator {
    match op {
        Operator::Eq => MobileOperator::Eq,
        Operator::Contains => MobileOperator::Contains,
        Operator::Gt => MobileOperator::Gt,
        Operator::Lt => MobileOperator::Lt,
        Operator::Is => MobileOperator::Is,
        Operator::WithinNext => MobileOperator::WithinNext,
        Operator::WithinLast => MobileOperator::WithinLast,
    }
}

fn unmap_operator(op: MobileOperator) -> Operator {
    match op {
        MobileOperator::Eq => Operator::Eq,
        MobileOperator::Contains => Operator::Contains,
        MobileOperator::Gt => Operator::Gt,
        MobileOperator::Lt => Operator::Lt,
        MobileOperator::Is => Operator::Is,
        MobileOperator::WithinNext => Operator::WithinNext,
        MobileOperator::WithinLast => Operator::WithinLast,
    }
}

/// [`hummingbird_domain::FieldType`], mirrored — ADR-0013's typed
/// catalogue. `Dynamic` is `snapshot_change`'s `value`/`previous` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileFieldType {
    Text,
    TextList,
    Number,
    Boolean,
    Timestamp,
    Date,
    Dynamic,
}

fn map_field_type(field_type: FieldType) -> MobileFieldType {
    match field_type {
        FieldType::String => MobileFieldType::Text,
        FieldType::StringList => MobileFieldType::TextList,
        FieldType::Number => MobileFieldType::Number,
        FieldType::Bool => MobileFieldType::Boolean,
        FieldType::Timestamp => MobileFieldType::Timestamp,
        FieldType::Date => MobileFieldType::Date,
        FieldType::Dynamic => MobileFieldType::Dynamic,
    }
}

/// [`rules::ValueWidget`], mirrored — the control one condition row's value
/// is edited with, decided by [`rules::widget_for`] and never by Kotlin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileValueWidget {
    Chips,
    Duration,
    Datetime,
    Boolean,
    Number,
    /// A pick from [`RuleFormRecord::sources`] — the `source` core field's
    /// frozen vocabulary. Kotlin renders it as a choice row; *which* field
    /// gets it is [`rules::widget_for`]'s answer, never Kotlin's.
    Source,
    Text,
}

fn map_widget(widget: rules::ValueWidget) -> MobileValueWidget {
    match widget {
        rules::ValueWidget::Chips => MobileValueWidget::Chips,
        rules::ValueWidget::Duration => MobileValueWidget::Duration,
        rules::ValueWidget::Datetime => MobileValueWidget::Datetime,
        rules::ValueWidget::Boolean => MobileValueWidget::Boolean,
        rules::ValueWidget::Number => MobileValueWidget::Number,
        rules::ValueWidget::Source => MobileValueWidget::Source,
        rules::ValueWidget::Text => MobileValueWidget::Text,
    }
}

/// [`hummingbird_domain::Tier`], mirrored — ADR-0012's two notification
/// tiers, which map to Android's own channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileTier {
    Urgent,
    Normal,
}

fn map_tier(tier: Tier) -> MobileTier {
    match tier {
        Tier::Urgent => MobileTier::Urgent,
        Tier::Normal => MobileTier::Normal,
    }
}

fn unmap_tier(tier: MobileTier) -> Tier {
    match tier {
        MobileTier::Urgent => Tier::Urgent,
        MobileTier::Normal => Tier::Normal,
    }
}

/// One condition row, decided. `value_display` is already rendered — a
/// scalar as itself, a list as its comma-joined members — so Kotlin never
/// inspects the untyped JSON a `Condition.value` actually is.
/// `below_alarm_interval` is #138's warning, already measured
/// ([`rules::is_below_alarm_interval`]): **warn, never reject**.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RuleConditionRecord {
    pub field: String,
    pub op: MobileOperator,
    pub value_display: String,
    pub negate: bool,
    pub widget: MobileValueWidget,
    pub below_alarm_interval: bool,
}

/// One rule, decided — the row behind the rules list and its editor.
///
/// `is_valid`/`invalid_fields` are [`rules::is_rule_valid`] already applied
/// against the compiled registry: a Kotlin re-derivation would be the
/// silently-dead-rule failure mode the whole check exists to prevent.
/// `enabled` is a *separate* fact and Kotlin must never read one for the
/// other — a disabled rule is not invalid, and an invalid rule is not
/// disabled.
///
/// `kind_label_key` is the kind's own registry key, or `"any_kind"` for
/// ADR-0013's null kind — a *key*, not a label, because the human wording
/// is rendering and stays per-client (ADR-0025's verdict table says so for
/// `kindLabel` explicitly).
///
/// `severity_is_unranked` is whether `severity` is outside
/// `hummingbird_domain::SEVERITIES` — a word `severity_rank` ranks at `0`,
/// so the ADR-0014 ratchet can never lift it. Answered here because it is
/// a vocabulary membership test, which is precisely the kind of thing a
/// hand-typed Kotlin list gets wrong.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RuleRecord {
    pub id: String,
    pub name: String,
    pub event_kind: Option<String>,
    pub kind_label_key: String,
    pub conditions: Vec<RuleConditionRecord>,
    pub severity: String,
    pub tier: MobileTier,
    pub enabled: bool,
    pub is_valid: bool,
    pub invalid_fields: Vec<String>,
    pub severity_is_unranked: bool,
    /// The CAS pivot every rule write needs — see [`AlertRecord`]'s own
    /// note on why a record carries its version.
    pub version: i64,
}

/// One selectable kind. `key: None` is ADR-0013's "any kind", always first.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct KindOptionRecord {
    pub key: Option<String>,
    pub label_key: String,
}

/// One legal operator on a field, paired with the value control that
/// operator implies — [`rules::widget_for`] is a function of *both* the
/// field and the operator, and a row whose operator has just changed must
/// be able to follow it. `source` is why: the frozen-vocabulary picker is
/// its answer under `eq`, but `contains` is substring matching (a
/// condition may name `city-waste` to reach both versions of it), which a
/// whole-value picker cannot express.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RuleOperatorRecord {
    pub operator: MobileOperator,
    pub widget: MobileValueWidget,
}

/// One field the editor offers, with its cascade already resolved:
/// the operators legal for its type ([`rules::legal_operators`], derived
/// from the authority's own gating function) each already carrying the
/// widget it implies, and the duration units a `date` field narrows to.
///
/// The widgets are resolved here, once per form open, rather than exposed
/// as a per-keystroke seam call — the crate header's rule — which is why
/// this is a list and not a function. `legal_operators` is the same list
/// of operators in the same order, kept for the operator picker itself.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RuleFieldRecord {
    pub name: String,
    pub field_type: MobileFieldType,
    pub legal_operators: Vec<MobileOperator>,
    pub operators: Vec<RuleOperatorRecord>,
    pub duration_units: Vec<String>,
}

/// One selectable `source` — [`rules::SourceOption`], mirrored.
/// `retired_as` is `Some(successor)` for a source ADR-0014 has bumped;
/// such an entry is still *listed* (an existing rule may name one, and a
/// picker that hid it would show nothing selected) and must not be
/// newly pickable, because the authority already 400s that save.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SourceOptionRecord {
    pub source: String,
    pub retired_as: Option<String>,
}

/// Everything the create-and-edit form needs for one chosen kind, decided
/// once per form open rather than per row — the seam rule this crate's
/// header states.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RuleFormRecord {
    pub kind_options: Vec<KindOptionRecord>,
    pub fields: Vec<RuleFieldRecord>,
    pub severities: Vec<String>,
    /// The `source` field's vocabulary, in the registry's own registration
    /// order — what a [`MobileValueWidget::Source`] row picks from. A
    /// hand-typed Kotlin list of source strings is exactly the drift this
    /// exists to prevent.
    pub sources: Vec<SourceOptionRecord>,
    /// The severity a fresh draft opens on —
    /// [`hummingbird_core::decisions::rules::DEFAULT_SEVERITY`], not the
    /// head of `severities`, which is a ratchet order and not a default.
    pub default_severity: String,
    pub tiers: Vec<MobileTier>,
    pub alarm_interval_ms: i64,
}

/// A draft rule's backtest against this device's own frontier (ADR-0011).
///
/// `corpus_note_key` names the caveat the UI must show beside the count,
/// never a bare "N matches": the corpus here is the frontier, not the
/// sweep's `load_live_items`, so triage-stage and blocked items are
/// outside it (`rules::backtest`'s header, gap 1). A key rather than the
/// sentence, for the same rendering-stays-per-client reason as
/// `kind_label_key`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BacktestRecord {
    pub is_available: bool,
    pub match_count: u32,
    pub corpus_note_key: String,
}

/// One condition as the editor collects it, on the way *in*. `value` is a
/// single string in every case — the typing back into a JSON literal is
/// decided Rust-side from the field's declared type, so Kotlin never
/// constructs the untyped value a `Condition` carries and cannot get a
/// number-vs-string literal wrong.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RuleConditionInput {
    pub field: String,
    pub op: MobileOperator,
    pub value: String,
    pub negate: bool,
}

/// A rules write failed. No field named `message` anywhere in this crate's
/// records — uniffi reserves it.
#[derive(Debug, uniffi::Error)]
pub enum MobileRuleError {
    RuleNotFound,
    /// A condition names a field the chosen kind does not declare, or a
    /// value its type cannot hold. Refused at the seam, never sent.
    InvalidCondition { detail: String },
    SaveFailed { detail: String },
}

impl std::fmt::Display for MobileRuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobileRuleError::RuleNotFound => write!(f, "rule not found"),
            MobileRuleError::InvalidCondition { detail } => write!(f, "{detail}"),
            MobileRuleError::SaveFailed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for MobileRuleError {}

/// ADR-0013's null kind, as a rendering key.
const ANY_KIND_KEY: &str = "any_kind";

/// The one caveat the backtest count must always carry — see
/// [`BacktestRecord`].
const BACKTEST_CORPUS_NOTE_KEY: &str = "backtest_corpus_frontier_only";

/// A condition's untyped JSON value, rendered for display. A list joins
/// with `", "` (ADR-0013's any-of); everything else renders as itself, a
/// string without its JSON quotes.
fn condition_value_display(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(items) => items
            .iter()
            .map(condition_value_display)
            .collect::<Vec<_>>()
            .join(", "),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn to_rule_condition_record(
    condition: &Condition,
    registry: &rules::KindRegistry,
    event_kind: Option<&str>,
    alarm_interval_ms: i64,
) -> RuleConditionRecord {
    let op = Operator::parse(&condition.op).unwrap_or(Operator::Eq);
    let field_type = rules::field_type(registry, event_kind, &condition.field)
        .unwrap_or(FieldType::String);
    let display = condition_value_display(&condition.value);
    RuleConditionRecord {
        field: condition.field.clone(),
        op: map_operator(op),
        value_display: display.clone(),
        negate: condition.negate,
        widget: map_widget(rules::widget_for(&condition.field, field_type, op)),
        below_alarm_interval: rules::is_below_alarm_interval(&display, alarm_interval_ms),
    }
}

fn to_rule_record(rule: &Rule, registry: &rules::KindRegistry) -> RuleRecord {
    let event_kind = rule.event_kind.as_deref();
    let invalid = rules::invalid_fields(&rule.conditions, event_kind, registry);
    RuleRecord {
        id: rule.id.clone(),
        name: rule.name.clone(),
        event_kind: rule.event_kind.clone(),
        kind_label_key: event_kind.unwrap_or(ANY_KIND_KEY).to_string(),
        conditions: rule
            .conditions
            .iter()
            .map(|condition| {
                to_rule_condition_record(
                    condition,
                    registry,
                    event_kind,
                    registry.alarm_interval_ms,
                )
            })
            .collect(),
        severity: rule.severity.clone(),
        tier: map_tier(rule.tier),
        enabled: rule.enabled,
        is_valid: invalid.is_empty(),
        invalid_fields: invalid,
        severity_is_unranked: !registry.severities.iter().any(|s| s == &rule.severity),
        version: rule.version,
    }
}

fn to_rule_field_record(field: &rules::KindField) -> RuleFieldRecord {
    let legal = rules::legal_operators(field.field_type);
    RuleFieldRecord {
        name: field.name.clone(),
        field_type: map_field_type(field.field_type),
        legal_operators: legal.iter().copied().map(map_operator).collect(),
        operators: legal
            .iter()
            .map(|op| RuleOperatorRecord {
                operator: map_operator(*op),
                widget: map_widget(rules::widget_for(&field.name, field.field_type, *op)),
            })
            .collect(),
        duration_units: rules::duration_units_for(field.field_type)
            .into_iter()
            .map(|unit| rules::duration::duration_unit_str(unit).to_string())
            .collect(),
    }
}

/// One editor-collected condition, typed back into the `Condition` the
/// wire carries. The literal's JSON kind follows the **field's declared
/// type**, exactly what `validate_rule` checks at save time — a `number`
/// field gets a number, a `bool` field a bool, a `string_list` field the
/// comma-split list ADR-0013's any-of means, everything else a string.
/// An unparseable number or bool is refused here rather than sent as a
/// string the authority would reject with a 400.
fn to_condition(
    input: &RuleConditionInput,
    registry: &rules::KindRegistry,
    event_kind: Option<&str>,
) -> Result<Condition, MobileRuleError> {
    let field_type = rules::field_type(registry, event_kind, &input.field).ok_or_else(|| {
        MobileRuleError::InvalidCondition {
            detail: format!("{} is not a field this kind declares", input.field),
        }
    })?;
    let op = unmap_operator(input.op);
    if !rules::legal_operators(field_type).contains(&op) {
        return Err(MobileRuleError::InvalidCondition {
            detail: format!("{} is not legal on {}", op.as_str(), input.field),
        });
    }
    let value = match field_type {
        FieldType::Number if !matches!(op, Operator::WithinNext | Operator::WithinLast) => {
            let parsed: f64 = input.value.trim().parse().map_err(|_| {
                MobileRuleError::InvalidCondition {
                    detail: format!("{:?} is not a number", input.value),
                }
            })?;
            serde_json::json!(parsed)
        }
        FieldType::Bool => {
            let parsed: bool = input.value.trim().parse().map_err(|_| {
                MobileRuleError::InvalidCondition {
                    detail: format!("{:?} is not true or false", input.value),
                }
            })?;
            serde_json::json!(parsed)
        }
        FieldType::StringList => serde_json::Value::Array(
            input
                .value
                .split(',')
                .map(|part| serde_json::Value::String(part.trim().to_string()))
                .filter(|part| part.as_str() != Some(""))
                .collect(),
        ),
        _ => serde_json::Value::String(input.value.clone()),
    };
    Ok(Condition {
        field: input.field.clone(),
        op: op.as_str().to_string(),
        value,
        negate: input.negate,
    })
}

fn to_conditions(
    inputs: &[RuleConditionInput],
    registry: &rules::KindRegistry,
    event_kind: Option<&str>,
) -> Result<Vec<Condition>, MobileRuleError> {
    inputs
        .iter()
        .map(|input| to_condition(input, registry, event_kind))
        .collect()
}

/// One mirrored item as the sweep's `item_threshold_event` would see it —
/// `occurred_at_utc` resolved by the caller's clock, exactly as the web's
/// seam does it, for the tzdb reason in `rules::backtest`'s header.
fn to_backtest_record(outcome: rules::BacktestOutcome) -> BacktestRecord {
    match outcome {
        rules::BacktestOutcome::Unavailable { .. } => BacktestRecord {
            is_available: false,
            match_count: 0,
            corpus_note_key: BACKTEST_CORPUS_NOTE_KEY.to_string(),
        },
        rules::BacktestOutcome::Ok { matched_ids } => BacktestRecord {
            is_available: true,
            match_count: matched_ids.len() as u32,
            corpus_note_key: BACKTEST_CORPUS_NOTE_KEY.to_string(),
        },
    }
}

fn to_backtest_item(item: &Item, occurred_at_utc: String) -> rules::BacktestItem {
    rules::BacktestItem {
        id: item.id.clone(),
        occurred_at_utc,
        title: item.title.clone(),
        body: item.description.clone(),
        url: item.source_url.clone(),
        deadline: item.deadline.clone(),
        scheduled_date: item.scheduled_date.clone(),
        stage: item.stage.as_str().to_string(),
        size: item.size.map(|size| size.as_str().to_string()),
        energy: item.energy.map(|energy| energy.as_str().to_string()),
        context: item.context.clone(),
        priority: item.priority,
        project_id: item.project_id.clone(),
        source: item.source.clone(),
        source_key: item.source_key.clone(),
    }
}

// ------------------------------------------------------------- panes (#536)
//
// The pane lane's mobile door (#536/M4, ADR-0025): the two-phase zone
// bridge plus the ranked-panes call, mirrored as `uniffi` types so Android
// receives **applied results** and never a per-pane decision function —
// this seam's own asymmetry rule (module header). `hummingbird_core::
// decisions::panes` already carries the whole decision; everything below
// is exposure, not judgement.
//
// **#537 grew the builder to the whole surface pair.** [`mobile_pane_inputs`]
// (né `status_pane_inputs`, #536) now wires every field a sunk pane reads:
// the status four's three sources, plus waste's and race's
// `context_snapshots` sources, the bindings table (waste/race/vacation) and
// the actionable-items list (weekend's merge). **#775 grew the source loop
// again**: `poller::poller_sources()` now names nine sources, not five, and
// the loop below reads every one of them whether or not a sunk pane also
// claims it. **#564/#621 filled the last
// field**: the calendar arm now carries this device's own `calendar_reads`
// and `calendar_connected` off the lane below, so weekend and vacation
// answer for real once a calendar is connected and fall back to their
// honest "not connected" state only when it is not — never a fabricated
// read either way. The door's shape (`surface` in,
// applied results out) did not change under this growth, exactly as #536
// predicted.
//
// **The drift gate.** [`map_standing_question`] matches
// [`StandingQuestion`] itself, exhaustively and with no wildcard arm — a
// ninth question sunk into `panes::SUNK` fails *this* match at compile
// time, before Android ever sees it, which is what forces
// `StatusScreen.kt`'s own exhaustive `when` to be touched rather than
// silently skipped.

/// [`Surface`], mirrored as a `uniffi::Enum` — ADR-0017's ranked-region
/// axis, named on this seam so a caller picks which region it wants
/// without a string to typo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileSurface {
    Now,
    Status,
}

fn map_surface(surface: MobileSurface) -> Surface {
    match surface {
        MobileSurface::Now => Surface::Now,
        MobileSurface::Status => Surface::Status,
    }
}

/// [`StandingQuestion`], mirrored as a `uniffi::Enum` — see this section's
/// header for why this mirror (rather than the plain wire string
/// `RankedPaneRecord.question` carries core-side) is what Android renders
/// against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileStandingQuestion {
    Homework,
    Scps,
    Waste,
    Weekend,
    Vacation,
    Race,
    Kimi,
    Github,
    Uptime,
    Reachability,
    Poller,
}

/// Exhaustive over [`StandingQuestion`] with no wildcard arm — the whole
/// drift gate this section's header describes.
fn map_standing_question(question: StandingQuestion) -> MobileStandingQuestion {
    match question {
        StandingQuestion::Homework => MobileStandingQuestion::Homework,
        StandingQuestion::Scps => MobileStandingQuestion::Scps,
        StandingQuestion::Waste => MobileStandingQuestion::Waste,
        StandingQuestion::Weekend => MobileStandingQuestion::Weekend,
        StandingQuestion::Vacation => MobileStandingQuestion::Vacation,
        StandingQuestion::Race => MobileStandingQuestion::Race,
        StandingQuestion::Kimi => MobileStandingQuestion::Kimi,
        StandingQuestion::Github => MobileStandingQuestion::Github,
        StandingQuestion::Uptime => MobileStandingQuestion::Uptime,
        StandingQuestion::Reachability => MobileStandingQuestion::Reachability,
        StandingQuestion::Poller => MobileStandingQuestion::Poller,
    }
}

/// [`map_standing_question`]'s inverse — also wildcard-free, so an
/// eleventh question fails to compile in both directions.
fn unmap_standing_question(question: MobileStandingQuestion) -> StandingQuestion {
    match question {
        MobileStandingQuestion::Homework => StandingQuestion::Homework,
        MobileStandingQuestion::Scps => StandingQuestion::Scps,
        MobileStandingQuestion::Waste => StandingQuestion::Waste,
        MobileStandingQuestion::Weekend => StandingQuestion::Weekend,
        MobileStandingQuestion::Vacation => StandingQuestion::Vacation,
        MobileStandingQuestion::Race => StandingQuestion::Race,
        MobileStandingQuestion::Kimi => StandingQuestion::Kimi,
        MobileStandingQuestion::Github => StandingQuestion::Github,
        MobileStandingQuestion::Uptime => StandingQuestion::Uptime,
        MobileStandingQuestion::Reachability => StandingQuestion::Reachability,
        MobileStandingQuestion::Poller => StandingQuestion::Poller,
    }
}

/// One standing-question roster entry (#714, ADR-0034 decision 4), mirrored
/// as a `uniffi::Record`.
///
/// `question` and `surface` cross as this seam's own enums so a Kotlin
/// `when` over them stays exhaustive; `bindings` crosses as the binding
/// keys' wire spellings, because that is what a `settings` row is keyed by
/// and what a Kotlin caller will match a binding row against.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileQuestionRosterEntry {
    pub question: MobileStandingQuestion,
    pub label: String,
    pub surface: MobileSurface,
    pub bindings: Vec<String>,
}

/// The whole roster, in `QUESTION_ORDER` — an **applied result**, this
/// seam's own rule (module header): Android receives the assembled list and
/// never the three per-question functions behind it, so it cannot hold an
/// opinion about which questions exist.
///
/// **Android does not render this yet.** ADR-0034 decision 4 splits the
/// rendering into #716 on purpose — that surface has no emulator matrix, so
/// a UI change there owes a device run — and this slice lands the door so
/// #716 is rendering-only.
#[uniffi::export]
pub fn question_roster() -> Vec<MobileQuestionRosterEntry> {
    hummingbird_core::decisions::question_roster()
        .into_iter()
        .map(|entry| MobileQuestionRosterEntry {
            question: map_standing_question(entry.question),
            label: entry.label.to_string(),
            surface: match entry.surface {
                Surface::Now => MobileSurface::Now,
                Surface::Status => MobileSurface::Status,
            },
            bindings: entry
                .bindings
                .iter()
                .map(|key| key.as_str().to_string())
                .collect(),
        })
        .collect()
}

/// [`AnswerState`], mirrored as a `uniffi::Enum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobilePaneAnswerState {
    Answered,
    BoundButUnacquired,
    Unbound,
}

fn map_answer_state(state: AnswerState) -> MobilePaneAnswerState {
    match state {
        AnswerState::Answered => MobilePaneAnswerState::Answered,
        AnswerState::BoundButUnacquired => MobilePaneAnswerState::BoundButUnacquired,
        AnswerState::Unbound => MobilePaneAnswerState::Unbound,
    }
}

/// [`Band`], mirrored as a `uniffi::Enum` — ADR-0015's five-band salience
/// vocabulary, in [`super::contract::BAND_ORDER`]'s own declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobilePaneBand {
    Live,
    Imminent,
    Near,
    Distant,
    Dormant,
}

fn map_band(band: Band) -> MobilePaneBand {
    match band {
        Band::Live => MobilePaneBand::Live,
        Band::Imminent => MobilePaneBand::Imminent,
        Band::Near => MobilePaneBand::Near,
        Band::Distant => MobilePaneBand::Distant,
        Band::Dormant => MobilePaneBand::Dormant,
    }
}

/// [`PaneAnswerCore`], mirrored as a `uniffi::Record` — the three decided
/// fields and nothing else (no headline, no glyph: both stay per-client,
/// exactly the core type's own doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct MobilePaneAnswer {
    pub answer_state: MobilePaneAnswerState,
    pub band: MobilePaneBand,
    pub within_band: Option<i64>,
}

fn to_mobile_pane_answer(answer: PaneAnswerCore) -> MobilePaneAnswer {
    MobilePaneAnswer {
        answer_state: map_answer_state(answer.answer_state),
        band: map_band(answer.band),
        within_band: answer.within_band,
    }
}

/// One ranked pane, as Android renders it — [`panes::contract::
/// RankedPaneRecord`] with its `question: String` resolved to
/// [`MobileStandingQuestion`] (this section's drift gate) rather than
/// carried across as the open wire string.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileRankedPane {
    pub standing_question: MobileStandingQuestion,
    /// Which subject this pane answers for (a workflow file name, a
    /// service id, or a fixed sentinel for a one-subject question) — never
    /// used to route rendering (that is [`standing_question`]'s job), only
    /// to label and key one pane among several the same question may
    /// return (`github_subjects`/`uptime_subjects`).
    pub subject_key: String,
    /// The stable per-pane identity ([`panes::contract::pane_key`]) — the
    /// collapse-state and React-key equivalent on this client.
    pub pane_key: String,
    pub answer: MobilePaneAnswer,
    /// This pane's own decided fact set (the pane-facts slice) — what the
    /// collapsed headline and the expanded rendering read. See the module
    /// header for why facts ride this record rather than per-pane doors.
    pub facts: MobilePaneFacts,
}

// ----------------------------------------------------- pane facts (mirrors)
// Every type below is a `uniffi` mirror of a `hummingbird_core::decisions::
// panes` fact/gap type — the ADR-0003 second-definition convention
// [`MobileUrgencyBand`] states: one derive-annotated twin per core type, one
// exhaustive mapping function with no wildcard arm as the only place the two
// may drift apart from. Facts are records, gaps are enums whose arms are
// KINDS carrying data, never sentences — the words stay per-client.

/// [`FreshnessFact`], mirrored as a `uniffi::Enum`.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobilePaneFreshness {
    Unknown,
    Age { age_ms: i64, declared_cadence_ms: Option<i64> },
}

fn map_pane_freshness(freshness: FreshnessFact) -> MobilePaneFreshness {
    match freshness {
        FreshnessFact::Unknown => MobilePaneFreshness::Unknown,
        FreshnessFact::Age { age_ms, declared_cadence_ms } => {
            MobilePaneFreshness::Age { age_ms, declared_cadence_ms }
        }
    }
}

/// [`waste::Stream`], mirrored — kerb order is the CLIENT's to apply
/// (`STREAM_ORDER`), so the list crosses in payload order exactly as the
/// core type carries it.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum MobileWasteStream {
    Trash,
    Recycling,
    Yard,
}

fn map_waste_stream(stream: waste::Stream) -> MobileWasteStream {
    match stream {
        waste::Stream::Trash => MobileWasteStream::Trash,
        waste::Stream::Recycling => MobileWasteStream::Recycling,
        waste::Stream::Yard => MobileWasteStream::Yard,
    }
}

/// [`waste::WasteGap`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileWasteGap {
    NotFetched,
    Malformed { reason: String },
    UnknownSchema { schema: String },
    NotJson,
    NotAnObject,
    NoZone,
    BadDates,
    UnknownStream,
    UnresolvableZone { zone: String },
    PastCollection { collected_on: String, weekday_index: u8 },
}

fn map_waste_gap(gap: waste::WasteGap) -> MobileWasteGap {
    match gap {
        waste::WasteGap::NotFetched => MobileWasteGap::NotFetched,
        waste::WasteGap::Malformed { reason } => MobileWasteGap::Malformed { reason },
        waste::WasteGap::UnknownSchema { schema } => MobileWasteGap::UnknownSchema { schema },
        waste::WasteGap::NotJson => MobileWasteGap::NotJson,
        waste::WasteGap::NotAnObject => MobileWasteGap::NotAnObject,
        waste::WasteGap::NoZone => MobileWasteGap::NoZone,
        waste::WasteGap::BadDates => MobileWasteGap::BadDates,
        waste::WasteGap::UnknownStream => MobileWasteGap::UnknownStream,
        waste::WasteGap::UnresolvableZone { zone } => MobileWasteGap::UnresolvableZone { zone },
        waste::WasteGap::PastCollection { collected_on, weekday_index } => {
            MobileWasteGap::PastCollection { collected_on, weekday_index }
        }
    }
}

/// [`waste::WasteFacts`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileWasteFacts {
    pub zone: String,
    pub scheduled: String,
    pub collected_on: String,
    pub streams: Vec<MobileWasteStream>,
    pub today: String,
    pub days_away: i64,
    pub holiday: bool,
    pub weekday_index: u8,
    pub stale: bool,
    pub starts_at_ms: i64,
    pub freshness: MobilePaneFreshness,
}

/// [`waste::WasteSetup`]'s **kind**, mirrored — the payload (`page`) is
/// deliberately dropped: nothing a client renders needs the address, and a
/// bound page already reaches the host inside the facts.
///
/// This crosses because [`waste::waste_facts`] folds `Unread` and `Unusable`
/// onto the same [`MobilePaneAnswerState::BoundButUnacquired`], so the answer
/// state alone cannot tell "this device has not read its bindings yet" from
/// "the binding holds something unusable" — the first is a wait, the second
/// is a repair the reader can make in Settings. The web recovers the same
/// distinction through its own `waste_setup_json` door (`seam.ts`), and
/// renders "Checking setup" against "Setup needs a look".
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileWasteSetup {
    Bound,
    Unread,
    Unusable,
    Unset,
}

fn map_waste_setup(setup: waste::WasteSetup) -> MobileWasteSetup {
    match setup {
        waste::WasteSetup::Bound { .. } => MobileWasteSetup::Bound,
        waste::WasteSetup::Unread => MobileWasteSetup::Unread,
        waste::WasteSetup::Unusable => MobileWasteSetup::Unusable,
        waste::WasteSetup::Unset => MobileWasteSetup::Unset,
    }
}

/// [`waste::WasteResolved`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileWasteResolved {
    Facts { facts: MobileWasteFacts },
    Gap { gap: MobileWasteGap },
}

fn map_waste_resolved(resolved: waste::WasteResolved) -> MobileWasteResolved {
    match resolved {
        waste::WasteResolved::Facts(facts) => MobileWasteResolved::Facts {
            facts: MobileWasteFacts {
                zone: facts.zone,
                scheduled: facts.scheduled,
                collected_on: facts.collected_on,
                streams: facts.streams.into_iter().map(map_waste_stream).collect(),
                today: facts.today,
                days_away: facts.days_away,
                holiday: facts.holiday,
                weekday_index: facts.weekday_index,
                stale: facts.stale,
                starts_at_ms: facts.starts_at_ms,
                freshness: map_pane_freshness(facts.freshness),
            },
        },
        waste::WasteResolved::Gap { gap } => MobileWasteResolved::Gap { gap: map_waste_gap(gap) },
    }
}

/// [`weekend::WeekendDay`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileWeekendDay {
    pub date: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// [`weekend::WeekendWindow`], mirrored — `days` holds only the days that
/// have not yet ended at the device (three while the weekend is still
/// ahead, then shrinking to Sunday alone; never empty), so the `Vec` is
/// load-bearing rather than a uniffi workaround. That field's own doc in
/// `weekend.rs` is canonical.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileWeekendWindow {
    pub start_ms: i64,
    pub end_ms: i64,
    pub days: Vec<MobileWeekendDay>,
    pub under_way: bool,
}

/// [`weekend::WindowCounts`], mirrored — tallied from the entries
/// themselves core-side, so the two can never disagree.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileWeekendCounts {
    pub events: i64,
    pub due: i64,
    pub scheduled: i64,
}

/// [`weekend::WeekendGap`], mirrored.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum MobileWeekendGap {
    NotConnected,
    Unacquired,
    UnresolvableZone,
}

fn map_weekend_entry(entry: weekend::WindowEntry) -> MobileWeekendEntry {
    MobileWeekendEntry {
        id: entry.id,
        kind: match entry.kind {
            weekend::EntryKind::Event => MobileWeekendEntryKind::Event,
            weekend::EntryKind::Due => MobileWeekendEntryKind::Due,
            weekend::EntryKind::Scheduled => MobileWeekendEntryKind::Scheduled,
        },
        title: entry.title,
        at_ms: entry.at_ms,
        anchor: match entry.anchor {
            weekend::EntryAnchor::Time => MobileWeekendEntryAnchor::Time,
            weekend::EntryAnchor::Day => MobileWeekendEntryAnchor::Day,
        },
        day_key: entry.day_key,
        source_id: entry.source_id,
        also_scheduled_on: entry.also_scheduled_on,
        deadline_outside_window: entry.deadline_outside_window,
    }
}

fn map_weekend_gap(gap: weekend::WeekendGap) -> MobileWeekendGap {
    match gap {
        weekend::WeekendGap::NotConnected => MobileWeekendGap::NotConnected,
        weekend::WeekendGap::Unacquired => MobileWeekendGap::Unacquired,
        weekend::WeekendGap::UnresolvableZone => MobileWeekendGap::UnresolvableZone,
    }
}

/// [`weekend::EntryKind`], mirrored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileWeekendEntryKind {
    Event,
    Due,
    Scheduled,
}

/// [`weekend::EntryAnchor`], mirrored — an instant within the day, or the
/// whole day. What a renderer needs to choose between "9:30 – 10:00" and
/// "all day", and nothing Kotlin re-derives from a timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileWeekendEntryAnchor {
    Time,
    Day,
}

/// [`weekend::WindowEntry`], mirrored (#564/#621). The merge — including
/// the due-beats-scheduled dedupe and both its residues — happens in
/// `weekend.rs`; this crossing carries the result.
///
/// `source_id` is the phone's handle back into its own item, and the reason
/// the plan chips can write: `also_scheduled_on` and `day_key` say which
/// chip is filled, `source_id` says which item a tap writes to.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileWeekendEntry {
    pub id: String,
    pub kind: MobileWeekendEntryKind,
    pub title: String,
    pub at_ms: i64,
    pub anchor: MobileWeekendEntryAnchor,
    pub day_key: String,
    pub source_id: String,
    pub also_scheduled_on: Option<String>,
    pub deadline_outside_window: Option<String>,
}

/// [`weekend::WeekendDayEntries`], mirrored.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileWeekendDayEntries {
    pub date: String,
    pub entries: Vec<MobileWeekendEntry>,
}

/// [`weekend::WeekendFacts`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileWeekendFacts {
    pub window: MobileWeekendWindow,
    pub counts: MobileWeekendCounts,
    /// One per day still ahead, in window order — `window.days` verbatim,
    /// so this shrinks as the weekend is spent. Never empty.
    pub days: Vec<MobileWeekendDayEntries>,
}

/// [`weekend::WeekendResolved`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileWeekendResolved {
    Facts { facts: MobileWeekendFacts },
    Gap { gap: MobileWeekendGap },
}

fn map_weekend_resolved(resolved: weekend::WeekendResolved) -> MobileWeekendResolved {
    match resolved {
        weekend::WeekendResolved::Facts(facts) => MobileWeekendResolved::Facts {
            facts: MobileWeekendFacts {
                window: MobileWeekendWindow {
                    start_ms: facts.window.start_ms,
                    end_ms: facts.window.end_ms,
                    days: facts
                        .window
                        .days
                        .iter()
                        .map(|day| MobileWeekendDay {
                            date: day.date.clone(),
                            start_ms: day.start_ms,
                            end_ms: day.end_ms,
                        })
                        .collect(),
                    under_way: facts.window.under_way,
                },
                counts: MobileWeekendCounts {
                    events: facts.counts.events,
                    due: facts.counts.due,
                    scheduled: facts.counts.scheduled,
                },
                days: facts
                    .days
                    .into_iter()
                    .map(|day| MobileWeekendDayEntries {
                        date: day.date,
                        entries: day.entries.into_iter().map(map_weekend_entry).collect(),
                    })
                    .collect(),
            },
        },
        weekend::WeekendResolved::Gap { gap } => {
            MobileWeekendResolved::Gap { gap: map_weekend_gap(gap) }
        }
    }
}

/// [`vacation::TripPhase`], mirrored.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum MobileTripPhase {
    Upcoming,
    DepartsToday,
    UnderWay,
    ReturnsToday,
    Past,
}

fn map_trip_phase(phase: vacation::TripPhase) -> MobileTripPhase {
    match phase {
        vacation::TripPhase::Upcoming => MobileTripPhase::Upcoming,
        vacation::TripPhase::DepartsToday => MobileTripPhase::DepartsToday,
        vacation::TripPhase::UnderWay => MobileTripPhase::UnderWay,
        vacation::TripPhase::ReturnsToday => MobileTripPhase::ReturnsToday,
        vacation::TripPhase::Past => MobileTripPhase::Past,
    }
}

/// [`vacation::Trip`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileTrip {
    pub id: String,
    pub location: Option<String>,
    pub start_date: String,
    pub last_date: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub phase: MobileTripPhase,
    pub days_until: i64,
    pub length_days: i64,
    pub day_of_trip: i64,
}

fn map_trip(trip: vacation::Trip) -> MobileTrip {
    MobileTrip {
        id: trip.id,
        location: trip.location,
        start_date: trip.start_date,
        last_date: trip.last_date,
        start_ms: trip.start_ms,
        end_ms: trip.end_ms,
        phase: map_trip_phase(trip.phase),
        days_until: trip.days_until,
        length_days: trip.length_days,
        day_of_trip: trip.day_of_trip,
    }
}

/// [`vacation::VacationGap`], mirrored.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum MobileVacationGap {
    UnresolvableZone,
}

/// [`vacation::VacationFacts`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileVacationFacts {
    pub next: Option<MobileTrip>,
    pub later: Vec<MobileTrip>,
    pub stale: bool,
    pub freshness: MobilePaneFreshness,
}

/// [`vacation::VacationResolved`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileVacationResolved {
    Facts { facts: MobileVacationFacts },
    Gap { gap: MobileVacationGap },
}

fn map_vacation_resolved(resolved: vacation::VacationResolved) -> MobileVacationResolved {
    match resolved {
        vacation::VacationResolved::Facts(facts) => MobileVacationResolved::Facts {
            facts: MobileVacationFacts {
                next: facts.next.map(map_trip),
                later: facts.later.into_iter().map(map_trip).collect(),
                stale: facts.stale,
                freshness: map_pane_freshness(facts.freshness),
            },
        },
        vacation::VacationResolved::Gap { gap } => MobileVacationResolved::Gap {
            gap: match gap {
                vacation::VacationGap::UnresolvableZone => MobileVacationGap::UnresolvableZone,
            },
        },
    }
}

/// [`scps::ScpsKind`], mirrored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileScpsKind {
    Meeting,
    Activity,
    HappyHour,
    Event,
}

fn map_scps_kind(kind: scps::ScpsKind) -> MobileScpsKind {
    match kind {
        scps::ScpsKind::Meeting => MobileScpsKind::Meeting,
        scps::ScpsKind::Activity => MobileScpsKind::Activity,
        scps::ScpsKind::HappyHour => MobileScpsKind::HappyHour,
        scps::ScpsKind::Event => MobileScpsKind::Event,
    }
}

/// [`scps::ScpsEvent`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileScpsEvent {
    pub id: String,
    pub kind: MobileScpsKind,
    pub topic: Option<String>,
    pub start_ms: i64,
    pub end_ms: i64,
    pub start_date: String,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub days_until: i64,
    pub in_progress: bool,
}

fn map_scps_event(event: scps::ScpsEvent) -> MobileScpsEvent {
    MobileScpsEvent {
        id: event.id,
        kind: map_scps_kind(event.kind),
        topic: event.topic,
        start_ms: event.start_ms,
        end_ms: event.end_ms,
        start_date: event.start_date,
        location: event.location,
        notes: event.notes,
        days_until: event.days_until,
        in_progress: event.in_progress,
    }
}

/// [`scps::ScpsQuestFact`], mirrored.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MobileScpsQuestFact {
    None,
    Current { phrase: String },
    Other { month: String, phrase: String },
}

fn map_scps_quest_fact(quest: scps::ScpsQuestFact) -> MobileScpsQuestFact {
    match quest {
        scps::ScpsQuestFact::None => MobileScpsQuestFact::None,
        scps::ScpsQuestFact::Current { phrase } => MobileScpsQuestFact::Current { phrase },
        scps::ScpsQuestFact::Other { month, phrase } => MobileScpsQuestFact::Other { month, phrase },
    }
}

/// [`scps::ScpsGap`], mirrored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileScpsGap {
    UnresolvableZone,
}

/// [`scps::ScpsFacts`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileScpsFacts {
    pub next: Option<MobileScpsEvent>,
    pub later: Vec<MobileScpsEvent>,
    pub quest: MobileScpsQuestFact,
    pub stale: bool,
    pub freshness: MobilePaneFreshness,
}

/// [`scps::ScpsResolved`], mirrored. Not boxed the way the core's own
/// `ScpsResolved::Facts` is: `uniffi::Enum` has no established boxed-field
/// precedent in this file (unlike the core-only enum, which answers only to
/// `clippy::large_enum_variant`), so this arm is allowed instead.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
#[allow(clippy::large_enum_variant)]
pub enum MobileScpsResolved {
    Facts { facts: MobileScpsFacts },
    Gap { gap: MobileScpsGap },
}

fn map_scps_resolved(resolved: scps::ScpsResolved) -> MobileScpsResolved {
    match resolved {
        scps::ScpsResolved::Facts(facts) => MobileScpsResolved::Facts {
            facts: MobileScpsFacts {
                next: facts.next.map(map_scps_event),
                later: facts.later.into_iter().map(map_scps_event).collect(),
                quest: map_scps_quest_fact(facts.quest),
                stale: facts.stale,
                freshness: map_pane_freshness(facts.freshness),
            },
        },
        scps::ScpsResolved::Gap { gap } => MobileScpsResolved::Gap {
            gap: match gap {
                scps::ScpsGap::UnresolvableZone => MobileScpsGap::UnresolvableZone,
            },
        },
    }
}

/// [`race::RaceSession`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileRaceSession {
    pub kind: String,
    pub label: String,
    pub starts_at_ms: i64,
}

/// [`race::RaceEvent`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileRaceEvent {
    pub name: String,
    pub locality: String,
    pub starts_at_ms: i64,
    pub sessions: Vec<MobileRaceSession>,
}

/// [`race::RaceFacts::next_start`]'s tuple, mirrored as a record — uniffi
/// has no tuples.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileRaceNextStart {
    pub label: String,
    pub starts_at_ms: i64,
}

/// [`race::RaceGap`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileRaceGap {
    NotFetched,
    Malformed { reason: String },
    UnknownSchema { schema: String },
    NotJson,
    NotAnObject,
    NoSeason,
    BadEvent,
}

fn map_race_gap(gap: race::RaceGap) -> MobileRaceGap {
    match gap {
        race::RaceGap::NotFetched => MobileRaceGap::NotFetched,
        race::RaceGap::Malformed { reason } => MobileRaceGap::Malformed { reason },
        race::RaceGap::UnknownSchema { schema } => MobileRaceGap::UnknownSchema { schema },
        race::RaceGap::NotJson => MobileRaceGap::NotJson,
        race::RaceGap::NotAnObject => MobileRaceGap::NotAnObject,
        race::RaceGap::NoSeason => MobileRaceGap::NoSeason,
        race::RaceGap::BadEvent => MobileRaceGap::BadEvent,
    }
}

/// [`race::RaceFacts`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileRaceFacts {
    pub series: String,
    pub event: Option<MobileRaceEvent>,
    pub next_start: Option<MobileRaceNextStart>,
    pub has_live_alert: bool,
    pub stale: bool,
    pub freshness: MobilePaneFreshness,
}

/// [`race::RaceSetup`]'s **kind**, mirrored — [`MobileWasteSetup`]'s twin,
/// for its reasons, with the payload (`series`) dropped for its reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileRaceSetup {
    Bound,
    Unread,
    Unusable,
    Unset,
}

fn map_race_setup(setup: race::RaceSetup) -> MobileRaceSetup {
    match setup {
        race::RaceSetup::Bound { .. } => MobileRaceSetup::Bound,
        race::RaceSetup::Unread => MobileRaceSetup::Unread,
        race::RaceSetup::Unusable => MobileRaceSetup::Unusable,
        race::RaceSetup::Unset => MobileRaceSetup::Unset,
    }
}

/// [`race::RaceResolved`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileRaceResolved {
    Facts { facts: MobileRaceFacts },
    Gap { gap: MobileRaceGap },
}

fn map_race_resolved(resolved: race::RaceResolved) -> MobileRaceResolved {
    match resolved {
        race::RaceResolved::Facts(facts) => MobileRaceResolved::Facts {
            facts: MobileRaceFacts {
                series: facts.series,
                event: facts.event.map(|event| MobileRaceEvent {
                    name: event.name,
                    locality: event.locality,
                    starts_at_ms: event.starts_at_ms,
                    sessions: event
                        .sessions
                        .into_iter()
                        .map(|session| MobileRaceSession {
                            kind: session.kind,
                            label: session.label,
                            starts_at_ms: session.starts_at_ms,
                        })
                        .collect(),
                }),
                next_start: facts.next_start.map(|(label, starts_at_ms)| MobileRaceNextStart {
                    label,
                    starts_at_ms,
                }),
                has_live_alert: facts.has_live_alert,
                stale: facts.stale,
                freshness: map_pane_freshness(facts.freshness),
            },
        },
        race::RaceResolved::Gap { gap } => MobileRaceResolved::Gap { gap: map_race_gap(gap) },
    }
}

/// [`kimi::KimiGap`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileKimiGap {
    NotFetched,
    Malformed { reason: String },
    UnknownSchema { schema: String },
    NotJson,
    NotAnObject,
    BadNumbers,
}

fn map_kimi_gap(gap: kimi::KimiGap) -> MobileKimiGap {
    match gap {
        kimi::KimiGap::NotFetched => MobileKimiGap::NotFetched,
        kimi::KimiGap::Malformed { reason } => MobileKimiGap::Malformed { reason },
        kimi::KimiGap::UnknownSchema { schema } => MobileKimiGap::UnknownSchema { schema },
        kimi::KimiGap::NotJson => MobileKimiGap::NotJson,
        kimi::KimiGap::NotAnObject => MobileKimiGap::NotAnObject,
        kimi::KimiGap::BadNumbers => MobileKimiGap::BadNumbers,
    }
}

/// [`kimi::KimiFacts`], mirrored — the one fact set with `f64` fields,
/// which is why [`MobileRankedPane`] (and everything holding one) derives
/// `PartialEq` without `Eq`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileKimiFacts {
    pub available_balance: f64,
    pub voucher_balance: f64,
    pub cash_balance: f64,
    pub stale: bool,
    pub freshness: MobilePaneFreshness,
}

/// [`kimi::KimiResolved`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileKimiResolved {
    Facts { facts: MobileKimiFacts },
    Gap { gap: MobileKimiGap },
}

fn map_kimi_resolved(resolved: kimi::KimiResolved) -> MobileKimiResolved {
    match resolved {
        kimi::KimiResolved::Facts(facts) => MobileKimiResolved::Facts {
            facts: MobileKimiFacts {
                available_balance: facts.available_balance,
                voucher_balance: facts.voucher_balance,
                cash_balance: facts.cash_balance,
                stale: facts.stale,
                freshness: map_pane_freshness(facts.freshness),
            },
        },
        kimi::KimiResolved::Gap { gap } => MobileKimiResolved::Gap { gap: map_kimi_gap(gap) },
    }
}

/// [`github::WorkflowBody`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileWorkflowBody {
    pub display_name: String,
    pub declared_cadence_ms: Option<i64>,
    pub last_run_conclusion: Option<String>,
    pub last_run_event: Option<String>,
    pub last_run_at_ms: Option<i64>,
    pub last_scheduled_success_at_ms: Option<i64>,
}

/// [`github::WorkflowGap`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileWorkflowGap {
    NotFetched,
    Malformed { reason: String },
    UnknownSchema { schema: String },
    NotJson,
    NotAnObject,
    UnreadableFields,
}

fn map_workflow_gap(gap: github::WorkflowGap) -> MobileWorkflowGap {
    match gap {
        github::WorkflowGap::NotFetched => MobileWorkflowGap::NotFetched,
        github::WorkflowGap::Malformed { reason } => MobileWorkflowGap::Malformed { reason },
        github::WorkflowGap::UnknownSchema { schema } => {
            MobileWorkflowGap::UnknownSchema { schema }
        }
        github::WorkflowGap::NotJson => MobileWorkflowGap::NotJson,
        github::WorkflowGap::NotAnObject => MobileWorkflowGap::NotAnObject,
        github::WorkflowGap::UnreadableFields => MobileWorkflowGap::UnreadableFields,
    }
}

/// [`github::WorkflowView`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileWorkflowView {
    pub body: MobileWorkflowBody,
    pub stale: bool,
    pub freshness: MobilePaneFreshness,
}

/// [`github::WorkflowResolved`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileWorkflowResolved {
    View { view: MobileWorkflowView },
    Gap { gap: MobileWorkflowGap },
}

fn map_workflow_resolved(resolved: github::WorkflowResolved) -> MobileWorkflowResolved {
    match resolved {
        github::WorkflowResolved::View(view) => MobileWorkflowResolved::View {
            view: MobileWorkflowView {
                body: MobileWorkflowBody {
                    display_name: view.body.display_name,
                    declared_cadence_ms: view.body.declared_cadence_ms,
                    last_run_conclusion: view.body.last_run_conclusion,
                    last_run_event: view.body.last_run_event,
                    last_run_at_ms: view.body.last_run_at_ms,
                    last_scheduled_success_at_ms: view.body.last_scheduled_success_at_ms,
                },
                stale: view.stale,
                freshness: map_pane_freshness(view.freshness),
            },
        },
        github::WorkflowResolved::Gap { gap } => {
            MobileWorkflowResolved::Gap { gap: map_workflow_gap(gap) }
        }
    }
}

/// [`uptime::Expected`], mirrored.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum MobileProbeExpected {
    On,
    Off,
}

/// [`uptime::ProbeBody`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileProbeBody {
    pub expected: MobileProbeExpected,
    pub expect_status: i64,
    pub observed_status: Option<i64>,
    pub error: Option<String>,
}

/// [`uptime::ProbeGap`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileProbeGap {
    NotFetched,
    Malformed { reason: String },
    UnknownSchema { schema: String },
    NotJson,
    NotAnObject,
    FieldsUnreadable,
    ObservationUnreadable,
}

fn map_probe_gap(gap: uptime::ProbeGap) -> MobileProbeGap {
    match gap {
        uptime::ProbeGap::NotFetched => MobileProbeGap::NotFetched,
        uptime::ProbeGap::Malformed { reason } => MobileProbeGap::Malformed { reason },
        uptime::ProbeGap::UnknownSchema { schema } => MobileProbeGap::UnknownSchema { schema },
        uptime::ProbeGap::NotJson => MobileProbeGap::NotJson,
        uptime::ProbeGap::NotAnObject => MobileProbeGap::NotAnObject,
        uptime::ProbeGap::FieldsUnreadable => MobileProbeGap::FieldsUnreadable,
        uptime::ProbeGap::ObservationUnreadable => MobileProbeGap::ObservationUnreadable,
    }
}

/// [`uptime::ProbeFacts`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileProbeFacts {
    pub service_id: String,
    pub body: MobileProbeBody,
    pub stale: bool,
    pub freshness: MobilePaneFreshness,
}

/// [`uptime::ProbeResolved`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileProbeResolved {
    Facts { facts: MobileProbeFacts },
    Gap { gap: MobileProbeGap },
}

fn map_probe_resolved(resolved: uptime::ProbeResolved) -> MobileProbeResolved {
    match resolved {
        uptime::ProbeResolved::Facts(facts) => MobileProbeResolved::Facts {
            facts: MobileProbeFacts {
                service_id: facts.service_id,
                body: MobileProbeBody {
                    expected: match facts.body.expected {
                        uptime::Expected::On => MobileProbeExpected::On,
                        uptime::Expected::Off => MobileProbeExpected::Off,
                    },
                    expect_status: facts.body.expect_status,
                    observed_status: facts.body.observed_status,
                    error: facts.body.error,
                },
                stale: facts.stale,
                freshness: map_pane_freshness(facts.freshness),
            },
        },
        uptime::ProbeResolved::Gap { gap } => MobileProbeResolved::Gap { gap: map_probe_gap(gap) },
    }
}

/// [`poller::PollerGap`], mirrored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobilePollerGap {
    NotFetched,
}

fn map_poller_gap(gap: poller::PollerGap) -> MobilePollerGap {
    match gap {
        poller::PollerGap::NotFetched => MobilePollerGap::NotFetched,
    }
}

/// [`poller::PollerFacts`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobilePollerFacts {
    pub freshness: MobilePaneFreshness,
    pub band: MobilePaneBand,
}

/// [`poller::PollerResolved`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobilePollerResolved {
    Facts { facts: MobilePollerFacts },
    Gap { gap: MobilePollerGap },
}

fn map_poller_resolved(resolved: poller::PollerResolved) -> MobilePollerResolved {
    match resolved {
        poller::PollerResolved::Facts(facts) => MobilePollerResolved::Facts {
            facts: MobilePollerFacts {
                freshness: map_pane_freshness(facts.freshness),
                band: map_band(facts.band),
            },
        },
        poller::PollerResolved::Gap { gap } => MobilePollerResolved::Gap { gap: map_poller_gap(gap) },
    }
}

/// [`reachability::ReachabilityFacts`], mirrored.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct MobileReachabilityFacts {
    pub age_ms: i64,
    pub stale: bool,
    pub latest_attempt_landed: bool,
}

fn map_reachability_facts(facts: reachability::ReachabilityFacts) -> MobileReachabilityFacts {
    MobileReachabilityFacts {
        age_ms: facts.age_ms,
        stale: facts.stale,
        latest_attempt_landed: facts.latest_attempt_landed,
    }
}

/// [`homework::HomeworkItem`], mirrored. `description` is `None` on every
/// entry but the winner — the core's own trim, carried through rather than
/// re-decided here.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileHomeworkItem {
    pub id: String,
    pub title: String,
    pub deadline: Option<String>,
    pub description: Option<String>,
}

/// [`homework::HomeworkFacts`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileHomeworkFacts {
    pub winner: Option<MobileHomeworkItem>,
    pub others: Vec<MobileHomeworkItem>,
    /// Whole civil days to the winner's deadline — negative when overdue,
    /// `0` today. **The number this client writes its own sentence from**
    /// (`PaneAnswers.kt`'s `homeworkCollapsedHeadline`): ADR-0025 crosses
    /// facts, never sentences.
    pub days_away: Option<i64>,
}

/// [`homework::HomeworkGap`], mirrored. One arm, and it is the zone
/// bridge's own — see that module's doc for why "nothing open" is an
/// answer rather than a gap.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum MobileHomeworkGap {
    UnresolvableZone,
}

/// [`homework::HomeworkResolved`], mirrored.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileHomeworkResolved {
    Facts { facts: MobileHomeworkFacts },
    Gap { gap: MobileHomeworkGap },
}

fn map_homework_item(item: homework::HomeworkItem) -> MobileHomeworkItem {
    MobileHomeworkItem {
        id: item.id,
        title: item.title,
        deadline: item.deadline,
        description: item.description,
    }
}

fn map_homework_resolved(resolved: homework::HomeworkResolved) -> MobileHomeworkResolved {
    match resolved {
        homework::HomeworkResolved::Facts(facts) => MobileHomeworkResolved::Facts {
            facts: MobileHomeworkFacts {
                winner: facts.winner.map(map_homework_item),
                others: facts.others.into_iter().map(map_homework_item).collect(),
                days_away: facts.days_away,
            },
        },
        homework::HomeworkResolved::Gap { gap } => MobileHomeworkResolved::Gap {
            gap: match gap {
                homework::HomeworkGap::UnresolvableZone => MobileHomeworkGap::UnresolvableZone,
            },
        },
    }
}

/// One pane's decided fact set, keyed by its question — the union
/// [`MobileRankedPane::facts`] carries. Two arms are `Option`al for the two
/// questions whose facts genuinely may not exist yet: `Vacation`'s facts
/// only exist for a bound calendar ([`vacation::vacation_view`] answers
/// `None` otherwise — the answer state already says which unbound flavour),
/// and `Reachability`'s only once this device has ever synced. Every other
/// question always resolves — to facts or to a gap KIND (a sentinel
/// subject resolves to its own honest `NotFetched`).
///
/// `Homework` carries its standing session `link` beside the resolved facts
/// for a related reason: the link is standing, so it has to reach the host
/// in the Gap arm too, and a field inside the facts could not.
///
/// `Waste` and `Race` each carry their `setup` kind beside the resolved
/// facts: their two unacquired flavours ("bindings not read yet" and
/// "binding unusable") share one answer state, so the kind is the only
/// thing that can tell a host which of the two it is looking at — see
/// [`MobileWasteSetup`].
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobilePaneFacts {
    /// `link` is a **sibling** of `resolved`, not a field inside the facts:
    /// the standing session link survives the Gap arm, which carries no
    /// facts at all (`homework.rs`'s own header). `None` is "nothing bound,
    /// or what is bound is not a web URL" — the host draws no button.
    Homework { resolved: MobileHomeworkResolved, link: Option<String> },
    /// `None` while unacquired — `scps.rs`'s own "never unbound" rule
    /// (#693): unlike `Vacation`'s `None`, which can mean either
    /// unacquired or genuinely unbound, this pane has no unbound arm at
    /// all, so `None` here means only "waiting for the first sync".
    Scps { resolved: Option<MobileScpsResolved> },
    Waste { setup: MobileWasteSetup, resolved: MobileWasteResolved },
    Weekend { resolved: MobileWeekendResolved },
    Vacation { resolved: Option<MobileVacationResolved> },
    Race { setup: MobileRaceSetup, resolved: MobileRaceResolved },
    Kimi { resolved: MobileKimiResolved },
    Github { resolved: MobileWorkflowResolved },
    Uptime { resolved: MobileProbeResolved },
    Reachability { facts: Option<MobileReachabilityFacts> },
    /// One `MobilePaneFacts::Poller` per source `poller::poller_sources`
    /// watches — `subjectKey` on the enclosing [`MobileRankedPane`] carries
    /// which source this is, exactly `Github`/`Uptime`'s own shape.
    Poller { resolved: MobilePollerResolved },
}

/// One `(zone, civil-date)` fact the core named — [`ZoneQuery`], mirrored
/// as a `uniffi::Enum`. `key` is not carried on this type: `java.time`
/// answers each query by resolving it itself, and
/// [`MobileZoneFact::key`] is what pairs a resolved answer back to the
/// query that asked for it (`ZoneQuery::key`, ported so the two sides
/// cannot disagree about it — see [`mobile_zone_query_key`]).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MobileZoneQuery {
    CivilDate { zone: String, at_ms: i64 },
    Midnight { zone: String, date: String },
}

fn to_mobile_zone_query(query: &ZoneQuery) -> MobileZoneQuery {
    match query {
        ZoneQuery::CivilDate { zone, at_ms } => {
            MobileZoneQuery::CivilDate { zone: zone.clone(), at_ms: *at_ms }
        }
        ZoneQuery::Midnight { zone, date } => {
            MobileZoneQuery::Midnight { zone: zone.clone(), date: date.clone() }
        }
    }
}

fn from_mobile_zone_query(query: &MobileZoneQuery) -> ZoneQuery {
    match query {
        MobileZoneQuery::CivilDate { zone, at_ms } => {
            ZoneQuery::CivilDate { zone: zone.clone(), at_ms: *at_ms }
        }
        MobileZoneQuery::Midnight { zone, date } => {
            ZoneQuery::Midnight { zone: zone.clone(), date: date.clone() }
        }
    }
}

/// [`ZoneQuery::key`], exposed so a host can pair its resolved answer back
/// onto the query that asked for it without re-deriving the key format
/// itself — the same "sent across rather than re-derived" rule
/// `zone-bridge.ts`'s own header states for the web resolver.
#[uniffi::export]
pub fn mobile_zone_query_key(query: MobileZoneQuery) -> String {
    from_mobile_zone_query(&query).key()
}

/// One resolved `(zone, civil-date)` answer, keyed by
/// [`mobile_zone_query_key`] — [`ZoneFact`], mirrored as a `uniffi::Enum`
/// and paired with the key the host resolved it for. A host that could not
/// resolve a query simply omits it from the list handed to
/// [`MobileTaskHost::rank_panes`] — the zone bridge's own "an unresolvable
/// zone is an absence" rule (`zone.rs`'s module header), never a null
/// entry.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileZoneFact {
    pub key: String,
    pub value: MobileZoneFactValue,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MobileZoneFactValue {
    Date { value: String },
    Instant { value: i64 },
}

fn to_zone_facts(facts: Vec<MobileZoneFact>) -> ZoneFacts {
    let map: HashMap<String, ZoneFact> = facts
        .into_iter()
        .map(|fact| {
            let value = match fact.value {
                MobileZoneFactValue::Date { value } => ZoneFact::Date(value),
                MobileZoneFactValue::Instant { value } => ZoneFact::Instant(value),
            };
            (fact.key, value)
        })
        .collect();
    ZoneFacts::from_keyed(map)
}

/// The device's own authority-sync history (#536) — [`SyncFacts`],
/// mirrored as a `uniffi::Record`. Unlike bindings or `context_snapshots`,
/// this is not something `Core` persists at all (it has no reason to: no
/// other decision reads it) — the host is the one durable copy, exactly
/// the web's own `QuestionSyncSnapshot`, kept in its store rather than in
/// `hummingbird-core`. Android persists this across restarts
/// (`SyncHistoryStore.kt`) so the reachability pane has something to
/// reason over on a cold start, before the first cycle of this session has
/// completed.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct MobileSyncFacts {
    pub latest_outcome_kind: Option<String>,
    pub latest_informative_at_ms: Option<i64>,
    pub last_successful_at_ms: Option<i64>,
}

fn to_sync_facts(facts: MobileSyncFacts) -> SyncFacts {
    SyncFacts {
        latest_outcome_kind: facts.latest_outcome_kind,
        latest_informative_at_ms: facts.latest_informative_at_ms,
        last_successful_at_ms: facts.last_successful_at_ms,
    }
}

/// One source's pane read, built off [`Core::pane_read`] — the same
/// conversion `ffi-web::decisions` would need, done once here rather than
/// crossing the whole [`hummingbird_core::pane::PaneRead`] DTO and
/// re-deriving it per pane (`panes::inputs`'s own "do not re-cross whole
/// DTOs" rule, applied at this seam).
fn to_pane_read_facts(read: &hummingbird_core::pane::PaneRead) -> PaneReadFacts {
    use hummingbird_core::decisions::panes::inputs::{
        PaneAlertFacts, PaneEnvelopeFacts, PaneSnapshotFacts,
    };

    PaneReadFacts {
        snapshots: read
            .snapshots
            .iter()
            .map(|snapshot| PaneSnapshotFacts {
                key: snapshot.key.clone(),
                envelope: match &snapshot.envelope {
                    PaneEnvelope::Parsed { schema, body, .. } => {
                        PaneEnvelopeFacts::Ok { schema: schema.clone(), body: body.clone() }
                    }
                    PaneEnvelope::Malformed { reason } => {
                        PaneEnvelopeFacts::Malformed { reason: reason.clone() }
                    }
                },
                freshness: to_freshness_fact(snapshot.freshness),
            })
            .collect(),
        // `PaneRead.alerts` is already this source's live-only rows
        // (`Core::pane_read`'s own ADR-0014 filter) — trimmed to the one
        // field a sunk pane's join reads (`race.rs`, the only reader
        // today, on a Now-surface question no Status caller reaches yet).
        // Carried through rather than left empty: it costs nothing extra
        // over the crossing `to_pane_read_facts` already does, and an
        // empty default here would have been a silent trap for #537's
        // race pane to land on.
        live_alerts: read
            .alerts
            .iter()
            .map(|alert| PaneAlertFacts { subject_key: alert.subject_key.clone() })
            .collect(),
    }
}

fn to_freshness_fact(
    freshness: hummingbird_core::freshness::Freshness,
) -> hummingbird_core::decisions::panes::inputs::FreshnessFact {
    use hummingbird_core::decisions::panes::inputs::FreshnessFact;
    match freshness {
        hummingbird_core::freshness::Freshness::Unknown => FreshnessFact::Unknown,
        hummingbird_core::freshness::Freshness::Age { age_ms, declared_cadence_ms } => {
            FreshnessFact::Age { age_ms, declared_cadence_ms }
        }
    }
}

/// [`Binding`] → [`BindingFact`], the one conversion every bindings-reading
/// pane (waste/race/vacation) needs — [`to_binding_value`]'s twin, into the
/// pane lane's own input shape rather than [`MobileBindingValue`] (the
/// Settings screen's own wire type, a different direction across a
/// different seam).
fn to_binding_fact(binding: &Binding) -> BindingFact {
    BindingFact {
        key: binding.key.clone(),
        value: match &binding.value {
            BindingValue::Unset => BindingValueFact::Unset,
            BindingValue::Text { text } => BindingValueFact::Text { text: text.clone() },
            BindingValue::Other { .. } => BindingValueFact::Other,
        },
    }
}

/// [`hummingbird_domain::Item`] → [`PaneItemFacts`], trimmed to the eight
/// fields a sunk pane's item reasoning reads (`inputs.rs`'s own doc on
/// [`PaneItemFacts`]) — never the whole DTO. `stage` crosses as
/// [`hummingbird_domain::Stage::as_str`]'s wire spelling, which is what
/// the web's own `TaskItemDTO.stage` carries, so the two hosts hand the
/// core byte-identical strings.
fn to_pane_item_facts(item: &hummingbird_domain::Item) -> PaneItemFacts {
    PaneItemFacts {
        id: item.id.clone(),
        title: item.title.clone(),
        deadline: item.deadline.clone(),
        scheduled_date: item.scheduled_date.clone(),
        stage: item.stage.as_str().to_string(),
        context: item.context.clone(),
        description: item.description.clone(),
        created_at: item.created_at,
    }
}

/// Every live item this device knows about — `NowScreen.tsx::
/// realQuestionInputs`'s own union, matched here field for field.
///
/// `frontier ∪ blocked` was the whole of it at #537:
/// [`hummingbird_core::Core::frontier`] deliberately excludes a
/// relation-blocked item ([`hummingbird_core::Core::blocked`] is the
/// separate section for those), so a due/scheduled item that happens to be
/// relation-blocked would silently vanish from the weekend pane's merge
/// without this — a per-client divergence in a sunk decision's own inputs
/// (#537 review). A blocked entry's own blockers never join this list:
/// only the blocked item itself is a candidate, exactly as `entry.item`
/// (never `entry.blockedByTitles`) is the only thing `realQuestionInputs`
/// reads off each `blocked` entry.
///
/// **#675 added the triage inbox, the grilling queue and the externally
/// blocked items.** The homework pane's subject is the operator's own items
/// and its reading of "open" is everything not `Done` — a captured piece of
/// homework is still homework, and so is one waiting on a callback. The
/// four queries beside `frontier` are what make this list the *whole* live
/// partition of the mirror rather than most of it: `Core::blocked` is
/// relation-blocked `Ready`/`InProgress` items and never
/// `Stage::Blocked` ones, so without `Core::externally_blocked` an item on
/// an external wait was readable from no query here at all and vanished
/// from the pane. The two surfaces must widen together or they answer
/// differently about the same mirror, which is the divergence #537's
/// review was about in the first place. The weekend pane was pinned
/// against the widening first (`weekend.rs`'s `MERGED_STAGES`); a question
/// added later that reads `items` owes the same explicit filter, because
/// this list is "every live item", never "the items your pane should
/// consider".
///
/// Factored out of [`mobile_pane_inputs`] so it is unit-testable without a
/// synced [`Core`]: a relation-blocked item only ever lands in the local
/// mirror through a real sync cycle, and this crate's own test harness has
/// no mock HTTP transport to produce one.
fn pane_item_facts(
    frontier: &[hummingbird_domain::Item],
    blocked: &[(hummingbird_domain::Item, Vec<hummingbird_domain::Item>)],
    triage_inbox: &[hummingbird_domain::Item],
    grilling: &[hummingbird_domain::Item],
    externally_blocked: &[hummingbird_domain::Item],
) -> Vec<PaneItemFacts> {
    frontier
        .iter()
        .chain(blocked.iter().map(|(item, _blockers)| item))
        .chain(triage_inbox.iter())
        .chain(grilling.iter())
        .chain(externally_blocked.iter())
        .map(to_pane_item_facts)
        .collect()
}

/// The calendar arm of [`mobile_pane_inputs`] — what this device has
/// mirrored for each question's own window, and whether it has ever
/// connected a calendar at all. Empty and `false` for phase one
/// ([`MobileTaskHost::pane_zone_queries`]), which runs *before* any zone is
/// resolved and so cannot name a window to read.
#[derive(Debug, Clone, Default, PartialEq)]
struct CalendarArm {
    reads: HashMap<String, CalendarReadFacts>,
    connected: bool,
}

/// Builds [`PaneInputs`] for every sunk pane on both surfaces — see this
/// section's header for what each field is for.
fn mobile_pane_inputs(
    core: &Core<FsSnapshotStore, FsSnapshotStore>,
    now_ms: i64,
    sync: MobileSyncFacts,
    calendar: CalendarArm,
) -> PaneInputs {
    // kimi/github/uptime/waste/race's own `SOURCE` constants are each one of
    // `poller::poller_sources`' nine — this loop reads every source any
    // sunk pane needs in one pass rather than one hand-written line per
    // question (`poller.rs`'s own "not a hand-maintained list" reasoning,
    // applied at this seam too). That coincidence is asserted below rather
    // than assumed: `poller_sources()` filters retired entries, so a future
    // retirement (`v1` -> `v2`, the shape `sources.rs` already has one of)
    // would otherwise silently drop a sunk pane's own source out of this
    // loop with no test failing and no compile error.
    let sources = poller::poller_sources();
    let mut pane_reads = HashMap::new();
    for source in &sources {
        pane_reads.insert(source.to_string(), to_pane_read_facts(&core.pane_read(source, now_ms)));
    }
    debug_assert!(
        [kimi::SOURCE, github::SOURCE, uptime::SOURCE, waste::SOURCE, race::SOURCE]
            .iter()
            .all(|sunk| sources.contains(sunk)),
        "a sunk pane's own SOURCE dropped out of poller::poller_sources()",
    );
    let bindings: Vec<BindingFact> = core.bindings().iter().map(to_binding_fact).collect();
    let items: Vec<PaneItemFacts> = pane_item_facts(
        &core.frontier(),
        &core.blocked(),
        &core.triage_inbox(),
        &core.grilling_items(),
        &core.externally_blocked(),
    );
    PaneInputs {
        now_ms,
        // Always `Some`: unlike the web's async table read, `Core::bindings`
        // is a synchronous local read, so there is no "not read yet" moment
        // on this seam to preserve (`PaneInputs::bindings`'s own doc names
        // that state for a host where one is possible).
        bindings: Some(bindings),
        pane_reads,
        calendar_reads: calendar.reads,
        calendar_connected: calendar.connected,
        items,
        sync: to_sync_facts(sync),
        // #715: passed straight through as `Core` decided it — the phone
        // honours a question switched off in the browser without being able
        // to change it (ADR-0034 decision 4; #716 renders the toggle).
        disabled_questions: core.disabled_questions(),
    }
}

/// Which [`StandingQuestion`] a [`panes::contract::RankedPaneRecord`]'s
/// `question` string names, resolved through [`panes::SUNK`] rather than a
/// hand-written string match — the two can never disagree, since `SUNK` is
/// the same list `rank_panes` itself iterated to produce the string in the
/// first place. `.expect` is safe here for exactly that reason: every
/// record `rank_panes` emits came from one of `SUNK`'s own entries.
fn standing_question_of(question: &str) -> StandingQuestion {
    panes::SUNK
        .iter()
        .map(|(question, _)| *question)
        .find(|candidate| candidate.as_str() == question)
        .unwrap_or_else(|| panic!("rank_panes produced an unsunk question {question:?}"))
}

/// One ranked pane's own fact set — the per-question `*_facts` call the web
/// makes through its per-question wasm exports, made here once per record
/// while [`MobileTaskHost::rank_panes`] already holds the inputs and the
/// resolved zone facts. Exhaustive over [`StandingQuestion`] with no
/// wildcard arm, the section's own drift gate: a ninth question is a
/// compile error here, never a pane whose facts silently never cross.
fn mobile_pane_facts_of(
    question: StandingQuestion,
    subject_key: &str,
    inputs: &PaneInputs,
    zone: &ZoneFacts,
) -> MobilePaneFacts {
    match question {
        StandingQuestion::Homework => MobilePaneFacts::Homework {
            resolved: map_homework_resolved(homework::homework_facts(inputs, zone)),
            link: homework::homework_link(inputs),
        },
        StandingQuestion::Scps => MobilePaneFacts::Scps {
            resolved: scps::scps_view(inputs, zone).map(map_scps_resolved),
        },
        StandingQuestion::Waste => MobilePaneFacts::Waste {
            setup: map_waste_setup(waste::waste_setup(inputs)),
            resolved: map_waste_resolved(waste::waste_facts(inputs, zone)),
        },
        StandingQuestion::Weekend => MobilePaneFacts::Weekend {
            resolved: map_weekend_resolved(weekend::weekend_facts(inputs, zone)),
        },
        StandingQuestion::Vacation => MobilePaneFacts::Vacation {
            resolved: vacation::vacation_view(inputs, zone).map(map_vacation_resolved),
        },
        StandingQuestion::Race => MobilePaneFacts::Race {
            setup: map_race_setup(race::race_setup(inputs)),
            // The setup sentinel has no snapshot row, so this resolves to
            // its own honest `NotFetched` — the answer state (unbound/
            // unacquired) plus the setup kind above are what route the
            // rendering.
            resolved: map_race_resolved(race::race_facts(subject_key, inputs)),
        },
        StandingQuestion::Kimi => {
            MobilePaneFacts::Kimi { resolved: map_kimi_resolved(kimi::kimi_facts(inputs)) }
        }
        StandingQuestion::Github => MobilePaneFacts::Github {
            resolved: map_workflow_resolved(github::github_facts(subject_key, inputs)),
        },
        StandingQuestion::Uptime => MobilePaneFacts::Uptime {
            resolved: map_probe_resolved(uptime::uptime_facts(subject_key, inputs)),
        },
        StandingQuestion::Reachability => MobilePaneFacts::Reachability {
            facts: reachability::reachability_facts(inputs).map(map_reachability_facts),
        },
        StandingQuestion::Poller => {
            MobilePaneFacts::Poller { resolved: map_poller_resolved(poller::poller_facts(subject_key, inputs)) }
        }
    }
}

// ---------------------------------------------------- the calendar (#564)
// Android's calendar lane. The mirror, the poll triggers and the read
// queries are `hummingbird_core::calendar::CalendarHostCore` — the same type
// the web host drives, moved into `core` by this issue rather than copied
// (that module's own header). What lives here is the two things a UniFFI
// host has to add: where the token comes from (`calendar_token.rs`, ADR-0028)
// and which doors Kotlin gets.
//
// **The credential stays in Rust.** Kotlin never sees a Google access token
// — not as a return value, not as a parameter, not in a log line. It asks
// for a connection and receives a *state*; the mint, the rotation and the
// 401 retry all happen below this seam. The only credential Kotlin holds is
// the device token it already held for sync, and that one it hands to
// `push_api_key` as it always did.
//
// **`play-services-auth` is deliberately absent from
// `libs.versions.toml`.** #564 was originally scoped around a native
// `AuthorizationClient` grant; the operator's 2026-08-21 decision replaced
// it with ADR-0028's authority-minted route, on the grounds that two
// mechanisms for one operator's one calendar is a maintenance tax with no
// matching risk reduction. The registered Android OAuth client from
// 2026-08-18 is left in place and unused. That decision's reopening
// conditions are recorded on #564.

/// One of #564's four Source-connection states, as Kotlin sees it. A
/// *decided* answer, never an error string for `SettingsScreen.kt` to match
/// on — the module header's own per-row rule, applied to a per-gesture
/// answer for the same reason `AlertRecord` ships `can_ack`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileCalendarState {
    /// Never opted in on this device. **The only state that offers
    /// Connect.**
    NeverConnected,
    /// Opted in, last mint succeeded.
    Connected,
    /// Opted in; the authority could not be reached. Reads as connected and
    /// keeps showing the (stale) mirror. Covers "phone offline" and
    /// "authority down" alike — the phone cannot tell them apart and does
    /// not need to.
    CannotConfirm,
    /// This device's own token is bad. Settings' existing token control is
    /// the remedy.
    RefusedDeviceToken,
    /// The server-side lane is broken (unset secrets, bad upstream,
    /// malformed answer). There is no per-device action.
    RefusedServerLane,
}

/// The answer to every connection gesture: connect, disconnect, init, and
/// each timer tick.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileCalendarConnection {
    pub state: MobileCalendarState,
    /// When the currently-held access token expires, or `None` when none is
    /// held. Kotlin does **not** schedule rotation off this — the tick does
    /// (see [`MobileTaskHost::calendar_on_timer`]); it is here so Settings
    /// can say something honest about a connection that is holding.
    pub expires_at_ms: Option<i64>,
    /// The raw failure code of the last attempt, or `None` if it succeeded.
    /// Raw and not a sentence, on `connection.ts`'s own rule: the words are
    /// the host's, and a health check needs the code. Never rendered
    /// directly.
    pub error: Option<String>,
}

/// One calendar the device's credential can read — the picker's options.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileCalendarEntry {
    pub id: String,
    pub summary: String,
}

/// The picker's option list, or why there is none. `kind` is
/// [`CalendarHostCore::list_calendars`]'s own vocabulary (`"ok"` /
/// `"no_credential"` / `"failed"`) — a failed list leaves the picker as it
/// stands rather than clearing it, exactly as on the web.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileCalendarList {
    pub kind: String,
    pub calendars: Vec<MobileCalendarEntry>,
}

/// A calendar the operator picked, with the poll horizon it was picked
/// under (#121).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileCalendarSelection {
    pub id: String,
    pub long_horizon: bool,
}

/// What one timer tick did: the poll's own outcome name (the same
/// vocabulary `client/web/src/store/protocol.ts` matches on) plus the
/// connection state the tick left the device in.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileCalendarTick {
    pub outcome: String,
    pub connection: MobileCalendarConnection,
}

fn to_calendar_selection(selection: &MobileCalendarSelection) -> CalendarSelection {
    CalendarSelection {
        id: selection.id.clone(),
        horizon: if selection.long_horizon {
            CalendarHorizon::Long
        } else {
            CalendarHorizon::Standard
        },
    }
}

fn map_calendar_state(state: CalendarState) -> MobileCalendarState {
    match state {
        CalendarState::NeverConnected => MobileCalendarState::NeverConnected,
        CalendarState::Connected => MobileCalendarState::Connected,
        CalendarState::CannotConfirm => MobileCalendarState::CannotConfirm,
        CalendarState::RefusedDeviceToken => MobileCalendarState::RefusedDeviceToken,
        CalendarState::RefusedServerLane => MobileCalendarState::RefusedServerLane,
    }
}

/// The calendar's own half of the host, behind its **own** mutex.
///
/// Deliberately not `Inner`'s lock. A calendar poll is a Google round trip
/// held across `await`; a sync cycle is an authority round trip held across
/// `await`. Sharing one lock would let either stall the other for the length
/// of a network call — an intermittent multi-second hang that no unit test
/// reaches and only hardware shows. They share nothing but the device token,
/// which is read out of `Inner` under a momentary lock that is released
/// before this one is taken (never the reverse, so there is no lock order to
/// get wrong and no deadlock to have).
struct CalendarHalf {
    host: CalendarHostCore<FsSnapshotStore>,
    client: reqwest::Client,
    base_url: String,
    /// The opt-in flag, as this process currently believes it. Durably the
    /// host's (Preferences DataStore, beside `FrontierPrefs`), handed back
    /// at each launch through [`MobileTaskHost::init_calendar`] — a flag,
    /// never a credential (`calendar/persistence.ts`'s own distinction).
    opted_in: bool,
    /// The expiry of the access token currently pushed into `host`, or
    /// `None` when none is.
    expires_at_ms: Option<i64>,
    /// The last mint's failure code, or `None` if it succeeded.
    last_error: Option<&'static str>,
    /// The **device's own** ticked calendars, as the picker last wrote them
    /// — never the polled set. What is actually polled is
    /// [`hummingbird_core::calendar::effective_selection`] over this plus
    /// the synced `trips-calendar` binding, re-derived on every push and
    /// every tick, so a binding that arrives by sync starts being polled
    /// without the operator touching the picker.
    stored_selections: Vec<CalendarSelection>,
}

/// One mirrored event, as a pane rule reads it — `inputs.rs`'s trimmed
/// shape, not the whole [`EventRecord`]. `recurrence_id`, `organizer`,
/// `provider_updated_at_ms` and `html_link` stay behind: no sunk pane reads
/// any of them, and `inputs.rs`'s own discipline is that a field crosses
/// only once some rule reads it.
fn to_calendar_event_facts(event: &EventRecord) -> CalendarEventFacts {
    CalendarEventFacts {
        provider_event_id: event.provider_event_id.clone(),
        calendar_id: event.calendar_id.clone(),
        title: event.title.clone(),
        when: match &event.when {
            EventWhen::AllDay { start_date, end_date } => CalendarEventWhenFacts::AllDay {
                start_date: start_date.clone(),
                end_date: end_date.clone(),
            },
            EventWhen::Timed { start_ms, end_ms } => CalendarEventWhenFacts::Timed {
                start_ms: *start_ms,
                end_ms: *end_ms,
            },
        },
        location: event.location.clone(),
        status: match event.status {
            EventStatus::Confirmed => CalendarEventStatusFact::Confirmed,
            EventStatus::Tentative => CalendarEventStatusFact::Tentative,
            EventStatus::Cancelled => CalendarEventStatusFact::Cancelled,
        },
        description: event.description.clone(),
    }
}

/// A calendar-arm answer, mapped to what the panes read.
///
/// **Anything that is not a real `"read"` becomes
/// [`CalendarReadFacts::NotRead`]** — including the wasm-only `"busy"`
/// kind, which this host cannot produce. That is not a flattening of two
/// facts into one: `not_read` already means "this device has nothing to say
/// about that window", and a host that could not answer has exactly that
/// much to say. The distinction the panes actually gate on — never
/// connected vs connected-but-nothing-landed — is `calendar_connected`, a
/// separate field.
fn to_calendar_read_facts(response: CalendarEventsResponse) -> CalendarReadFacts {
    if response.kind != "read" {
        return CalendarReadFacts::NotRead;
    }
    CalendarReadFacts::Read {
        events: response.events.iter().map(to_calendar_event_facts).collect(),
        freshness: match response.freshness {
            None | Some(Freshness::Unknown) => FreshnessFact::Unknown,
            Some(Freshness::Age { age_ms, declared_cadence_ms }) => {
                FreshnessFact::Age { age_ms, declared_cadence_ms }
            }
        },
    }
}

impl CalendarHalf {
    fn connection(&self) -> MobileCalendarConnection {
        MobileCalendarConnection {
            state: map_calendar_state(connection_state(self.opted_in, self.last_error)),
            expires_at_ms: self.expires_at_ms,
            error: self.last_error.map(str::to_string),
        }
    }

    /// One mint, pushed into the poller on success. Records the code either
    /// way; **never clears `opted_in`** — a failed re-mint on an opted-in
    /// device leaves the device connected-but-stale, which is
    /// `shouldKeepExistingConnection`'s whole point, ported.
    async fn mint(&mut self, device_token: Option<&str>) -> bool {
        match mint_calendar_token(&self.client, &self.base_url, device_token).await {
            MintOutcome::Minted {
                access_token,
                expires_at_ms,
            } => {
                self.host.push_token(access_token);
                self.expires_at_ms = Some(expires_at_ms);
                self.last_error = None;
                true
            }
            MintOutcome::Failed(code) => {
                self.last_error = Some(code);
                false
            }
        }
    }

    /// Pushes the polled set: this device's picks unioned with the bound
    /// Trips calendar, which is always polled and always at the long
    /// horizon. `effectiveSelection`'s rule, read out of core so there is
    /// no ffi-side copy of it.
    fn apply_selection(&self, trips_id: Option<&str>) {
        self.host
            .set_calendar_selections(effective_selection(&self.stored_selections, trips_id));
    }

    /// Whether the held token is close enough to expiry to rotate now —
    /// `connection.ts`'s `msUntilRotation` reaching zero, expressed as the
    /// predicate the tick actually asks.
    fn due_for_rotation(&self, now_ms: i64) -> bool {
        match self.expires_at_ms {
            None => true,
            Some(expires_at_ms) => now_ms >= expires_at_ms - ROTATION_MARGIN_MS,
        }
    }
}

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
    /// #564's calendar lane, behind its own lock — see [`CalendarHalf`] for
    /// why it is not `inner`'s.
    calendar: tokio::sync::Mutex<CalendarHalf>,
    /// #710: every acquisition of `inner` above goes through
    /// [`Self::lock_inner`], which reads/updates this breadcrumb and emits
    /// `core.wait_started`/`core.acquired`/`core.released` around it — see
    /// `core_lock`'s own module doc.
    core_owner: core_lock::CoreOwnershipTracker,
    diag_session: core_lock::CoreLockSession,
    diag_sink: core_lock::BufferingSink,
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
        let namespace_path = namespace.clone();
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
        let write_transport = ReqwestMutationTransport::new(client.clone(), base_url.clone());
        // The calendar mirror is a fourth snapshot slot under the same
        // host-supplied namespace, sibling to the queue/mirror/grill-draft
        // files `Core::init` just laid down (ADR-0003: the namespace is the
        // one thing the host contributes). Selections start empty — a
        // never-opted-in device polls nothing — and arrive at
        // `init_calendar`.
        let calendar = CalendarHalf {
            host: CalendarHostCore::new(
                FsSnapshotStore::new(std::path::Path::new(&namespace_path).join("calendar.json")),
                Vec::new(),
            ),
            client,
            base_url,
            opted_in: false,
            expires_at_ms: None,
            last_error: None,
            stored_selections: Vec::new(),
        };
        Ok(Arc::new(Self {
            inner: tokio::sync::Mutex::new(Inner {
                core,
                read_transport,
                write_transport,
                api_key: shadow_key,
            }),
            calendar: tokio::sync::Mutex::new(calendar),
            core_owner: core_lock::CoreOwnershipTracker::new(),
            // #710 review round 1: shares `DIAGNOSTIC_SESSION.seq` — the
            // one counter every Android-sourced event in this process
            // advances — rather than minting a second, colliding one.
            diag_session: core_lock::CoreLockSession::new(&DIAGNOSTIC_SESSION.seq),
            diag_sink: core_lock::BufferingSink::new(),
        }))
    }

    /// Drains every buffered `core.*`/`operation.*` event (#710) — see
    /// `core_lock`'s module doc for the production-wiring tradeoff this
    /// exists to name: a host that never calls this loses nothing (the
    /// buffer just fills, oldest-drops-first, up to `BUFFER_CAPACITY`), it
    /// just never sees these particular events in its exported journal.
    /// `SyncWorker` calls this twice per run — once before `core.run` and
    /// once after it returns — and `SettingsViewModel`'s export path calls
    /// it again before writing the export; each forwards every line to
    /// `DiagnosticsRecorder.appendRaw`. Draining an already-empty buffer is
    /// free, so the repetition costs nothing.
    pub async fn take_diagnostic_events(&self) -> Vec<MobileDiagnosticLine> {
        self.diag_sink
            .drain()
            .into_iter()
            // `DiagnosticEventV1` is plain strings/numbers/enums, so a real
            // serialization failure here is not realistic — but on the
            // remote chance one occurred, skipping that one line (never a
            // synthesized `"{}"`) is the right failure mode: a caller that
            // appends this straight to the journal (`DiagnosticJournal
            // .append`'s own doc: "every stored line ... is already
            // complete, valid JSON") must never receive a line this
            // function itself knows is not the real event.
            .filter_map(|event| {
                serde_json::to_string(&event)
                    .ok()
                    .map(|json| MobileDiagnosticLine { wall_clock_ms: event.wall_clock_ms, json })
            })
            .collect()
    }

    /// The wrapped core's public API version — same value as the free
    /// [`core_api_version`], surfaced on the object so a host holding only
    /// the handle can show it.
    pub async fn api_version(&self) -> u32 {
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Settings)
            .await
            .core
            .api_version()
    }

    /// The mirror's active-item population (ADR-0001's watchline figure) —
    /// the M0 proof screen's number.
    pub async fn active_item_count(&self) -> u32 {
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Read).await.core.active_item_count() as u32
    }

    /// The outbound queue's current depth — the "queued" sync-status
    /// figure.
    pub async fn queue_depth(&self) -> u32 {
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Read).await.core.queue_depth() as u32
    }

    /// Every dead-lettered entry (#535), per [`Core::dead_letters`] — S9's
    /// "1 edit didn't apply" affordance, mapped the same shape
    /// `ffi-web::task_host::map_dead_letter` uses (that crate's own copy,
    /// since the two FFI crates share no DTO layer — ADR-0001 seam rule 2
    /// is that each surfaces `Core` verbatim, not that they surface each
    /// other).
    pub async fn dead_letters(&self) -> Vec<MobileDeadLetterRecord> {
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Settings).await
            .core
            .dead_letters()
            .iter()
            .map(to_dead_letter_record)
            .collect()
    }

    /// Every standing-question binding (#535/#118), per [`Core::bindings`].
    pub async fn bindings(&self) -> Vec<MobileBindingRecord> {
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Settings).await
            .core
            .bindings()
            .iter()
            .map(to_binding_record)
            .collect()
    }

    /// Sets one binding (#535/#118). `key` is the wire's kebab-case binding
    /// name, resolved through [`BindingKey::parse`] before it ever reaches
    /// [`Core::set_binding`] — the same "reject before the seam" discipline
    /// [`MobileTaskHost::capture`] applies to its own vocabulary, and
    /// load-bearing here for a second reason: `settings` has no DELETE, so
    /// a key minted by mistake can never be taken back out of the table.
    pub async fn set_binding(
        &self,
        seed: String,
        key: String,
        value: String,
        now_ms: i64,
    ) -> Result<(), MobileSetBindingError> {
        let Some(key) = BindingKey::parse(&key) else {
            return Err(MobileSetBindingError::UnknownKey);
        };
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Settings).await
            .core
            .set_binding(&seed, key, &value, now_ms)
            .await
            .map_err(|error| MobileSetBindingError::WriteFailed {
                detail: error.to_string(),
            })
    }

    /// Every standing question's off switch (#715, ADR-0034), per
    /// [`Core::question_switches`] — in `QUESTION_ORDER`, every question
    /// present whether it has a row or not, and each carrying whether an
    /// unconfirmed local write is overlaid on it.
    ///
    /// An **applied result**, this seam's own rule: Kotlin receives the
    /// assembled list and never the absence-means-enabled reading behind
    /// it. Pair it with [`question_roster`] for the labels — the roster is
    /// a constant of the build and this is device state, which is why they
    /// are two doors (`decisions::questions`'s own header).
    ///
    /// **Android does not render this yet** (#716); the door lands here so
    /// that slice is rendering-only.
    pub async fn question_switches(&self) -> Vec<MobileQuestionSwitch> {
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Settings).await
            .core
            .question_switches()
            .iter()
            .map(to_question_switch_record)
            .collect()
    }

    /// Switches one standing question on or off (#715), per
    /// [`Core::set_question_enabled`] — which overlays, so a following
    /// [`MobileTaskHost::question_switches`] reports the new state and
    /// `pending` immediately.
    ///
    /// The question crosses as this seam's own enum rather than a wire
    /// string, so there is no vocabulary to typo and no "unknown question"
    /// outcome to model — the `BindingKey::parse` rejection
    /// [`MobileTaskHost::set_binding`] needs is bought here by the type.
    /// The seed is minted internally ([`mint_mutation_seed`]), as every
    /// mutation door added since #529 does.
    pub async fn set_question_enabled(
        &self,
        question: MobileStandingQuestion,
        enabled: bool,
        now_ms: i64,
    ) -> Result<(), MobileSetBindingError> {
        let seed = mint_mutation_seed("question-enabled", now_ms);
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Settings).await
            .core
            .set_question_enabled(&seed, unmap_standing_question(question), enabled, now_ms)
            .await
            .map_err(|error| MobileSetBindingError::WriteFailed {
                detail: error.to_string(),
            })
    }

    /// A fresh device token from the person (first entry, or rotation
    /// after a `credential_needed` event). Always resumes a hold — see
    /// [`Core::push_api_key`].
    pub async fn push_api_key(&self, api_key: String) {
        let mut inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Settings).await;
        inner.api_key = Some(api_key.clone());
        inner.core.push_api_key(api_key);
    }

    /// The host reloading a token it already had stored (app start), never
    /// resuming a hold — see [`Core::rehydrate_api_key`].
    pub async fn rehydrate_api_key(&self, api_key: String) {
        let mut inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Settings).await;
        inner.api_key = Some(api_key.clone());
        inner.core.rehydrate_api_key(api_key);
    }

    /// "Forget token": clears the in-memory credential. Nothing durable to
    /// clean up — the core never persisted it.
    pub async fn clear_api_key(&self) {
        let mut inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Settings).await;
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
        let inner = &mut *self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Sync).await;
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
        // #710: `operation.requested`/`operation.local_commit` bracket the
        // durable local write `Core::capture` does — this call never
        // touches the network (it only enqueues, `Core::capture`'s own
        // doc), so `operation.local_commit` is recorded synchronously
        // right after that enqueue succeeds, always before any
        // `http.started` this operation's id could ever carry (there
        // isn't one, by construction) — pinned by this module's own
        // `a_successful_capture_orders_operation_local_commit_before_any_http_started`
        // test, over a real `MobileTaskHost::capture` call.
        let operation_id = mint_mutation_seed("capture-op", now_ms);
        let session_id = current_diagnostic_session_id();
        self.diag_session
            .emit_operation_requested(&self.diag_sink, &session_id, &operation_id);
        let mut inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Capture).await;
        let result = inner
            .core
            .capture(&seed, draft.title, stage, now_ms, options, Some(&operation_id))
            .await
            .map_err(|error| MobileCaptureError::CaptureFailed {
                detail: error.to_string(),
            });
        if result.is_ok() {
            self.diag_session
                .emit_operation_local_commit(&self.diag_sink, &session_id, &operation_id);
        }
        result
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
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Projects).await
            .core
            .projects()
            .into_iter()
            .map(|project| MobileProject { id: project.id, name: project.name })
            .collect()
    }

    /// The Done screen's whole read (M3/#532):
    /// [`hummingbird_core::Core::done`] — every live `Done` item — ordered
    /// most-recently-touched first ([`roster::order_done`]), the sink of
    /// `done-order.ts`'s own `orderDone`. Kotlin does no ordering; the seam
    /// hands it the finished order.
    pub async fn done_items(&self) -> Vec<MobileDoneRecord> {
        let inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Read).await;
        let items = inner.core.done();
        let roster_items: Vec<roster::RosterItem> = items.iter().map(to_roster_item).collect();
        let order = roster::order_done(&roster_items);
        let by_id: HashMap<&str, &Item> =
            items.iter().map(|item| (item.id.as_str(), item)).collect();
        order
            .into_iter()
            .filter_map(|id| {
                by_id.get(id.as_str()).map(|item| MobileDoneRecord {
                    id: item.id.clone(),
                    title: item.title.clone(),
                    updated_at: item.updated_at,
                    pending: inner.core.is_pending(&item.id),
                })
            })
            .collect()
    }

    /// The Ledger's whole read (M3/#532):
    /// [`hummingbird_core::Core::ledger`] — every item this mirror has ever
    /// known — pre-ordered ([`roster::order_ledger`], last-touched first)
    /// with each row already carrying its [`MobileLedgerRowState`]
    /// ([`roster::ledger_row_state`]), last-touched instant
    /// ([`roster::last_touched_ms`]) and one-click `can_mark_done` gate: the
    /// sink of `ledger-order.ts`'s three exports plus `item-actions.ts`'s
    /// widened rule, applied once here rather than per row in Kotlin.
    pub async fn ledger_rows(&self, now_ms: i64) -> Vec<MobileLedgerRowRecord> {
        let inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Read).await;
        let entries = inner.core.ledger(now_ms);
        let roster_items: Vec<roster::LedgerRosterItem> =
            entries.iter().map(to_ledger_roster_item).collect();
        let order = roster::order_ledger(&roster_items);
        let by_id: HashMap<&str, &hummingbird_core::LedgerEntry> =
            entries.iter().map(|entry| (entry.item.id.as_str(), entry)).collect();
        order
            .into_iter()
            .filter_map(|id| {
                by_id.get(id.as_str()).map(|entry| {
                    let roster_item = to_ledger_roster_item(entry);
                    let state = roster::ledger_row_state(&roster_item);
                    MobileLedgerRowRecord {
                        id: entry.item.id.clone(),
                        title: entry.item.title.clone(),
                        stage: entry.item.stage.as_str().to_string(),
                        state: map_ledger_row_state(state),
                        last_touched_ms: roster::last_touched_ms(&roster_item),
                        pending: inner.core.is_pending(&entry.item.id),
                        dead_lettered: entry.dead_lettered,
                        has_live_alert: entry.has_live_alert,
                        can_mark_done: matches!(state, roster::LedgerRowState::Live)
                            && can_mark_done(entry.item.stage, entry.item.archived_at.is_some()),
                    }
                })
            })
            .collect()
    }

    /// Drains queued [`CoreEvent`]s (today: `credential_needed`) — a
    /// pull-based drain, never a host-implemented callback (ADR-0003).
    pub async fn take_events(&self) -> Vec<MobileEvent> {
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Read).await
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
        let inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Read).await;
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
        let mut inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Act).await;
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
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Read).await
            .core
            .item_detail(&item_id, now_ms)
            .map(|detail| to_item_detail_record(&detail, now_ms))
    }

    /// Confirms a completed Grill interview (#354/#539, ADR-0023) —
    /// [`hummingbird_core::Core::complete_grill`] verbatim: one atomic
    /// mutation, `session_steps` compared against the item's LIVE steps to
    /// force a re-review on drift ([`MobileGrillCompletionError::NeedsReReview`]).
    /// Returns the minted Grill id, the same "the takeover stays up until
    /// this answers `ok`" contract the web's own `useGrillTakeoverWiring.ts`
    /// documents — this door decides nothing about when to close; the
    /// caller does, off this result.
    pub async fn complete_grill(
        &self,
        item_id: String,
        session_steps: Vec<ItemStepRecord>,
        completion: MobileGrillCompletion,
        now_ms: i64,
    ) -> Result<String, MobileGrillCompletionError> {
        let seed = mint_mutation_seed("grill", now_ms);
        let steps = to_domain_steps(&session_steps);
        let mut inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Grill).await;
        inner
            .core
            .complete_grill(
                &seed,
                &item_id,
                &steps,
                hummingbird_core::GrillCompletion {
                    transcript: completion.transcript,
                    summary: completion.summary,
                    verdict: unmap_verdict(completion.verdict),
                    model_proposal: completion.model_proposal,
                    applied_patch: completion.applied_patch,
                    delete_unticked_plan: completion.delete_unticked_plan,
                },
                now_ms,
            )
            .await
            .map_err(|error| match error {
                hummingbird_core::CompleteGrillError::ItemNotFound => {
                    MobileGrillCompletionError::ItemNotFound
                }
                hummingbird_core::CompleteGrillError::ItemDone => MobileGrillCompletionError::ItemDone,
                hummingbird_core::CompleteGrillError::NeedsReReview => {
                    MobileGrillCompletionError::NeedsReReview
                }
                other => MobileGrillCompletionError::CompletionFailed { detail: other.to_string() },
            })
    }

    /// #356's device-local draft read: this item's saved Grill turns,
    /// typed — `None` when the item has no draft. Kotlin never parses JSON
    /// (ADR-0025's own rule for this seam): the draft is opaque
    /// `serde_json::Value` on the core side, and this door is where it
    /// becomes the same typed `Vec<MobileGrillTurn>` [`save_grill_draft`]
    /// takes, resolved through [`skills::GrillTurn`]'s own shape. A draft
    /// this build cannot read back as that shape (a future format, or
    /// corruption) answers `None`, exactly like no draft at all — nothing
    /// worth resuming.
    pub async fn grill_draft(&self, item_id: String) -> Option<Vec<MobileGrillTurn>> {
        let value = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Grill).await.core.grill_draft(&item_id)?.clone();
        let turns: Vec<skills::GrillTurn> = serde_json::from_value(value).ok()?;
        Some(turns.into_iter().map(from_domain_turn).collect())
    }

    /// Whether `item_id` carries a saved draft — the Triage row's own
    /// "Grill me"/"Resume grill" label source, one item at a time.
    pub async fn has_grill_draft(&self, item_id: String) -> bool {
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Grill).await.core.has_grill_draft(&item_id)
    }

    /// Saves (or replaces) `item_id`'s draft — #356's "every completed turn
    /// re-saves automatically" contract.
    pub async fn save_grill_draft(
        &self,
        item_id: String,
        turns: Vec<MobileGrillTurn>,
        now_ms: i64,
    ) -> Result<(), MobileGrillDraftError> {
        let turns: Vec<skills::GrillTurn> = turns.into_iter().map(map_turn).collect();
        let value = serde_json::to_value(&turns)
            .map_err(|error| MobileGrillDraftError::SaveFailed { detail: error.to_string() })?;
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Grill).await
            .core
            .save_grill_draft(&item_id, value, now_ms)
            .await
            .map_err(|error| MobileGrillDraftError::SaveFailed { detail: error.to_string() })
    }

    /// #356's explicit, confirmed "Discard" gesture — a no-op, not an
    /// error, when no draft exists. Also the one place a completed Grill's
    /// `"ok"` clears the interview that produced it; the caller's job to
    /// call this only then, matching the web's own
    /// `useGrillTakeoverWiring.ts`.
    pub async fn discard_grill_draft(
        &self,
        item_id: String,
        now_ms: i64,
    ) -> Result<(), MobileGrillDraftError> {
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Grill).await
            .core
            .discard_grill_draft(&item_id, now_ms)
            .await
            .map_err(|error| MobileGrillDraftError::SaveFailed { detail: error.to_string() })
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
        let patch = to_triage_patch(&edit).map_err(|detail| MobileEditError::EditFailed { detail })?;
        let seed = mint_mutation_seed("edit", now_ms);
        let mut inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Triage).await;
        inner
            .core
            .triage(&seed, &item_id, false, patch, now_ms, None)
            .await
            .map_err(|error| match error {
                hummingbird_core::ActError::ItemNotFound => MobileEditError::ItemNotFound,
                other => MobileEditError::EditFailed {
                    detail: other.to_string(),
                },
            })
    }

    /// The Triage screen's whole read (M3/#531) — [`build_triage_board`]'s
    /// pure core over [`hummingbird_core::Core::triage_inbox`],
    /// [`hummingbird_core::Core::grilling_items`] and
    /// [`hummingbird_core::Core::grill_draft_item_ids`], the same three
    /// reads [`MobileTaskHost::now_board`] already combines for the
    /// frontier board's inline triage rows — this door decides the same
    /// queue on its own, for the screen whose whole reason to exist is that
    /// queue.
    pub async fn triage_board(&self, now: String) -> TriageBoardRecord {
        let inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Triage).await;
        let triage_items = inner.core.triage_inbox();
        let grilling_items = inner.core.grilling_items();
        let draft_item_ids = inner.core.grill_draft_item_ids();
        build_triage_board(&triage_items, &grilling_items, &draft_item_ids, &now)
    }

    /// The Triage screen's mutation (M3/#531) —
    /// [`hummingbird_core::Core::triage`] verbatim, the same [`ItemEdit`]→
    /// [`hummingbird_core::TriagePatch`] conversion [`to_triage_patch`]
    /// [`MobileTaskHost::edit_item`] shares, except `promote_to_ready` rides
    /// as a real caller-supplied argument rather than pinned `false`:
    /// promoting to Ready is this screen's one destination, and every edit
    /// alongside it still lands in the same single CAS `PATCH`
    /// [`Core::triage`]'s own doc argues for.
    pub async fn triage_item(
        &self,
        item_id: String,
        promote_to_ready: bool,
        edit: ItemEdit,
        now_ms: i64,
    ) -> Result<(), MobileEditError> {
        let patch = to_triage_patch(&edit).map_err(|detail| MobileEditError::EditFailed { detail })?;
        let seed = mint_mutation_seed("triage", now_ms);
        let mut inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Triage).await;
        inner
            .core
            .triage(&seed, &item_id, promote_to_ready, patch, now_ms, None)
            .await
            .map_err(|error| match error {
                hummingbird_core::ActError::ItemNotFound => MobileEditError::ItemNotFound,
                other => MobileEditError::EditFailed {
                    detail: other.to_string(),
                },
            })
    }

    /// The weekend-plans pane's do-date chip (#537, #122): one new seam
    /// mutation wrapping [`hummingbird_core::Core::triage`] with
    /// `promote_to_ready: false` and only `scheduled_date` touched — a
    /// scheduling write, never a promotion, exactly the shape `Core::
    /// triage`'s own doc names for this write ("the weekend-plans pane's
    /// do-date chip"). `scheduled_date: None` clears an already-planned
    /// day (a second tap on its own chip); `Some(date)` sets it. Not
    /// [`MobileTaskHost::edit_item`] with an otherwise-`Untouched`
    /// [`ItemEdit`]: that would make the pane build a nine-field record for
    /// a one-field write, the same "wrap the entry point, not the whole
    /// editor" call `App.tsx`'s own `handleSetScheduledDate` makes on web.
    pub async fn set_scheduled_date(
        &self,
        item_id: String,
        scheduled_date: Option<String>,
        now_ms: i64,
    ) -> Result<(), MobileEditError> {
        let patch = hummingbird_core::TriagePatch {
            scheduled_date: Some(scheduled_date),
            ..hummingbird_core::TriagePatch::default()
        };
        let seed = mint_mutation_seed("weekend-schedule", now_ms);
        let mut inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Triage).await;
        inner
            .core
            .triage(&seed, &item_id, false, patch, now_ms, None)
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
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Read).await
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
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Act).await
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
        let mut inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Act).await;
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
        let inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Settings).await;
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
                    operation_id: None,
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

    // ------------------------------------------------------------- M4 (#540)

    /// Every rule this device has mirrored, decided into [`RuleRecord`]s —
    /// [`hummingbird_core::Core::rules`] verbatim, in the mirror's own
    /// order. Validity is measured against the compiled registry
    /// ([`rules::compiled_registry`]), the same catalogue the authority
    /// evaluates with, so a rule flagged here is a rule that really has
    /// stopped being able to fire.
    pub async fn rules(&self) -> Vec<RuleRecord> {
        let registry = rules::compiled_registry();
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Rules).await
            .core
            .rules()
            .iter()
            .map(|rule| to_rule_record(rule, &registry))
            .collect()
    }

    /// One rule by id, or `None` if this device has not mirrored it — the
    /// editor's own read.
    pub async fn rule(&self, rule_id: String) -> Option<RuleRecord> {
        let registry = rules::compiled_registry();
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Rules).await
            .core
            .rules()
            .iter()
            .find(|rule| rule.id == rule_id)
            .map(|rule| to_rule_record(rule, &registry))
    }

    /// Everything the create-and-edit form needs for `event_kind`, decided
    /// once per form open: the selectable kinds, the fields that kind
    /// offers with their legal operators and widgets already resolved, the
    /// severity vocabulary in ADR-0014's ratchet order, both tiers, and the
    /// alarm interval a duration warning is measured against.
    ///
    /// `None` is ADR-0013's "any kind", which narrows the field list to the
    /// Event core — decided by [`rules::fields_for_kind`], not by Kotlin.
    /// No `Core` state is read, so this never contends the lock.
    pub async fn rule_form(&self, event_kind: Option<String>) -> RuleFormRecord {
        let registry = rules::compiled_registry();
        let mut kind_options = vec![KindOptionRecord {
            key: None,
            label_key: ANY_KIND_KEY.to_string(),
        }];
        kind_options.extend(registry.kinds.iter().map(|kind| KindOptionRecord {
            key: Some(kind.key.clone()),
            label_key: kind.key.clone(),
        }));
        RuleFormRecord {
            kind_options,
            fields: rules::fields_for_kind(&registry, event_kind.as_deref())
                .iter()
                .map(to_rule_field_record)
                .collect(),
            severities: registry.severities.clone(),
            sources: registry
                .sources
                .iter()
                .map(|source| SourceOptionRecord {
                    source: source.source.clone(),
                    retired_as: source.retired_as.clone(),
                })
                .collect(),
            default_severity: rules::DEFAULT_SEVERITY.to_string(),
            tiers: vec![MobileTier::Urgent, MobileTier::Normal],
            alarm_interval_ms: registry.alarm_interval_ms,
        }
    }

    /// Creates a rule — [`hummingbird_core::Core::create_rule`], enqueued
    /// durably like every other mutation here. Conditions arrive as
    /// [`RuleConditionInput`]s and are typed into the wire's `Condition`
    /// shape Rust-side; a field the kind does not declare, an operator its
    /// type does not permit, or a value its type cannot hold is refused at
    /// this seam rather than sent for the authority to 400.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_rule(
        &self,
        name: String,
        event_kind: Option<String>,
        conditions: Vec<RuleConditionInput>,
        severity: String,
        tier: MobileTier,
        enabled: bool,
        now_ms: i64,
    ) -> Result<String, MobileRuleError> {
        let registry = rules::compiled_registry();
        let conditions = to_conditions(&conditions, &registry, event_kind.as_deref())?;
        let seed = mint_mutation_seed("create-rule", now_ms);
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Rules).await
            .core
            .create_rule(
                &seed,
                name,
                event_kind,
                conditions,
                severity,
                unmap_tier(tier),
                enabled,
                now_ms,
            )
            .await
            .map_err(|error| MobileRuleError::SaveFailed {
                detail: error.to_string(),
            })
    }

    /// Patches a rule — [`hummingbird_core::Core::patch_rule`], one CAS
    /// `PATCH` for the whole edit. Every field is a [`FieldPatch`]-shaped
    /// three-way answer for the same reason [`ItemEdit`]'s are: **"this
    /// rule now names no kind" is not the same as "I did not touch the
    /// kind"**, and only `event_kind` can be cleared (to ADR-0013's null
    /// kind), so it is the only one that needs all three. The others are
    /// plain `Option`s — `NOT NULL` columns cannot be cleared, the same
    /// asymmetry the authority enforces with a 400.
    ///
    /// **The enable/disable toggle is this method with `enabled` set and
    /// everything else `None`** — one CAS field, exactly #140's acceptance
    /// criterion, and no second entry point that could drift from it.
    ///
    /// `conditions` is re-typed against `event_kind` when that is being set
    /// too, and against the rule's current kind otherwise — so a condition
    /// list is never validated against a kind the save is about to leave
    /// behind.
    #[allow(clippy::too_many_arguments)]
    pub async fn patch_rule(
        &self,
        rule_id: String,
        name: Option<String>,
        event_kind: FieldPatch,
        conditions: Option<Vec<RuleConditionInput>>,
        severity: Option<String>,
        tier: Option<MobileTier>,
        enabled: Option<bool>,
        now_ms: i64,
    ) -> Result<(), MobileRuleError> {
        let registry = rules::compiled_registry();
        let mut inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Rules).await;
        let current = inner
            .core
            .rules()
            .into_iter()
            .find(|rule| rule.id == rule_id)
            .ok_or(MobileRuleError::RuleNotFound)?;

        let event_kind_patch = event_kind.to_text();
        // The kind the conditions are validated against: the one being set,
        // or the rule's current one when the kind is untouched.
        let effective_kind = match &event_kind_patch {
            Some(next) => next.clone(),
            None => current.event_kind.clone(),
        };
        let conditions = match conditions {
            Some(inputs) => Some(to_conditions(&inputs, &registry, effective_kind.as_deref())?),
            None => None,
        };

        let seed = mint_mutation_seed("patch-rule", now_ms);
        inner
            .core
            .patch_rule(
                &seed,
                &current,
                name,
                event_kind_patch,
                conditions,
                severity,
                tier.map(unmap_tier),
                enabled,
                // Untouched: an edit says nothing about whether the rule is
                // deleted, and [`Core::patch_rule`]'s `None` is exactly
                // that. `delete_rule` below is the one caller that sets it.
                None,
                now_ms,
            )
            .await
            .map_err(|error| MobileRuleError::SaveFailed {
                detail: error.to_string(),
            })
    }

    /// Deletes a rule — the same `PATCH /api/rules/:id`
    /// ([`hummingbird_core::Core::patch_rule`]) with `deleted_at` as its one
    /// touched field. A **soft** delete: the flagged row still rides the
    /// delta pull, which is what makes it leave every other device's screen
    /// rather than linger until a full sweep.
    ///
    /// A named method rather than a seventh argument on [`Self::patch_rule`]
    /// because it is a different gesture with a different confirmation
    /// behind it, and because every existing Kotlin caller of that method
    /// passes six fields it *is* editing. The write underneath is the same
    /// one CAS patch either way — there is no `Core::delete_rule` for this
    /// to drift from.
    ///
    /// `now_ms` is both the flag's own timestamp and the enqueue clock: the
    /// host owns the wall clock here as it does at every other mutation
    /// entry point in this file.
    pub async fn delete_rule(&self, rule_id: String, now_ms: i64) -> Result<(), MobileRuleError> {
        let mut inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Rules).await;
        let current = inner
            .core
            .rules()
            .into_iter()
            .find(|rule| rule.id == rule_id)
            .ok_or(MobileRuleError::RuleNotFound)?;
        let seed = mint_mutation_seed("delete-rule", now_ms);
        inner
            .core
            .patch_rule(
                &seed,
                &current,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(Some(now_ms)),
                now_ms,
            )
            .await
            .map_err(|error| MobileRuleError::SaveFailed {
                detail: error.to_string(),
            })
    }

    /// A draft rule's backtest against this device's own frontier
    /// (ADR-0011) — [`rules::backtest`], with the corpus caveat travelling
    /// beside the count rather than left for the reader to know.
    ///
    /// `now_local` and `now_utc` are the same instant in the two frames the
    /// evaluation reads: `deadline`/`scheduled_date` are device-local civil
    /// strings, `occurred_at` is stamped UTC by the authority. The host
    /// resolves both, because neither this crate nor `hummingbird-core`
    /// holds a timezone table — see `rules::backtest`'s header.
    pub async fn backtest_rule(
        &self,
        event_kind: Option<String>,
        conditions: Vec<RuleConditionInput>,
        now_local: String,
        now_utc: String,
    ) -> Result<BacktestRecord, MobileRuleError> {
        let registry = rules::compiled_registry();
        let conditions = to_conditions(&conditions, &registry, event_kind.as_deref())?;
        let items: Vec<rules::BacktestItem> = self
            .inner
            .lock()
            .await
            .core
            .frontier()
            .iter()
            // `item_threshold_event` stamps `occurred_at:
            // now_as_deadline(item.updated_at)` — a poll-time-derived core
            // field, exact from the same `updated_at` the mirror holds.
            .map(|item| to_backtest_item(item, hummingbird_domain::now_as_deadline(item.updated_at)))
            .collect();
        let clock = rules::BacktestClock {
            now_local,
            now_utc,
        };
        Ok(to_backtest_record(rules::backtest(
            event_kind.as_deref(),
            &conditions,
            &items,
            &clock,
        )))
    }

    /// Phase one of the pane lane's zone bridge (#536, ADR-0025): every
    /// `(zone, civil-date)` fact `surface`'s sunk questions need, given
    /// this device's own state. Empty for [`MobileSurface::Status`] today —
    /// none of the status five (kimi/github/uptime/reachability/poller) is
    /// civil-date reasoning (`panes::mod`'s own test) — kept generic over
    /// `surface` so #537's Now questions reach it unchanged.
    pub async fn pane_zone_queries(&self, surface: MobileSurface, now_ms: i64) -> Vec<MobileZoneQuery> {
        let inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Read).await;
        // Phase one reads no calendar arm — it runs before any zone is
        // resolved, and every calendar window this lane needs is a function
        // of the reader's own zone. `weekend`/`vacation` ask for their zone
        // facts here and read their events in phase two.
        let inputs = mobile_pane_inputs(
            &inner.core,
            now_ms,
            MobileSyncFacts::default(),
            CalendarArm::default(),
        );
        panes::zone_queries(map_surface(surface), &inputs)
            .iter()
            .map(to_mobile_zone_query)
            .collect()
    }

    /// Phase two: `surface`'s sunk questions, ranked and ready to render —
    /// [`panes::rank_panes`], with the host's resolved [`MobileZoneFact`]s
    /// and its own persisted sync history ([`MobileSyncFacts`], since
    /// `hummingbird-core` keeps none — see that type's own doc) folded in.
    /// Already in display order; `StatusScreen.kt` renders the list
    /// directly. Each record also carries its question's decided
    /// [`MobilePaneFacts`] (the pane-facts slice) — see the module header
    /// for why facts ride this one crossing rather than per-pane doors.
    pub async fn rank_panes(
        &self,
        surface: MobileSurface,
        now_ms: i64,
        zone_facts: Vec<MobileZoneFact>,
        sync: MobileSyncFacts,
    ) -> Vec<MobileRankedPane> {
        let zone = to_zone_facts(zone_facts);
        // The calendar lock is taken and released BEFORE `inner`'s, never
        // while holding it — see [`CalendarHalf`] for the whole of this
        // crate's lock discipline.
        let calendar = self.calendar_arm(now_ms, &zone).await;
        let inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Read).await;
        let inputs = mobile_pane_inputs(&inner.core, now_ms, sync, calendar);
        panes::rank_panes(map_surface(surface), &inputs, &zone)
            .into_iter()
            .map(|record| {
                let question = standing_question_of(&record.question);
                let facts = mobile_pane_facts_of(question, &record.subject_key, &inputs, &zone);
                MobileRankedPane {
                    standing_question: map_standing_question(question),
                    subject_key: record.subject_key,
                    pane_key: record.pane_key,
                    answer: to_mobile_pane_answer(record.answer),
                    facts,
                }
            })
            .collect()
    }

    /// [`panes::status_alarm`] — the Status nav destination's whole
    /// reading: the most salient band the Status surface currently answers,
    /// or `None` when nothing there raises the nav.
    ///
    /// **A door of its own rather than a fold over [`Self::rank_panes`].**
    /// `MainActivity` draws the bar for every screen and holds no ranked
    /// panes; making it call `rank_panes` and reduce the list would put the
    /// fold's two opinions (which answer states count, which bands are
    /// loud) in Kotlin, where the web would need its own copy — exactly the
    /// duplication ADR-0025 sinks. What stays here is the last step alone:
    /// which colour a band paints as.
    ///
    /// **Takes no zone facts but does take [`MobileSyncFacts`]**, and the
    /// asymmetry is not an oversight. Status asks for no zone facts at all
    /// ([`panes::status_alarm`]'s own doc), so there is nothing for a host
    /// to resolve. The sync history is the opposite case: `reachability` is
    /// one of the status four and *bands* off it (`reachability.rs`), so a
    /// door that defaulted it would read every device as never-synced and
    /// quietly disagree with the very board it sits above — the caller
    /// hands over the same [`SyncHistoryStore`] value it gives
    /// [`Self::rank_panes`].
    pub async fn status_alarm(&self, now_ms: i64, sync: MobileSyncFacts) -> Option<MobilePaneBand> {
        let inner = self.inner.lock().await;
        // No calendar arm, and so no calendar lock taken: the status five
        // are kimi/github/uptime/reachability/poller, none of which reads
        // the calendar at all — every question that does is on `Surface::Now`.
        let inputs = mobile_pane_inputs(&inner.core, now_ms, sync, CalendarArm::default());
        panes::status_alarm(&inputs).map(map_band)
    }

    /// Recall's whole read (#542/#478): [`hummingbird_core::Core::search`]
    /// matches, groups and orders entirely core-side, capped at
    /// [`hummingbird_core::search::CAP`] — this door only maps the answer
    /// to the wire shape and stamps `pending`. `query` crosses unmodified;
    /// an empty or whitespace-only one already answers empty rows and a
    /// zero `total` from `Core::search` itself, so there is no client-side
    /// short-circuit to duplicate here.
    pub async fn search(&self, query: String, now_ms: i64) -> MobileRecallOutcome {
        let inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Read).await;
        let outcome = inner.core.search(&query, now_ms);
        MobileRecallOutcome {
            rows: outcome
                .rows
                .iter()
                .map(|row| MobileRecallRowRecord {
                    id: row.item.id.clone(),
                    title: row.item.title.clone(),
                    stage: row.item.stage.as_str().to_string(),
                    group: map_recall_group(row.group),
                    updated_at: row.item.updated_at,
                    pending: inner.core.is_pending(&row.item.id),
                })
                .collect(),
            total: outcome.total as u32,
        }
    }

    // --------------------------------------------- the calendar (#564)
    // Mirrors the web host's `CalendarHost` shim one door for one, over the
    // same `CalendarHostCore`. See the `CalendarHalf` section above for why
    // the token never crosses this seam.

    /// Re-arms the lane at app start from the host's persisted state: the
    /// opt-in flag and the picked calendars, both DataStore-owned
    /// (Slice 3), neither a credential. An opted-in device mints once here,
    /// so a launch that is online is already connected by the time Settings
    /// renders; a never-opted-in one does nothing and stays
    /// [`MobileCalendarState::NeverConnected`] — `initConnection`'s own
    /// two-branch shape.
    pub async fn init_calendar(
        &self,
        was_previously_connected: bool,
        selections: Vec<MobileCalendarSelection>,
    ) -> MobileCalendarConnection {
        let device_token = self.device_token().await;
        let trips_id = self.trips_calendar_id().await;
        let mut calendar = self.calendar.lock().await;
        calendar.stored_selections = selections.iter().map(to_calendar_selection).collect();
        calendar.apply_selection(trips_id.as_deref());
        if !was_previously_connected {
            return calendar.connection();
        }
        calendar.opted_in = true;
        calendar.mint(device_token.as_deref()).await;
        calendar.connection()
    }

    /// The interactive Connect gesture: mint once and report what happened.
    ///
    /// A device that was already opted in stays opted in whatever this
    /// returns — `shouldKeepExistingConnection`'s rule, and the reason a
    /// cancelled or failed *re*-connect cannot cost the operator their
    /// stale-but-real mirror. A **first** connect that fails leaves the
    /// device never-connected, so the Connect affordance is still offered.
    pub async fn connect_calendar(&self) -> MobileCalendarConnection {
        let device_token = self.device_token().await;
        let mut calendar = self.calendar.lock().await;
        let was_opted_in = calendar.opted_in;
        let minted = calendar.mint(device_token.as_deref()).await;
        calendar.opted_in = was_opted_in || minted;
        calendar.connection()
    }

    /// "Disconnect": forget the opt-in and dispose of the held access
    /// token.
    ///
    /// **The mirror is not cleared.** A disconnected device's already-polled
    /// events stay on disk — the mirror is disposable, not a lie, and
    /// deleting it would turn a revocation into data loss for the panes that
    /// read it. Nothing polls it again until the device reconnects, and the
    /// panes gate on the flag this clears.
    ///
    /// **The credential is disposed of, not merely unused.** Clearing the
    /// flag alone would leave the access token live inside
    /// [`CalendarHostCore`], where [`Self::list_calendars`] reads it — and
    /// Settings lists again on exactly this state change, so a disconnected
    /// device would have gone on making authenticated Google requests until
    /// the token expired.
    pub async fn disconnect_calendar(&self) -> MobileCalendarConnection {
        let mut calendar = self.calendar.lock().await;
        calendar.opted_in = false;
        calendar.expires_at_ms = None;
        calendar.last_error = None;
        calendar.host.clear_token();
        calendar.connection()
    }

    /// The picker's options — [`CalendarHostCore::list_calendars`] over the
    /// token already pushed for polling, never a second credential
    /// crossing.
    pub async fn list_calendars(&self) -> MobileCalendarList {
        let calendar = self.calendar.lock().await;
        let response = calendar.host.list_calendars().await;
        MobileCalendarList {
            kind: response.kind.to_string(),
            calendars: response
                .calendars
                .into_iter()
                .map(|entry| MobileCalendarEntry {
                    id: entry.id,
                    summary: entry.summary,
                })
                .collect(),
        }
    }

    /// The picker's current selection. Durably the host's, like the opt-in
    /// flag.
    ///
    /// **It polls straight away rather than waiting for the next tick**, and
    /// that is a correctness matter, not a nicety: an empty selection is not
    /// a no-op poll, it is a *successful* one over zero calendars, so the
    /// first tick after Connect saves a real, fresh, empty snapshot. Leaving
    /// the new selection until the next 15-minute tick would have the
    /// weekend pane answer a confident "nothing on" for a quarter of an hour
    /// after the operator picked the calendar that says otherwise.
    /// `ContextPoller::refresh` is the user-invoked trigger this is.
    pub async fn set_calendar_selections(
        &self,
        selections: Vec<MobileCalendarSelection>,
        now_ms: i64,
    ) -> MobileCalendarTick {
        let device_token = self.device_token().await;
        let trips_id = self.trips_calendar_id().await;
        let mut calendar = self.calendar.lock().await;
        calendar.stored_selections = selections.iter().map(to_calendar_selection).collect();
        calendar.apply_selection(trips_id.as_deref());
        if !calendar.opted_in {
            return MobileCalendarTick {
                outcome: outcome_name(hummingbird_core::context::PollOutcome::NoCredential)
                    .to_string(),
                connection: calendar.connection(),
            };
        }
        if calendar.due_for_rotation(now_ms) {
            calendar.mint(device_token.as_deref()).await;
        }
        let outcome = calendar.host.refresh(now_ms).await;
        MobileCalendarTick {
            outcome: outcome_name(outcome).to_string(),
            connection: calendar.connection(),
        }
    }

    /// One foreground timer tick, at [`calendar_poll_interval_ms`].
    ///
    /// The whole mint/rotate/401 loop is here, not in Kotlin:
    ///
    /// 1. A device that never opted in polls nothing and answers
    ///    `"no_credential"`.
    /// 2. If the held token is inside `connection.ts`'s rotation margin (or
    ///    there is none), re-mint **before** polling — that margin is
    ///    smaller than the authority's own cache margin, so this request is
    ///    normally a cache hit that nonetheless comes back with a genuinely
    ///    fresh token.
    /// 3. Poll.
    /// 4. Drain the poller's credential events (a live 401 mid-poll) and
    ///    re-mint on any of them, so the *next* tick starts with a good
    ///    token rather than repeating the 401.
    pub async fn calendar_on_timer(&self, now_ms: i64) -> MobileCalendarTick {
        let device_token = self.device_token().await;
        let trips_id = self.trips_calendar_id().await;
        let mut calendar = self.calendar.lock().await;
        // Re-derived every tick, not just at a picker gesture: the Trips
        // binding is *synced*, so it can arrive from another device between
        // two ticks and must start being polled without the operator
        // opening Settings.
        calendar.apply_selection(trips_id.as_deref());
        if !calendar.opted_in {
            return MobileCalendarTick {
                outcome: outcome_name(hummingbird_core::context::PollOutcome::NoCredential)
                    .to_string(),
                connection: calendar.connection(),
            };
        }
        if calendar.due_for_rotation(now_ms) {
            calendar.mint(device_token.as_deref()).await;
        }
        let outcome = calendar.host.on_timer(now_ms).await;
        if !calendar.host.take_credential_events().is_empty() {
            calendar.mint(device_token.as_deref()).await;
        }
        MobileCalendarTick {
            outcome: outcome_name(outcome).to_string(),
            connection: calendar.connection(),
        }
    }

}

/// Not `#[uniffi::export]`ed: this block holds the calendar lane's internal
/// helper, which is Rust-side plumbing rather than a door. Kotlin has no
/// business asking for the device token back.
impl MobileTaskHost {
    /// #710: the *only* way any method on this type reaches `inner` — every
    /// existing `self.inner.lock().await` call site was migrated to a call
    /// through this, so "every acquisition… emits `core.wait_started`/
    /// `core.acquired`/`core.released`" is true by construction, not by
    /// each call site remembering to instrument itself. See `core_lock`'s
    /// module doc for what `owner` means on each of the three events and
    /// why release goes through a `Drop` guard.
    async fn lock_inner(&self, owner: hummingbird_core::diagnostics::CoreOwner) -> core_lock::OwnedGuard<'_, Inner> {
        let session_id = current_diagnostic_session_id();
        core_lock::lock_with_diagnostics(
            &self.inner,
            &self.core_owner,
            &self.diag_session,
            &self.diag_sink,
            &session_id,
            owner,
        )
        .await
    }

    /// The device token, copied out from under `inner`'s lock and released
    /// again before the calendar's is taken — see [`CalendarHalf`] for why
    /// that order is the whole lock discipline here.
    async fn device_token(&self) -> Option<String> {
        self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Calendar)
            .await
            .api_key
            .clone()
    }

    /// The designated Trips calendar, read off the synced bindings table
    /// under `inner`'s lock and released before the calendar's is taken —
    /// the same lock order [`Self::device_token`] keeps, for the same
    /// reason.
    ///
    /// It is a *binding*, not a device preference: the operator names their
    /// Trips calendar once, anywhere, and every device polls it. That is
    /// why the polled set cannot be the picker's list alone.
    async fn trips_calendar_id(&self) -> Option<String> {
        let inner = self.lock_inner(hummingbird_core::diagnostics::CoreOwner::Calendar).await;
        inner
            .core
            .bindings()
            .into_iter()
            .find(|binding| binding.key == vacation::TRIPS_CALENDAR_BINDING_KEY)
            .and_then(|binding| match binding.value {
                BindingValue::Text { text } => {
                    let id = text.trim().to_string();
                    if id.is_empty() {
                        None
                    } else {
                        Some(id)
                    }
                }
                _ => None,
            })
    }

    /// Phase two's calendar arm (#621): each calendar-reading question's own
    /// window, read off this device's mirror.
    ///
    /// **The windows are computed core-side, never here.**
    /// `weekend_calendar_interval` and `vacation_calendar_interval` hold the
    /// bounds the web's `calendarRequests` computes for itself — a Kotlin or
    /// ffi-side copy of "Friday 17:00 through Sunday" or "seven days back,
    /// seven hundred and thirty ahead" is exactly the drift ADR-0025 exists
    /// to prevent. The web is **not** rewired to these yet (it needs an
    /// `ffi-web` export #564 did not scope), so the two copies coexist;
    /// ADR-0025's second #564 amendment records that as a divergence rather
    /// than a completed sink. A question whose zone could not be
    /// resolved contributes **no entry at all**, which the panes already
    /// read as "not requested yet" — never an empty `Read`, which would
    /// claim this device had looked and found nothing.
    async fn calendar_arm(&self, now_ms: i64, zone: &ZoneFacts) -> CalendarArm {
        let calendar = self.calendar.lock().await;
        let mut reads = HashMap::new();
        let intervals = [
            (weekend::CALENDAR_REQUEST_KEY, weekend::weekend_calendar_interval(now_ms, zone)),
            (vacation::CALENDAR_REQUEST_KEY, vacation::vacation_calendar_interval(now_ms, zone)),
            // #693: wired in alongside its two siblings the day it landed —
            // unlike the Android *card* (Compose UI, #694's own scope,
            // `NowPanesExpanded.kt`'s `MobilePaneFacts.Scps -> Unit`), this
            // is the same read-model plumbing `weekend`/`vacation` already
            // get, and leaving it out would make `MobilePaneFacts::Scps`
            // permanently `{ resolved: None }` on every device for no
            // reason connected to #694's own scope.
            (scps::CALENDAR_REQUEST_KEY, scps::scps_calendar_interval(now_ms, zone)),
        ];
        for (key, interval) in intervals {
            let Some(interval) = interval else { continue };
            let response = calendar
                .host
                .events_in_interval(
                    interval.start_ms,
                    interval.end_ms,
                    interval.start_date,
                    interval.end_date,
                    now_ms,
                )
                .await;
            reads.insert(key.to_string(), to_calendar_read_facts(response));
        }
        CalendarArm { reads, connected: calendar.opted_in }
    }
}

/// The calendar lane's declared poll cadence, read through the seam so
/// Android adds **zero** new places the 15 minutes is written down —
/// `MainActivity`'s calendar loop takes its interval from here, the way
/// `useCalendarWiring.ts` takes it from the same constant.
///
/// A free door rather than a method: it is a constant, and there is no host
/// state to reach for it.
#[uniffi::export]
pub fn calendar_poll_interval_ms() -> i64 {
    CALENDAR_POLL_INTERVAL_MS
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
// arrive) while [`grill_turn_no_token`] carries `false`. A decline a
// backend answered is not evidence any backend is unreachable, and nothing
// downstream can recover the difference from the prose (`decline.rs`
// forbids matching on it).
//
// [`grill_turn_transport_failed`] is the one report that **takes** the flag
// rather than fixing it, because its two cases are indistinguishable from
// here: a socket that died before the response resolved answered nothing,
// while a body that tore after its headers arrived was answered by a
// backend that then lost the run. The web draws exactly this line —
// `route-run.ts` sets `answered` on `fetch`'s resolve path and its comment
// states that "a body that later tears mid-stream does not unset it" — and
// only the transport knows which side of `execute()` its `IOException`
// came from.
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

/// `grill-me`'s terminal proposal. **`patch_json` is opaque on the write
/// path** — the raw object text, carried whole to `Core::complete_grill`'s
/// `applied_patch`; Kotlin never parses it. A JSON string rather than a map
/// because uniffi has no `serde_json::Value`, and because a typed mirror
/// here would be a second schema for a field the write path never reads
/// into. The one reader is [`grill_proposal_rows`] (#595), which turns it
/// into labelled rows **on this side of the boundary** so the confirm
/// screen shows words — the parse stays in Rust, per ADR-0025.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileGrillProposal {
    pub summary: String,
    pub verdict: MobileGrillVerdict,
    pub patch_json: String,
}

/// One labelled row of the review card's "Proposed edit" section (#595) —
/// [`hummingbird_core::decisions::skills::review::ProposedEditRow`],
/// mirrored. `current` is `None` when the item holds nothing for the field.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileProposedEditRow {
    pub field: String,
    pub label: String,
    pub current: Option<String>,
    pub proposed: String,
}

/// #595: the review card's "Proposed edit" rows, decided core-side — the
/// same pure-seam shape as [`notification_tap_target`]: applied results,
/// never a per-row decision function (ADR-0025). Takes the proposal's
/// `patch_json` and the detail record the takeover already holds; an
/// unparseable patch yields no rows, and the card states the empty fact.
/// The rows change nothing about what Confirm records — `patch_json` still
/// travels whole as `applied_patch`.
#[uniffi::export]
pub fn grill_proposal_rows(
    patch_json: String,
    item: ItemDetailRecord,
) -> Vec<MobileProposedEditRow> {
    let patch: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&patch_json).unwrap_or_default();
    let current = hummingbird_core::decisions::skills::review::CurrentItemFields {
        title: item.title,
        description: item.description,
        size: item.size,
        energy: item.energy,
        context: item.context,
        priority: item.priority,
        deadline: item.deadline,
    };
    hummingbird_core::decisions::skills::review::proposal_rows(&patch, &current)
        .into_iter()
        .map(|row| MobileProposedEditRow {
            field: row.field,
            label: row.label,
            current: row.current,
            proposed: row.proposed,
        })
        .collect()
}

/// One completed round, threaded back on the next request — `grill-me` is
/// stateless and every request carries the whole conversation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileGrillTurn {
    pub question: MobileGrillQuestion,
    pub answer: String,
}

/// [`hummingbird_core::GrillCompletion`], mirrored — the review card's
/// Confirm (#354/#539, ADR-0023).
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileGrillCompletion {
    pub transcript: String,
    pub summary: String,
    pub verdict: MobileGrillVerdict,
    pub model_proposal: String,
    pub applied_patch: String,
    pub delete_unticked_plan: bool,
}

/// [`hummingbird_core::CompleteGrillError`], mirrored — the same
/// `ItemNotFound`/`ActFailed`-shaped split [`MobileActError`] draws for its
/// own seam, plus `NeedsReReview` (#354's own guard: the item's live steps
/// drifted from this session's snapshot, and the caller must send the
/// reader back to the review card rather than close it).
#[derive(Debug, uniffi::Error)]
pub enum MobileGrillCompletionError {
    ItemNotFound,
    ItemDone,
    NeedsReReview,
    CompletionFailed { detail: String },
}

impl std::fmt::Display for MobileGrillCompletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobileGrillCompletionError::ItemNotFound => write!(f, "item not found"),
            MobileGrillCompletionError::ItemDone => write!(f, "item is done"),
            MobileGrillCompletionError::NeedsReReview => {
                write!(f, "the item's steps changed since this review started")
            }
            MobileGrillCompletionError::CompletionFailed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for MobileGrillCompletionError {}

/// #356's device-local draft store failed to persist — a storage failure,
/// not a validation one.
#[derive(Debug, uniffi::Error)]
pub enum MobileGrillDraftError {
    SaveFailed { detail: String },
}

impl std::fmt::Display for MobileGrillDraftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobileGrillDraftError::SaveFailed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for MobileGrillDraftError {}

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

fn from_domain_turn(turn: skills::GrillTurn) -> MobileGrillTurn {
    MobileGrillTurn { question: map_question(turn.question), answer: turn.answer }
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

/// The one report whose `answered` the transport must supply rather than
/// this file: a socket can tear *before* the response resolves (nothing
/// answered) or *after* its headers arrived and the body died mid-stream (a
/// backend did answer, and the run was its to lose). Both land in the same
/// `IOException`, and only the caller knows which side of `execute()` it was
/// on.
fn transport_failed_event(detail: &str, answered: bool) -> skills::SkillEvent {
    skills::SkillEvent::Failed {
        error: skills::decline_for_transport(detail),
        backend: None,
        model: None,
        answered,
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
    answered: bool,
) -> MobileGrillTurnState {
    reduce_grill(state, transport_failed_event(&detail, answered))
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
    answered: bool,
) -> MobileSkillRunState {
    reduce_run_state(state, transport_failed_event(&detail, answered))
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

/// #274's one-tap fallback offer — [`skills::declined_backend_fallback`]
/// verbatim (ADR-0001 rule 2/ADR-0025: this crate maps, it does not decide;
/// the four-clause predicate itself lives in `hummingbird_core::decisions::
/// skills::backend`, not here, since #539's round-2 review). A thin mapping
/// wrapper the same shape [`notification_tap_target`] is for its own
/// core-decided verdict.
#[uniffi::export]
pub fn declined_backend_fallback(
    state: MobileSkillRunState,
    selection: String,
    registry_ids: Vec<String>,
) -> Option<String> {
    skills::declined_backend_fallback(&unmap_run_state(state), &selection, &registry_ids)
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

/// The Grill button's own label — `hummingbird_core::decisions::
/// grill_button_label` verbatim (#539: the Triage row's button is live).
#[uniffi::export]
pub fn item_grill_button_label(has_draft: bool) -> String {
    hummingbird_core::decisions::grill_button_label(has_draft).to_string()
}

/// Whether item detail's own Grill button renders at all —
/// `hummingbird_core::decisions::can_grill` verbatim (#539: item detail's
/// mount, the Triage board's own rows read this through
/// [`TriageItemRecord::can_grill`] instead, already decided per row).
#[uniffi::export]
pub fn item_can_grill(stage: String) -> bool {
    match hummingbird_domain::Stage::parse(&stage) {
        Some(stage) => can_grill(stage),
        None => false,
    }
}

// ---- #274's backend picker (#539): the tier fallback and the
// degrade-to-Auto rule. The registry itself — label, model, endpoint,
// connect timeout — stays a Kotlin-side list of ids; see
// `hummingbird_core::decisions::skills::backend`'s own header for why.

/// The sentinel selection value — never a registered id.
#[uniffi::export]
pub fn backend_auto_selection() -> String {
    skills::AUTO_SELECTION.to_string()
}

/// The one-tap fallback offered when a pin declines: the next registered id
/// that is not the dead one, in registry order.
#[uniffi::export]
pub fn fallback_backend_id(registry_ids: Vec<String>, dead_id: String) -> Option<String> {
    skills::fallback_backend_id(&registry_ids, &dead_id)
}

/// Auto when nothing is stored, or when the stored id no longer names a
/// registered entry.
#[uniffi::export]
pub fn resolve_backend_selection(stored: Option<String>, registry_ids: Vec<String>) -> String {
    skills::resolve_backend_selection(stored.as_deref(), &registry_ids)
}

// ---- the Grill review card's predicates (#355/#359, ADR-0023; sunk here
// from `client/web/src/screens/grill-review.ts` at #539).

/// Whether confirming this verdict risks stranding a live plan.
#[uniffi::export]
pub fn grill_would_strand_plan(verdict: MobileGrillVerdict, steps: Vec<ItemStepRecord>) -> bool {
    skills::would_strand_plan(unmap_verdict(verdict), &to_domain_steps(&steps))
}

/// The plan-replacement tick's own label, naming the live undone count.
#[uniffi::export]
pub fn grill_plan_replacement_label(steps: Vec<ItemStepRecord>) -> String {
    skills::plan_replacement_label(&to_domain_steps(&steps))
}

/// Whether confirming this verdict takes a STARTED item off the frontier.
/// `false` for a `stage` this build does not recognise — every stage that
/// matters here (`ready`, `in_progress`) always parses.
#[uniffi::export]
pub fn grill_demotes_from_frontier(verdict: MobileGrillVerdict, stage: String) -> bool {
    match hummingbird_domain::Stage::parse(&stage) {
        Some(stage) => skills::demotes_from_frontier(unmap_verdict(verdict), stage),
        None => false,
    }
}

/// The sentence [`grill_demotes_from_frontier`] gates.
#[uniffi::export]
pub fn grill_frontier_demotion_warning() -> String {
    skills::FRONTIER_DEMOTION_WARNING.to_string()
}

/// [`ItemStepRecord`] back to the domain [`hummingbird_domain::Step`] the
/// review predicates read — `item_id` and `version` ride along as whatever
/// this call already knows/does not need; neither predicate reads either
/// field, only `id`/`body`/`done`/`deleted_at`.
fn to_domain_steps(steps: &[ItemStepRecord]) -> Vec<hummingbird_domain::Step> {
    steps
        .iter()
        .map(|step| hummingbird_domain::Step {
            id: step.id.clone(),
            item_id: String::new(),
            body: step.body.clone(),
            done: step.done,
            position: step.position,
            deleted_at: step.deleted_at,
            version: 0,
        })
        .collect()
}

// -------------------------------------------------------------- #535 (M4)
// The Settings screen: bindings, the outbound queue's dead-letter journal,
// and the sync-status readout — the last of these sunk to
// `hummingbird_core::decisions::settings` in the same slice (ADR-0025), so
// this screen carries no Kotlin-side classification of what "stale"/
// "held"/"synced" mean, matching the web's `shell/sync-status.ts` rewire.

use hummingbird_core::decisions::settings;

/// [`BindingValue`], mirrored as a `uniffi::Enum` — [`bindings.rs`]'s own
/// three states, never collapsed into a nullable string.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MobileBindingValue {
    Unset,
    Text { text: String },
    Other { raw: String },
}

fn to_binding_value(value: &BindingValue) -> MobileBindingValue {
    match value {
        BindingValue::Unset => MobileBindingValue::Unset,
        BindingValue::Text { text } => MobileBindingValue::Text { text: text.clone() },
        BindingValue::Other { raw } => MobileBindingValue::Other { raw: raw.clone() },
    }
}

/// [`Binding`], mirrored — every [`BindingKey`] this build knows (set or
/// not) plus every other live `settings` row, exactly as
/// [`Core::bindings`]'s own doc describes.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileBindingRecord {
    pub key: String,
    pub known: bool,
    pub pending: bool,
    pub value: MobileBindingValue,
}

/// One question's off switch (#715), mirrored as a `uniffi::Record`.
///
/// `question` crosses as this seam's own enum, not the wire string a
/// [`hummingbird_core::question_switch::QuestionSwitch`] carries, so a
/// Kotlin `when` over the roster and over the switches matches the same
/// vocabulary. `pending` is the read-time overlay fact
/// [`MobileBindingRecord::pending`] already carries, and means the same
/// thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct MobileQuestionSwitch {
    pub question: MobileStandingQuestion,
    pub enabled: bool,
    pub pending: bool,
}

fn to_question_switch_record(
    switch: &hummingbird_core::question_switch::QuestionSwitch,
) -> MobileQuestionSwitch {
    MobileQuestionSwitch {
        // `Core::question_switches` walks `QUESTION_ORDER` itself, so every
        // spelling it emits is one `StandingQuestion::as_str` produced —
        // the same reasoning `standing_question_of` states for `SUNK`.
        question: map_standing_question(
            QUESTION_ORDER
                .iter()
                .copied()
                .find(|question| question.as_str() == switch.question)
                .unwrap_or_else(|| {
                    panic!("Core::question_switches produced an unknown question {:?}", switch.question)
                }),
        ),
        enabled: switch.enabled,
        pending: switch.pending,
    }
}

fn to_binding_record(binding: &Binding) -> MobileBindingRecord {
    MobileBindingRecord {
        key: binding.key.clone(),
        known: binding.known,
        pending: binding.pending,
        value: to_binding_value(&binding.value),
    }
}

/// [`MobileTaskHost::set_binding`] failed. `UnknownKey` is the seam
/// rejecting a key that is not in ADR-0015's closed vocabulary — a caller
/// mistake, and the one outcome that never reaches [`Core::set_binding`] at
/// all; `WriteFailed` is a durability failure enqueueing the write. Same
/// split [`MobileRuleError`] draws for its own seam.
#[derive(Debug, uniffi::Error)]
pub enum MobileSetBindingError {
    UnknownKey,
    WriteFailed { detail: String },
}

impl std::fmt::Display for MobileSetBindingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobileSetBindingError::UnknownKey => write!(f, "unrecognised binding key"),
            MobileSetBindingError::WriteFailed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for MobileSetBindingError {}

/// One field a dead-lettered [`DeadLetterReason::Conflict`] disagreed on —
/// the local and server values carried as their own canonical JSON text
/// (uniffi has no `serde_json::Value` equivalent, and this is opaque
/// material the screen only displays, never parses further).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileDeadLetterFieldRecord {
    pub field: String,
    pub local_json: String,
    pub server_json: String,
}

/// [`DeadLetterReason`], mirrored. `Permanent` carries `detail` (never
/// `message` — the uniffi field-naming trap [`MobileInitError`]'s own doc
/// records); `Conflict` and `Contention` carry no fields of their own here,
/// since [`MobileDeadLetterRecord::fields`] already carries a `Conflict`'s
/// field-level detail alongside the reason, matching
/// `ffi-web::task_host::DeadLetterEntryDTO`'s split.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MobileDeadLetterReason {
    Permanent { detail: String },
    Conflict,
    Contention,
}

/// One dead-lettered entry — S9's "1 edit didn't apply" affordance.
/// `entity`/`entity_id` are what the abandoned change was *about*
/// ([`MutationIntent::subject`]), not the queue entry's own tracking id.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileDeadLetterRecord {
    pub id: String,
    pub reason: MobileDeadLetterReason,
    pub fields: Vec<MobileDeadLetterFieldRecord>,
    pub at_ms: i64,
    pub entity: String,
    pub entity_id: Option<String>,
}

/// The touched fields' *intended* (local) values a [`MutationIntent`]
/// carries — [`ffi-web::task_host::local_field_values`]'s own copy of this
/// reasoning: only a `Patch` has any.
fn local_field_values(intent: &MutationIntent) -> serde_json::Map<String, serde_json::Value> {
    match intent {
        MutationIntent::Patch { patch_fields, .. } => {
            patch_fields.as_object().cloned().unwrap_or_default()
        }
        MutationIntent::Create { .. } | MutationIntent::CompleteGrill { .. } => {
            serde_json::Map::new()
        }
    }
}

fn to_dead_letter_record(entry: &DeadLetterEntry) -> MobileDeadLetterRecord {
    let subject = entry.entry.intent.subject();
    match &entry.reason {
        DeadLetterReason::Permanent(detail) => MobileDeadLetterRecord {
            id: entry.entry.id.clone(),
            reason: MobileDeadLetterReason::Permanent { detail: detail.clone() },
            fields: Vec::new(),
            at_ms: entry.at_ms,
            entity: subject.entity,
            entity_id: subject.id,
        },
        DeadLetterReason::Conflict { fields, current } => {
            let local = local_field_values(&entry.entry.intent);
            let mapped_fields = fields
                .iter()
                .map(|field| MobileDeadLetterFieldRecord {
                    field: field.clone(),
                    local_json: local
                        .get(field)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null)
                        .to_string(),
                    server_json: current
                        .get(field)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null)
                        .to_string(),
                })
                .collect();
            MobileDeadLetterRecord {
                id: entry.entry.id.clone(),
                reason: MobileDeadLetterReason::Conflict,
                fields: mapped_fields,
                at_ms: entry.at_ms,
                entity: subject.entity,
                entity_id: subject.id,
            }
        }
        DeadLetterReason::Contention { .. } => MobileDeadLetterRecord {
            id: entry.entry.id.clone(),
            reason: MobileDeadLetterReason::Contention,
            fields: Vec::new(),
            at_ms: entry.at_ms,
            entity: subject.entity,
            entity_id: subject.id,
        },
    }
}

/// [`settings::SyncStatusInput`], mirrored — the sync card's whole input,
/// gathered so the summary below can never answer off three different
/// snapshots of "now". `last_sync_outcome_kind` is the wire's own kind
/// string (`RunOutcome::kind`), never re-parsed into a Kotlin enum.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileSyncStatusInput {
    pub online: bool,
    pub last_sync_outcome_kind: Option<String>,
    pub last_sync_at_ms: Option<i64>,
    pub queue_depth: Option<u32>,
    pub now_ms: i64,
}

/// [`settings::SyncStatusTone`], mirrored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileSyncStatusTone {
    Neutral,
    Warn,
    Danger,
    Success,
}

fn map_sync_status_tone(tone: settings::SyncStatusTone) -> MobileSyncStatusTone {
    match tone {
        settings::SyncStatusTone::Neutral => MobileSyncStatusTone::Neutral,
        settings::SyncStatusTone::Warn => MobileSyncStatusTone::Warn,
        settings::SyncStatusTone::Danger => MobileSyncStatusTone::Danger,
        settings::SyncStatusTone::Success => MobileSyncStatusTone::Success,
    }
}

/// [`settings::sync_status_tone`]/[`settings::sync_status_label`]/
/// [`settings::sync_status_tone_word`], answered together off one input —
/// the badge and its label can never disagree about which state they
/// describe.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileSyncStatusSummary {
    pub tone: MobileSyncStatusTone,
    pub label: String,
    pub tone_word: String,
}

#[uniffi::export]
pub fn sync_status_summary(input: MobileSyncStatusInput) -> MobileSyncStatusSummary {
    let core_input = settings::SyncStatusInput {
        online: input.online,
        last_sync_outcome_kind: input.last_sync_outcome_kind,
        last_sync_at_ms: input.last_sync_at_ms,
        queue_depth: input.queue_depth,
        now_ms: input.now_ms,
    };
    MobileSyncStatusSummary {
        tone: map_sync_status_tone(settings::sync_status_tone(&core_input)),
        label: settings::sync_status_label(&core_input),
        tone_word: settings::sync_status_tone_word(&core_input),
    }
}

/// [`settings::dead_letter_heading`] — the dead-letter affordance's
/// heading, pluralised off the real count.
#[uniffi::export]
pub fn dead_letter_heading(count: u32) -> String {
    settings::dead_letter_heading(count)
}

/// [`settings::is_informative_sync_outcome`] — whether a completed cycle's
/// `RunOutcome::kind` says anything about how stale the mirror is. The
/// host filters on this before overwriting its own last-outcome/
/// last-synced-at state, the same guard `store/worker-client.ts` applies
/// on the web side.
#[uniffi::export]
pub fn is_informative_sync_outcome(kind: String) -> bool {
    settings::is_informative_sync_outcome(&kind)
}

// -------------------------------------------------------------- #709: diagnostics

/// Mirrors only the handful of [`hummingbird_core::diagnostics::DiagnosticEvent`]
/// variants Android mints on its own — `session.started` (the mobile FFI
/// host's own init, `CoreHolder.create`), `worker.started`/`worker.finished`
/// (`SyncWorker`, around its `run` call), `push.received`
/// (`HbMessagingService.onMessageReceived`) and, since #710,
/// `network.changed` (`NetworkMonitor`'s `ConnectivityManager` callback).
/// Never the whole closed family: `Core::run_observed`'s own
/// `sync.*`/`http.*`/`operation.*` events would need Android to call the
/// observed path, which this slice deliberately leaves unwired (see this
/// crate's `core_lock` module doc for why) — this is the same "mirror a
/// Rust-owned enum, don't redefine it" shape
/// [`MobileUrgencyBand`]/[`MobileFrontierAxis`] already use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileDiagnosticEvent {
    SessionStarted,
    WorkerStarted { trigger: MobileWorkerTrigger, attempt_count: u32 },
    WorkerFinished { trigger: MobileWorkerTrigger, attempt_count: u32, success: bool },
    PushReceived,
    NetworkChanged {
        online: bool,
        transport: MobileNetworkTransport,
        internet_capable: bool,
        validated: bool,
        metered: bool,
        roaming: bool,
    },
}

/// One buffered `core.*`/`operation.*` event (#710), already serialized —
/// [`MobileTaskHost::take_diagnostic_events`]'s return shape.
/// `wall_clock_ms` rides alongside the JSON rather than being re-parsed out
/// of it, the same "the caller already has this, don't make Kotlin decode
/// JSON to get it back" rule `DiagnosticJournal.append`'s own signature
/// follows.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileDiagnosticLine {
    pub wall_clock_ms: i64,
    pub json: String,
}

/// Mirrors [`hummingbird_core::diagnostics::WorkerTrigger`] — `SyncWorker`'s
/// own two-member trigger vocabulary (`TRIGGER_TIMER`/`TRIGGER_PUSH`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileWorkerTrigger {
    Timer,
    Push,
}

/// Mirrors [`hummingbird_core::diagnostics::NetworkTransport`] —
/// `ConnectivityManager`'s transport bits collapsed to one value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileNetworkTransport {
    Cellular,
    Wifi,
    Vpn,
    Other,
    None,
}

fn map_mobile_diagnostic_event(
    event: MobileDiagnosticEvent,
) -> hummingbird_core::diagnostics::DiagnosticEvent {
    use hummingbird_core::diagnostics::{
        DiagnosticEvent, NetworkTransport, OperationOutcome, WorkerTrigger,
    };
    fn map_trigger(trigger: MobileWorkerTrigger) -> WorkerTrigger {
        match trigger {
            MobileWorkerTrigger::Timer => WorkerTrigger::Timer,
            MobileWorkerTrigger::Push => WorkerTrigger::Push,
        }
    }
    fn map_transport(transport: MobileNetworkTransport) -> NetworkTransport {
        match transport {
            MobileNetworkTransport::Cellular => NetworkTransport::Cellular,
            MobileNetworkTransport::Wifi => NetworkTransport::Wifi,
            MobileNetworkTransport::Vpn => NetworkTransport::Vpn,
            MobileNetworkTransport::Other => NetworkTransport::Other,
            MobileNetworkTransport::None => NetworkTransport::None,
        }
    }
    match event {
        MobileDiagnosticEvent::SessionStarted => DiagnosticEvent::SessionStarted,
        MobileDiagnosticEvent::WorkerStarted { trigger, attempt_count } => {
            DiagnosticEvent::WorkerStarted {
                trigger: map_trigger(trigger),
                attempt_count,
            }
        }
        MobileDiagnosticEvent::WorkerFinished {
            trigger,
            attempt_count,
            success,
        } => DiagnosticEvent::WorkerFinished {
            trigger: map_trigger(trigger),
            attempt_count,
            outcome: if success {
                OperationOutcome::Success
            } else {
                OperationOutcome::Failure
            },
        },
        MobileDiagnosticEvent::PushReceived => DiagnosticEvent::PushReceived,
        MobileDiagnosticEvent::NetworkChanged {
            online,
            transport,
            internet_capable,
            validated,
            metered,
            roaming,
        } => DiagnosticEvent::NetworkChanged {
            online,
            transport: Some(map_transport(transport)),
            internet_capable: Some(internet_capable),
            validated: Some(validated),
            metered: Some(metered),
            roaming: Some(roaming),
        },
    }
}

/// The process-wide diagnostic session state (#709): one per Android
/// process, matching the recorder it feeds — `seq` keeps counting and
/// `origin_monotonic_ms` stays fixed for the process's whole life, exactly
/// [`hummingbird_core::diagnostics::DiagnosticSession`]'s own contract for
/// what one session is. Held here (not in [`diagnostics`]) so that module's
/// own tests build a fresh counter per case instead of fighting a
/// [`std::sync::OnceLock`] only the first test in the binary could ever set.
struct DiagnosticSessionState {
    /// The id and origin are set together, in one `OnceLock`, precisely so
    /// neither can move independently of the other once a session exists —
    /// review round 1 caught an earlier version of this struct storing the
    /// origin in its own unconditional `AtomicU64::store`, which made this
    /// function's own doc ("a later call cannot move the origin") false on
    /// the Rust side; it only looked true because `DiagnosticsRecorder`
    /// happens to call this with the same `by lazy` value every time.
    identity: std::sync::OnceLock<(String, u64)>,
    seq: AtomicU64,
}

static DIAGNOSTIC_SESSION: DiagnosticSessionState = DiagnosticSessionState {
    identity: std::sync::OnceLock::new(),
    seq: AtomicU64::new(0),
};

/// Sets the process-wide session's id and the one monotonic reading its
/// `elapsed_ms` is measured from. Called once, at the mobile FFI host's own
/// init (`CoreHolder.create`) — which is also where `session.started` gets
/// minted, immediately after. Idempotent **by construction**: `identity` is
/// a single `OnceLock<(String, u64)>`, so a later call with a different id
/// or origin cannot move either — `seq`/`elapsed_ms` staying monotonic
/// *within* one session is the whole point of the split
/// (`hummingbird_core::diagnostics::context`'s own doc).
#[uniffi::export]
pub fn diagnostic_init_session(session_id: String, origin_monotonic_ms: u64) {
    let _ = DIAGNOSTIC_SESSION
        .identity
        .set((session_id, origin_monotonic_ms));
}

/// The session id [`core_lock`]'s mutex/operation spans stamp — read fresh
/// on every emission rather than cached, since [`MobileTaskHost::init`] can
/// run before [`diagnostic_init_session`] ever has (see `core_lock`'s own
/// doc on [`core_lock::CoreLockSession`]). `"uninitialized"` before that
/// call has ever landed, the same fallback [`diagnostic_event_json`] uses.
fn current_diagnostic_session_id() -> String {
    DIAGNOSTIC_SESSION
        .identity
        .get()
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| "uninitialized".to_string())
}

/// Mints one Android-sourced `DiagnosticEventV1` and returns it serialized
/// as the exact NDJSON line the Kotlin recorder appends — see
/// [`diagnostics::event_json`]. `wall_clock_ms` is the caller's own
/// `System.currentTimeMillis()`; `monotonic_ms` is `SystemClock
/// .elapsedRealtime()`, measured against the origin [`diagnostic_init_session`]
/// fixed for this process.
#[uniffi::export]
pub fn diagnostic_event_json(
    wall_clock_ms: i64,
    monotonic_ms: u64,
    event: MobileDiagnosticEvent,
) -> String {
    let (session_id, origin_monotonic_ms) = DIAGNOSTIC_SESSION
        .identity
        .get()
        .map(|(id, origin)| (id.as_str(), *origin))
        .unwrap_or(("uninitialized", 0));
    let seq = DIAGNOSTIC_SESSION.seq.fetch_add(1, Ordering::Relaxed);
    diagnostics::event_json(
        session_id,
        seq,
        origin_monotonic_ms,
        wall_clock_ms,
        monotonic_ms,
        map_mobile_diagnostic_event(event),
    )
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

    /// #710's acceptance criterion 7, over a real `MobileTaskHost::capture`
    /// call rather than a citation to a test that (review round 1 caught)
    /// did not exist: the exact event order this capture's own operation
    /// produces.
    ///
    /// **What the two halves of this test actually prove, stated plainly.**
    /// The sequence assertion is a real, failable pin: reordering
    /// `emit_operation_local_commit` against the durable write breaks it.
    /// The `http.started` half is **still vacuous by construction here** —
    /// `capture` only enqueues, so this path never issues HTTP and the
    /// buffer holds no `http.started` for any assertion to catch. That is
    /// not the structural gap #739 closed, though: #739 gave the queued
    /// entry itself an `operation_id` (`hummingbird_core::Core::capture`
    /// now stamps it, and `drain`'s eventual `http.started`/`http.finished`
    /// carry it through — proven at the core level, spanning a real cycle
    /// boundary, by
    /// `hummingbird_core::sync::cycle::tests::observed::operation_local_commit_precedes_http_started_for_the_same_operation_across_the_cycle_boundary`).
    /// What is still missing on **this** host is that
    /// [`MobileTaskHost::run`] drives the unobserved `Core::run`, not
    /// `Core::run_observed` — same reason `ffi-web`'s own `run` doesn't
    /// either (no watchdog clock wired up here) — so no `http.started` is
    /// emitted by this production surface at all, whatever operation id it
    /// would carry once one is. Wiring `run_observed` into this host is a
    /// separate, already-tracked follow-up, not #739's; until it lands this
    /// assertion is kept as a regression tripwire for the day capture
    /// *does* reach the network, not as evidence the join is exercised
    /// today.
    #[tokio::test]
    async fn a_successful_capture_orders_operation_local_commit_before_any_http_started() {
        use hummingbird_core::diagnostics::{DiagnosticEvent, DiagnosticEventV1};

        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("capture-diag-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://authority.example".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        host.capture(title_only_draft("pin the ordering"), 1_000).await.unwrap();

        let envelopes: Vec<DiagnosticEventV1> = host
            .take_diagnostic_events()
            .await
            .iter()
            .map(|line| serde_json::from_str(&line.json).unwrap())
            .collect();

        let names: Vec<&'static str> = envelopes
            .iter()
            .map(|envelope| match envelope.event {
                DiagnosticEvent::OperationRequested => "operation.requested",
                DiagnosticEvent::CoreWaitStarted { .. } => "core.wait_started",
                DiagnosticEvent::CoreAcquired { .. } => "core.acquired",
                DiagnosticEvent::OperationLocalCommit => "operation.local_commit",
                DiagnosticEvent::CoreReleased { .. } => "core.released",
                DiagnosticEvent::HttpStarted { .. } => "http.started",
                _ => "other",
            })
            .collect();

        assert_eq!(
            names,
            vec![
                "operation.requested",
                "core.wait_started",
                "core.acquired",
                "operation.local_commit",
                "core.released",
            ],
            "capture's own diagnostic stream, in order: {names:?}",
        );

        let capture_operation_id = envelopes
            .iter()
            .find(|envelope| matches!(envelope.event, DiagnosticEvent::OperationLocalCommit))
            .and_then(|envelope| envelope.operation_id.clone())
            .expect("operation.local_commit carries the capture's operation_id");

        let local_commit_index = names.iter().position(|name| *name == "operation.local_commit").unwrap();
        if let Some(http_started_index) = names.iter().position(|name| *name == "http.started") {
            assert!(
                http_started_index > local_commit_index,
                "an http.started ever emitted here must come after operation.local_commit",
            );
        }
        assert!(
            !envelopes.iter().any(|envelope| {
                matches!(envelope.event, DiagnosticEvent::HttpStarted { .. })
                    && envelope.operation_id.as_deref() == Some(capture_operation_id.as_str())
            }),
            "no http.started event may ever carry capture's own operation_id",
        );
    }

    // ---------------------------------------------- the calendar (#564)
    // Everything reachable without a live authority. The mint itself is
    // covered by `calendar_token`'s own tests; what these pin is the state
    // machine `MobileTaskHost` wraps it in — which is where the four
    // Source-connection states actually get decided.

    /// A host whose base URL refuses connections, so every mint answers
    /// `authority_unreachable` without a network of any kind.
    async fn unreachable_host(dir: &tempfile::TempDir, api_key: &str) -> Arc<MobileTaskHost> {
        MobileTaskHost::init(
            dir.path().join("cal-ns").to_str().unwrap().to_string(),
            "http://127.0.0.1:1".to_string(),
            api_key.to_string(),
        )
        .await
        .unwrap()
    }

    /// The polled set is the picker's list unioned with the synced Trips
    /// binding, at the long horizon — the failure this pins is a Vacation
    /// pane that answers "nothing booked" because the one calendar it is
    /// about was never fetched.
    #[tokio::test]
    async fn the_bound_trips_calendar_is_polled_whether_or_not_the_picker_ticked_it() {
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "device-token").await;
        host.set_binding(
            "seed-trips".to_string(),
            vacation::TRIPS_CALENDAR_BINDING_KEY.to_string(),
            "trips@g".to_string(),
            1_000,
        )
        .await
        .unwrap();

        host.init_calendar(
            true,
            vec![MobileCalendarSelection { id: "personal@g".to_string(), long_horizon: false }],
        )
        .await;

        let polled = host.calendar.lock().await.host.calendar_selections();
        assert_eq!(
            polled,
            vec![CalendarSelection::standard("personal@g"), CalendarSelection::long("trips@g")]
        );
    }

    /// The binding is *synced*: it can arrive from the browser between two
    /// ticks, and the phone must start polling it without the operator
    /// opening Settings.
    #[tokio::test]
    async fn a_trips_binding_that_arrives_later_is_picked_up_on_the_next_tick() {
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "device-token").await;
        host.init_calendar(true, Vec::new()).await;
        assert!(host.calendar.lock().await.host.calendar_selections().is_empty());

        host.set_binding(
            "seed-trips".to_string(),
            vacation::TRIPS_CALENDAR_BINDING_KEY.to_string(),
            "trips@g".to_string(),
            1_000,
        )
        .await
        .unwrap();
        host.calendar_on_timer(2_000).await;

        assert_eq!(
            host.calendar.lock().await.host.calendar_selections(),
            vec![CalendarSelection::long("trips@g")]
        );
    }

    /// #621's whole point: the weekend and vacation panes gate on
    /// `calendar_connected` **first**, so before #564 they were
    /// permanently "not set up" on the phone whatever the mirror held.
    /// This is the wiring, at the seam that used to hardcode `false`.
    #[tokio::test]
    async fn connecting_a_calendar_moves_the_weekend_pane_off_not_set_up() {
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "device-token").await;

        fn weekend_state(panes: &[MobileRankedPane]) -> MobilePaneAnswerState {
            panes
                .iter()
                .find(|pane| pane.standing_question == MobileStandingQuestion::Weekend)
                .expect("the weekend pane ranks on Now")
                .answer
                .answer_state
        }

        let before = host
            .rank_panes(MobileSurface::Now, 1_000, Vec::new(), MobileSyncFacts::default())
            .await;
        // `Unbound` is the "go set this up" reading — the only state
        // `!calendar_connected` produces.
        assert_eq!(weekend_state(&before), MobilePaneAnswerState::Unbound);

        host.init_calendar(true, Vec::new()).await;

        let after = host
            .rank_panes(MobileSurface::Now, 1_000, Vec::new(), MobileSyncFacts::default())
            .await;
        // Connected, but this device has never actually polled — a
        // different fact, and the one that stops a connected phone reading
        // as un-set-up.
        assert_eq!(
            weekend_state(&after),
            MobilePaneAnswerState::BoundButUnacquired
        );
    }

    /// Phase one must stay calendar-free: it runs before any zone is
    /// resolved, so there is no window to read, and asking anyway would be
    /// a disk read per tick for an answer nothing uses.
    #[tokio::test]
    async fn the_zone_query_phase_reads_no_calendar_arm() {
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "device-token").await;
        host.init_calendar(true, Vec::new()).await;

        // The queries a connected device asks are the same ones a
        // never-connected one does — the calendar arm contributes none.
        let connected = host.pane_zone_queries(MobileSurface::Now, 1_000).await;
        host.disconnect_calendar().await;
        let disconnected = host.pane_zone_queries(MobileSurface::Now, 1_000).await;

        assert_eq!(connected, disconnected);
    }

    #[tokio::test]
    async fn a_fresh_host_has_never_connected_a_calendar() {
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "device-token").await;

        let connection = host.init_calendar(false, Vec::new()).await;

        assert_eq!(connection.state, MobileCalendarState::NeverConnected);
        assert_eq!(connection.expires_at_ms, None);
        // Never opted in is not a failure, and has no code to report — the
        // same "never tried" vs "tried and failed" split `initConnection`
        // keeps, and what Settings gates its message on.
        assert_eq!(connection.error, None);
    }

    #[tokio::test]
    async fn a_never_connected_device_polls_nothing_and_reaches_no_network() {
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "device-token").await;

        let tick = host.calendar_on_timer(1_000).await;

        assert_eq!(tick.outcome, "no_credential");
        assert_eq!(tick.connection.state, MobileCalendarState::NeverConnected);
    }

    #[tokio::test]
    async fn a_first_connect_that_cannot_reach_the_authority_stays_offerable() {
        // The Connect affordance must still be there afterwards: a device
        // that has never opted in and whose first attempt failed is not
        // connected, however the attempt failed.
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "device-token").await;

        let connection = host.connect_calendar().await;

        assert_eq!(connection.state, MobileCalendarState::NeverConnected);
        assert_eq!(connection.error.as_deref(), Some("authority_unreachable"));
    }

    #[tokio::test]
    async fn a_device_with_no_token_at_all_is_refused_at_its_own_token_control() {
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "").await;

        // Opted in at launch, so this is a re-mint on an established
        // connection rather than a first attempt.
        let connection = host.init_calendar(true, Vec::new()).await;

        assert_eq!(connection.state, MobileCalendarState::RefusedDeviceToken);
        assert_eq!(connection.error.as_deref(), Some("no_device_token"));
    }

    #[tokio::test]
    async fn an_opted_in_device_that_cannot_reach_the_authority_reads_as_cannot_confirm() {
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "device-token").await;

        let connection = host.init_calendar(true, Vec::new()).await;

        // Not `NeverConnected`: the phone cannot tell "I am offline" from
        // "the authority is down", and neither is a reason to un-opt-in
        // this device or to offer Connect again.
        assert_eq!(connection.state, MobileCalendarState::CannotConfirm);
        assert_eq!(connection.error.as_deref(), Some("authority_unreachable"));
    }

    #[tokio::test]
    async fn a_failed_reconnect_never_un_opts_in_the_device() {
        // `shouldKeepExistingConnection`, ported: the same button is
        // Connect and Reconnect, and a failed reconnect that wrote
        // `connected: false` would take the stale-but-real mirror and the
        // Reconnect affordance down with it.
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "device-token").await;
        host.init_calendar(true, Vec::new()).await;

        let connection = host.connect_calendar().await;

        assert_eq!(connection.state, MobileCalendarState::CannotConfirm);
    }

    #[tokio::test]
    async fn disconnecting_returns_the_device_to_never_connected() {
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "device-token").await;
        host.init_calendar(true, Vec::new()).await;

        let connection = host.disconnect_calendar().await;

        assert_eq!(connection.state, MobileCalendarState::NeverConnected);
        assert_eq!(connection.error, None);
        // And the lane really is off: the next tick polls nothing.
        assert_eq!(host.calendar_on_timer(1_000).await.outcome, "no_credential");
    }

    #[tokio::test]
    async fn an_opted_in_tick_reports_the_pollers_own_outcome_vocabulary() {
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "device-token").await;
        host.init_calendar(true, Vec::new()).await;

        let tick = host.calendar_on_timer(1_000).await;

        // The mint failed, so nothing was ever pushed into the poller and
        // it has no credential to poll with — the same string
        // `client/web/src/store/protocol.ts` matches on.
        assert_eq!(tick.outcome, "no_credential");
        assert_eq!(tick.connection.state, MobileCalendarState::CannotConfirm);
    }

    #[tokio::test]
    async fn listing_calendars_before_any_mint_leaves_the_picker_alone() {
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "device-token").await;

        let list = host.list_calendars().await;

        assert_eq!(list.kind, "no_credential");
        assert_eq!(list.calendars, Vec::new());
    }

    #[tokio::test]
    async fn the_poll_cadence_crosses_the_seam_rather_than_being_restated_in_kotlin() {
        assert_eq!(
            calendar_poll_interval_ms(),
            hummingbird_core::calendar::CALENDAR_POLL_INTERVAL_MS
        );
    }

    #[tokio::test]
    async fn no_calendar_answer_ever_carries_the_device_token() {
        let dir = tempfile::tempdir().unwrap();
        let host = unreachable_host(&dir, "s3cret-device-token").await;

        let connection = host.init_calendar(true, Vec::new()).await;
        let tick = host.calendar_on_timer(1_000).await;
        let list = host.list_calendars().await;

        for answer in [
            format!("{connection:?}"),
            format!("{tick:?}"),
            format!("{list:?}"),
        ] {
            assert!(!answer.contains("s3cret"), "{answer}");
        }
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

    /// The deadline split/join bindings, pinned the same way: the record
    /// mapping is the only place these can drift from the core, so both
    /// directions are crossed against the real functions rather than
    /// re-asserting the grammar (which `decisions::urgency`'s own suite
    /// owns). The round-trip case is what the Android control actually
    /// does on every keystroke of the time picker.
    #[test]
    fn the_deadline_split_binding_is_the_core_rule_verbatim() {
        for value in [
            "",
            "2026-09-01",
            "2026-09-01T09:30",
            "2026-02-30T09:30",
            "next tuesday",
            "abcd-ef-gh",
            // A mis-shaped time must fall to the free-text branch whole,
            // not split into a date plus a half-time the picker would then
            // "correct" on the reader's behalf.
            "2026-09-01T9:3",
        ] {
            let core = urgency::split_deadline(value);
            let crossed = split_deadline(value);
            assert_eq!(crossed.date, core.date, "{value:?} split its date differently");
            assert_eq!(crossed.time, core.time, "{value:?} split its time differently");
            assert_eq!(
                join_deadline(&crossed.date, crossed.time),
                value,
                "{value:?} did not survive a round trip across the binding",
            );
        }
    }

    /// `None` and `Some("")` are the same fact to the core — the Android
    /// control clears a time by handing back the former, and the latter is
    /// what an empty picker would produce. Both must land on a whole day
    /// rather than on `T00:00`, which is a different deadline
    /// (`server/domain/src/deadline.rs` reads a bare date as end of day).
    #[test]
    fn the_deadline_join_binding_clears_a_time_the_way_the_core_does() {
        for time in [None, Some(String::new())] {
            assert_eq!(join_deadline("2026-09-01", time), "2026-09-01");
        }
        assert_eq!(join_deadline("2026-09-01", Some("09:30".into())), "2026-09-01T09:30");
        assert_eq!(join_deadline("", Some("09:30".into())), "");
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
    /// query that shows it instead, exposed to a mobile host by
    /// `MobileTaskHost::now_board` (M3/#530) and again, on its own, by
    /// `MobileTaskHost::triage_board` (M3/#531).
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
            vec!["@home", "@computer", "@phone", "@errands", "@garden", "@homework"],
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

    /// #595: the two decisions that live in this layer, not in the core —
    /// an unparseable patch yields no rows rather than an error (the card
    /// states the empty fact), and a well-formed patch reaches the core's
    /// `proposal_rows` with the record's own current values beside it.
    #[test]
    fn grill_proposal_rows_parse_here_and_never_error() {
        let item = ItemDetailRecord {
            id: "hb-1".to_string(),
            seq: Some(1),
            title: "Plan India trip".to_string(),
            description: None,
            stage: "grilling".to_string(),
            size: Some("normal".to_string()),
            energy: None,
            context: None,
            agent: false,
            priority: 2,
            project_id: None,
            project_name: None,
            deadline: None,
            scheduled_date: None,
            source_url: None,
            vault_path: None,
            updated_at: 0,
            version: 1,
            steps: vec![],
            open_blockers: vec![],
            live_alert: None,
            is_archived: false,
            is_editable: true,
            available_actions: vec![],
            can_mark_done: true,
            microtask_affordance: None,
        };

        assert_eq!(grill_proposal_rows("not json".to_string(), item.clone()), vec![]);

        let rows = grill_proposal_rows(r#"{"size":"deep"}"#.to_string(), item);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Size");
        assert_eq!(rows[0].current.as_deref(), Some("normal"));
        assert_eq!(rows[0].proposed, "deep");
    }

    /// `ui/forms/PriorityRow.kt` hardcodes its display order as
    /// `1, 2, 3, 4` (Urgent..Low) because a plain
    /// JVM test cannot call a generated JNI binding directly
    /// (`CaptureSubmitRefusalTest`'s own doc — no host-arch `.so` in that
    /// process), so the pin lives here, on the Rust side of the seam: if
    /// `decisions::frontier::priority_rank`'s ordering ever changes, this
    /// test breaks and names the Kotlin literal (`PriorityRow`,
    /// `ui/forms/PriorityRow.kt`) that must change to match.
    ///
    /// It reads the core rule **directly**, never through a
    /// `#[uniffi::export]`ed pass-through: exporting one would put a
    /// per-item decision function on the mobile seam that no Kotlin caller
    /// wants (the module doc's own asymmetry — Android reads applied
    /// results), to buy a test nothing but the test would use.
    ///
    /// The wire value `0` is still sorted here though the row stopped
    /// drawing a chip for it on 2026-08-20 (not picking a priority is what
    /// says "none"): its landing *last*, behind every priority the row does
    /// draw, is precisely what makes an unrendered `0` a safe resting state
    /// rather than a value the reader is silently stuck at the top of the
    /// board with.
    #[test]
    fn the_priority_row_order_matches_priority_rank() {
        let mut wire_values = vec![0i64, 1, 2, 3, 4];
        wire_values.sort_by_key(|raw| frontier::priority_rank(*raw));
        assert_eq!(
            wire_values,
            vec![1, 2, 3, 4, 0],
            "PriorityRow's hardcoded Kotlin order (ui/forms/PriorityRow.kt) must match this",
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
            "NowScreen.kt's facet SIZE_VALUES must match this — order included, since the level glyphs' ramp position is the list index (#558). The detail pane's own editor no longer holds a copy: it reads `captureFormMeta`.",
        );

        let energies: Vec<String> = hummingbird_core::decisions::vocabulary::energy_options()
            .into_iter()
            .map(|option| option.value)
            .collect();
        assert_eq!(
            energies,
            vec!["low", "medium", "high"],
            "NowScreen.kt's facet ENERGY_VALUES must match this — order included, since the level glyphs' ramp position is the list index (#558). The detail pane's own editor no longer holds a copy: it reads `captureFormMeta`.",
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
            vault_path: None,
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

    /// `shown_count`/`total_count` are the filter disclosure's "N of M
    /// shown" meta line (`FrontierColumns.tsx`'s own), over the same
    /// ordered list the columns group from: post-facet and pre-facet.
    /// Blocked rows count in neither — facets never apply to them, so a
    /// total that included them would overstate what filtering can reach.
    #[test]
    fn now_board_counts_shown_and_total_over_the_facetable_list_only() {
        let phone = Item { context: Some("@phone".to_string()), ..item("a", 0, None) };
        let computer = Item { context: Some("@computer".to_string()), ..item("b", 0, None) };
        let unjudged = item("c", 0, None);
        let blocked_item = item("d", 0, None);
        let blocker = item("e", 0, None);
        let picked = frontier::toggle_facet(&no_facets(), frontier::Facet::Context, "@phone");

        let board = build_now_board(
            &[phone, computer, unjudged],
            &[],
            &[],
            &[],
            &[(blocked_item, vec![blocker])],
            &[],
            frontier::FrontierAxis::Context,
            &picked,
            "2026-08-15T12:00",
        );

        assert_eq!(board.shown_count, 1);
        assert_eq!(board.total_count, 3);
        assert_eq!(board.blocked.len(), 1);
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
                    None,
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
                    None,
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

    // -------------------------------------------------------------- M3 (#532)

    #[tokio::test]
    async fn done_items_orders_most_recently_touched_first() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m3-done-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let first = host.capture(title_only_draft("first"), 1_000).await.unwrap();
        let second = host.capture(title_only_draft("second"), 1_000).await.unwrap();
        host.act(first.clone(), "complete".to_string(), 2_000).await.unwrap();
        host.act(second.clone(), "complete".to_string(), 3_000).await.unwrap();

        let done = host.done_items().await;
        assert_eq!(done.iter().map(|record| record.id.clone()).collect::<Vec<_>>(), vec![
            second.clone(),
            first,
        ]);
        // Both completions are still unconfirmed local writes, both pending.
        assert!(done.iter().all(|record| record.pending));
        assert_eq!(done[0].title, "second");
    }

    #[tokio::test]
    async fn ledger_rows_are_pre_ordered_and_pre_gated_for_mark_done() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m3-ledger-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let older = host.capture(title_only_draft("older"), 1_000).await.unwrap();
        let newer = host.capture(title_only_draft("newer"), 2_000).await.unwrap();

        let rows = host.ledger_rows(3_000).await;
        assert_eq!(
            rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>(),
            vec![newer, older],
        );
        // A fresh Triage-stage capture is live and offers the one-click
        // checkmark — `item-actions.ts`'s widened rule, mirrored.
        assert!(rows.iter().all(|row| matches!(row.state, MobileLedgerRowState::Live)));
        assert!(rows.iter().all(|row| row.can_mark_done));
        assert!(rows.iter().all(|row| !row.dead_lettered && !row.has_live_alert));
    }

    // -------------------------------------------------------------- M4 (#542)

    #[tokio::test]
    async fn search_reaches_a_completed_item_and_labels_it_done() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m4-recall-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let live = host.capture(title_only_draft("buy stamps"), 1_000).await.unwrap();
        let done = host.capture(title_only_draft("buy stamps too"), 1_000).await.unwrap();
        host.act(done.clone(), "complete".to_string(), 2_000).await.unwrap();

        let outcome = host.search("stamps".to_string(), 3_000).await;
        let by_id: std::collections::HashMap<String, &MobileRecallRowRecord> =
            outcome.rows.iter().map(|row| (row.id.clone(), row)).collect();

        assert_eq!(outcome.total, 2);
        assert_eq!(by_id.get(&live).unwrap().group, MobileRecallGroup::Live);
        assert_eq!(by_id.get(&done).unwrap().group, MobileRecallGroup::Done);
        // Both are still-unconfirmed local writes, both pending.
        assert!(outcome.rows.iter().all(|row| row.pending));
    }

    #[tokio::test]
    async fn search_reaches_an_archived_item_and_labels_it() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m4-recall-archived-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let item_id = host.capture(title_only_draft("widget report"), 1_000).await.unwrap();
        host.act(item_id.clone(), "cancel".to_string(), 2_000).await.unwrap();

        let outcome = host.search("widget".to_string(), 3_000).await;
        assert_eq!(outcome.rows.len(), 1);
        assert_eq!(outcome.rows[0].id, item_id);
        assert_eq!(outcome.rows[0].group, MobileRecallGroup::Archived);
    }

    #[tokio::test]
    async fn an_empty_query_answers_no_rows_and_a_zero_total() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m4-recall-empty-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();
        host.capture(title_only_draft("anything"), 1_000).await.unwrap();

        let outcome = host.search(String::new(), 2_000).await;
        assert!(outcome.rows.is_empty());
        assert_eq!(outcome.total, 0);
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
                .triage("seed", &id, true, hummingbird_core::TriagePatch::default(), 2_000, None)
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

    // ------------------------------------------------------- M3 (#531)

    /// Captured and Grilling items combine into one queue, in the core's
    /// order — local drafts first (none here), then Grilling, then captured
    /// Triage, oldest-first within each group — the same order
    /// `queue::triage_process_queue` itself pins.
    #[test]
    fn triage_board_orders_grilling_before_captured_triage() {
        let captured = Item { stage: Stage::Triage, ..item("captured-1", 0, None) };
        let grilling = Item {
            stage: Stage::Grilling,
            created_at: 1,
            ..item("grilling-1", 0, None)
        };

        let board = build_triage_board(&[captured], &[grilling], &[], "2026-08-15T12:00");

        assert_eq!(
            board.items.iter().map(|record| record.id.clone()).collect::<Vec<_>>(),
            vec!["grilling-1", "captured-1"],
        );
    }

    /// The header's two counts are the record's own fields, never a caller
    /// recomputation over `items.len()`.
    #[test]
    fn triage_board_counts_are_exact_never_recomputed() {
        let captured = vec![
            Item { stage: Stage::Triage, ..item("c-1", 0, None) },
            Item { stage: Stage::Triage, ..item("c-2", 0, None) },
        ];
        let grilling = vec![Item { stage: Stage::Grilling, ..item("g-1", 0, None) }];

        let board = build_triage_board(&captured, &grilling, &[], "2026-08-15T12:00");

        assert_eq!(board.captured_count, 2);
        assert_eq!(board.grilling_count, 1);
        assert_eq!(board.items.len(), 3);
    }

    /// A row carries every field the seeded editor needs, and its own
    /// stage — a combined queue can hold a Grilling row beside a Triage
    /// one, so the badge must never assume "triage".
    #[test]
    fn triage_item_records_carry_the_seeded_editor_fields_and_own_stage() {
        let grilling = Item {
            stage: Stage::Grilling,
            description: Some("notes".to_string()),
            context: Some("@computer".to_string()),
            ..item("g-1", 2, Some("2026-08-20"))
        };

        let board = build_triage_board(&[], &[grilling], &[], "2026-08-15T12:00");

        let record = &board.items[0];
        assert_eq!(record.stage, "grilling");
        assert_eq!(record.description.as_deref(), Some("notes"));
        assert_eq!(record.context.as_deref(), Some("@computer"));
        assert_eq!(record.priority, 2);
        assert_eq!(record.deadline.as_deref(), Some("2026-08-20"));
    }

    /// Triage and Grilling both offer nothing in `available_actions`
    /// (neither is action yet), so the checkmark rides on the wider,
    /// separate `can_mark_done` rule instead — never `&[]`-implies-false.
    #[test]
    fn triage_item_records_carry_can_mark_done_not_available_actions() {
        let triage = Item { stage: Stage::Triage, ..item("t-1", 0, None) };
        let archived = Item {
            stage: Stage::Triage,
            archived_at: Some(1),
            ..item("t-2", 0, None)
        };

        let board = build_triage_board(&[triage, archived], &[], &[], "2026-08-15T12:00");

        let live = board.items.iter().find(|record| record.id == "t-1").unwrap();
        let done = board.items.iter().find(|record| record.id == "t-2").unwrap();
        assert!(live.can_mark_done);
        assert!(!done.can_mark_done);
    }

    /// #539: the Grill button's own two facts, decided rather than left for
    /// `TriageRow.kt` to gate on `enabled = false` forever.
    #[test]
    fn triage_item_records_carry_can_grill_and_has_grill_draft() {
        let triage = Item { stage: Stage::Triage, ..item("t-1", 0, None) };
        let drafted = Item { stage: Stage::Triage, ..item("t-2", 0, None) };

        let board = build_triage_board(&[triage, drafted], &[], &["t-2".to_string()], "2026-08-15T12:00");

        let no_draft = board.items.iter().find(|record| record.id == "t-1").unwrap();
        let has_draft = board.items.iter().find(|record| record.id == "t-2").unwrap();
        assert!(no_draft.can_grill);
        assert!(!no_draft.has_grill_draft);
        assert!(has_draft.can_grill);
        assert!(has_draft.has_grill_draft);
    }

    /// The Triage-parity slice: a row's urgency band is `compute_urgency`
    /// over its deadline and the caller's device-local now — the same
    /// decided band the frontier board's rows carry, so the shared card's
    /// swatch/word never re-derive it Kotlin-side.
    #[test]
    fn triage_item_records_carry_the_decided_urgency_band() {
        let overdue = Item { stage: Stage::Triage, ..item("t-overdue", 0, Some("2026-08-10")) };
        let calm = Item { stage: Stage::Triage, ..item("t-calm", 0, None) };

        let board = build_triage_board(&[overdue, calm], &[], &[], "2026-08-15T12:00");

        let hot = board.items.iter().find(|record| record.id == "t-overdue").unwrap();
        let cool = board.items.iter().find(|record| record.id == "t-calm").unwrap();
        assert_eq!(hot.urgency, MobileUrgencyBand::Overdue);
        assert_eq!(cool.urgency, MobileUrgencyBand::Calm);
    }

    fn untouched_triage_edit() -> ItemEdit {
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

    /// The read door, end to end: a fresh capture lands in the board with
    /// the right count, and disappears from it once promoted.
    #[tokio::test]
    async fn triage_board_reads_a_fresh_capture_and_drops_it_once_promoted() {
        let dir = tempfile::tempdir().unwrap();
        let host = MobileTaskHost::init(
            dir.path().join("triage-board-ns").to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();
        let id = host.capture(title_only_draft("buy milk"), 1_000).await.unwrap();

        let board = host.triage_board("2026-08-15T12:00".to_string()).await;
        assert_eq!(board.captured_count, 1);
        assert_eq!(board.grilling_count, 0);
        assert_eq!(board.items[0].id, id);

        host.triage_item(id, true, untouched_triage_edit(), 2_000).await.unwrap();

        let board = host.triage_board("2026-08-15T12:00".to_string()).await;
        assert_eq!(board.captured_count, 0);
        assert!(board.items.is_empty());
    }

    /// Promotion and a field edit are one CAS `PATCH`: the edit lands, and
    /// the item leaves Triage, in a single durable entry.
    #[tokio::test]
    async fn triage_item_promotes_and_edits_in_one_durable_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let host = MobileTaskHost::init(
            dir.path().join("triage-item-ns").to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();
        let id = host.capture(title_only_draft("buy milk"), 1_000).await.unwrap();

        host.triage_item(
            id.clone(),
            true,
            ItemEdit {
                title: Some("buy oat milk".into()),
                ..untouched_triage_edit()
            },
            2_000,
        )
        .await
        .unwrap();

        let detail = host.item_detail(id, 2_000).await.expect("captured item");
        assert_eq!(detail.title, "buy oat milk");
        assert_eq!(detail.stage, "ready");
        assert_eq!(
            host.queue_depth().await,
            2,
            "the capture and the triage are two durable entries"
        );
    }

    /// `promote_to_ready: false` is a pure edit — the item stays in Triage,
    /// exactly the weekend-plans-pane reasoning `Core::triage`'s own doc
    /// gives for the same flag on `edit_item`.
    #[tokio::test]
    async fn triage_item_with_promotion_false_edits_without_promoting() {
        let dir = tempfile::tempdir().unwrap();
        let host = MobileTaskHost::init(
            dir.path().join("triage-item-ns-2").to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();
        let id = host.capture(title_only_draft("buy milk"), 1_000).await.unwrap();

        host.triage_item(
            id.clone(),
            false,
            ItemEdit {
                context: FieldPatch::Set { value: "@errands".into() },
                ..untouched_triage_edit()
            },
            2_000,
        )
        .await
        .unwrap();

        let detail = host.item_detail(id, 2_000).await.expect("captured item");
        assert_eq!(detail.stage, "triage");
        assert_eq!(detail.context.as_deref(), Some("@errands"));
    }

    #[tokio::test]
    async fn triaging_an_item_this_device_has_never_seen_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let host = MobileTaskHost::init(
            dir.path().join("triage-item-ns-3").to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        assert!(matches!(
            host.triage_item("nope".to_string(), true, untouched_triage_edit(), 1_000).await,
            Err(MobileEditError::ItemNotFound)
        ));
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
            vault_path: None,
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
            can_mark_done: !archived,
            microtask_affordance: if archived {
                None
            } else {
                Some(hummingbird_core::decisions::skills::MicrotaskAffordance::Rewrite { undone_count: 1 })
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
        assert_eq!(
            record.microtask_affordance,
            Some(MobileMicrotaskAffordance::Rewrite { undone_count: 1 }),
            "#539: the applied result, not left for Kotlin to re-derive",
        );

        let archived = to_item_detail_record(&fixture_detail(true), 1_000);
        assert!(archived.is_archived);
        assert!(!archived.is_editable);
        assert!(archived.available_actions.is_empty());
        assert!(!archived.can_mark_done);
        assert_eq!(archived.microtask_affordance, None, "an archived item has no live gesture");
    }

    /// The detail record's own version of
    /// `triage_item_records_carry_can_mark_done_not_available_actions`: the
    /// checkmark must survive the two stages whose act vocabulary is
    /// empty, because the Triage host opens this pane on exactly those.
    #[test]
    fn item_detail_records_carry_can_mark_done_where_available_actions_is_empty() {
        let mut detail = fixture_detail(false);
        detail.item.stage = hummingbird_domain::Stage::Triage;
        detail.available_actions = Vec::new();
        detail.can_mark_done = true;

        let record = to_item_detail_record(&detail, 1_000);
        assert_eq!(record.stage, "triage");
        assert!(
            record.available_actions.is_empty(),
            "the stage that offers no act vocabulary"
        );
        assert!(record.can_mark_done, "and still draws the checkmark");
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

    // ------------------------------------------------------------- M4 (#540)

    fn rule_fixture(conditions: Vec<Condition>, event_kind: Option<&str>) -> Rule {
        Rule {
            id: "r-1".to_string(),
            name: "passport".to_string(),
            event_kind: event_kind.map(str::to_string),
            conditions,
            severity: "high".to_string(),
            tier: Tier::Urgent,
            enabled: true,
            updated_at: 1,
            version: 3,
            deleted_at: None,
        }
    }

    fn condition_fixture(field: &str, op: &str, value: serde_json::Value) -> Condition {
        Condition {
            field: field.to_string(),
            op: op.to_string(),
            value,
            negate: false,
        }
    }

    /// Every enum this crate mirrors is exhaustive with no wildcard arm —
    /// the compile-time drift gate the brief names. These pin the round
    /// trip on the two that cross in both directions.
    #[test]
    fn the_operator_and_tier_mirrors_round_trip() {
        for op in Operator::ALL {
            assert_eq!(unmap_operator(map_operator(op)), op);
        }
        for tier in Tier::ALL {
            assert_eq!(unmap_tier(map_tier(tier)), tier);
        }
    }

    #[test]
    fn every_field_type_and_widget_maps_to_a_distinct_mobile_variant() {
        let mapped: Vec<MobileFieldType> =
            FieldType::ALL.into_iter().map(map_field_type).collect();
        assert_eq!(mapped.len(), FieldType::ALL.len());
        for (index, one) in mapped.iter().enumerate() {
            assert!(
                !mapped[index + 1..].contains(one),
                "{one:?} is the mapping of two different field types",
            );
        }
        let widgets: Vec<MobileValueWidget> = rules::ValueWidget::ALL
            .into_iter()
            .map(map_widget)
            .collect();
        assert_eq!(widgets.len(), rules::ValueWidget::ALL.len());
    }

    #[test]
    fn a_rule_record_carries_its_validity_already_decided() {
        let registry = rules::compiled_registry();
        let record = to_rule_record(
            &rule_fixture(
                vec![condition_fixture("removed_field", "eq", serde_json::json!("x"))],
                Some("item_threshold"),
            ),
            &registry,
        );
        assert!(!record.is_valid);
        assert_eq!(record.invalid_fields, ["removed_field"]);
        // Validity and enablement are separate facts, and Kotlin must never
        // read one for the other.
        assert!(record.enabled);
        assert_eq!(record.version, 3);
        assert_eq!(record.kind_label_key, "item_threshold");
    }

    #[test]
    fn a_null_kind_rule_carries_the_any_kind_rendering_key() {
        let registry = rules::compiled_registry();
        let record = to_rule_record(&rule_fixture(Vec::new(), None), &registry);
        assert_eq!(record.event_kind, None);
        assert_eq!(record.kind_label_key, ANY_KIND_KEY);
        assert!(record.is_valid);
    }

    #[test]
    fn a_severity_outside_the_ratchet_vocabulary_is_flagged() {
        let registry = rules::compiled_registry();
        let mut rule = rule_fixture(Vec::new(), None);
        assert!(!to_rule_record(&rule, &registry).severity_is_unranked);
        rule.severity = "extremely".to_string();
        assert!(to_rule_record(&rule, &registry).severity_is_unranked);
    }

    #[test]
    fn a_condition_record_renders_its_value_and_measures_the_alarm_warning() {
        let registry = rules::compiled_registry();
        let record = to_rule_record(
            &rule_fixture(
                vec![
                    condition_fixture("deadline", "within_next", serde_json::json!("5m")),
                    condition_fixture("deadline", "within_next", serde_json::json!("2h")),
                ],
                Some("item_threshold"),
            ),
            &registry,
        );
        assert_eq!(record.conditions[0].value_display, "5m");
        assert!(record.conditions[0].below_alarm_interval);
        assert!(!record.conditions[1].below_alarm_interval);
        // `deadline` specifically gets the date/time picker, decided by
        // `rules::widget_for` — never by Kotlin.
        assert_eq!(record.conditions[0].widget, MobileValueWidget::Datetime);
    }

    #[test]
    fn a_list_condition_value_renders_comma_joined() {
        assert_eq!(
            condition_value_display(&serde_json::json!(["a", "b"])),
            "a, b",
        );
        assert_eq!(condition_value_display(&serde_json::json!(3)), "3");
        assert_eq!(condition_value_display(&serde_json::json!(true)), "true");
        assert_eq!(condition_value_display(&serde_json::Value::Null), "");
    }

    #[test]
    fn a_condition_input_is_typed_from_the_fields_declared_type() {
        let registry = rules::compiled_registry();
        let kind = Some("item_threshold");
        let typed = |field: &str, op: MobileOperator, value: &str| {
            to_condition(
                &RuleConditionInput {
                    field: field.to_string(),
                    op,
                    value: value.to_string(),
                    negate: false,
                },
                &registry,
                kind,
            )
            .map(|c| c.value)
        };
        assert_eq!(
            typed("priority", MobileOperator::Gt, "2").unwrap(),
            serde_json::json!(2.0),
        );
        assert_eq!(
            typed("stage", MobileOperator::Eq, "ready").unwrap(),
            serde_json::json!("ready"),
        );
        assert_eq!(
            typed("calendar_busy", MobileOperator::Is, "true").unwrap(),
            serde_json::json!(true),
        );
    }

    #[test]
    fn a_condition_input_is_refused_before_the_seam_never_sent() {
        let registry = rules::compiled_registry();
        let kind = Some("item_threshold");
        let attempt = |field: &str, op: MobileOperator, value: &str| {
            to_condition(
                &RuleConditionInput {
                    field: field.to_string(),
                    op,
                    value: value.to_string(),
                    negate: false,
                },
                &registry,
                kind,
            )
            .is_err()
        };
        // A field the kind does not declare.
        assert!(attempt("subject", MobileOperator::Eq, "x"));
        // An operator the field's type does not permit.
        assert!(attempt("priority", MobileOperator::Contains, "2"));
        // A value the field's type cannot hold.
        assert!(attempt("priority", MobileOperator::Gt, "soon"));
        assert!(attempt("calendar_busy", MobileOperator::Is, "yes"));
    }

    #[test]
    fn a_string_list_condition_splits_on_commas_and_drops_empties() {
        let registry = rules::compiled_registry();
        let value = to_condition(
            &RuleConditionInput {
                field: "to".to_string(),
                op: MobileOperator::Contains,
                value: " a , ,b ".to_string(),
                negate: false,
            },
            &registry,
            Some("email"),
        )
        .expect("`to` is a string_list field on email")
        .value;
        assert_eq!(value, serde_json::json!(["a", "b"]));
    }

    #[test]
    fn a_field_record_carries_its_cascade_already_resolved() {
        let registry = rules::compiled_registry();
        let fields = rules::fields_for_kind(&registry, Some("item_threshold"));
        let scheduled = fields
            .iter()
            .find(|f| f.name == "scheduled_date")
            .expect("item_threshold declares scheduled_date");
        let record = to_rule_field_record(scheduled);
        assert_eq!(record.field_type, MobileFieldType::Date);
        assert_eq!(
            record.legal_operators,
            [MobileOperator::WithinNext, MobileOperator::WithinLast],
        );
        // A `date` field is day-grained only (ADR-0013).
        assert_eq!(record.duration_units, ["d"]);
        assert_eq!(
            record.operators,
            [
                RuleOperatorRecord {
                    operator: MobileOperator::WithinNext,
                    widget: MobileValueWidget::Duration,
                },
                RuleOperatorRecord {
                    operator: MobileOperator::WithinLast,
                    widget: MobileValueWidget::Duration,
                },
            ],
            "every legal operator arrives already carrying its control",
        );
    }

    /// The `source` core field arrives asking for the vocabulary picker
    /// under `eq` and a text box under `contains` — [`rules::widget_for`]'s
    /// answer, per operator rather than per field. A typed source is a rule
    /// that silently matches nothing, which is why `eq` is not free text;
    /// `contains` is substring matching over a partial source name, which
    /// the picker cannot express.
    #[test]
    fn the_source_field_asks_for_the_picker_under_eq_and_a_text_box_under_contains() {
        let registry = rules::compiled_registry();
        let source = rules::fields_for_kind(&registry, None)
            .into_iter()
            .find(|f| f.name == "source")
            .expect("`source` is an Event core field");
        let record = to_rule_field_record(&source);
        let widget = |op: MobileOperator| {
            record
                .operators
                .iter()
                .find(|entry| entry.operator == op)
                .map(|entry| entry.widget)
                .expect("a legal operator on `source`")
        };
        assert_eq!(widget(MobileOperator::Eq), MobileValueWidget::Source);
        assert_eq!(widget(MobileOperator::Contains), MobileValueWidget::Text);
    }

    /// The form ships the whole registry, in its own registration order,
    /// **retired entries included and named as such** — a rule already
    /// naming `city-waste/v1` has to render as itself, and a fresh pick of
    /// it is what the screen greys out rather than sending for the
    /// authority to 400.
    #[test]
    fn the_form_carries_the_whole_source_vocabulary_with_retirement_marked() {
        let registry = rules::compiled_registry();
        let sources: Vec<SourceOptionRecord> = registry
            .sources
            .iter()
            .map(|source| SourceOptionRecord {
                source: source.source.clone(),
                retired_as: source.retired_as.clone(),
            })
            .collect();

        assert_eq!(sources.len(), hummingbird_domain::REGISTRY.len());
        assert_eq!(
            sources.iter().map(|s| s.source.as_str()).collect::<Vec<_>>(),
            hummingbird_domain::REGISTRY.iter().map(|e| e.source).collect::<Vec<_>>(),
        );
        let retired = sources
            .iter()
            .find(|s| s.source == "city-waste/v1")
            .expect("the registry's retired entry still ships");
        assert_eq!(retired.retired_as.as_deref(), Some("city-waste/v2"));
    }

    #[test]
    fn a_backtest_record_carries_the_corpus_caveat_beside_every_count() {
        let unavailable = to_backtest_record(rules::BacktestOutcome::Unavailable {
            reason: rules::backtest::BacktestUnavailable::NoLocalHistory,
        });
        assert!(!unavailable.is_available);
        assert_eq!(unavailable.match_count, 0);
        assert_eq!(unavailable.corpus_note_key, BACKTEST_CORPUS_NOTE_KEY);

        let ok = to_backtest_record(rules::BacktestOutcome::Ok {
            matched_ids: vec!["a".to_string(), "b".to_string()],
        });
        assert!(ok.is_available);
        assert_eq!(ok.match_count, 2);
        assert_eq!(ok.corpus_note_key, BACKTEST_CORPUS_NOTE_KEY);
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
    /// prose: no token and a socket that never resolved are unanswered, the
    /// two response-side reports are answered, and a transport failure
    /// *after* the response arrived — a body torn mid-stream — is answered
    /// too, because a backend did reply and then lost the run.
    #[test]
    fn answered_tracks_whether_a_response_arrived() {
        let cases: Vec<(MobileGrillTurnState, bool)> = vec![
            (grill_turn_no_token(asking()), false),
            (grill_turn_transport_failed(asking(), "connection reset".to_string(), false), false),
            (grill_turn_transport_failed(asking(), "unexpected end of stream".to_string(), true), true),
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
            words(grill_turn_transport_failed(asking(), "  boom  ".to_string(), false)),
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
            false,
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
            words(skill_run_transport_failed(started(), String::new(), false)),
            (skills::decline_for_transport(""), false),
        );
        // The same mid-stream tear the Grill door has, on this one too: the
        // wording is identical and only `answered` differs, which is the
        // whole reason it is a parameter rather than a constant.
        assert_eq!(
            words(skill_run_transport_failed(started(), String::new(), true)),
            (skills::decline_for_transport(""), true),
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

    // ---- #539's backend-picker doors

    #[test]
    fn item_grill_button_label_is_the_cores_rule_verbatim() {
        assert_eq!(item_grill_button_label(false), "Grill me");
        assert_eq!(item_grill_button_label(true), "Resume grill");
    }

    #[test]
    fn item_can_grill_matches_the_cores_rule_and_refuses_an_unrecognised_stage() {
        assert!(item_can_grill("triage".to_string()));
        assert!(item_can_grill("grilling".to_string()));
        assert!(!item_can_grill("done".to_string()));
        assert!(!item_can_grill("not-a-stage".to_string()));
    }

    #[test]
    fn backend_auto_selection_is_the_cores_sentinel() {
        assert_eq!(backend_auto_selection(), skills::AUTO_SELECTION);
    }

    /// The predicate itself is `hummingbird_core::decisions::skills::backend
    /// ::declined_backend_fallback`'s own test to own (#539's round-2
    /// review) — this only pins that the mapping wrapper reaches it and
    /// answers unchanged.
    #[test]
    fn declined_backend_fallback_maps_and_answers_the_cores_verdict_unchanged() {
        let declined = MobileSkillRunState::Declined {
            messages: Vec::new(),
            reason: "Could not reach the server.".to_string(),
            backend: None,
            model: None,
            answered: false,
        };
        assert_eq!(
            declined_backend_fallback(declined, "cloud".to_string(), vec!["cloud".to_string(), "home".to_string()]),
            Some("home".to_string()),
        );
        assert_eq!(
            declined_backend_fallback(skill_run_idle(), "cloud".to_string(), vec!["cloud".to_string()]),
            None,
        );
    }

    #[test]
    fn fallback_backend_id_skips_the_dead_entry() {
        assert_eq!(
            fallback_backend_id(vec!["a".to_string(), "b".to_string()], "a".to_string()).as_deref(),
            Some("b"),
        );
        assert_eq!(fallback_backend_id(vec!["cloud".to_string()], "cloud".to_string()), None);
    }

    #[test]
    fn resolve_backend_selection_degrades_a_retired_pin_to_auto() {
        assert_eq!(
            resolve_backend_selection(Some("retired".to_string()), vec!["cloud".to_string()]),
            skills::AUTO_SELECTION,
        );
        assert_eq!(
            resolve_backend_selection(Some("cloud".to_string()), vec!["cloud".to_string()]),
            "cloud",
        );
        assert_eq!(resolve_backend_selection(None, vec!["cloud".to_string()]), skills::AUTO_SELECTION);
    }

    // ---- #539's Grill review predicates

    fn undone_step(id: &str) -> ItemStepRecord {
        ItemStepRecord {
            id: id.to_string(),
            body: "pack".to_string(),
            done: false,
            position: 0,
            deleted_at: None,
        }
    }

    #[test]
    fn grill_would_strand_plan_is_true_only_for_fog_remains_with_a_live_undone_step() {
        assert!(grill_would_strand_plan(MobileGrillVerdict::FogRemains, vec![undone_step("s")]));
        assert!(!grill_would_strand_plan(MobileGrillVerdict::FogRemains, Vec::new()));
        assert!(!grill_would_strand_plan(MobileGrillVerdict::Resolved, vec![undone_step("s")]));
    }

    #[test]
    fn grill_plan_replacement_label_names_the_live_undone_count() {
        assert_eq!(grill_plan_replacement_label(vec![undone_step("a")]), "Also delete 1 unfinished step");
        assert_eq!(
            grill_plan_replacement_label(vec![undone_step("a"), undone_step("b")]),
            "Also delete 2 unfinished steps",
        );
    }

    #[test]
    fn grill_demotes_from_frontier_matches_the_cores_stage_table() {
        assert!(grill_demotes_from_frontier(MobileGrillVerdict::FogRemains, "ready".to_string()));
        assert!(grill_demotes_from_frontier(MobileGrillVerdict::FogRemains, "in_progress".to_string()));
        assert!(!grill_demotes_from_frontier(MobileGrillVerdict::FogRemains, "triage".to_string()));
        assert!(!grill_demotes_from_frontier(MobileGrillVerdict::Resolved, "ready".to_string()));
        assert!(!grill_demotes_from_frontier(MobileGrillVerdict::FogRemains, "not-a-stage".to_string()));
    }

    #[test]
    fn grill_frontier_demotion_warning_is_the_cores_sentence() {
        assert_eq!(grill_frontier_demotion_warning(), skills::FRONTIER_DEMOTION_WARNING);
    }

    // ---- #539's complete_grill / draft doors (host-level)

    async fn grill_test_host(namespace: &str) -> std::sync::Arc<MobileTaskHost> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(namespace);
        // Leaked deliberately: the tempdir must outlive the host for the
        // duration of the test, and these tests are short-lived processes
        // (the same trade every other tempdir-backed test here already
        // makes implicitly by never calling `.close()`).
        std::mem::forget(dir);
        MobileTaskHost::init(path.to_str().unwrap().to_string(), "https://invalid.invalid".to_string(), String::new())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn complete_grill_resolves_and_promotes_a_triage_item_to_ready() {
        let host = grill_test_host("grill-complete").await;
        let id = host
            .capture(
                CaptureDraft {
                    title: "book flights".to_string(),
                    destination: CaptureDestination::Triage,
                    size: String::new(),
                    energy: String::new(),
                    context: String::new(),
                    description: String::new(),
                    project_id: String::new(),
                    priority: String::new(),
                    deadline: String::new(),
                    scheduled_date: String::new(),
                },
                1_000,
            )
            .await
            .unwrap();

        let grill_id = host
            .complete_grill(
                id.clone(),
                Vec::new(),
                MobileGrillCompletion {
                    transcript: "Q: Which airport?\nA: SEA".to_string(),
                    summary: "Settled on SEA".to_string(),
                    verdict: MobileGrillVerdict::Resolved,
                    model_proposal: "{}".to_string(),
                    applied_patch: "{}".to_string(),
                    delete_unticked_plan: false,
                },
                2_000,
            )
            .await
            .unwrap();
        assert!(!grill_id.is_empty());

        let detail = host.item_detail(id, 3_000).await.expect("captured item");
        assert_eq!(detail.stage, "ready");
    }

    #[tokio::test]
    async fn complete_grill_on_an_unknown_item_is_item_not_found() {
        let host = grill_test_host("grill-not-found").await;
        let error = host
            .complete_grill(
                "nope".to_string(),
                Vec::new(),
                MobileGrillCompletion {
                    transcript: String::new(),
                    summary: String::new(),
                    verdict: MobileGrillVerdict::Resolved,
                    model_proposal: "{}".to_string(),
                    applied_patch: "{}".to_string(),
                    delete_unticked_plan: false,
                },
                1_000,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, MobileGrillCompletionError::ItemNotFound));
    }

    #[tokio::test]
    async fn a_saved_draft_reads_back_and_marks_the_item_as_having_one() {
        let host = grill_test_host("grill-draft").await;
        let id = host
            .capture(
                CaptureDraft {
                    title: "renew the passport".to_string(),
                    destination: CaptureDestination::Triage,
                    size: String::new(),
                    energy: String::new(),
                    context: String::new(),
                    description: String::new(),
                    project_id: String::new(),
                    priority: String::new(),
                    deadline: String::new(),
                    scheduled_date: String::new(),
                },
                1_000,
            )
            .await
            .unwrap();

        assert!(!host.has_grill_draft(id.clone()).await);
        assert_eq!(host.grill_draft(id.clone()).await, None);

        let turn = MobileGrillTurn {
            question: MobileGrillQuestion {
                prompt: "Which airport?".to_string(),
                recommended_answer: "SEA".to_string(),
                choices: vec!["SEA".to_string(), "PDX".to_string()],
            },
            answer: "SEA".to_string(),
        };
        host.save_grill_draft(id.clone(), vec![turn.clone()], 2_000).await.unwrap();
        assert!(host.has_grill_draft(id.clone()).await);
        assert_eq!(host.grill_draft(id.clone()).await, Some(vec![turn]));

        host.discard_grill_draft(id.clone(), 3_000).await.unwrap();
        assert!(!host.has_grill_draft(id.clone()).await);
        assert_eq!(host.grill_draft(id).await, None);
    }
}

/// The M4 (#535) Settings doors. Every exported function above is
/// exercised here, for the same `dead_code` reason `skills_tests` states.
#[cfg(test)]
mod settings_tests {
    use super::*;

    fn dummy_entry(id: &str, patch_fields: serde_json::Value) -> DeadLetterEntry {
        DeadLetterEntry {
            entry: hummingbird_core::sync::queue::QueueEntry {
                id: id.to_string(),
                intent: MutationIntent::Patch {
                    path: "settings/city-waste-page".to_string(),
                    method: hummingbird_core::sync::write::transport::HttpMethod::Put,
                    base: serde_json::json!({}),
                    base_updated_at: 0,
                    patch_fields,
                    rebase_fields: None,
                },
                operation_id: None,
            },
            reason: DeadLetterReason::Permanent("rejected".to_string()),
            at_ms: 5_000,
        }
    }

    #[test]
    fn a_binding_record_carries_its_three_states_verbatim() {
        assert_eq!(
            to_binding_value(&BindingValue::Unset),
            MobileBindingValue::Unset,
        );
        assert_eq!(
            to_binding_value(&BindingValue::Text { text: "f1".to_string() }),
            MobileBindingValue::Text { text: "f1".to_string() },
        );
        assert_eq!(
            to_binding_value(&BindingValue::Other { raw: "7".to_string() }),
            MobileBindingValue::Other { raw: "7".to_string() },
        );

        let binding = Binding {
            key: "race-series".to_string(),
            known: true,
            pending: false,
            value: BindingValue::Text { text: "motogp".to_string() },
        };
        let record = to_binding_record(&binding);
        assert_eq!(record.key, "race-series");
        assert!(record.known);
        assert!(!record.pending);
        assert_eq!(record.value, MobileBindingValue::Text { text: "motogp".to_string() });
    }

    #[test]
    fn a_permanent_dead_letter_carries_its_detail_and_no_fields() {
        let entry = dummy_entry("q-1", serde_json::json!({}));
        let record = to_dead_letter_record(&entry);
        assert_eq!(record.id, "q-1");
        assert_eq!(
            record.reason,
            MobileDeadLetterReason::Permanent { detail: "rejected".to_string() },
        );
        assert!(record.fields.is_empty());
        assert_eq!(record.at_ms, 5_000);
        assert_eq!(record.entity, "settings");
        assert_eq!(record.entity_id.as_deref(), Some("city-waste-page"));
    }

    #[test]
    fn a_conflict_dead_letter_pairs_each_named_field_with_its_local_and_server_value() {
        let mut entry = dummy_entry(
            "q-2",
            serde_json::json!({ "value": "new-page" }),
        );
        entry.reason = DeadLetterReason::Conflict {
            fields: vec!["value".to_string()],
            current: serde_json::json!({ "value": "someone-elses-page" }),
        };
        let record = to_dead_letter_record(&entry);
        assert_eq!(record.reason, MobileDeadLetterReason::Conflict);
        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].field, "value");
        assert_eq!(record.fields[0].local_json, "\"new-page\"");
        assert_eq!(record.fields[0].server_json, "\"someone-elses-page\"");
    }

    #[test]
    fn a_contention_dead_letter_carries_neither_detail_nor_fields() {
        let mut entry = dummy_entry("q-3", serde_json::json!({}));
        entry.reason = DeadLetterReason::Contention { current: serde_json::json!({}) };
        let record = to_dead_letter_record(&entry);
        assert_eq!(record.reason, MobileDeadLetterReason::Contention);
        assert!(record.fields.is_empty());
    }

    #[test]
    fn the_sync_status_summary_is_the_core_rule_verbatim() {
        let summary = sync_status_summary(MobileSyncStatusInput {
            online: true,
            last_sync_outcome_kind: Some("completed".to_string()),
            last_sync_at_ms: Some(0),
            queue_depth: Some(2),
            now_ms: 60_000,
        });
        assert_eq!(summary.tone, MobileSyncStatusTone::Success);
        assert_eq!(summary.label, "Synced — as of 1m ago · 2 queued");
        assert_eq!(summary.tone_word, "synced");
    }

    #[test]
    fn a_held_outcome_reads_as_held_never_a_silent_success() {
        let summary = sync_status_summary(MobileSyncStatusInput {
            online: true,
            last_sync_outcome_kind: Some("credential_needed".to_string()),
            last_sync_at_ms: Some(0),
            queue_depth: None,
            now_ms: 60_000,
        });
        assert_eq!(summary.tone, MobileSyncStatusTone::Warn);
        assert_eq!(summary.label, "Held — device token needed");
    }

    #[test]
    fn the_dead_letter_heading_is_the_core_rule_verbatim() {
        assert_eq!(dead_letter_heading(1), "1 edit didn't apply");
        assert_eq!(dead_letter_heading(2), "2 edits didn't apply");
    }

    #[test]
    fn informativeness_is_the_core_rule_verbatim() {
        assert!(!is_informative_sync_outcome("skipped".to_string()));
        assert!(!is_informative_sync_outcome("busy".to_string()));
        assert!(is_informative_sync_outcome("completed".to_string()));
        assert!(is_informative_sync_outcome("held".to_string()));
    }

    // ------------------------------------------------------- the roster (#714)

    #[test]
    fn the_roster_crosses_in_the_cores_own_order_with_the_cores_own_labels() {
        // The seam must not re-derive the list: same length, same order,
        // same label, same surface, same keys, entry by entry against
        // `hummingbird_core::decisions::question_roster` itself.
        let core = hummingbird_core::decisions::question_roster();
        let crossed = question_roster();
        assert_eq!(crossed.len(), core.len());
        for (mobile, decided) in crossed.iter().zip(core.iter()) {
            assert_eq!(mobile.question, map_standing_question(decided.question));
            assert_eq!(mobile.label, decided.label);
            assert_eq!(
                mobile.surface,
                match decided.surface {
                    Surface::Now => MobileSurface::Now,
                    Surface::Status => MobileSurface::Status,
                }
            );
            let keys: Vec<String> =
                decided.bindings.iter().map(|key| key.as_str().to_string()).collect();
            assert_eq!(mobile.bindings, keys);
        }
    }

    #[test]
    fn the_roster_lists_every_question_this_seam_can_name() {
        // The order, spelled out once so a reordering of `QUESTION_ORDER`
        // is a visible diff here rather than a silent reshuffle of a
        // Settings screen nobody re-read.
        let questions: Vec<MobileStandingQuestion> =
            question_roster().into_iter().map(|entry| entry.question).collect();
        assert_eq!(
            questions,
            vec![
                MobileStandingQuestion::Homework,
                MobileStandingQuestion::Scps,
                MobileStandingQuestion::Waste,
                MobileStandingQuestion::Weekend,
                MobileStandingQuestion::Vacation,
                MobileStandingQuestion::Race,
                MobileStandingQuestion::Kimi,
                MobileStandingQuestion::Github,
                MobileStandingQuestion::Uptime,
                MobileStandingQuestion::Reachability,
                MobileStandingQuestion::Poller,
            ]
        );
        // A question with no binding is present with an empty list, never
        // dropped — the roster is the only place it can be seen at all.
        let weekend = question_roster()
            .into_iter()
            .find(|entry| entry.question == MobileStandingQuestion::Weekend)
            .expect("weekend is listed");
        assert!(weekend.bindings.is_empty());
    }


    // ------------------------------------------------ the off switch (#715)

    #[tokio::test]
    async fn a_fresh_device_reports_every_question_on_with_nothing_written() {
        let host = pane_host("switches-fresh").await;
        let switches = host.question_switches().await;

        assert_eq!(switches.len(), 11);
        // Same order as the roster, so #716 can zip the two lists.
        let questions: Vec<MobileStandingQuestion> =
            switches.iter().map(|switch| switch.question).collect();
        let roster: Vec<MobileStandingQuestion> =
            question_roster().into_iter().map(|entry| entry.question).collect();
        assert_eq!(questions, roster);
        assert!(switches.iter().all(|switch| switch.enabled));
        assert!(switches.iter().all(|switch| !switch.pending));
    }

    /// #715's Android criterion, which is a seam test rather than a screen:
    /// the phone honours a question switched off — without yet being able
    /// to switch one itself (#716 renders the control). Both halves of
    /// "hidden and unpolled" are checked, since `rank_panes` and
    /// `pane_zone_queries` are two separate doors.
    #[tokio::test]
    async fn switching_a_question_off_removes_its_pane_from_this_seam() {
        let host = pane_host("switches-hide").await;
        let before = host
            .rank_panes(MobileSurface::Now, 1_000, Vec::new(), MobileSyncFacts::default())
            .await;
        assert!(before
            .iter()
            .any(|pane| pane.standing_question == MobileStandingQuestion::Weekend));

        host.set_question_enabled(MobileStandingQuestion::Weekend, false, 1_000)
            .await
            .unwrap();

        let switch = host
            .question_switches()
            .await
            .into_iter()
            .find(|switch| switch.question == MobileStandingQuestion::Weekend)
            .expect("weekend is listed");
        assert!(!switch.enabled);
        assert!(switch.pending, "the write has not drained yet, and says so");

        let after = host
            .rank_panes(MobileSurface::Now, 1_000, Vec::new(), MobileSyncFacts::default())
            .await;
        assert_eq!(after.len(), before.len() - 1);
        assert!(!after
            .iter()
            .any(|pane| pane.standing_question == MobileStandingQuestion::Weekend));

        // The unpolled half: weekend is the question that asks for the
        // rolling window, so its queries must go with its pane.
        let queries = host.pane_zone_queries(MobileSurface::Now, 1_000).await;
        let before_queries = pane_host("switches-hide-baseline")
            .await
            .pane_zone_queries(MobileSurface::Now, 1_000)
            .await;
        assert!(
            queries.len() < before_queries.len(),
            "a question nobody can see must not cost the host a zone lookup"
        );
    }

    #[tokio::test]
    async fn switching_a_question_back_on_restores_its_pane() {
        let host = pane_host("switches-restore").await;
        host.set_question_enabled(MobileStandingQuestion::Race, false, 1_000)
            .await
            .unwrap();
        host.set_question_enabled(MobileStandingQuestion::Race, true, 2_000)
            .await
            .unwrap();

        let ranked = host
            .rank_panes(MobileSurface::Now, 3_000, Vec::new(), MobileSyncFacts::default())
            .await;
        assert!(ranked
            .iter()
            .any(|pane| pane.standing_question == MobileStandingQuestion::Race));
    }

    #[tokio::test]
    async fn a_switch_row_is_not_offered_to_the_bindings_editor() {
        let host = pane_host("switches-not-a-binding").await;
        host.set_question_enabled(MobileStandingQuestion::Kimi, false, 1_000)
            .await
            .unwrap();

        let keys: Vec<String> =
            host.bindings().await.into_iter().map(|binding| binding.key).collect();
        assert!(keys.contains(&"race-series".to_string()));
        assert!(!keys.iter().any(|key| key.starts_with("question-enabled-")));
    }

    // -------------------------------------------------------------- panes (#536)

    async fn pane_host(namespace: &str) -> Arc<MobileTaskHost> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(namespace);
        std::mem::forget(dir);
        MobileTaskHost::init(
            path.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            "token".to_string(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a_fresh_device_ranks_the_status_five_as_never_polled_sentinels() {
        let host = pane_host("panes-status-fresh").await;
        let ranked = host.rank_panes(MobileSurface::Status, 1_000, Vec::new(), MobileSyncFacts::default()).await;
        // kimi/github/uptime/reachability contribute one pane each; poller
        // contributes one per source it watches, always ranked.
        assert_eq!(ranked.len(), 4 + poller::poller_sources().len());
        assert!(ranked.iter().all(|pane| pane.answer.answer_state == MobilePaneAnswerState::BoundButUnacquired));
        let questions: Vec<MobileStandingQuestion> = ranked
            .iter()
            .map(|pane| pane.standing_question)
            .filter(|q| *q != MobileStandingQuestion::Poller)
            .collect();
        assert_eq!(
            questions,
            vec![
                MobileStandingQuestion::Kimi,
                MobileStandingQuestion::Github,
                MobileStandingQuestion::Uptime,
                MobileStandingQuestion::Reachability,
            ],
        );
        assert!(ranked.iter().any(|pane| pane.standing_question == MobileStandingQuestion::Poller));
    }

    #[tokio::test]
    async fn the_nav_alarm_stays_quiet_on_a_fresh_device_and_agrees_with_the_board() {
        let host = pane_host("panes-status-alarm").await;
        let now_ms: i64 = 1_700_000_000_000;

        // Nothing polled, never synced: every one of the four is a gap, and
        // a gap is silent on the nav (`alarm.rs`'s own rule).
        assert_eq!(host.status_alarm(now_ms, MobileSyncFacts::default()).await, None);

        // Nothing synced in a long while — `reachability` escalates to
        // `live`, and the nav must reach the same reading the board does
        // off the same sync history, which is why this door takes it.
        let long_quiet = MobileSyncFacts {
            latest_outcome_kind: Some("completed".to_string()),
            latest_informative_at_ms: Some(now_ms - 24 * 60 * 60 * 1000),
            last_successful_at_ms: Some(now_ms - 24 * 60 * 60 * 1000),
        };
        let board = host
            .rank_panes(MobileSurface::Status, now_ms, Vec::new(), long_quiet.clone())
            .await;
        let reachability = board
            .iter()
            .find(|pane| pane.standing_question == MobileStandingQuestion::Reachability)
            .unwrap();
        assert_eq!(reachability.answer.band, MobilePaneBand::Live);
        assert_eq!(
            host.status_alarm(now_ms, long_quiet).await,
            Some(MobilePaneBand::Live),
        );
    }

    #[tokio::test]
    async fn the_status_surface_asks_for_no_zone_facts_none_of_the_four_is_civil_date_reasoning() {
        let host = pane_host("panes-status-zone").await;
        let queries = host.pane_zone_queries(MobileSurface::Status, 1_000).await;
        assert!(queries.is_empty());
    }

    #[tokio::test]
    async fn a_synced_device_reads_reachability_off_its_own_persisted_sync_history() {
        let host = pane_host("panes-status-sync").await;
        let now_ms: i64 = 1_700_000_000_000;
        // Fresh and just synced: dormant, quiet.
        let fresh = host
            .rank_panes(
                MobileSurface::Status,
                now_ms,
                Vec::new(),
                MobileSyncFacts {
                    latest_outcome_kind: Some("completed".to_string()),
                    latest_informative_at_ms: Some(now_ms - 60_000),
                    last_successful_at_ms: Some(now_ms - 60_000),
                },
            )
            .await;
        let reachability = fresh
            .iter()
            .find(|pane| pane.standing_question == MobileStandingQuestion::Reachability)
            .unwrap();
        assert_eq!(reachability.answer.answer_state, MobilePaneAnswerState::Answered);
        assert_eq!(reachability.answer.band, MobilePaneBand::Dormant);

        // Nothing synced in a long while: escalates to live.
        let stale = host
            .rank_panes(
                MobileSurface::Status,
                now_ms,
                Vec::new(),
                MobileSyncFacts {
                    latest_outcome_kind: Some("pull_failed".to_string()),
                    latest_informative_at_ms: Some(now_ms - 6 * 60_000),
                    last_successful_at_ms: Some(now_ms - 10 * 60 * 60 * 1000),
                },
            )
            .await;
        let reachability = stale
            .iter()
            .find(|pane| pane.standing_question == MobileStandingQuestion::Reachability)
            .unwrap();
        assert_eq!(reachability.answer.band, MobilePaneBand::Live);
    }

    #[test]
    fn every_standing_question_maps_to_a_distinct_mobile_variant() {
        let cases = [
            (StandingQuestion::Homework, MobileStandingQuestion::Homework),
            (StandingQuestion::Waste, MobileStandingQuestion::Waste),
            (StandingQuestion::Weekend, MobileStandingQuestion::Weekend),
            (StandingQuestion::Vacation, MobileStandingQuestion::Vacation),
            (StandingQuestion::Race, MobileStandingQuestion::Race),
            (StandingQuestion::Kimi, MobileStandingQuestion::Kimi),
            (StandingQuestion::Github, MobileStandingQuestion::Github),
            (StandingQuestion::Uptime, MobileStandingQuestion::Uptime),
            (StandingQuestion::Reachability, MobileStandingQuestion::Reachability),
        ];
        for (core_question, expected) in cases {
            assert_eq!(map_standing_question(core_question), expected);
        }
    }

    #[test]
    fn a_zone_query_key_round_trips_through_the_mobile_mirror() {
        let civil = MobileZoneQuery::CivilDate { zone: "America/Los_Angeles".to_string(), at_ms: 17 };
        assert_eq!(mobile_zone_query_key(civil), "civil:America/Los_Angeles:17");
        let midnight =
            MobileZoneQuery::Midnight { zone: "Europe/London".to_string(), date: "2026-08-17".to_string() };
        assert_eq!(mobile_zone_query_key(midnight), "midnight:Europe/London:2026-08-17");
    }

    #[test]
    fn zone_facts_cross_by_key_and_an_unresolved_key_reads_as_absent() {
        let facts = to_zone_facts(vec![MobileZoneFact {
            key: "civil:Europe/London:0".to_string(),
            value: MobileZoneFactValue::Date { value: "2026-08-17".to_string() },
        }]);
        assert_eq!(facts.civil_date("Europe/London", 0).as_deref(), Some("2026-08-17"));
        assert_eq!(facts.civil_date("Europe/Paris", 0), None);
    }

    // ---------------------------------------------------------- panes (#537)

    /// A [`CaptureDraft`] with only `title` set — this module's own copy of
    /// `tests::title_only_draft`, since that helper is private to its own
    /// `mod tests` and this module reaches for the same shape.
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

    /// This module's own copy of `tests::untouched_triage_edit`, same
    /// reason as [`title_only_draft`] above.
    fn untouched_triage_edit() -> ItemEdit {
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

    /// A minimal live [`hummingbird_domain::Item`] for [`pane_item_facts`]'s
    /// own tests — every field the seam does not read is pinned to a
    /// neutral default, the same "only the fields a test actually reads
    /// vary" discipline `tests::item` (this crate's `now_board` fixture)
    /// uses, kept as this module's own copy since that one is private to
    /// its sibling `mod tests`.
    fn pane_item(id: &str, deadline: Option<&str>, scheduled_date: Option<&str>) -> hummingbird_domain::Item {
        staged_pane_item(id, deadline, scheduled_date, hummingbird_domain::Stage::Ready, None)
    }

    /// [`pane_item`] with the two fields #675's homework pane reads —
    /// its stage and its context — set explicitly.
    fn staged_pane_item(
        id: &str,
        deadline: Option<&str>,
        scheduled_date: Option<&str>,
        stage: hummingbird_domain::Stage,
        context: Option<&str>,
    ) -> hummingbird_domain::Item {
        hummingbird_domain::Item {
            id: id.to_string(),
            seq: None,
            title: format!("item {id}"),
            description: None,
            stage,
            size: None,
            energy: None,
            context: context.map(str::to_string),
            priority: 0,
            project_id: None,
            project_pos: None,
            deadline: deadline.map(str::to_string),
            scheduled_date: scheduled_date.map(str::to_string),
            source: None,
            source_key: None,
            source_url: None,
            vault_path: None,
            archived_at: None,
            agent: false,
            created_at: 0,
            updated_at: 0,
            version: 0,
        }
    }

    #[test]
    fn pane_items_include_both_the_frontier_and_the_relation_blocked_section() {
        // `NowScreen.tsx::realQuestionInputs`'s own union
        // (`[...frontier, ...blocked.map(e => e.item)]`) — a relation-
        // blocked item due this weekend must still reach the weekend
        // pane's merge, never silently drop out just because
        // `Core::frontier` excludes it (#537 review).
        let frontier = vec![pane_item("f-1", Some("2026-08-15"), None)];
        let blocked =
            vec![(pane_item("b-1", None, Some("2026-08-16")), vec![pane_item("blocker", None, None)])];

        let facts = pane_item_facts(&frontier, &blocked, &[], &[], &[]);

        assert_eq!(facts.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), vec!["f-1", "b-1"]);
        let blocked_fact = facts.iter().find(|f| f.id == "b-1").unwrap();
        assert_eq!(blocked_fact.scheduled_date.as_deref(), Some("2026-08-16"));
    }

    #[test]
    fn pane_items_never_include_a_blocked_entrys_own_blockers() {
        let blocked = vec![(pane_item("b-1", None, None), vec![pane_item("blocker-only", None, None)])];

        let facts = pane_item_facts(&[], &blocked, &[], &[], &[]);

        assert_eq!(facts.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), vec!["b-1"]);
    }

    #[tokio::test]
    async fn a_fresh_device_ranks_the_now_five_with_only_homework_answered() {
        // Nothing bound (waste/race), no calendar connected (weekend/
        // vacation) — those four are `unbound`, still ranked rather than
        // vanishing (`panes::mod`'s own "a pane nobody has bound yet must
        // still be discoverable" rule).
        //
        // Homework (#675) and scps (#693) are the fifth and sixth. Neither
        // has a binding to be unset, so their own unanswerable state is the
        // zone bridge's gap rather than `unbound` — this caller resolved
        // nothing, which is the honest "this host answered no queries" case
        // and not a setup prompt (scps has no setup prompt at all —
        // `scps.rs`'s own "never unbound" rule).
        let host = pane_host("panes-now-fresh").await;
        let ranked = host.rank_panes(MobileSurface::Now, 1_000, Vec::new(), MobileSyncFacts::default()).await;
        assert_eq!(ranked.len(), 6);
        let questions: Vec<MobileStandingQuestion> = ranked.iter().map(|pane| pane.standing_question).collect();
        assert_eq!(
            questions,
            vec![
                MobileStandingQuestion::Homework,
                MobileStandingQuestion::Scps,
                MobileStandingQuestion::Waste,
                MobileStandingQuestion::Weekend,
                MobileStandingQuestion::Vacation,
                MobileStandingQuestion::Race,
            ],
        );
        assert!(ranked
            .iter()
            .filter(|pane| pane.standing_question != MobileStandingQuestion::Homework
                && pane.standing_question != MobileStandingQuestion::Scps)
            .all(|pane| pane.answer.answer_state == MobilePaneAnswerState::Unbound));
        for question in [MobileStandingQuestion::Homework, MobileStandingQuestion::Scps] {
            let pane = ranked.iter().find(|pane| pane.standing_question == question).unwrap();
            assert_eq!(pane.answer.answer_state, MobilePaneAnswerState::BoundButUnacquired);
        }
    }

    #[test]
    fn pane_items_include_the_triage_inbox_and_the_grilling_queue() {
        // The #675 widening, matched to `NowScreen.tsx::realQuestionInputs`.
        // A captured piece of homework is still homework, and before this
        // the triage inbox never crossed the seam at all.
        let facts = pane_item_facts(
            &[pane_item("f-1", None, None)],
            &[],
            &[pane_item("t-1", None, None)],
            &[pane_item("g-1", None, None)],
            &[],
        );
        assert_eq!(
            facts.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
            vec!["f-1", "t-1", "g-1"],
        );
    }

    #[test]
    fn pane_items_include_the_externally_blocked_items() {
        // The last arm of the live partition, and the one the first cut of
        // #675 missed: `Core::blocked` is relation-blocked Ready/InProgress
        // items only, so a `Stage::Blocked` item — an external wait — was
        // reachable from none of the four queries and disappeared from the
        // homework pane, which counts every stage but `done`.
        let facts = pane_item_facts(
            &[pane_item("f-1", None, None)],
            &[],
            &[],
            &[],
            &[pane_item("x-1", None, None)],
        );
        assert_eq!(facts.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), vec!["f-1", "x-1"]);
    }

    #[test]
    fn an_item_crosses_the_seam_with_the_four_fields_the_homework_pane_reads() {
        // The seam's own half of the widening: a stage spelled as the wire
        // spells it (so both hosts hand the core byte-identical strings),
        // the context, the notes and the created stamp.
        let mut item = staged_pane_item(
            "hw",
            Some("2026-08-24"),
            None,
            hummingbird_domain::Stage::Triage,
            Some(homework::HOMEWORK_CONTEXT),
        );
        item.description = Some("read chapter 4".to_string());
        item.created_at = 1_700;

        let facts = to_pane_item_facts(&item);

        assert_eq!(facts.stage, "triage");
        assert_eq!(facts.context.as_deref(), Some("@homework"));
        assert_eq!(facts.description.as_deref(), Some("read chapter 4"));
        assert_eq!(facts.created_at, 1_700);
    }

    /// The host half of the zone bridge, in UTC — what `java.time` does on
    /// device, done here so a test can drive the real `rank_panes` door
    /// rather than the core function behind it. Built out of `zone.rs`'s
    /// own calendar arithmetic (which needs no tzdb) rather than a second
    /// date library in this crate.
    fn resolve_zone_facts(queries: Vec<MobileZoneQuery>) -> Vec<MobileZoneFact> {
        const DAY_MS: i64 = 24 * 60 * 60 * 1000;
        queries
            .into_iter()
            .map(|query| {
                let key = mobile_zone_query_key(query.clone());
                let value = match query {
                    MobileZoneQuery::CivilDate { at_ms, .. } => MobileZoneFactValue::Date {
                        value: add_civil_days("1970-01-01", at_ms.div_euclid(DAY_MS)).unwrap(),
                    },
                    MobileZoneQuery::Midnight { date, .. } => MobileZoneFactValue::Instant {
                        value: civil_days_between("1970-01-01", &date).unwrap() * DAY_MS,
                    },
                };
                MobileZoneFact { key, value }
            })
            .collect()
    }

    #[tokio::test]
    async fn a_captured_homework_item_answers_the_homework_pane_through_the_real_door() {
        // Wiring proof end to end on this seam: capture an item with the
        // `@homework` context, rank the Now surface, and read the winner's
        // notes off the pane's own facts — the same crossing
        // `NowPanesExpanded.kt` renders.
        let host = pane_host("panes-now-homework").await;
        let mut draft = title_only_draft("Prep for Thursday");
        draft.context = homework::HOMEWORK_CONTEXT.to_string();
        draft.description = "read chapter 4".to_string();
        host.capture(draft, 1_000).await.unwrap();

        let zone = resolve_zone_facts(host.pane_zone_queries(MobileSurface::Now, 1_000).await);
        let ranked =
            host.rank_panes(MobileSurface::Now, 1_000, zone, MobileSyncFacts::default()).await;
        let pane = ranked
            .iter()
            .find(|pane| pane.standing_question == MobileStandingQuestion::Homework)
            .unwrap();
        assert_eq!(pane.answer.answer_state, MobilePaneAnswerState::Answered);
        let MobilePaneFacts::Homework { resolved, .. } = &pane.facts else {
            panic!("the homework pane carried another question's facts");
        };
        let MobileHomeworkResolved::Facts { facts } = resolved else {
            panic!("expected facts, got {resolved:?}");
        };
        let winner = facts.winner.as_ref().expect("a captured item is open homework");
        assert_eq!(winner.title, "Prep for Thursday");
        assert_eq!(winner.description.as_deref(), Some("read chapter 4"));
    }

    #[tokio::test]
    async fn the_standing_homework_link_crosses_even_when_nothing_is_open() {
        // The "standing" half of #675's link, on this seam: the pane is
        // dormant (nothing captured) and the link still reaches the host,
        // because it rides beside `resolved` rather than inside the facts.
        let host = pane_host("panes-now-homework-link").await;
        host.set_binding(
            "seed-link".to_string(),
            homework::HOMEWORK_LINK_BINDING_KEY.to_string(),
            "https://example.com/j/000000000".to_string(),
            1_000,
        )
        .await
        .unwrap();

        let zone = resolve_zone_facts(host.pane_zone_queries(MobileSurface::Now, 1_000).await);
        let ranked =
            host.rank_panes(MobileSurface::Now, 1_000, zone, MobileSyncFacts::default()).await;
        let pane = ranked
            .iter()
            .find(|pane| pane.standing_question == MobileStandingQuestion::Homework)
            .unwrap();
        let MobilePaneFacts::Homework { resolved, link } = &pane.facts else {
            panic!("the homework pane carried another question's facts");
        };
        let MobileHomeworkResolved::Facts { facts } = resolved else {
            panic!("expected facts, got {resolved:?}");
        };
        assert_eq!(facts.winner, None, "nothing was captured");
        assert_eq!(link.as_deref(), Some("https://example.com/j/000000000"));
    }

    #[tokio::test]
    async fn an_unbound_homework_link_crosses_as_nothing_to_draw() {
        let host = pane_host("panes-now-homework-nolink").await;
        let zone = resolve_zone_facts(host.pane_zone_queries(MobileSurface::Now, 1_000).await);
        let ranked =
            host.rank_panes(MobileSurface::Now, 1_000, zone, MobileSyncFacts::default()).await;
        let pane = ranked
            .iter()
            .find(|pane| pane.standing_question == MobileStandingQuestion::Homework)
            .unwrap();
        let MobilePaneFacts::Homework { link, .. } = &pane.facts else {
            panic!("the homework pane carried another question's facts");
        };
        assert_eq!(*link, None);
    }

    #[tokio::test]
    async fn the_now_surface_asks_for_more_than_the_status_surfaces_empty_list() {
        // Weekend/vacation/waste are all civil-date reasoning (`panes::mod`'s
        // own test pins this core-side); the mobile door must carry that
        // non-empty list through rather than collapsing it, unlike Status.
        let host = pane_host("panes-now-zone").await;
        let queries = host.pane_zone_queries(MobileSurface::Now, 1_000).await;
        assert!(!queries.is_empty());
    }

    #[tokio::test]
    async fn a_bound_waste_page_answers_once_the_source_has_been_read() {
        // Wiring proof for `mobile_pane_inputs`' new bindings/pane_reads
        // arms: setting the waste binding and landing a `city-waste/v2`
        // snapshot must reach the Now surface's waste pane exactly as it
        // already reaches the panes-family unit tests core-side.
        let host = pane_host("panes-now-waste").await;
        host.set_binding(
            "seed-1".to_string(),
            waste::BINDING_KEY.to_string(),
            "https://example.gov".to_string(),
            1_000,
        )
        .await
        .unwrap();
        let ranked = host.rank_panes(MobileSurface::Now, 1_000, Vec::new(), MobileSyncFacts::default()).await;
        let waste = ranked
            .iter()
            .find(|pane| pane.standing_question == MobileStandingQuestion::Waste)
            .unwrap();
        // Bound but never polled: no snapshot has landed for this device.
        assert_eq!(waste.answer.answer_state, MobilePaneAnswerState::BoundButUnacquired);
    }

    // ------------------------------------------------- set_scheduled_date (#537)

    #[tokio::test]
    async fn set_scheduled_date_writes_only_the_do_date_and_leaves_stage_alone() {
        let host = pane_host("panes-now-schedule").await;
        let id = host.capture(title_only_draft("plan the weekend"), 1_000).await.unwrap();
        // Promote to Ready first — a triage-only item is never on the
        // frontier a weekend-plans merge reads, and the write must not be
        // the thing that promotes it.
        host.triage_item(id.clone(), true, untouched_triage_edit(), 1_000).await.unwrap();

        host.set_scheduled_date(id.clone(), Some("2026-08-15".to_string()), 2_000).await.unwrap();

        let detail = host.item_detail(id, 2_000).await.expect("captured item");
        assert_eq!(detail.scheduled_date.as_deref(), Some("2026-08-15"));
        assert_eq!(detail.stage, "ready", "a do-date write is not a promotion");
    }

    #[tokio::test]
    async fn set_scheduled_date_clears_a_previously_set_do_date() {
        let host = pane_host("panes-now-schedule-clear").await;
        let id = host.capture(title_only_draft("plan the weekend"), 1_000).await.unwrap();
        host.set_scheduled_date(id.clone(), Some("2026-08-15".to_string()), 1_000).await.unwrap();

        host.set_scheduled_date(id.clone(), None, 2_000).await.unwrap();

        let detail = host.item_detail(id, 2_000).await.expect("captured item");
        assert_eq!(detail.scheduled_date, None);
    }

    #[tokio::test]
    async fn set_scheduled_date_on_an_unknown_item_is_item_not_found() {
        let host = pane_host("panes-now-schedule-missing").await;
        assert!(matches!(
            host.set_scheduled_date("nope".to_string(), Some("2026-08-15".to_string()), 1_000).await,
            Err(MobileEditError::ItemNotFound)
        ));
    }

    // ---------------------------------------------- pane facts (mirrors)
    // One facts-arm and one gap-arm round-trip per question, driven through
    // `mobile_pane_facts_of` with serde-built `PaneInputs` — the same
    // fixture idiom `panes::mod`'s own tests use — plus host-level checks
    // that every ranked record carries its own question's arm.

    use hummingbird_core::decisions::panes::zone::{add_civil_days, civil_days_between};

    /// A host resolver stood up from the queries the core itself asked —
    /// `weekend.rs`'s own test fixture, generic over the query's zone: a
    /// fixed UTC-7 offset, consistency being what the arithmetic relies on.
    fn resolve_zone(queries: &[ZoneQuery]) -> ZoneFacts {
        const OFFSET_MS: i64 = 7 * 3_600_000;
        const DAY_MS: i64 = 24 * 3_600_000;
        let mut facts = ZoneFacts::default();
        for query in queries {
            match query {
                ZoneQuery::CivilDate { at_ms, .. } => {
                    let days = (at_ms - OFFSET_MS).div_euclid(DAY_MS);
                    facts.insert(query, ZoneFact::Date(add_civil_days("1970-01-01", days).unwrap()));
                }
                ZoneQuery::Midnight { date, .. } => {
                    let days = civil_days_between("1970-01-01", date).unwrap();
                    facts.insert(query, ZoneFact::Instant(days * DAY_MS + OFFSET_MS));
                }
            }
        }
        facts
    }

    /// `panes::mod`'s own fixture instant — 2026-08-09-ish, a week before
    /// the payload dates below.
    const PANE_NOW_MS: i64 = 1_786_377_600_000;

    fn pane_inputs(value: serde_json::Value) -> PaneInputs {
        serde_json::from_value(value).unwrap()
    }

    fn empty_pane_inputs() -> PaneInputs {
        pane_inputs(serde_json::json!({ "nowMs": PANE_NOW_MS, "bindings": [] }))
    }

    #[test]
    fn waste_facts_round_trip_facts_and_gap() {
        let inputs = pane_inputs(serde_json::json!({
            "nowMs": PANE_NOW_MS,
            "bindings": [
                {"key": waste::BINDING_KEY, "value": {"state":"text","text":"https://example.gov"}}
            ],
            "paneReads": {
                waste::SOURCE: {"snapshots": [{
                    "key": waste::SNAPSHOT_KEY,
                    "envelope": {"kind":"ok","schema":waste::SOURCE,
                                 "body":"{\"zone\":\"America/Los_Angeles\",\"scheduled\":\"2026-08-17\",\"collected_on\":\"2026-08-17\",\"streams\":[\"trash\",\"yard\"]}"},
                    "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs":86400000},
                }]},
            },
        }));
        let zone = resolve_zone(&waste::waste_zone_queries(&inputs));

        let arm = mobile_pane_facts_of(StandingQuestion::Waste, waste::SNAPSHOT_KEY, &inputs, &zone);
        let MobilePaneFacts::Waste { setup, resolved: MobileWasteResolved::Facts { facts } } = arm
        else {
            panic!("expected the waste facts arm, got {arm:?}");
        };
        assert_eq!(setup, MobileWasteSetup::Bound);
        assert_eq!(facts.collected_on, "2026-08-17");
        assert!(!facts.holiday);
        assert!(facts.days_away >= 0);
        assert_eq!(facts.streams, vec![MobileWasteStream::Trash, MobileWasteStream::Yard]);

        let gap = mobile_pane_facts_of(
            StandingQuestion::Waste,
            waste::SNAPSHOT_KEY,
            &empty_pane_inputs(),
            &ZoneFacts::default(),
        );
        // The setup kind is what separates this device's two unacquired
        // flavours; the gap alone cannot.
        assert!(matches!(
            gap,
            MobilePaneFacts::Waste {
                setup: MobileWasteSetup::Unset,
                resolved: MobileWasteResolved::Gap { gap: MobileWasteGap::NotFetched },
            }
        ));
    }

    /// The reason the setup kind crosses at all: `Unread` and `Unusable`
    /// both answer `BoundButUnacquired`, so the answer state cannot separate
    /// "this device has not read its bindings yet" from "the binding holds
    /// something unusable" — one is a wait, the other a repair.
    #[test]
    fn the_setup_kind_separates_the_two_unacquired_flavours() {
        let facts_for = |bindings: serde_json::Value| {
            mobile_pane_facts_of(
                StandingQuestion::Waste,
                waste::SNAPSHOT_KEY,
                &pane_inputs(serde_json::json!({ "nowMs": PANE_NOW_MS, "bindings": bindings })),
                &ZoneFacts::default(),
            )
        };
        let setup_of = |facts: MobilePaneFacts| match facts {
            MobilePaneFacts::Waste { setup, .. } => setup,
            other => panic!("expected the waste arm, got {other:?}"),
        };

        // Bindings never read on this device.
        assert_eq!(setup_of(facts_for(serde_json::Value::Null)), MobileWasteSetup::Unread);
        // A row holding something that is not text.
        assert_eq!(
            setup_of(facts_for(serde_json::json!([
                {"key": waste::BINDING_KEY, "value": {"state":"other"}}
            ]))),
            MobileWasteSetup::Unusable,
        );
        // No row at all — the one genuinely unbound arm.
        assert_eq!(setup_of(facts_for(serde_json::json!([]))), MobileWasteSetup::Unset);
    }

    #[test]
    fn weekend_facts_round_trip_facts_and_gap() {
        let inputs = pane_inputs(serde_json::json!({
            "nowMs": PANE_NOW_MS,
            "bindings": [],
            "calendarConnected": true,
            "calendarReads": {
                weekend::CALENDAR_REQUEST_KEY: {
                    "state": "read",
                    "events": [],
                    "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs":null},
                },
            },
        }));
        let zone = resolve_zone(&weekend::weekend_zone_queries(inputs.now_ms));

        let arm = mobile_pane_facts_of(StandingQuestion::Weekend, weekend::SUBJECT_KEY, &inputs, &zone);
        let MobilePaneFacts::Weekend { resolved: MobileWeekendResolved::Facts { facts } } = arm else {
            panic!("expected the weekend facts arm, got {arm:?}");
        };
        assert_eq!(facts.window.days.len(), 3);
        assert_eq!(facts.counts, MobileWeekendCounts { events: 0, due: 0, scheduled: 0 });

        // The shrink crosses the seam: asked on the Sunday of that same
        // weekend (`PANE_NOW_MS` is the Monday before, device-local), the
        // window carries Sunday alone. `MobileWeekendWindow.days` is a
        // `Vec` and nothing here re-imposes an arity, but this is what
        // says so — Android's card derives its day columns AND its plan
        // chips from exactly this list.
        const DAY_MS: i64 = 24 * 60 * 60 * 1000;
        let sunday_inputs = pane_inputs(serde_json::json!({
            "nowMs": PANE_NOW_MS + 6 * DAY_MS,
            "bindings": [],
            "calendarConnected": true,
            "calendarReads": {
                weekend::CALENDAR_REQUEST_KEY: {
                    "state": "read",
                    "events": [],
                    "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs":null},
                },
            },
        }));
        let sunday_zone = resolve_zone(&weekend::weekend_zone_queries(sunday_inputs.now_ms));
        let sunday_arm = mobile_pane_facts_of(
            StandingQuestion::Weekend,
            weekend::SUBJECT_KEY,
            &sunday_inputs,
            &sunday_zone,
        );
        let MobilePaneFacts::Weekend { resolved: MobileWeekendResolved::Facts { facts: sunday } } =
            sunday_arm
        else {
            panic!("expected the weekend facts arm on Sunday");
        };
        assert_eq!(
            sunday.window.days.iter().map(|day| day.date.as_str()).collect::<Vec<_>>(),
            vec!["2026-08-16"],
        );
        assert_eq!(
            sunday.days.iter().map(|day| day.date.as_str()).collect::<Vec<_>>(),
            vec!["2026-08-16"],
        );

        // THIS is the arm a device with no calendar connected still sees —
        // `calendar_connected` is real since #564, so it is now one of two
        // arms rather than the only one.
        let gap = mobile_pane_facts_of(
            StandingQuestion::Weekend,
            weekend::SUBJECT_KEY,
            &empty_pane_inputs(),
            &ZoneFacts::default(),
        );
        assert!(matches!(
            gap,
            MobilePaneFacts::Weekend {
                resolved: MobileWeekendResolved::Gap { gap: MobileWeekendGap::NotConnected }
            }
        ));
    }

    #[test]
    fn vacation_facts_round_trip_bound_and_unbound() {
        let inputs = pane_inputs(serde_json::json!({
            "nowMs": PANE_NOW_MS,
            "bindings": [
                {"key": vacation::TRIPS_CALENDAR_BINDING_KEY, "value": {"state":"text","text":"cal-1"}}
            ],
            "calendarConnected": true,
            "calendarReads": {
                vacation::CALENDAR_REQUEST_KEY: {
                    "state": "read",
                    "events": [],
                    "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs":null},
                },
            },
        }));
        let zone = resolve_zone(&vacation::vacation_zone_queries(&inputs));

        let arm = mobile_pane_facts_of(StandingQuestion::Vacation, vacation::SUBJECT_KEY, &inputs, &zone);
        let MobilePaneFacts::Vacation { resolved: Some(MobileVacationResolved::Facts { facts }) } = arm
        else {
            panic!("expected the bound vacation facts arm, got {arm:?}");
        };
        assert_eq!(facts.next, None);
        assert!(facts.later.is_empty());

        // Unbound (no calendar): no resolved facts at all — the `None` arm,
        // never a fabricated gap.
        let unbound = mobile_pane_facts_of(
            StandingQuestion::Vacation,
            vacation::SUBJECT_KEY,
            &empty_pane_inputs(),
            &ZoneFacts::default(),
        );
        assert!(matches!(unbound, MobilePaneFacts::Vacation { resolved: None }));
    }

    #[test]
    fn race_facts_round_trip_facts_and_sentinel_gap() {
        let race_start = PANE_NOW_MS + 6 * 24 * 3_600_000;
        let body = serde_json::json!({
            "events": [{
                "name": "Belgian Grand Prix",
                "locality": "Spa",
                "starts_at_ms": race_start,
                "sessions": [
                    {"kind": "practice", "label": "FP1", "starts_at_ms": race_start - 2 * 24 * 3_600_000}
                ],
            }],
        });
        let inputs = pane_inputs(serde_json::json!({
            "nowMs": PANE_NOW_MS,
            "bindings": [
                {"key": race::BINDING_KEY, "value": {"state":"text","text":"f1"}}
            ],
            "paneReads": {
                race::SOURCE: {"snapshots": [{
                    "key": "f1",
                    "envelope": {"kind":"ok","schema":race::SOURCE, "body": body.to_string()},
                    "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs":86400000},
                }]},
            },
        }));

        let arm = mobile_pane_facts_of(StandingQuestion::Race, "f1", &inputs, &ZoneFacts::default());
        let MobilePaneFacts::Race { setup, resolved: MobileRaceResolved::Facts { facts } } = arm
        else {
            panic!("expected the race facts arm, got {arm:?}");
        };
        assert_eq!(setup, MobileRaceSetup::Bound);
        assert_eq!(facts.series, "f1");
        assert_eq!(facts.event.as_ref().map(|event| event.name.as_str()), Some("Belgian Grand Prix"));
        // Friday practice is the next thing on track, not Sunday's race.
        assert_eq!(facts.next_start.as_ref().map(|next| next.label.as_str()), Some("FP1"));

        // The setup sentinel has no snapshot row: its own honest NotFetched.
        let sentinel = mobile_pane_facts_of(
            StandingQuestion::Race,
            race::SETUP_SUBJECT,
            &empty_pane_inputs(),
            &ZoneFacts::default(),
        );
        assert!(matches!(
            sentinel,
            MobilePaneFacts::Race {
                setup: MobileRaceSetup::Unset,
                resolved: MobileRaceResolved::Gap { gap: MobileRaceGap::NotFetched },
            }
        ));
    }

    #[test]
    fn kimi_facts_round_trip_facts_and_gap() {
        let inputs = pane_inputs(serde_json::json!({
            "nowMs": PANE_NOW_MS,
            "bindings": [],
            "paneReads": {
                kimi::SOURCE: {"snapshots": [{
                    "key": kimi::SNAPSHOT_KEY,
                    "envelope": {"kind":"ok","schema":kimi::SOURCE,
                                 "body":"{\"available_balance\":12.5,\"voucher_balance\":10.0,\"cash_balance\":2.5}"},
                    "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs":86400000},
                }]},
            },
        }));

        let arm = mobile_pane_facts_of(StandingQuestion::Kimi, kimi::SNAPSHOT_KEY, &inputs, &ZoneFacts::default());
        let MobilePaneFacts::Kimi { resolved: MobileKimiResolved::Facts { facts } } = arm else {
            panic!("expected the kimi facts arm, got {arm:?}");
        };
        assert_eq!(facts.available_balance, 12.5);
        assert!(!facts.stale);

        let gap = mobile_pane_facts_of(
            StandingQuestion::Kimi,
            kimi::SNAPSHOT_KEY,
            &empty_pane_inputs(),
            &ZoneFacts::default(),
        );
        assert!(matches!(
            gap,
            MobilePaneFacts::Kimi { resolved: MobileKimiResolved::Gap { gap: MobileKimiGap::NotFetched } }
        ));
    }

    #[test]
    fn github_facts_round_trip_view_and_sentinel_gap() {
        let inputs = pane_inputs(serde_json::json!({
            "nowMs": PANE_NOW_MS,
            "bindings": [],
            "paneReads": {
                github::SOURCE: {"snapshots": [{
                    "key": "deploy.yml",
                    "envelope": {"kind":"ok","schema":github::SOURCE,
                                 "body":"{\"display_name\":\"Deploy\",\"declared_cadence_ms\":null,\"last_run_conclusion\":\"success\",\"last_run_event\":\"push\",\"last_run_at_ms\":1786000000000,\"last_scheduled_success_at_ms\":null}"},
                    "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs":null},
                }]},
            },
        }));

        let arm = mobile_pane_facts_of(StandingQuestion::Github, "deploy.yml", &inputs, &ZoneFacts::default());
        let MobilePaneFacts::Github { resolved: MobileWorkflowResolved::View { view } } = arm else {
            panic!("expected the github view arm, got {arm:?}");
        };
        assert_eq!(view.body.display_name, "Deploy");
        assert_eq!(view.body.last_run_conclusion.as_deref(), Some("success"));

        let sentinel = mobile_pane_facts_of(
            StandingQuestion::Github,
            github::NEVER_POLLED_SUBJECT,
            &empty_pane_inputs(),
            &ZoneFacts::default(),
        );
        assert!(matches!(
            sentinel,
            MobilePaneFacts::Github {
                resolved: MobileWorkflowResolved::Gap { gap: MobileWorkflowGap::NotFetched }
            }
        ));
    }

    #[test]
    fn uptime_facts_are_the_subjects_own_never_the_first_snapshots() {
        let snapshot = |key: &str, expected: &str| {
            serde_json::json!({
                "key": key,
                "envelope": {"kind":"ok","schema":uptime::SOURCE,
                             "body": format!("{{\"expected\":\"{expected}\",\"expect_status\":200,\"observed_status\":200,\"error\":null}}")},
                "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs":300000},
            })
        };
        let inputs = pane_inputs(serde_json::json!({
            "nowMs": PANE_NOW_MS,
            "bindings": [],
            "paneReads": {
                uptime::SOURCE: {"snapshots": [snapshot("svc-a", "on"), snapshot("svc-b", "off")]},
            },
        }));

        for (subject, expected) in [("svc-a", MobileProbeExpected::On), ("svc-b", MobileProbeExpected::Off)] {
            let arm = mobile_pane_facts_of(StandingQuestion::Uptime, subject, &inputs, &ZoneFacts::default());
            let MobilePaneFacts::Uptime { resolved: MobileProbeResolved::Facts { facts } } = arm else {
                panic!("expected the uptime facts arm for {subject}, got {arm:?}");
            };
            assert_eq!(facts.service_id, subject);
            assert_eq!(facts.body.expected, expected);
        }

        let sentinel = mobile_pane_facts_of(
            StandingQuestion::Uptime,
            uptime::NEVER_POLLED_SUBJECT,
            &empty_pane_inputs(),
            &ZoneFacts::default(),
        );
        assert!(matches!(
            sentinel,
            MobilePaneFacts::Uptime {
                resolved: MobileProbeResolved::Gap { gap: MobileProbeGap::NotFetched }
            }
        ));
    }

    #[test]
    fn reachability_facts_round_trip_synced_and_never_synced() {
        let inputs = pane_inputs(serde_json::json!({
            "nowMs": PANE_NOW_MS,
            "bindings": [],
            "sync": {
                "latestOutcomeKind": "completed",
                "latestInformativeAtMs": PANE_NOW_MS - 60_000,
                "lastSuccessfulAtMs": PANE_NOW_MS - 60_000,
            },
        }));
        let arm = mobile_pane_facts_of(
            StandingQuestion::Reachability,
            reachability::SUBJECT_KEY,
            &inputs,
            &ZoneFacts::default(),
        );
        let MobilePaneFacts::Reachability { facts: Some(facts) } = arm else {
            panic!("expected reachability facts, got {arm:?}");
        };
        assert_eq!(facts.age_ms, 60_000);
        assert!(!facts.stale);
        assert!(facts.latest_attempt_landed);

        let never = mobile_pane_facts_of(
            StandingQuestion::Reachability,
            reachability::SUBJECT_KEY,
            &empty_pane_inputs(),
            &ZoneFacts::default(),
        );
        assert!(matches!(never, MobilePaneFacts::Reachability { facts: None }));
    }

    /// Host-level: every ranked record carries its OWN question's facts arm
    /// — on a fresh device, whose gaps double as the honest fresh-state
    /// answers (waste NotFetched, weekend NotConnected since
    /// `mobile_pane_inputs` hardcodes no calendar, vacation unbound → None).
    #[tokio::test]
    async fn every_ranked_pane_carries_its_own_questions_facts_arm() {
        let host = pane_host("panes-facts-arms").await;
        for surface in [MobileSurface::Now, MobileSurface::Status] {
            let ranked = host.rank_panes(surface, 1_000, Vec::new(), MobileSyncFacts::default()).await;
            assert!(!ranked.is_empty());
            for pane in ranked {
                let matches = match pane.standing_question {
                    MobileStandingQuestion::Homework => {
                        matches!(pane.facts, MobilePaneFacts::Homework { .. })
                    }
                    MobileStandingQuestion::Scps => matches!(pane.facts, MobilePaneFacts::Scps { .. }),
                    MobileStandingQuestion::Waste => matches!(pane.facts, MobilePaneFacts::Waste { .. }),
                    MobileStandingQuestion::Weekend => matches!(pane.facts, MobilePaneFacts::Weekend { .. }),
                    MobileStandingQuestion::Vacation => matches!(pane.facts, MobilePaneFacts::Vacation { .. }),
                    MobileStandingQuestion::Race => matches!(pane.facts, MobilePaneFacts::Race { .. }),
                    MobileStandingQuestion::Kimi => matches!(pane.facts, MobilePaneFacts::Kimi { .. }),
                    MobileStandingQuestion::Github => matches!(pane.facts, MobilePaneFacts::Github { .. }),
                    MobileStandingQuestion::Uptime => matches!(pane.facts, MobilePaneFacts::Uptime { .. }),
                    MobileStandingQuestion::Reachability => {
                        matches!(pane.facts, MobilePaneFacts::Reachability { .. })
                    }
                    MobileStandingQuestion::Poller => matches!(pane.facts, MobilePaneFacts::Poller { .. }),
                };
                assert!(matches, "{:?} carried a foreign facts arm: {:?}", pane.standing_question, pane.facts);
            }
        }
    }

    #[tokio::test]
    async fn a_fresh_devices_now_facts_are_the_honest_gaps() {
        let host = pane_host("panes-facts-fresh-now").await;
        let ranked = host.rank_panes(MobileSurface::Now, 1_000, Vec::new(), MobileSyncFacts::default()).await;
        let facts_of = |question: MobileStandingQuestion| {
            ranked.iter().find(|pane| pane.standing_question == question).unwrap().facts.clone()
        };
        assert!(matches!(
            facts_of(MobileStandingQuestion::Waste),
            MobilePaneFacts::Waste {
                setup: MobileWasteSetup::Unset,
                resolved: MobileWasteResolved::Gap { gap: MobileWasteGap::NotFetched },
            }
        ));
        assert!(matches!(
            facts_of(MobileStandingQuestion::Weekend),
            MobilePaneFacts::Weekend {
                resolved: MobileWeekendResolved::Gap { gap: MobileWeekendGap::NotConnected }
            }
        ));
        assert!(matches!(
            facts_of(MobileStandingQuestion::Vacation),
            MobilePaneFacts::Vacation { resolved: None }
        ));
    }
}

#[cfg(test)]
mod diagnostic_ffi_tests {
    use super::*;

    /// The whole wiring, end to end: `diagnostic_init_session` fixes the
    /// origin, then `diagnostic_event_json` mints a real, parseable
    /// `DiagnosticEventV1` line carrying that session id and `source:
    /// "android"`. A later `diagnostic_init_session` call is a no-op —
    /// `DIAGNOSTIC_SESSION` is a process-wide `static`, shared with every
    /// other test in this binary, so this only pins that the *first* id
    /// this process ever sets is the one that sticks (never asserts an
    /// exact `seq`, since other tests in this file may run first and share
    /// the same counter).
    #[test]
    fn diagnostic_event_json_carries_the_session_and_android_source() {
        // `diagnostic_init_session` is idempotent, and `DIAGNOSTIC_SESSION`
        // is one process-wide `static` shared with every other test in
        // this binary — this call may or may not be the one that actually
        // wins, so this asserts the *shape* (a non-empty id, whatever it
        // is), never a literal one specific test happened to pass in.
        diagnostic_init_session("some-session".to_string(), 0);
        let json = diagnostic_event_json(1_700_000_000_000, 0, MobileDiagnosticEvent::PushReceived);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value["session_id"].as_str().is_some_and(|id| !id.is_empty()));
        assert_eq!(value["source"], "android");
        assert_eq!(value["event"]["name"], "push.received");
    }

    /// Review round 1, finding 2: `core_lock`'s mutex/operation spans and
    /// `diagnostic_event_json`'s own worker/push/network mints must share
    /// **one** `seq` counter — two independent 0-based counters under one
    /// `session_id`, both landing in one exported journal, would mint
    /// colliding values with no total order, which is exactly what would
    /// defeat #712's ability to interleave a `core.wait_started` against
    /// the `worker.started` of the run it was contending with.
    ///
    /// This interleaves both families from one thread — `diagnostic_event_json`
    /// (the `session.started`/`worker.*`/`push.received`/`network.changed`
    /// path) around a real `MobileTaskHost::capture` (the `core_lock` path,
    /// via `take_diagnostic_events`) — and asserts every `seq` value seen,
    /// from both families together, is distinct and increases in call
    /// order. `DIAGNOSTIC_SESSION.seq` is a process-wide `static` shared
    /// with every other test in this binary, so this cannot assert
    /// *contiguous* values (another thread may interleave its own), only
    /// that these two families, from this one thread, never collide or
    /// go backwards against each other.
    #[tokio::test]
    async fn core_lock_events_and_diagnostic_event_json_share_one_seq_counter_with_no_collision() {
        diagnostic_init_session("shared-seq-session".to_string(), 0);

        let before = diagnostic_event_json(0, 0, MobileDiagnosticEvent::PushReceived);
        let before_seq = seq_of(&before);

        let dir = tempfile::tempdir().unwrap();
        let host = MobileTaskHost::init(
            dir.path().join("seq-ns").to_str().unwrap().to_string(),
            "https://authority.example".to_string(),
            String::new(),
        )
        .await
        .unwrap();
        let draft = CaptureDraft {
            title: "seq collision check".to_string(),
            destination: CaptureDestination::Triage,
            size: String::new(),
            energy: String::new(),
            context: String::new(),
            description: String::new(),
            project_id: String::new(),
            priority: String::new(),
            deadline: String::new(),
            scheduled_date: String::new(),
        };
        host.capture(draft, 1_000).await.unwrap();
        let core_lock_seqs: Vec<u64> = host
            .take_diagnostic_events()
            .await
            .iter()
            .map(|line| seq_of(&line.json))
            .collect();

        let after = diagnostic_event_json(0, 0, MobileDiagnosticEvent::PushReceived);
        let after_seq = seq_of(&after);

        assert!(!core_lock_seqs.is_empty(), "capture must have produced at least one core_lock event");
        // Every `core_lock` seq must fall strictly between the two
        // `diagnostic_event_json` calls that bracket it in real call
        // order, and none may repeat a value either side already used —
        // the two families sharing one counter, proven by interleaving.
        for &seq in &core_lock_seqs {
            assert!(seq > before_seq, "a core_lock seq ({seq}) must exceed the bracketing before-seq ({before_seq})");
            assert!(seq < after_seq, "a core_lock seq ({seq}) must precede the bracketing after-seq ({after_seq})");
        }
        let mut all_seqs = core_lock_seqs.clone();
        all_seqs.push(before_seq);
        all_seqs.push(after_seq);
        let distinct: std::collections::HashSet<u64> = all_seqs.iter().copied().collect();
        assert_eq!(distinct.len(), all_seqs.len(), "no two events from either family may share a seq: {all_seqs:?}");
    }

    fn seq_of(json: &str) -> u64 {
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        value["seq"].as_u64().unwrap()
    }

    /// Review round 1, finding 6 (numbered 1 in the coordinator's own
    /// list): the mechanism the Kotlin-side fix depends on —
    /// `take_diagnostic_events` must never depend on `inner`'s lock, so a
    /// drain succeeds even while something else holds `inner` forever
    /// (#704's own scenario: a sync stuck inside a hung network await,
    /// with the lock held). This directly holds the real `inner` field
    /// (the same private-field access this file's own pre-existing tests
    /// already use, e.g. `host.inner.lock().await` at several points
    /// above) to stand in for that hang, then proves the drain still
    /// completes and still carries the spans a prior capture buffered.
    #[tokio::test]
    async fn take_diagnostic_events_drains_even_while_something_else_holds_inner_forever() {
        let dir = tempfile::tempdir().unwrap();
        let host = Arc::new(
            MobileTaskHost::init(
                dir.path().join("drain-during-hang-ns").to_str().unwrap().to_string(),
                "https://authority.example".to_string(),
                String::new(),
            )
            .await
            .unwrap(),
        );

        let draft = CaptureDraft {
            title: "buffer something before the hang".to_string(),
            destination: CaptureDestination::Triage,
            size: String::new(),
            energy: String::new(),
            context: String::new(),
            description: String::new(),
            project_id: String::new(),
            priority: String::new(),
            deadline: String::new(),
            scheduled_date: String::new(),
        };
        host.capture(draft, 1_000).await.unwrap();

        // Stands in for #704's own incident: something holds `inner`
        // forever (a hung network await inside a real `Core::run`, in
        // production; here, directly, since this test's whole point is
        // the *lock*, not what happens to be awaited while it is held).
        let holder = host.clone();
        let hold_task = tokio::spawn(async move {
            let _guard = holder.inner.lock().await;
            std::future::pending::<()>().await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let lines = tokio::time::timeout(std::time::Duration::from_secs(2), host.take_diagnostic_events())
            .await
            .expect("take_diagnostic_events must not hang while something else holds inner");

        assert!(!lines.is_empty(), "the capture's own buffered spans must still be there");
        assert!(
            lines.iter().any(|line| line.json.contains("core.wait_started") || line.json.contains("core.acquired")),
            "must contain the capture's own core.* spans: {lines:?}",
        );

        hold_task.abort();
    }

    #[test]
    fn worker_finished_success_maps_to_the_success_outcome() {
        diagnostic_init_session("some-session".to_string(), 0);
        let json = diagnostic_event_json(
            0,
            0,
            MobileDiagnosticEvent::WorkerFinished {
                trigger: MobileWorkerTrigger::Timer,
                attempt_count: 1,
                success: true,
            },
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["event"]["payload"]["outcome"], "success");
        assert_eq!(value["event"]["payload"]["trigger"], "timer");
        assert_eq!(value["event"]["payload"]["attempt_count"], 1);
    }

    /// #710's `worker.started` carries the attempt count WorkManager itself
    /// reports — the field this whole slice exists to make visible, since
    /// it is only available inside the worker (`runAttemptCount`).
    #[test]
    fn worker_started_carries_trigger_and_attempt_count() {
        diagnostic_init_session("some-session".to_string(), 0);
        let json = diagnostic_event_json(
            0,
            0,
            MobileDiagnosticEvent::WorkerStarted {
                trigger: MobileWorkerTrigger::Push,
                attempt_count: 3,
            },
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["event"]["name"], "worker.started");
        assert_eq!(value["event"]["payload"]["trigger"], "push");
        assert_eq!(value["event"]["payload"]["attempt_count"], 3);
    }

    /// #710's `network.changed`: no IP address or SSID field exists on the
    /// mirror at all (there is nothing here to redact — the shape itself
    /// carries neither), and every capability bit crosses.
    #[test]
    fn network_changed_carries_transport_and_capability_bits_with_no_address() {
        diagnostic_init_session("some-session".to_string(), 0);
        let json = diagnostic_event_json(
            0,
            0,
            MobileDiagnosticEvent::NetworkChanged {
                online: true,
                transport: MobileNetworkTransport::Cellular,
                internet_capable: true,
                validated: false,
                metered: true,
                roaming: true,
            },
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["event"]["name"], "network.changed");
        let payload = &value["event"]["payload"];
        assert_eq!(payload["online"], true);
        assert_eq!(payload["transport"], "cellular");
        assert_eq!(payload["internet_capable"], true);
        assert_eq!(payload["validated"], false);
        assert_eq!(payload["metered"], true);
        assert_eq!(payload["roaming"], true);
        assert!(payload.get("ip").is_none());
        assert!(payload.get("ip_address").is_none());
        assert!(payload.get("ssid").is_none());
    }

    /// Review round 1, finding: an earlier version of `diagnostic_init_session`
    /// stored the id in a `OnceLock` but the origin in a plain
    /// `AtomicU64::store`, so a *second* call — with a different id and a
    /// wildly different origin — silently moved the origin every time it
    /// ran, even though the doc claimed otherwise. `DIAGNOSTIC_SESSION` is
    /// one process-wide `static` shared with every other test in this
    /// binary, so this cannot assert which caller's id "won" the race to
    /// go first — only that *whichever one did* is what every later call
    /// still sees: two calls back to back, with different ids and wildly
    /// different origins, produce two events with the identical
    /// `session_id` and `elapsed_ms`.
    #[test]
    fn diagnostic_init_session_is_idempotent_a_later_call_cannot_move_the_origin() {
        diagnostic_init_session("first-caller".to_string(), 111);
        let first = diagnostic_event_json(0, 500, MobileDiagnosticEvent::PushReceived);
        diagnostic_init_session("second-caller".to_string(), 999_999);
        let second = diagnostic_event_json(0, 500, MobileDiagnosticEvent::PushReceived);

        let first: serde_json::Value = serde_json::from_str(&first).unwrap();
        let second: serde_json::Value = serde_json::from_str(&second).unwrap();
        assert_eq!(first["session_id"], second["session_id"]);
        assert_eq!(first["elapsed_ms"], second["elapsed_ms"]);
    }
}
