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

use hummingbird_core::bindings::{Binding, BindingKey};
use hummingbird_core::sync::queue::{DeadLetterEntry, DeadLetterReason, MutationIntent};
use hummingbird_core::sync::write::ReqwestMutationTransport;
use hummingbird_core::sync::{ReqwestSyncTransport, Trigger};
use hummingbird_core::freshness::Freshness;
use hummingbird_core::pane::PaneSnapshot;
use hummingbird_core::{
    ActError, CaptureOptions, Core, CoreCycleOutcome, CoreEvent, CoreInitError, ItemAction,
    TriageDestination, TriagePatch,
};
use hummingbird_domain::{
    core_field_type, is_valid_deadline, Alert, Condition, Energy, EventKindEntry, FieldType, Item,
    Project, Rule, Size, Stage, Tier, CORE_FIELDS, EVENT_KINDS,
};

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

/// Every field of an item a triage request may edit beyond `destination`,
/// as the JS side sends them — one JSON object, camelCased, deserialized
/// here rather than spread across a positional argument list that would now
/// run to thirteen scalars.
///
/// **Absent, null and a value are three different instructions**, and the
/// double-`Option` on each nullable field is what keeps them apart:
/// a key the JS object never set is `None` ("leave this field alone"), an
/// explicit `null` is `Some(None)` ("clear it"), and a value is
/// `Some(Some(v))`. That is [`hummingbird_domain::ItemPatch`]'s own wire
/// contract, mirrored here so an editor can remove a value it can set.
/// `title` and `priority` are `NOT NULL` columns: single-`Option`, so a
/// `null` on either is a deserialization error, not a silent clear.
///
/// `deny_unknown_fields` on purpose: a misspelled key that deserialized to
/// "leave that field alone" would silently drop an edit the person made.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TriageEdits {
    #[serde(default, deserialize_with = "non_null_title")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "touched")]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "touched")]
    pub project_id: Option<Option<String>>,
    /// The wire's snake_case size name (`hummingbird_domain::Size::parse`);
    /// resolved by name through the vocabulary, never a raw index or a
    /// hardcoded id.
    #[serde(default, deserialize_with = "touched")]
    pub size: Option<Option<String>>,
    /// Same "resolved by name" contract as `size`
    /// (`hummingbird_domain::Energy::parse`).
    #[serde(default, deserialize_with = "touched")]
    pub energy: Option<Option<String>>,
    #[serde(default, deserialize_with = "touched")]
    pub context: Option<Option<String>>,
    #[serde(default, deserialize_with = "non_null_priority")]
    pub priority: Option<i64>,
    /// `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`, checked with
    /// `hummingbird_domain::is_valid_deadline` before the seam — the same
    /// function the authority validates with, so the two cannot disagree.
    #[serde(default, deserialize_with = "touched")]
    pub deadline: Option<Option<String>>,
    /// A whole civil day (`YYYY-MM-DD`) and never a date-time: a scheduled
    /// date is the do-date a human chose, which has no minute. #122's
    /// three-state do-date edit — `TriagePatch::scheduled_date`'s own shape,
    /// unchanged across this seam: outer `None` leaves it alone,
    /// `Some(None)` clears it, `Some(Some(date))` sets it.
    #[serde(default, deserialize_with = "touched")]
    pub scheduled_date: Option<Option<String>>,
}

/// Distinguishes "key absent" from "key present, value null" for a
/// double-`Option` field. Without it serde folds both onto `None` and a
/// clear becomes a no-op; the same shim the authority's own patch bodies use
/// (`hummingbird_domain::api`).
fn touched<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    <Option<T> as serde::Deserialize>::deserialize(deserializer).map(Some)
}

/// `NOT NULL` columns cannot be cleared: an explicit JSON `null` on one is a
/// deserialize error, not a silent skip — read as "leave this field alone" it
/// would swallow an edit whose intent was impossible. Named shims because
/// `deserialize_with` needs a path; the same pair the authority's own patch
/// bodies carry (`hummingbird_domain::api`).
fn non_null_title<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    non_null(d, "title")
}

fn non_null_priority<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    non_null(d, "priority")
}

fn non_null<'de, T, D>(deserializer: D, field: &'static str) -> Result<Option<T>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    match <Option<T> as serde::Deserialize>::deserialize(deserializer)? {
        Some(value) => Ok(Some(value)),
        None => Err(serde::de::Error::custom(format!("{field} may not be null"))),
    }
}

/// One edit field rejected before it could reach [`Core::triage`] — the
/// message is what the host shows, so it names the field and what was wrong
/// with it rather than just failing.
fn reject(message: String) -> TriageResponse {
    TriageResponse { kind: "failed", error: Some(message) }
}

/// Resolves a touched size/energy field by name: `None` stays untouched,
/// `Some(None)` stays a clear, and a value is looked up in the vocabulary —
/// an unrecognised name is an `Err` the caller turns into a rejection,
/// never a silent clear.
fn parse_named<T>(
    field: &str,
    raw: Option<Option<String>>,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Option<Option<T>>, String> {
    match raw {
        None => Ok(None),
        Some(None) => Ok(Some(None)),
        Some(Some(value)) => match parse(&value) {
            Some(parsed) => Ok(Some(Some(parsed))),
            None => Err(format!("unrecognised {field} {value:?}")),
        },
    }
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

/// One Ledger row (`Core::ledger`): the item's fields flat at the top level
/// exactly like [`FrontierItemDTO`] (same `#[serde(flatten)]` reasoning),
/// plus the row's derivable facts — `pending` stamped through the same
/// single site as every other item read ([`TaskHostCore::with_pending`]),
/// the mirror's retention stamp, and the two badges. Never a history
/// record: the ledger derives, it does not record.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LedgerRowDTO {
    #[serde(flatten)]
    pub item: Item,
    pub pending: bool,
    /// `Some` when the mirror retains this row as absent (archived, or
    /// missing from a complete sweep) — the label the Ledger shows instead
    /// of hiding the row.
    pub absent_since_ms: Option<i64>,
    /// A dead-lettered edit targets this item (device-local — the journal
    /// never syncs).
    pub dead_lettered: bool,
    /// A live alert names this item (`source_key == "item:<id>"`).
    pub has_live_alert: bool,
}

/// The wrapper around [`TaskHostCore::ledger`]'s answer. Same `"busy"`
/// contract as [`ItemListResponse`], and load-bearing for the same reason as
/// the pane read: an empty ledger renders as "nothing has ever been
/// tracked", a claim a core that has not loaded may not make.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LedgerListResponse {
    pub kind: &'static str,
    pub rows: Vec<LedgerRowDTO>,
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
/// `fields` and no `message`; `"contention"` (#163) carries neither — a
/// genuinely disjoint second 409 has no colliding field name to show, only
/// repeated churn.
///
/// `entity`/`entity_id` are what the abandoned change was *about*
/// ([`MutationIntent::subject`]) — the journal's own `id` is the queue
/// entry's, which names the attempt rather than the thing, so this affordance
/// could previously say a write had been abandoned without saying whose.
/// `entity_id` is `None` only when the intent genuinely named no single row.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DeadLetterEntryDTO {
    pub id: String,
    pub reason: &'static str,
    pub message: Option<String>,
    pub fields: Vec<DeadLetterFieldDTO>,
    pub at_ms: i64,
    pub entity: String,
    pub entity_id: Option<String>,
}

/// The wrapper around [`TaskHostCore::dead_letters`]'s answer.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DeadLettersResponse {
    pub kind: &'static str,
    pub entries: Vec<DeadLetterEntryDTO>,
}

/// The wrapper around [`TaskHostCore::snapshot_freshness`]'s answer
/// (ADR-0015). `freshness` is [`Freshness`]'s own serde shape —
/// `{"state":"unknown"}` or `{"state":"age","age_ms":…,
/// "declared_cadence_ms":…|null}` — deliberately re-exported rather than
/// flattened into nullable fields here: a flattened `age_ms: null` is the
/// boolean collapse ADR-0015 rejected, arriving in another costume.
///
/// The subtraction happens core-side, so what crosses is the finished
/// answer. TS supplies only the threshold, per pane, because the driver is
/// the cost of a wrong answer and not the cadence.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FreshnessResponse {
    pub kind: &'static str,
    pub freshness: Freshness,
}

/// The wrapper around [`TaskHostCore::pane_read`]'s answer (#245,
/// ADR-0015) — one source's snapshot rows and its live alerts, the generic
/// read every standing question's pane starts from.
///
/// `snapshots` carries [`PaneSnapshot`]'s own serde shape (`freshness` as
/// the tagged union above; `envelope` as `{"state":"parsed",…}` /
/// `{"state":"malformed","reason":…}`, whose `body` is a **string
/// containing JSON** — opaque all the way across, parsed only by the pane
/// that owns the shape). `alerts` carries raw
/// [`hummingbird_domain::Alert`] rows, `subject_key` included: the
/// `(source, subject_key)` ↔ `(source, key)` join is additive and belongs
/// to the pane, in TS.
///
/// Same `"busy"` contract as [`ItemListResponse`], and it matters more here
/// than most: an empty pane read renders as "nothing is due", so a busy
/// core answering `[]` would tell the operator a *fact* it has not read.
/// `lib.rs`'s `BUSY_PANE_READ` is dropped by the host rather than stored.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PaneReadResponse {
    pub kind: &'static str,
    pub snapshots: Vec<PaneSnapshot>,
    pub alerts: Vec<Alert>,
}

/// The wrapper around [`TaskHostCore::bindings`]'s answer (#118) — every
/// standing-question binding, known-first. Same `"busy"` contract as
/// [`ItemListResponse`]: no answer, never an empty one, since an editor
/// rendering "nothing is bound" from a busy core would invite the operator
/// to overwrite values it simply had not read yet.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BindingListResponse {
    pub kind: &'static str,
    pub bindings: Vec<Binding>,
}

/// What [`TaskHostCore::set_binding`] resolves to. `"unknown_key"` is
/// distinct from `"failed"` on purpose: it is the seam rejecting a key that
/// is not in ADR-0015's closed vocabulary — a caller mistake, and the one
/// outcome that never reaches `Core` at all — while `"failed"` is a
/// durability failure enqueueing the write. Same shape as [`ActResponse`]'s
/// three-way split.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SetBindingResponse {
    pub kind: &'static str,
    pub error: Option<String>,
}

// -- #140: rules --------------------------------------------------------

/// The wrapper around [`TaskHostCore::rules`]'s answer. Same `"busy"`
/// contract as [`ItemListResponse`]: no answer, never an empty one — a
/// busy core answering `[]` would read as "no rules exist" rather than
/// "not read yet."
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RuleListResponse {
    pub kind: &'static str,
    pub rules: Vec<Rule>,
}

/// What [`TaskHostCore::create_rule`] resolves to. `"failed"` is a
/// durability failure enqueueing the create; a rejected-at-save condition
/// (an unknown field, an illegal operator, a malformed duration — #133's
/// `validate_rule`) is discovered later, at drain time, and surfaces
/// through the ordinary dead-letter journal, since it is a 400 the
/// authority returns from an async send this call has already returned
/// from.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CreateRuleResponse {
    pub kind: &'static str,
    pub id: Option<String>,
    pub error: Option<String>,
}

/// What [`TaskHostCore::patch_rule`] resolves to (the enable/disable
/// toggle and every other rule edit share this one entry point).
/// `"failed"` is a durability failure enqueueing the write; a 409 is
/// **handled, not swallowed** — [`Core::patch_rule`] enqueues it through
/// the ordinary CAS path (`patch_with_rebase`), so it either auto-rebases
/// or lands in the dead-letter journal like any other conflicted write,
/// never silently lost.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PatchRuleResponse {
    pub kind: &'static str,
    pub error: Option<String>,
}

/// One field the kind registry declares, at the wire — [`FieldDescriptor`]
/// plus a `name` the JSON export already carries as `"name"`, so this
/// exists only to give the Event core's eight fields (which have no
/// [`hummingbird_domain::FieldDescriptor`] of their own — they are a bare
/// `&'static [&'static str]`) the identical shape, so #140's UI reads one
/// list either way rather than special-casing the core.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CoreFieldDTO {
    pub name: &'static str,
    pub field_type: FieldType,
}

/// The kind registry export (#133/#140, ADR-0013): the exact
/// [`EVENT_KINDS`] definition the rule engine evaluates against, serialized
/// for the rules screen's cascading kind → field → operator → value
/// editor. Adding a kind to [`EVENT_KINDS`] changes this response with no
/// UI-side change — the whole reason ADR-0013 calls the registry "a
/// `domain` artifact."
///
/// `alarm_interval_ms` is **duplicated** from
/// `hummingbird_authority::sweep::ALARM_INTERVAL_MS` (#138) rather than
/// imported: `server/authority` is a native-only crate (rusqlite, `std::fs`
/// fixtures) with no business in this crate's `wasm32-unknown-unknown`
/// dependency graph, the same reasoning that keeps `server/city-waste` out
/// of the worker's build (see this repo's `CLAUDE.md`). If the DO alarm
/// cadence ever changes, this constant must change with it — there is no
/// mechanical guard against that drift today.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KindRegistryResponse {
    pub kind: &'static str,
    pub kinds: &'static [EventKindEntry],
    pub core_fields: Vec<CoreFieldDTO>,
    pub alarm_interval_ms: i64,
    /// `hummingbird_domain::SEVERITIES`, verbatim and in its own order — the
    /// exact vocabulary `severity_rank` (`server/domain/src/severity.rs`)
    /// ranks against, so the rules screen's severity dropdown can never
    /// disagree with the ADR-0014 ratchet. No import problem the
    /// `alarm_interval_ms` doc above warns about: `hummingbird_domain` is
    /// already this crate's dependency, unlike `hummingbird_authority`.
    pub severities: &'static [&'static str],
}

/// Mirrors `hummingbird_authority::sweep::ALARM_INTERVAL_MS` — see
/// [`KindRegistryResponse`]'s own doc for why this is a duplicate constant
/// rather than a dependency.
pub const ALARM_INTERVAL_MS: i64 = 15 * 60 * 1000;

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
    // Derived in the core, where it is natively tested, and carried onto
    // every variant below rather than into one of them: what a change was
    // about is the same fact however the change died.
    let subject = entry.entry.intent.subject();
    match &entry.reason {
        DeadLetterReason::Permanent(message) => DeadLetterEntryDTO {
            id: entry.entry.id.clone(),
            reason: "permanent",
            message: Some(message.clone()),
            fields: Vec::new(),
            at_ms: entry.at_ms,
            entity: subject.entity,
            entity_id: subject.id,
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
                entity: subject.entity,
                entity_id: subject.id,
            }
        }
        DeadLetterReason::Contention { .. } => DeadLetterEntryDTO {
            id: entry.entry.id.clone(),
            reason: "contention",
            message: None,
            fields: Vec::new(),
            at_ms: entry.at_ms,
            entity: subject.entity,
            entity_id: subject.id,
        },
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

    /// Issue #196 (shape 2): the rehydration counterpart to
    /// [`TaskHostCore::push_api_key`] — the host reloading a token it
    /// already had stored (core start, or a later view under #126's shared
    /// core reaching `ready`), never resuming a hold. See
    /// [`Core::rehydrate_api_key`].
    pub fn rehydrate_api_key(&mut self, api_key: String) {
        self.core.rehydrate_api_key(api_key);
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

    /// The complete retained roster, per [`Core::ledger`] — every item this
    /// device's mirror has ever known, live, Done and archived alike, with
    /// each row's derivable facts. `now_ms` is host-supplied, like every
    /// other clock read crossing this seam, and resolves only alert
    /// liveness. Same per-item `pending` stamp as [`TaskHostCore::frontier`].
    pub fn ledger(&self, now_ms: i64) -> LedgerListResponse {
        LedgerListResponse {
            kind: "ok",
            rows: self
                .core
                .ledger(now_ms)
                .into_iter()
                .map(|entry| {
                    let pending = self.core.is_pending(&entry.item.id);
                    LedgerRowDTO {
                        pending,
                        absent_since_ms: entry.absent_since_ms,
                        dead_lettered: entry.dead_lettered,
                        has_live_alert: entry.has_live_alert,
                        item: entry.item,
                    }
                })
                .collect(),
        }
    }

    /// Every live `Done` item, per [`Core::done`] — the Done screen's read.
    /// Same per-item `pending` stamp as [`TaskHostCore::frontier`], through
    /// the same single site.
    pub fn done(&self) -> ItemListResponse {
        ItemListResponse {
            kind: "ok",
            items: self
                .core
                .done()
                .into_iter()
                .map(|item| self.with_pending(item))
                .collect(),
        }
    }

    /// How old this device's answer to one standing question is, per
    /// [`Core::snapshot_freshness`] (ADR-0015). `now_ms` is host-supplied,
    /// like every other clock read that crosses this seam.
    ///
    /// No row is `{"state":"unknown"}`, which `Freshness` guarantees can
    /// never be read as fresh — including by the shim's busy answer, which
    /// is the same `unknown` rather than a zero age (`lib.rs`'s
    /// `BUSY_FRESHNESS`). A core that is still loading has not measured
    /// anything.
    pub fn snapshot_freshness(&self, source: &str, key: &str, now_ms: i64) -> FreshnessResponse {
        FreshnessResponse {
            kind: "ok",
            freshness: self.core.snapshot_freshness(source, key, now_ms),
        }
    }

    /// One source's whole pane-facing read, per [`Core::pane_read`] (#245).
    /// `now_ms` is host-supplied, like every other clock read crossing this
    /// seam, and it decides two things at once: each row's measured age and
    /// which alerts are still live (ADR-0014's predicate, applied core-side
    /// so no pane can re-spell it).
    pub fn pane_read(&self, source: &str, now_ms: i64) -> PaneReadResponse {
        let read = self.core.pane_read(source, now_ms);
        PaneReadResponse {
            kind: "ok",
            snapshots: read.snapshots,
            alerts: read.alerts,
        }
    }

    /// One item's Steps, per [`Core::steps_for`] — item detail (issue #96).
    pub fn steps(&self, item_id: &str) -> StepListResponse {
        StepListResponse {
            kind: "ok",
            steps: self.core.steps_for(item_id),
        }
    }

    /// Every standing-question binding, per [`Core::bindings`] (#118).
    pub fn bindings(&self) -> BindingListResponse {
        BindingListResponse {
            kind: "ok",
            bindings: self.core.bindings(),
        }
    }

    /// Sets one binding (#118). `key` is the wire's kebab-case binding name,
    /// resolved through [`BindingKey::parse`]; an unrecognised one fails
    /// without ever touching [`Core::set_binding`] — the same "reject before
    /// the seam" discipline [`TaskHostCore::capture`]/[`TaskHostCore::act`]
    /// apply to their own vocabularies, and load-bearing here for a second
    /// reason: `settings` has no DELETE, so a key minted by mistake can
    /// never be taken back out of the table.
    pub async fn set_binding(
        &mut self,
        seed: &str,
        key: &str,
        value: &str,
        now_ms: i64,
    ) -> SetBindingResponse {
        let Some(key) = BindingKey::parse(key) else {
            return SetBindingResponse {
                kind: "unknown_key",
                error: Some(format!("unrecognised binding key {key:?}")),
            };
        };
        match self.core.set_binding(seed, key, value, now_ms).await {
            Ok(()) => SetBindingResponse { kind: "ok", error: None },
            Err(error) => SetBindingResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
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

    /// Every rule, per [`Core::rules`] (#140).
    pub fn rules(&self) -> RuleListResponse {
        RuleListResponse {
            kind: "ok",
            rules: self.core.rules(),
        }
    }

    /// The kind registry export (#133/#140, ADR-0013) — the field/operator
    /// catalogue the rules editor cascades through. An associated function,
    /// not a method: it needs no `Core` state at all, only a read of
    /// [`EVENT_KINDS`]/[`CORE_FIELDS`], so unlike every other answer here it
    /// never answers `"busy"` and needs no checked-out host to call.
    pub fn kind_registry() -> KindRegistryResponse {
        KindRegistryResponse {
            kind: "ok",
            kinds: EVENT_KINDS,
            core_fields: CORE_FIELDS
                .iter()
                .filter_map(|&name| {
                    core_field_type(name).map(|field_type| CoreFieldDTO { name, field_type })
                })
                .collect(),
            alarm_interval_ms: ALARM_INTERVAL_MS,
            severities: &hummingbird_domain::SEVERITIES[..],
        }
    }

    /// Creates a rule, per [`Core::create_rule`] (#140). `tier` is the
    /// wire's snake_case name (`"urgent"`/`"normal"`), resolved through
    /// [`Tier::parse`] before it can reach `Core` — the same "reject before
    /// the seam" discipline every other wire vocabulary here follows.
    /// `conditions` arrives as a JSON array string (`Vec<Condition>`'s own
    /// serde shape) rather than individual scalar arguments, since a
    /// condition list is open-ended and `wasm_bindgen` has no tuple-list
    /// argument shape.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_rule(
        &mut self,
        seed: &str,
        name: &str,
        event_kind: Option<String>,
        conditions_json: &str,
        severity: &str,
        tier: &str,
        enabled: bool,
        now_ms: i64,
    ) -> CreateRuleResponse {
        let Some(tier) = Tier::parse(tier) else {
            return CreateRuleResponse {
                kind: "failed",
                id: None,
                error: Some(format!("unrecognised tier {tier:?}")),
            };
        };
        let conditions: Vec<Condition> = match serde_json::from_str(conditions_json) {
            Ok(conditions) => conditions,
            Err(error) => {
                return CreateRuleResponse {
                    kind: "failed",
                    id: None,
                    error: Some(format!("malformed conditions: {error}")),
                }
            }
        };
        match self
            .core
            .create_rule(seed, name, event_kind, conditions, severity, tier, enabled, now_ms)
            .await
        {
            Ok(id) => CreateRuleResponse {
                kind: "ok",
                id: Some(id),
                error: None,
            },
            Err(error) => CreateRuleResponse {
                kind: "failed",
                id: None,
                error: Some(error.to_string()),
            },
        }
    }

    /// Patches a rule, per [`Core::patch_rule`] (#140) — the enable/disable
    /// toggle and every other rule edit. `current` is the caller's own
    /// last-known copy of the row (from [`TaskHostCore::rules`]), so this
    /// method never re-reads the mirror for it — the same "caller supplies
    /// `base`" contract every other CAS write here follows.
    /// `event_kind`/`conditions`/`severity`/`tier`/`enabled` are each
    /// `None` to mean "leave this field alone." `tier`, when present, is
    /// the wire's snake_case name, resolved through [`Tier::parse`] before
    /// it can reach `Core`.
    #[allow(clippy::too_many_arguments)]
    pub async fn patch_rule(
        &mut self,
        seed: &str,
        current: &Rule,
        name: Option<String>,
        // `Some(None)` clears `event_kind` to "any kind"; `None` leaves it
        // untouched — the same double-`Option` `RulePatch` itself carries.
        event_kind_touched: bool,
        event_kind: Option<String>,
        conditions_json: Option<String>,
        severity: Option<String>,
        tier: Option<String>,
        enabled: Option<bool>,
        now_ms: i64,
    ) -> PatchRuleResponse {
        let tier = match tier {
            Some(tier) => match Tier::parse(&tier) {
                Some(tier) => Some(tier),
                None => {
                    return PatchRuleResponse {
                        kind: "failed",
                        error: Some(format!("unrecognised tier {tier:?}")),
                    }
                }
            },
            None => None,
        };
        let conditions: Option<Vec<Condition>> = match conditions_json {
            Some(json) => match serde_json::from_str(&json) {
                Ok(conditions) => Some(conditions),
                Err(error) => {
                    return PatchRuleResponse {
                        kind: "failed",
                        error: Some(format!("malformed conditions: {error}")),
                    }
                }
            },
            None => None,
        };
        let event_kind = event_kind_touched.then_some(event_kind);
        match self
            .core
            .patch_rule(seed, current, name, event_kind, conditions, severity, tier, enabled, now_ms)
            .await
        {
            Ok(()) => PatchRuleResponse { kind: "ok", error: None },
            Err(error) => PatchRuleResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
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
    ///
    /// `size`/`energy` (#208) are each resolved by name through
    /// `hummingbird_domain`'s own vocabulary (`Size::parse`/`Energy::parse`),
    /// the same "reject before the seam" discipline
    /// [`TaskHostCore::triage`] already applies to its own `size`/`energy`
    /// edits — an unrecognised name fails here and never reaches
    /// [`Core::capture`]. `context` carries straight through unparsed, same
    /// as `TriageEdits::context`.
    #[allow(clippy::too_many_arguments)]
    pub async fn capture(
        &mut self,
        seed: &str,
        title: &str,
        stage: &str,
        size: Option<String>,
        energy: Option<String>,
        context: Option<String>,
        now_ms: i64,
    ) -> CaptureResponse {
        let Some(stage) = Stage::parse(stage) else {
            return CaptureResponse {
                kind: "failed",
                id: None,
                error: Some(format!("unrecognised stage {stage:?}")),
            };
        };
        let size = match size {
            Some(raw) => match Size::parse(&raw) {
                Some(size) => Some(size),
                None => {
                    return CaptureResponse {
                        kind: "failed",
                        id: None,
                        error: Some(format!("unrecognised size {raw:?}")),
                    };
                }
            },
            None => None,
        };
        let energy = match energy {
            Some(raw) => match Energy::parse(&raw) {
                Some(energy) => Some(energy),
                None => {
                    return CaptureResponse {
                        kind: "failed",
                        id: None,
                        error: Some(format!("unrecognised energy {raw:?}")),
                    };
                }
            },
            None => None,
        };
        let options = CaptureOptions { size, energy, context };
        match self.core.capture(seed, title, stage, now_ms, options).await {
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
    /// (never one mutation per field — [`Core::triage`]'s own doc).
    /// `destination` is the wire's snake_case destination name
    /// ([`parse_destination`]), or `None` (#122) to leave `stage` untouched
    /// entirely — the weekend-plans pane's do-date chip triages an item that
    /// may already be `InProgress`, which `TriageDestination`'s two-value
    /// vocabulary cannot name, so a caller that only wants
    /// `edits.scheduled_date` applied passes no destination at all rather
    /// than one that would silently demote the item back to `Ready`.
    ///
    /// Everything else a wire value could get wrong is rejected HERE, before
    /// [`Core::triage`] is ever called, the same "reject before the seam"
    /// discipline [`TaskHostCore::capture`]/[`TaskHostCore::act`] use:
    /// `edits.size`/`edits.energy` are resolved by name through the shared
    /// vocabulary (`Size::parse`, `Energy::parse`), `edits.priority` must be
    /// 0..=4, `edits.deadline` must satisfy
    /// `hummingbird_domain::is_valid_deadline`, and `edits.scheduled_date`
    /// must be a whole civil day. Each of those is a rule the authority also
    /// enforces with a 400 — checking here is what turns a rejected write
    /// into a message on the form instead of a dead-lettered mutation the
    /// person cannot see.
    pub async fn triage(
        &mut self,
        seed: &str,
        item_id: &str,
        destination: Option<&str>,
        edits: TriageEdits,
        now_ms: i64,
    ) -> TriageResponse {
        let destination = match destination {
            Some(raw) => match parse_destination(raw) {
                Some(destination) => Some(destination),
                None => {
                    return reject(format!("unrecognised triage destination {raw:?}"));
                }
            },
            None => None,
        };
        if edits.title.as_deref() == Some("") {
            // The authority answers 400 on an empty title; a `NOT NULL`
            // column has no "cleared" state to fall back to.
            return reject("title must be non-empty".to_string());
        }
        let size = match parse_named("size", edits.size, Size::parse) {
            Ok(size) => size,
            Err(message) => return reject(message),
        };
        let energy = match parse_named("energy", edits.energy, Energy::parse) {
            Ok(energy) => energy,
            Err(message) => return reject(message),
        };
        if let Some(priority) = edits.priority {
            if !(0..=4).contains(&priority) {
                return reject("priority must be between 0 and 4".to_string());
            }
        }
        if let Some(Some(deadline)) = &edits.deadline {
            if !is_valid_deadline(deadline) {
                return reject(
                    "deadline must be YYYY-MM-DD or YYYY-MM-DDTHH:MM".to_string(),
                );
            }
        }
        if let Some(Some(scheduled_date)) = &edits.scheduled_date {
            // A whole day, so `is_valid_deadline`'s date-time form is not
            // enough on its own — the length check is what rules out
            // `YYYY-MM-DDTHH:MM` while still borrowing the shared calendar
            // validation (leap years, month lengths) rather than re-deriving
            // it here.
            if scheduled_date.len() != 10 || !is_valid_deadline(scheduled_date) {
                return reject("scheduled date must be YYYY-MM-DD".to_string());
            }
        }
        let patch = TriagePatch {
            title: edits.title,
            description: edits.description,
            size,
            energy,
            context: edits.context,
            priority: edits.priority,
            project_id: edits.project_id,
            deadline: edits.deadline,
            scheduled_date: edits.scheduled_date,
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
        host.capture("seed-1", "buy milk", "ready", None, None, None, 1_000).await;
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
        host.capture("seed-1", "buy milk", "ready", None, None, None, 1_000).await;
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
        host.capture("seed-1", "buy milk", "ready", None, None, None, 1_000).await;
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
        host.capture("seed-1", "buy milk", "ready", None, None, None, 1_000).await;
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
        host.capture("seed-1", "someday maybe", "triage", None, None, None, 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();

        let response = host
            .triage("seed-triage-1", &id, Some("backlog"), TriageEdits::default(), 2_000)
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
        host.capture("seed-1", "someday maybe", "triage", None, None, None, 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();

        let response = host
            .triage(
                "seed-triage-1",
                &id,
                Some("ready"),
                TriageEdits { size: Some(Some("giant".to_string())), ..TriageEdits::default() },
                2_000,
            )
            .await;

        assert_eq!(response.kind, "failed");
        assert!(response.error.is_some());
        assert_eq!(host.triage_inbox().items.len(), 1, "the item is untouched");
    }

    /// The wire contract `store/worker-client.ts` writes against: camelCase
    /// keys, an absent key meaning "leave this field alone", an explicit
    /// `null` meaning "clear it", and an unknown key refused outright rather
    /// than silently dropping the edit someone made.
    #[test]
    fn triage_edits_read_absent_null_and_a_value_as_three_different_instructions() {
        let edits: TriageEdits = serde_json::from_str(
            r#"{"projectId":"p1","deadline":null,"scheduledDate":"2026-08-12"}"#,
        )
        .unwrap();
        assert_eq!(edits.project_id, Some(Some("p1".to_string())), "a value sets");
        assert_eq!(edits.deadline, Some(None), "an explicit null clears");
        assert_eq!(edits.scheduled_date, Some(Some("2026-08-12".to_string())));
        assert_eq!(edits.context, None, "an absent key leaves the field alone");
        assert_eq!(edits.title, None);
        assert_eq!(edits.priority, None);

        assert!(
            serde_json::from_str::<TriageEdits>(r#"{"project_id":"p1"}"#).is_err(),
            "snake_case is not this wire's spelling, and a dropped edit is worse than an error"
        );
        assert!(
            serde_json::from_str::<TriageEdits>(r#"{"titel":"typo"}"#).is_err(),
            "an unknown key is refused, not read as `leave every field alone`"
        );
        assert!(
            serde_json::from_str::<TriageEdits>(r#"{"title":null}"#).is_err(),
            "title is NOT NULL — there is no cleared state to fall back to"
        );
    }

    /// Every value rule the authority answers 400 on is checked before
    /// `Core::triage` is reached, so the person sees a message on the form
    /// rather than a mutation that dead-letters later.
    #[tokio::test]
    async fn triaging_rejects_every_invalid_field_value_before_reaching_core_triage() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-triage-validation");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "someday maybe", "triage", None, None, None, 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();

        let rejected: Vec<(&str, TriageEdits)> = vec![
            ("empty title", TriageEdits { title: Some(String::new()), ..Default::default() }),
            (
                "unrecognised energy",
                TriageEdits { energy: Some(Some("plenty".to_string())), ..Default::default() },
            ),
            ("priority above the range", TriageEdits { priority: Some(5), ..Default::default() }),
            ("negative priority", TriageEdits { priority: Some(-1), ..Default::default() }),
            (
                "a deadline with seconds",
                TriageEdits {
                    deadline: Some(Some("2026-08-14T09:30:00".to_string())),
                    ..Default::default()
                },
            ),
            (
                "a calendar date that does not exist",
                TriageEdits {
                    deadline: Some(Some("2026-02-30".to_string())),
                    ..Default::default()
                },
            ),
            (
                "a scheduled date carrying a time",
                TriageEdits {
                    scheduled_date: Some(Some("2026-08-12T09:30".to_string())),
                    ..Default::default()
                },
            ),
        ];

        for (what, edits) in rejected {
            let response = host.triage("seed-triage-1", &id, Some("ready"), edits, 2_000).await;
            assert_eq!(response.kind, "failed", "{what} must be refused");
            assert!(response.error.is_some(), "{what} must say what was wrong");
            assert_eq!(
                host.triage_inbox().items.len(),
                1,
                "{what} must leave the item exactly where it was"
            );
        }

        // The boundaries themselves are legal: 0 and 4 are real priorities,
        // and both deadline shapes are the documented ones.
        let response = host
            .triage(
                "seed-triage-ok",
                &id,
                Some("ready"),
                TriageEdits {
                    priority: Some(4),
                    deadline: Some(Some("2026-08-14T09:30".to_string())),
                    scheduled_date: Some(Some("2026-08-12".to_string())),
                    ..Default::default()
                },
                3_000,
            )
            .await;
        assert_eq!(response.kind, "ok");
    }

    /// A triage may REMOVE a value, not only add one — the editor's own
    /// "Not set" option on a field that already holds something.
    #[tokio::test]
    async fn a_triage_clears_the_fields_it_nulls_and_leaves_the_rest_alone() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-triage-clear");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "someday maybe", "triage", None, None, None, 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();
        host.triage(
            "seed-triage-1",
            &id,
            Some("grilling"),
            TriageEdits {
                context: Some(Some("@computer".to_string())),
                size: Some(Some("deep".to_string())),
                ..Default::default()
            },
            2_000,
        )
        .await;

        let response = host
            .triage(
                "seed-triage-2",
                &id,
                Some("ready"),
                TriageEdits { size: Some(None), ..Default::default() },
                3_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        let item = &host.frontier().items[0].item;
        assert_eq!(item.size, None, "the nulled field is cleared");
        assert_eq!(
            item.context.as_deref(),
            Some("@computer"),
            "a field the edit never mentioned is untouched by the clear"
        );
    }

    #[tokio::test]
    async fn triaging_on_an_unknown_item_id_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-triage-3");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host
            .triage("seed-triage-1", "no-such-item", Some("ready"), TriageEdits::default(), 1_000)
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
        host.capture("seed-1", "someday maybe", "triage", None, None, None, 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();

        let response = host
            .triage(
                "seed-triage-1",
                &id,
                Some("ready"),
                TriageEdits {
                    title: Some("buy milk".to_string()),
                    project_id: Some(Some("project-1".to_string())),
                    size: Some(Some("quick".to_string())),
                    energy: Some(Some("low".to_string())),
                    context: Some(Some("@errands".to_string())),
                    ..TriageEdits::default()
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
        host.capture("seed-1", "someday maybe", "triage", None, None, None, 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();

        let response = host
            .triage("seed-triage-1", &id, Some("grilling"), TriageEdits::default(), 2_000)
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.triage_inbox().items.len(), 0);
        assert_eq!(host.frontier().items.len(), 0);
    }

    /// #122: `destination: None` at this seam must reach `Core::triage` as
    /// a genuine `None`, not accidentally coerced into some destination —
    /// the whole reason the weekend-plans pane's do-date chip can touch an
    /// `InProgress` item without demoting it.
    #[tokio::test]
    async fn a_none_destination_edits_scheduled_date_without_touching_stage() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-triage-6");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "buy milk", "ready", None, None, None, 1_000).await;
        let id = host.frontier().items[0].item.id.clone();
        host.act("seed-act-1", &id, "start", 1_500).await;
        assert_eq!(host.frontier().items[0].item.stage, Stage::InProgress);

        let response = host
            .triage(
                "seed-triage-6",
                &id,
                None,
                TriageEdits {
                    scheduled_date: Some(Some("2026-08-15".to_string())),
                    ..TriageEdits::default()
                },
                2_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        let item = &host.frontier().items[0].item;
        assert_eq!(item.stage, Stage::InProgress, "a None destination must never change stage");
        assert_eq!(item.scheduled_date.as_deref(), Some("2026-08-15"));
    }

    /// The clear half of the same three-state edit, at this seam.
    #[tokio::test]
    async fn clearing_scheduled_date_through_this_seam_is_a_real_null() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-triage-7");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "buy milk", "ready", None, None, None, 1_000).await;
        let id = host.frontier().items[0].item.id.clone();
        host.triage(
            "seed-triage-7a",
            &id,
            None,
            TriageEdits {
                scheduled_date: Some(Some("2026-08-15".to_string())),
                ..TriageEdits::default()
            },
            1_500,
        )
        .await;
        assert_eq!(host.frontier().items[0].item.scheduled_date.as_deref(), Some("2026-08-15"));

        host.triage(
            "seed-triage-7b",
            &id,
            None,
            TriageEdits { scheduled_date: Some(None), ..TriageEdits::default() },
            2_000,
        )
        .await;

        assert_eq!(host.frontier().items[0].item.scheduled_date, None);
    }
}

#[cfg(test)]
mod ledger_tests {
    use super::*;

    /// The wire contract `task-worker.ts` parses: the item's own snake_case
    /// fields flat at the top level (exactly `FrontierItemDTO`'s shape) plus
    /// the four row facts — pinned on the keys, since a renamed key on this
    /// seam fails silently as an always-absent badge.
    #[tokio::test]
    async fn a_ledger_row_serializes_item_fields_flat_beside_the_row_facts() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-ledger-1");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "buy milk", "ready", None, None, None, 1_000).await;

        let response = host.ledger(2_000);
        assert_eq!(response.kind, "ok");
        assert_eq!(response.rows.len(), 1);
        let json: serde_json::Value =
            serde_json::to_value(&response.rows[0]).unwrap();
        for key in [
            "id",
            "title",
            "stage",
            "archived_at",
            "updated_at",
            "pending",
            "absent_since_ms",
            "dead_lettered",
            "has_live_alert",
        ] {
            assert!(json.get(key).is_some(), "ledger row must carry {key:?} at the top level");
        }
        assert_eq!(json["pending"], serde_json::Value::Bool(true));
        assert_eq!(json["absent_since_ms"], serde_json::Value::Null);
        assert_eq!(json["dead_lettered"], serde_json::Value::Bool(false));
        assert_eq!(json["has_live_alert"], serde_json::Value::Bool(false));
    }

    /// The two reads compose with the act path exactly like frontier does:
    /// an offline complete is on Done immediately, and the ledger never
    /// loses the row whatever stage it reaches.
    #[tokio::test]
    async fn completing_an_item_moves_it_onto_done_and_keeps_it_in_the_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-ledger-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture("seed-1", "buy milk", "ready", None, None, None, 1_000).await;
        let id = host.frontier().items[0].item.id.clone();

        host.act("seed-act-1", &id, "complete", 2_000).await;

        let done = host.done();
        assert_eq!(done.kind, "ok");
        assert_eq!(done.items.len(), 1);
        assert_eq!(done.items[0].item.id, id);
        assert!(done.items[0].pending);
        assert_eq!(host.ledger(3_000).rows.len(), 1);

        // A cancel drops it off Done but never out of the ledger.
        host.act("seed-act-2", &id, "cancel", 4_000).await;
        assert_eq!(host.done().items.len(), 0);
        assert_eq!(host.ledger(5_000).rows.len(), 1);
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
    async fn the_freshness_wire_shape_keeps_the_two_unknowns_apart() {
        // What crosses is the finished answer, not the parts (ADR-0015).
        // A host with nothing synced has measured nothing, and it must say
        // so in a shape TS cannot mistake for a fresh, zero-age answer.
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-freshness");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host.snapshot_freshness("city-waste/v2", "next_collection", 100_000);
        assert_eq!(
            response,
            FreshnessResponse { kind: "ok", freshness: Freshness::Unknown }
        );
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"kind":"ok","freshness":{"state":"unknown"}}"#,
        );

        // The second, different unknown — an age we know, a cadence we do
        // not — must not serialize to the same thing.
        assert_eq!(
            serde_json::to_string(&FreshnessResponse {
                kind: "ok",
                freshness: Freshness::Age { age_ms: 0, declared_cadence_ms: None },
            })
            .unwrap(),
            r#"{"kind":"ok","freshness":{"state":"age","age_ms":0,"declared_cadence_ms":null}}"#,
        );
    }

    #[tokio::test]
    async fn capturing_with_an_unrecognised_stage_never_reaches_core_capture() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host.capture("seed-1", "buy milk", "not-a-stage", None, None, None, 1_000).await;

        assert_eq!(response.kind, "failed");
        assert!(response.id.is_none());
        assert!(response.error.is_some());
        assert_eq!(host.frontier().items.len(), 0);
    }

    /// #208: an unrecognised size name is rejected at the seam and never
    /// reaches `Core::capture` — same "reject before the seam" discipline
    /// `capturing_with_an_unrecognised_stage_never_reaches_core_capture`
    /// pins for `stage`, and `triaging_with_an_unrecognised_size_never_reaches_core_triage`
    /// pins for triage's own `size` edit.
    #[tokio::test]
    async fn capturing_with_an_unrecognised_size_never_reaches_core_capture() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-capture-size");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host
            .capture(
                "seed-1",
                "buy milk",
                "ready",
                Some("giant".to_string()),
                None,
                None,
                1_000,
            )
            .await;

        assert_eq!(response.kind, "failed");
        assert!(response.id.is_none());
        assert!(response.error.is_some());
        assert_eq!(host.frontier().items.len(), 0);
    }

    /// #208: same rejection, for an unrecognised energy name.
    #[tokio::test]
    async fn capturing_with_an_unrecognised_energy_never_reaches_core_capture() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-capture-energy");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host
            .capture(
                "seed-1",
                "buy milk",
                "ready",
                None,
                Some("blazing".to_string()),
                None,
                1_000,
            )
            .await;

        assert_eq!(response.kind, "failed");
        assert!(response.id.is_none());
        assert!(response.error.is_some());
        assert_eq!(host.frontier().items.len(), 0);
    }

    /// #208's headline acceptance at this layer: setting Energy, Size and
    /// Context and capturing produces an item carrying exactly those
    /// values, resolved by name through `hummingbird_domain`'s vocabulary.
    #[tokio::test]
    async fn capturing_with_size_energy_and_context_sets_them_on_the_item() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-capture-meta");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host
            .capture(
                "seed-1",
                "buy milk",
                "ready",
                Some("deep".to_string()),
                Some("high".to_string()),
                Some("@errands".to_string()),
                1_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        let frontier = host.frontier();
        assert_eq!(frontier.items[0].item.size, Some(Size::Deep));
        assert_eq!(frontier.items[0].item.energy, Some(Energy::High));
        assert_eq!(frontier.items[0].item.context.as_deref(), Some("@errands"));
    }

    #[tokio::test]
    async fn a_capture_is_readable_from_the_frontier_and_marked_pending() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-3");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host.capture("seed-1", "buy milk", "ready", None, None, None, 1_000).await;

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

        host.capture("seed-1", "someday maybe", "triage", None, None, None, 1_000).await;

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
        let response = host.capture("seed-1", "buy milk", "ready", None, None, None, 1_000).await;
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

        host.capture("seed-1", "buy milk", "ready", None, None, None, 1_000).await;

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
                entity: "items".to_string(),
                // This fixture's create body carries no `id`, so there is no
                // row to name and the DTO says so rather than inventing one.
                entity_id: None,
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
                    rebase_fields: None,
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
                // The abandoned change was about this item — the `id` above
                // is the queue entry's, which names the attempt instead.
                entity: "items".to_string(),
                entity_id: Some("item-1".to_string()),
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
                    rebase_fields: None,
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
            agent: false,
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
                entity: "items".to_string(),
                entity_id: Some("a-1".to_string()),
            }],
        };
        assert_eq!(
            serde_json::to_string(&permanent).unwrap(),
            r#"{"kind":"ok","entries":[{"id":"item-1","reason":"permanent","message":"validation","fields":[],"at_ms":5000,"entity":"items","entity_id":"a-1"}]}"#
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
                entity: "settings".to_string(),
                // `null` on the wire, not an absent key — `task-worker.ts`
                // reads it as "named no single row" and must see the field.
                entity_id: None,
            }],
        };
        assert_eq!(
            serde_json::to_string(&conflict).unwrap(),
            r#"{"kind":"ok","entries":[{"id":"item-2","reason":"conflict","message":null,"fields":[{"field":"title","local":"buy oat milk","server":"someone else's"}],"at_ms":6000,"entity":"settings","entity_id":null}]}"#
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

#[cfg(test)]
mod pane_read_tests {
    use super::*;
    use hummingbird_core::freshness::Freshness;
    use hummingbird_core::pane::PaneEnvelope;

    /// Pins the whole wire shape `task-worker.ts`'s `RawPaneReadResponse`
    /// parses, in one string: the tagged `freshness`/`envelope` unions, the
    /// `body` that is a JSON *string* rather than a nested object, and the
    /// raw alert row with `subject_key` on it. Nothing on the TS side
    /// re-derives any of this from serde's output.
    #[test]
    fn pane_read_response_serializes_with_the_exact_keys_the_pane_shell_ts_parses() {
        let response = PaneReadResponse {
            kind: "ok",
            snapshots: vec![
                PaneSnapshot {
                    source: "city-waste/v2".to_string(),
                    key: "collection".to_string(),
                    fetched_at: 1_000,
                    version: 2,
                    freshness: Freshness::Age {
                        age_ms: 60_000,
                        declared_cadence_ms: Some(86_400_000),
                    },
                    envelope: PaneEnvelope::Parsed {
                        schema: "city-waste/v2".to_string(),
                        polled_every_ms: Some(86_400_000),
                        body: r#"{"zone":"Europe/London"}"#.to_string(),
                    },
                },
                PaneSnapshot {
                    source: "city-waste/v2".to_string(),
                    key: "broken".to_string(),
                    fetched_at: 2_000,
                    version: 1,
                    freshness: Freshness::Age { age_ms: 0, declared_cadence_ms: None },
                    envelope: PaneEnvelope::Malformed {
                        reason: "`body` is missing".to_string(),
                    },
                },
            ],
            alerts: vec![Alert {
                id: "alert-1".to_string(),
                source: "city-waste/v2".to_string(),
                source_key: "collection:2026-08-11".to_string(),
                subject_key: Some("collection".to_string()),
                title: "Collection moved".to_string(),
                body: None,
                url: None,
                severity: None,
                raised_at: 900,
                resolved_at: None,
                dismissed_at: None,
                expires_at: None,
                version: 1,
            }],
        };

        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            concat!(
                r#"{"kind":"ok","snapshots":["#,
                r#"{"source":"city-waste/v2","key":"collection","fetched_at":1000,"version":2,"#,
                r#""freshness":{"state":"age","age_ms":60000,"declared_cadence_ms":86400000},"#,
                r#""envelope":{"state":"parsed","schema":"city-waste/v2","polled_every_ms":86400000,"#,
                r#""body":"{\"zone\":\"Europe/London\"}"}},"#,
                r#"{"source":"city-waste/v2","key":"broken","fetched_at":2000,"version":1,"#,
                r#""freshness":{"state":"age","age_ms":0,"declared_cadence_ms":null},"#,
                r#""envelope":{"state":"malformed","reason":"`body` is missing"}}],"#,
                r#""alerts":[{"id":"alert-1","source":"city-waste/v2","#,
                r#""source_key":"collection:2026-08-11","subject_key":"collection","#,
                r#""title":"Collection moved","body":null,"url":null,"severity":null,"#,
                r#""raised_at":900,"resolved_at":null,"dismissed_at":null,"expires_at":null,"#,
                r#""version":1}]}"#,
            )
        );
    }

    #[tokio::test]
    async fn a_fresh_host_answers_ok_and_empty_which_is_not_busy() {
        // "Nothing synced yet" is an answer. `busy` — no answer at all — is
        // the shim's, and the host must never conflate the two.
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-pane-1");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host.pane_read("city-waste/v2", 1_000);
        assert_eq!(response.kind, "ok");
        assert!(response.snapshots.is_empty());
        assert!(response.alerts.is_empty());
    }
}

#[cfg(test)]
mod binding_tests {
    use super::*;

    #[test]
    fn binding_responses_serialize_with_the_exact_keys_task_worker_ts_parses() {
        let list = BindingListResponse {
            kind: "ok",
            bindings: vec![
                Binding {
                    key: "race-series".to_string(),
                    known: true,
                    pending: true,
                    value: hummingbird_core::bindings::BindingValue::Text {
                        text: "f1".to_string(),
                    },
                },
                Binding {
                    key: "trips-calendar".to_string(),
                    known: true,
                    pending: false,
                    value: hummingbird_core::bindings::BindingValue::Unset,
                },
            ],
        };
        assert_eq!(
            serde_json::to_string(&list).unwrap(),
            r#"{"kind":"ok","bindings":[{"key":"race-series","known":true,"pending":true,"value":{"state":"text","text":"f1"}},{"key":"trips-calendar","known":true,"pending":false,"value":{"state":"unset"}}]}"#
        );

        assert_eq!(
            serde_json::to_string(&SetBindingResponse { kind: "ok", error: None }).unwrap(),
            r#"{"kind":"ok","error":null}"#
        );
    }

    #[tokio::test]
    async fn setting_a_binding_with_an_unrecognised_key_never_reaches_core_set_binding() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-binding-1");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        // A versioned spelling is exactly the mistake ADR-0015's unversioned
        // keys exist to prevent, and `settings` has no DELETE to undo it.
        let response = host.set_binding("seed-1", "race-series/v1", "f1", 1_000).await;

        assert_eq!(response.kind, "unknown_key");
        assert!(
            host.bindings()
                .bindings
                .iter()
                .all(|binding| binding.key != "race-series/v1"),
            "a rejected key must never reach the table"
        );
    }

    #[tokio::test]
    async fn a_binding_set_through_the_seam_reads_back_from_the_same_seam() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-binding-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        assert_eq!(host.set_binding("seed-1", "trips-calendar", "cal-trips", 1_000).await.kind, "ok");

        let bindings = host.bindings().bindings;
        let trips = bindings
            .iter()
            .find(|binding| binding.key == "trips-calendar")
            .expect("trips-calendar is always listed");
        assert_eq!(
            trips.value,
            hummingbird_core::bindings::BindingValue::Text {
                text: "cal-trips".to_string()
            }
        );
        assert!(trips.pending, "nothing has synced it yet");
    }
}

#[cfg(test)]
mod rule_tests {
    use super::*;

    /// The kind registry export needs no `Core` state and never answers
    /// `"busy"` — proof that it reads static domain data alone. Also pins
    /// that every launch kind and the Event core survive the wire.
    #[tokio::test]
    async fn kind_registry_lists_every_launch_kind_and_the_core_fields() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-kind-registry");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        let _ = host; // the registry needs no host state at all
        let response = TaskHostCore::kind_registry();
        assert_eq!(response.kind, "ok");
        assert_eq!(response.kinds.len(), hummingbird_domain::EVENT_KINDS.len());
        assert!(response.kinds.iter().any(|k| k.key == "email"));
        assert_eq!(response.core_fields.len(), hummingbird_domain::CORE_FIELDS.len());
        assert!(response.core_fields.iter().any(|f| f.name == "source"));
        assert_eq!(response.alarm_interval_ms, ALARM_INTERVAL_MS);
        assert_eq!(response.severities, hummingbird_domain::SEVERITIES);

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""key":"email""#));
        assert!(json.contains(r#""alarm_interval_ms":900000"#));
        assert!(json.contains(r#""severities":["low","normal","high","urgent"]"#));
    }

    #[tokio::test]
    async fn creating_a_rule_with_an_unrecognised_tier_never_reaches_core_create_rule() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-create-rule-bad-tier");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        let response = host
            .create_rule("seed-1", "trash slide", None, "[]", "high", "not-a-tier", true, 1_000)
            .await;

        assert_eq!(response.kind, "failed");
        assert!(response.id.is_none());
        assert_eq!(host.rules().rules.len(), 0);
    }

    #[tokio::test]
    async fn creating_a_rule_enqueues_it_and_lists_it_in_the_queue_depth() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-create-rule-ok");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        let response = host
            .create_rule(
                "seed-1",
                "trash slide",
                Some("snapshot_change".to_string()),
                r#"[{"field":"key","op":"eq","value":"city-waste/v2","negate":false}]"#,
                "high",
                "urgent",
                true,
                1_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        assert!(response.id.is_some());
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
    }

    #[tokio::test]
    async fn patching_a_rules_enabled_field_touches_only_that_field() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-rule");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        let rule = hummingbird_domain::Rule {
            id: "r-1".to_string(),
            name: "trash slide".to_string(),
            event_kind: None,
            conditions: vec![],
            severity: "high".to_string(),
            tier: hummingbird_domain::Tier::Urgent,
            enabled: true,
            updated_at: 1,
            version: 3,
        };

        let response = host
            .patch_rule("seed-1", &rule, None, false, None, None, None, None, Some(false), 2_000)
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
    }

    #[tokio::test]
    async fn patching_a_rule_with_an_unrecognised_tier_never_reaches_core_patch_rule() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-rule-bad-tier");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        let rule = hummingbird_domain::Rule {
            id: "r-1".to_string(),
            name: "trash slide".to_string(),
            event_kind: None,
            conditions: vec![],
            severity: "high".to_string(),
            tier: hummingbird_domain::Tier::Urgent,
            enabled: true,
            updated_at: 1,
            version: 3,
        };

        let response = host
            .patch_rule(
                "seed-1",
                &rule,
                None,
                false,
                None,
                None,
                None,
                Some("not-a-tier".to_string()),
                None,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "failed");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 0 });
    }
}
