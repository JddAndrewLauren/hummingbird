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

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU64, Ordering};

use hummingbird_core::bindings::{Binding, BindingKey};
use hummingbird_core::decisions::panes::contract::QUESTION_ORDER;
use hummingbird_core::diagnostics::{
    DiagnosticEvent, DiagnosticEventV1, DiagnosticSession, DiagnosticSink, OperationOutcome,
    SyncOutcome,
};
/// Re-exported so `lib.rs`'s wasm bindings (the checkout call sites) can
/// name it as `task_host::CoreOwner` alongside [`TaskCoreCell`], rather
/// than importing it from `hummingbird_core` a second way.
pub use hummingbird_core::diagnostics::CoreOwner;
use hummingbird_core::question_switch::QuestionSwitch;
use hummingbird_core::sync::queue::{DeadLetterEntry, DeadLetterReason, MutationIntent};
use hummingbird_core::sync::write::ReqwestMutationTransport;
use hummingbird_core::sync::{CycleOutcome, ReqwestSyncTransport, Trigger};
use hummingbird_core::freshness::Freshness;
use hummingbird_core::pane::PaneSnapshot;
use hummingbird_core::search::Group;
use hummingbird_core::{
    ActError, CaptureOptions, CompleteGrillError, Core, CoreCycleOutcome, CoreEvent,
    CoreInitError, GrillCompletion, ItemAction, TriagePatch,
};
use hummingbird_domain::{
    core_field_type, is_valid_deadline, is_valid_github_repo, Alert, Condition, Energy,
    EventKindEntry, FieldType, Fog, Item, Project, ProjectLink, Route, Rule, Size, Stage, Step,
    Tier, CORE_FIELDS, EVENT_KINDS, GrillVerdict,
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

/// Every field the capture box may set on a brand-new item, as the JS side
/// sends them — one JSON object, camelCased, deserialized here rather than the
/// seven-scalar positional list this used to be.
///
/// **Plain `Option<T>`, deliberately not [`TriageEdits`]' double option.**
/// These are creation-time values on an item that does not exist yet, so there
/// is no stored value a `null` could be clearing: absent and null are the same
/// instruction — don't send the field — and the three-state distinction
/// `touched` exists for has nothing to distinguish. The UI leans on that: an
/// untouched control sends `null` and the item is created without the field.
///
/// `deny_unknown_fields` and `camelCase` on purpose, and pinned by tests: this
/// struct's field names are a contract with `store/protocol.ts`, and a
/// misspelling on either side would deserialize to "unset" — a capture that
/// silently drops the deadline someone typed.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureFields {
    /// The wire's snake_case size name (`hummingbird_domain::Size::parse`);
    /// resolved by name through the vocabulary, never a raw index.
    #[serde(default)]
    pub size: Option<String>,
    /// Same "resolved by name" contract as `size`
    /// (`hummingbird_domain::Energy::parse`).
    #[serde(default)]
    pub energy: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    /// `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`, checked with
    /// `hummingbird_domain::is_valid_deadline` before the seam — the same
    /// function the authority validates with, so the two cannot disagree.
    #[serde(default)]
    pub deadline: Option<String>,
    /// A whole civil day (`YYYY-MM-DD`) and never a date-time: a scheduled
    /// date is the do-date a human chose, which has no minute.
    #[serde(default)]
    pub scheduled_date: Option<String>,
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

/// What [`TaskHostCore::complete_grill`] resolves to (#355, ADR-0023).
/// Four failure kinds, each distinct from `"failed"` because the caller's
/// recovery differs: `"not_found"` (no such item —
/// [`CompleteGrillError::ItemNotFound`]), `"item_done"` (the item is out of
/// the Grill plan's scope — [`CompleteGrillError::ItemDone`]) and
/// `"needs_re_review"` (the reviewed session's Steps have drifted since —
/// [`CompleteGrillError::NeedsReReview`], the review card's cue to refuse
/// the stale Confirm and show fresh state instead of silently re-sending
/// it). `"failed"` covers an unrecognised `verdict` string, unreadable
/// `session_steps` JSON, and a durability failure enqueueing the mutation.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CompleteGrillResponse {
    pub kind: &'static str,
    pub id: Option<String>,
    pub error: Option<String>,
}

/// What [`TaskHostCore::save_grill_draft`]/[`TaskHostCore::discard_grill_draft`]
/// resolve to (#356, ADR-0023): `"failed"` covers unreadable `turns` JSON
/// (save only) and a durability failure writing the draft store.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SaveGrillDraftResponse {
    pub kind: &'static str,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DiscardGrillDraftResponse {
    pub kind: &'static str,
    pub error: Option<String>,
}

/// One item's draft read (#356). `exists` is `false` and `turns` is
/// `None` when the item has no draft — distinct from an item this build
/// merely has not asked about yet, which is `client/web`'s own "not read
/// yet" gap (`TaskState.grillDraftByItem`'s doc), not a state this
/// response has any business naming.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GrillDraftResponse {
    pub kind: &'static str,
    pub exists: bool,
    pub turns: Option<serde_json::Value>,
}

/// Every item id carrying a draft (#356) — the bulk read a Triage-inbox-wide
/// "Resume grill" label needs without one request per row.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GrillDraftItemIdsResponse {
    pub kind: &'static str,
    pub item_ids: Vec<String>,
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
    /// The archived half (#624's Show-archived toggle), carried on the same
    /// answer rather than behind a second request: the projects read already
    /// has exactly one app-wide requester (`shell/useFrontierWiring.ts`), and
    /// a second door for the same read would be a second clock for it.
    /// Always disjoint from `projects` — [`crate::Core::archived_projects`]
    /// is what an archived project demotes *into*, never a duplicate of the
    /// live list.
    pub archived: Vec<Project>,
}

/// What [`TaskHostCore::create_project`] resolves to (#624). Same three-way
/// split as [`CreateRuleResponse`]: `"ok"` carries the minted id, `"failed"`
/// is either a name this seam refused outright or a durability failure
/// enqueueing the create, and `"busy"` is the core answering nothing at all.
/// An `"ok"` means *enqueued*, not *saved* — there is no optimistic overlay
/// ([`Core::create_project`]), so the project appears in
/// [`TaskHostCore::projects`] only after a completed cycle pulls it back.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CreateProjectResponse {
    pub kind: &'static str,
    pub id: Option<String>,
    pub error: Option<String>,
}

/// What [`TaskHostCore::patch_project`] resolves to (#625) — the dossier's
/// properties card, and every other project edit, share this one entry
/// point. Same shape as [`PatchRuleResponse`]: `"failed"` covers both a
/// `github_repo` this seam refused outright and a durability failure
/// enqueueing the write; a 409 is handled, not swallowed, through the
/// ordinary CAS path.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PatchProjectResponse {
    pub kind: &'static str,
    pub error: Option<String>,
}

/// The wrapper around [`TaskHostCore::projectLinks`]'s answer — the
/// dossier aside's read (#626, ADR-0030 decision 4). Same `"busy"`
/// contract as [`ItemListResponse`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProjectLinkListResponse {
    pub kind: &'static str,
    pub links: Vec<ProjectLink>,
}

/// What [`TaskHostCore::create_project_link`] resolves to (#626). Same
/// three-way split as [`CreateProjectResponse`]: `"ok"` carries the minted
/// id, `"failed"` is either a url this seam refused outright or a
/// durability failure enqueueing the create, and `"busy"` is the core
/// answering nothing at all. An `"ok"` means *enqueued*, not *saved* —
/// there is no optimistic overlay ([`Core::create_project_link`]), so the
/// link appears in [`TaskHostCore::project_links`] only after a completed
/// cycle pulls it back.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CreateProjectLinkResponse {
    pub kind: &'static str,
    pub id: Option<String>,
    pub error: Option<String>,
}

/// What [`TaskHostCore::patch_project_link`] resolves to (#626) — editing,
/// reordering and removing a link all share this one entry point. Same
/// shape as [`PatchProjectResponse`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PatchProjectLinkResponse {
    pub kind: &'static str,
    pub error: Option<String>,
}

/// The wrapper around [`TaskHostCore::route`]'s answer — the dossier's
/// reading column's read (#627, ADR-0030 decision 1). Same `"busy"`
/// contract as [`ProjectLinkListResponse`]. `route` is `None` both when the
/// mirror has not pulled this project's Route yet and when `busy` — the two
/// share a JSON shape by design, since [`Core::route`]'s own doc makes the
/// same "not read yet" claim, not "this project has none" (every project
/// has exactly one Route, created structurally by
/// [`hummingbird_core::Core::create_project`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RouteResponse {
    pub kind: &'static str,
    pub route: Option<Route>,
}

/// What [`TaskHostCore::patch_route`] resolves to (#627) — the dossier's
/// reading column edits destination and notes through this one entry
/// point. Same shape as [`PatchProjectResponse`]: `"failed"` is a
/// durability failure enqueueing the write; a 409 is handled, not
/// swallowed, through the ordinary CAS path (ADR-0030 decision 1 — the
/// route's content is shared-owned with `/to-actions`, so this is an
/// ordinary outcome, not a bespoke conflict).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PatchRouteResponse {
    pub kind: &'static str,
    pub error: Option<String>,
}

/// The wrapper around [`TaskHostCore::open_fog`]'s answer — the dossier
/// reading column's read (#628, ADR-0030 decision 1). Same `"busy"`
/// contract as [`ProjectLinkListResponse`]; `fog` carries only **open**
/// rows, position order — a resolved one is retained but never answers
/// here ([`Core::open_fog_for`]'s own doc).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FogListResponse {
    pub kind: &'static str,
    pub fog: Vec<Fog>,
}

/// What [`TaskHostCore::create_fog`] resolves to (#628). Same three-way
/// split as [`CreateProjectLinkResponse`]: `"ok"` carries the minted id,
/// `"failed"` is either a question this seam refused outright or a
/// durability failure enqueueing the create, and `"busy"` is the core
/// answering nothing at all. An `"ok"` means *enqueued*, not *saved* —
/// there is no optimistic overlay ([`Core::create_fog`]), so the segment
/// appears in [`TaskHostCore::open_fog`] only after a completed cycle
/// pulls it back.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CreateFogResponse {
    pub kind: &'static str,
    pub id: Option<String>,
    pub error: Option<String>,
}

/// What [`TaskHostCore::patch_fog`] resolves to (#628) — rewording,
/// repositioning and resolving/reopening a segment all share this one
/// entry point. Same shape as [`PatchProjectLinkResponse`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PatchFogResponse {
    pub kind: &'static str,
    pub error: Option<String>,
}

/// What [`TaskHostCore::patch_action_position`] resolves to (#629) — the
/// dossier's reorder control. Same shape as [`PatchProjectResponse`]:
/// `"failed"` is a durability failure enqueueing the write; a 409 is
/// handled, not swallowed, through the ordinary CAS path (ADR-0030
/// decision 1 — `project_pos` is shared-owned with `/to-actions`, so this
/// is an ordinary outcome, not a bespoke conflict).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PatchActionPositionResponse {
    pub kind: &'static str,
    pub error: Option<String>,
}

/// What [`TaskHostCore::create_step`] resolves to (#629). Same three-way
/// split as [`CreateFogResponse`]: `"ok"` carries the minted id, `"failed"`
/// is either a body this seam refused outright or a durability failure
/// enqueueing the create, and `"busy"` is the core answering nothing at
/// all. An `"ok"` means *enqueued*, not *saved* — there is no optimistic
/// overlay ([`Core::create_step`]), so the Step appears in
/// [`TaskHostCore::steps`] only after a completed cycle pulls it back.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CreateStepResponse {
    pub kind: &'static str,
    pub id: Option<String>,
    pub error: Option<String>,
}

/// What [`TaskHostCore::patch_step`] resolves to (#629) — ticking,
/// rewording, repositioning, or flagging/clearing a Step's deletion all
/// share this one entry point. Same shape as [`PatchFogResponse`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PatchStepResponse {
    pub kind: &'static str,
    pub error: Option<String>,
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

/// One Recall result row (`Core::search`, #478): the item's fields flat at
/// the top level exactly like [`LedgerRowDTO`], plus the same per-item
/// `pending` stamp every other item read carries and the [`Group`] it
/// matched in ("live"/"done"/"archived", per [`Group`]'s serde). Never
/// carries the resolved project name — the query matched against it
/// core-side, but rows are read-only in this slice and a caller already has
/// [`TaskHostCore::projects`] for display.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SearchRowDTO {
    #[serde(flatten)]
    pub item: Item,
    pub pending: bool,
    pub group: Group,
}

/// The wrapper around [`TaskHostCore::search`]'s answer. Same `"busy"`
/// contract as [`ItemListResponse`] — a `"busy"` answer carries an empty
/// list and a zero `total` because the shape demands both, and the host
/// drops it rather than storing it. `total` is the un-capped match count
/// (decision 8): the UI's "N more" line reads it directly rather than
/// re-deriving a count from a capped list.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SearchResponse {
    pub kind: &'static str,
    pub rows: Vec<SearchRowDTO>,
    pub total: usize,
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

/// The wrapper around [`TaskHostCore::question_switches`]'s answer (#715)
/// — every standing question's on/off state. Same `"busy"` contract as
/// [`BindingListResponse`], and load-bearing for the same reason with the
/// polarity reversed: a busy core answering `[]` would leave the Settings
/// roster with no switch state at all, and a screen defaulting that to
/// "everything is on" would say so about a workspace it had not read.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct QuestionSwitchListResponse {
    pub kind: &'static str,
    pub switches: Vec<QuestionSwitch>,
}

/// What [`TaskHostCore::set_question_enabled`] resolves to.
/// `"unknown_question"` is [`SetBindingResponse`]'s `"unknown_key"` for the
/// question vocabulary: the seam rejecting a name outside
/// `StandingQuestion`, before `Core` is reached.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SetQuestionEnabledResponse {
    pub kind: &'static str,
    pub error: Option<String>,
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
    /// `hummingbird_domain::REGISTRY`, in registration order — the `source`
    /// core field's frozen vocabulary, so the rules screen's source
    /// dropdown offers exactly what the authority will accept. Same
    /// justification as `severities`: `hummingbird_domain` is already this
    /// crate's dependency. Retired entries are carried, not filtered — an
    /// existing rule may name one, and hiding it would render a blank
    /// `<select>` rather than the value actually stored
    /// (`hummingbird_core::decisions::rules::SourceOption`).
    pub sources: Vec<SourceOptionDTO>,
}

/// One entry of [`KindRegistryResponse::sources`] — a registered `source`
/// and, when ADR-0014 has bumped it, the successor it was retired in favour
/// of. `retired_as: Some(_)` is what makes an option unselectable in the
/// editor: the authority already 400s that save
/// (`RuleProblem::RetiredSource`), and this is what lets the operator see
/// it first.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SourceOptionDTO {
    pub source: &'static str,
    pub retired_as: Option<&'static str>,
}

/// Mirrors `hummingbird_authority::sweep::ALARM_INTERVAL_MS` — see
/// [`KindRegistryResponse`]'s own doc for why this is a mirrored constant
/// rather than a dependency. The mirror itself lives once, in
/// `hummingbird_core::decisions::rules` (#540), so the mobile seam reads
/// the same number rather than restating it a third time.
pub const ALARM_INTERVAL_MS: i64 = hummingbird_core::decisions::rules::ALARM_INTERVAL_MS;

/// The wrapper around [`TaskHostCore::mirror_snapshot`]'s answer — S9's
/// mirror download button.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MirrorSnapshotResponse {
    pub kind: &'static str,
    pub mirror: serde_json::Value,
}

/// The touched fields' *intended* (local) values a [`MutationIntent`]
/// carries — only a `Patch` has any; a `Create` never conflicts (deterministic
/// ids, ADR-0007), so it is never the intent behind a `Conflict` reason. A
/// `CompleteGrill` is the same story as `Create` for this purpose: it names
/// no item field by value (see that variant's own doc — none of its keys
/// are foreign-shaped onto the item it targets), so it never carries a
/// meaningful `Conflict` either.
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

// ------------------------------------------------------------- #708: diagnostics

/// A monotonic-enough millisecond reading for `core.*`/`operation.*`
/// `elapsed_ms` — real wall-clock time on `wasm32` (`Date.now()`, callable
/// synchronously with no `async`/`sleep` involved, unlike
/// [`hummingbird_core::diagnostics::DiagnosticClock`]'s full contract —
/// this crate's own checkout/operation spans never race a real network
/// call the way a sync cycle's `http.*` slow/stalled watchdog does, so
/// there is no `sleep_ms` to implement here), and a process-local
/// `Instant`-based counter natively so this module's own `cargo test`
/// suite (this file's header: "kept free of `wasm_bindgen`") sees a real,
/// monotonic reading too — the same per-target split [`TaskStore`] above
/// already uses for the identical reason.
fn now_monotonic_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::Instant;
        thread_local! {
            static ORIGIN: Instant = Instant::now();
        }
        ORIGIN.with(|origin| origin.elapsed().as_millis() as u64)
    }
}

/// #708: the in-memory ring the wasm host's own `core.*`/`operation.*`
/// events accumulate into between drains — the Rust-side half of #707's
/// already-built JS drain contract
/// (`client/web/src/worker/diagnostics-journal.ts`'s `drainAroundRequest`,
/// `task-worker.ts`'s `TaskHostLike.drainDiagnostics`, both of which were
/// written *before* this slice landed, anticipating this exact shape).
/// Not persisted: if a session ends before a drain, whatever is still
/// buffered is lost — the same in-memory, host-drained contract
/// [`TaskHostCore::take_events`] already established for `CoreEvent`.
/// A `Mutex`, not a `RefCell` — [`DiagnosticSink`] requires `Send + Sync`
/// (`hummingbird_core::diagnostics::test_support::RecordingSink` makes the
/// identical choice for the identical reason), even though the wasm32
/// target this actually ships on is single-threaded.
#[derive(Default)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub struct DiagnosticBuffer {
    events: std::sync::Mutex<Vec<DiagnosticEventV1>>,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
impl DiagnosticBuffer {
    /// Every event recorded since the last drain, in order, clearing the
    /// buffer — mirrors [`TaskHostCore::take_events`]'s own "drain, don't
    /// peek" contract.
    pub fn drain(&self) -> Vec<DiagnosticEventV1> {
        std::mem::take(&mut self.events.lock().unwrap())
    }
}

impl DiagnosticSink for DiagnosticBuffer {
    fn record(&self, event: DiagnosticEventV1) {
        self.events.lock().unwrap().push(event);
    }
}

/// Whatever a synchronous setter had to defer because [`TaskCoreCell`]'s
/// host was checked out. Moved here from `ffi-web/src/lib.rs`'s
/// `TaskShared` (#708): the checkout it defers around now lives here too,
/// so the two travel together rather than splitting the re-entrancy guard
/// across two files. NOT simple last-wins — see each variant's own doc.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub enum PendingApiKeyOp {
    Push(String),
    /// Issue #196 (shape 2): the rehydration counterpart to `Push`.
    /// Deliberately does NOT unconditionally supersede whatever is already
    /// queued — see [`TaskCoreCell::rehydrate_api_key`].
    Rehydrate(String),
    Clear,
}

/// The bundle [`TaskCoreCell`] hands a checkout or a read: the session
/// every emitted event correlates through (`seq`, `session_id`) plus the
/// sink it writes into, and the monotonic origin `elapsed_ms` measures
/// from. A thin, `Copy`-able view — nothing here owns `TaskCoreCell`'s
/// `RefCell`, so holding one across an `.await` is always safe.
#[derive(Clone, Copy)]
pub struct OperationDiagnostics<'a> {
    session: &'a DiagnosticSession<'a>,
    sink: &'a dyn DiagnosticSink,
    origin_monotonic_ms: u64,
}

impl<'a> OperationDiagnostics<'a> {
    fn emit(&self, wall_clock_ms: i64, operation_id: Option<&str>, event: DiagnosticEvent) {
        let elapsed_ms = now_monotonic_ms().saturating_sub(self.origin_monotonic_ms);
        self.session.emit(self.sink, wall_clock_ms, elapsed_ms, operation_id, event);
    }

    /// [`TaskHostCore::capture`]/[`TaskHostCore::triage`]'s own "did the
    /// request even reach `Core`" marker (#708) — emitted before the seam's
    /// validation, the one moment every attempt (accepted, rejected, or
    /// blocked behind a busy core) shares.
    pub fn emit_operation_requested(&self, wall_clock_ms: i64, operation_id: &str) {
        self.emit(wall_clock_ms, Some(operation_id), DiagnosticEvent::OperationRequested);
    }

    /// The moment the durable mutation actually commits — inside the
    /// outbound-queue enqueue path, never at the entry point (see this
    /// module's own `capture`/`triage` docs for why the placement matters):
    /// this is what separates "never reached the core" from "saved
    /// locally, never synchronised" (#704's hypothesis 4).
    pub fn emit_operation_local_commit(&self, wall_clock_ms: i64, operation_id: &str) {
        self.emit(wall_clock_ms, Some(operation_id), DiagnosticEvent::OperationLocalCommit);
    }

    pub fn emit_operation_finished(&self, wall_clock_ms: i64, operation_id: &str, outcome: OperationOutcome) {
        self.emit(
            wall_clock_ms,
            Some(operation_id),
            DiagnosticEvent::OperationFinished { outcome },
        );
    }

    /// #708: closes the observability gap #704 names directly — before
    /// this, [`Core::run`]/[`Core::run_observed`] returning
    /// [`CoreCycleOutcome::Held`] (a credential already known dead from an
    /// earlier 401) or [`CoreCycleOutcome::NoCredential`] short-circuits
    /// *before* any cycle diagnostics fire, so every subsequent sync
    /// attempt while the hold lasts left the journal completely silent —
    /// no `sync.started`, nothing. `Held` becomes a
    /// `sync.finished{outcome: credential_needed}`, the same outcome name
    /// the original 401 itself produced, so the journal shows the ongoing
    /// hold rather than a gap; never the token value, only the closed
    /// outcome name. `NoCredential` (nobody ever pushed a token) is not a
    /// "hold" and emits nothing, the same as it always has. A real cycle
    /// (`CoreCycleOutcome::Cycle`) is mapped the same way `client/core`'s
    /// own `sync_outcome_of` maps [`CycleOutcome`] — covering, in
    /// particular, a connection error (`PullFailed`) and a persistence
    /// failure (`PersistFailed`) with the classified outcome the brief's
    /// acceptance list asks for, since [`TaskHostCore::run`] does not (this
    /// slice, deliberately — see its own doc) route through the full
    /// `run_observed`/`DiagnosticsContext` machinery.
    pub fn emit_sync_outcome(&self, wall_clock_ms: i64, outcome: &CoreCycleOutcome) {
        let mapped = match outcome {
            CoreCycleOutcome::NoCredential => return,
            CoreCycleOutcome::Held => SyncOutcome::CredentialNeeded,
            CoreCycleOutcome::Cycle(cycle) => sync_outcome_of(cycle),
        };
        self.emit(wall_clock_ms, None, DiagnosticEvent::SyncFinished { outcome: mapped });
    }
}

/// Collapses [`CycleOutcome`] to the redacted [`SyncOutcome`] — the exact
/// mapping `client/core/src/diagnostics/context.rs::sync_outcome_of` uses
/// for its own (cycle-scoped) emission; duplicated here in miniature
/// rather than imported because that function takes a `&DiagnosticsContext`
/// call site this crate's own (cycle-less) `run` never builds. Kept
/// side-by-side with that function's own doc so a future change to either
/// vocabulary is easy to notice.
fn sync_outcome_of(outcome: &CycleOutcome) -> SyncOutcome {
    match outcome {
        CycleOutcome::Skipped => SyncOutcome::Skipped,
        CycleOutcome::Blocked { .. } => SyncOutcome::Blocked,
        CycleOutcome::CredentialNeeded { .. } => SyncOutcome::CredentialNeeded,
        CycleOutcome::PersistFailed { .. } => SyncOutcome::PersistFailed,
        CycleOutcome::PullFailed { .. } => SyncOutcome::PullFailed,
        CycleOutcome::Completed { .. } => SyncOutcome::Completed,
    }
}

/// #708: the single [`TaskHostCore`] checkout, instrumented. Wraps the
/// `RefCell<Option<_>>` re-entrancy guard issue #95 already required
/// (moved here from `ffi-web/src/lib.rs`'s `TaskShared`, whose own
/// `push_api_key`/`rehydrate_api_key`/`clear_api_key` and their deferred
/// [`PendingApiKeyOp`] travel with it) with the `core.*` span every
/// acquisition now gets, and tracks *who* currently holds it so a
/// concurrent `core.busy` can name them (`DiagnosticEvent::CoreBusy`'s
/// `owner`, #708's amendment to the shared contract) — the one fact a bare
/// `RefCell::take()` returning `None` could never answer. Lives here
/// rather than in `lib.rs` so it is testable with plain `cargo test`, the
/// same reason [`TaskHostCore`] itself is (this module's own header).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub struct TaskCoreCell {
    host: RefCell<Option<TaskHostCore>>,
    /// Which [`CoreOwner`] currently holds `host`, if any.
    holder: Cell<Option<CoreOwner>>,
    pending_op: RefCell<Option<PendingApiKeyOp>>,
    sink: DiagnosticBuffer,
    session: DiagnosticSession<'static>,
    origin_monotonic_ms: u64,
    next_operation_seq: AtomicU64,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
impl TaskCoreCell {
    /// `session_id` is leaked to `'static` once, for the life of this
    /// host — a `TaskCoreCell` is constructed exactly once per browser tab
    /// group's `SharedWorker` (ADR-0010), so this is a single, bounded
    /// leak, not a per-call one.
    pub fn new(host: TaskHostCore, session_id: String) -> Self {
        let session_id: &'static str = Box::leak(session_id.into_boxed_str());
        Self {
            host: RefCell::new(Some(host)),
            holder: Cell::new(None),
            pending_op: RefCell::new(None),
            sink: DiagnosticBuffer::default(),
            session: DiagnosticSession::new(session_id, now_monotonic_ms()),
            origin_monotonic_ms: now_monotonic_ms(),
            next_operation_seq: AtomicU64::new(0),
        }
    }

    fn diagnostics(&self) -> OperationDiagnostics<'_> {
        OperationDiagnostics {
            session: &self.session,
            sink: &self.sink,
            origin_monotonic_ms: self.origin_monotonic_ms,
        }
    }

    /// Mints a fresh, host-local id for correlating one call's `core.*`
    /// and (for capture/triage) `operation.*` events — independent of the
    /// caller's own mutation `seed` (a different concept: `seed` mints a
    /// deterministic *item* id sent to the server, ADR-0007; this
    /// correlates *diagnostic events about one call*, purely local, and
    /// never crosses the wire).
    pub fn mint_operation_id(&self) -> String {
        format!("op-{}", self.next_operation_seq.fetch_add(1, Ordering::Relaxed))
    }

    /// Drains every buffered event since the last drain — the wasm
    /// binding's `drainDiagnostics` (`lib.rs`) calls this directly.
    pub fn drain_diagnostics(&self) -> Vec<DiagnosticEventV1> {
        self.sink.drain()
    }

    /// The bare re-entrancy slot, for the read-only getters this slice did
    /// not extend to emit `core.*` diagnostics (a scoped decision — see
    /// this issue's posted finding). `.borrow()`/`.borrow_mut()` on it is
    /// exactly the un-instrumented behaviour every one of those getters
    /// already had before #708, just reached through `TaskCoreCell` now
    /// that it owns the slot instead of `lib.rs`'s old `TaskShared`.
    /// [`TaskCoreCell::read`] is the instrumented alternative — used by
    /// [`TaskHostCore::projects`]'s wasm binding specifically, since that
    /// is the acceptance criterion's named "a project read".
    pub fn host_ref(&self) -> &RefCell<Option<TaskHostCore>> {
        &self.host
    }

    /// Checks out the host for `owner`'s write, emitting `core.wait_started`
    /// then either `core.acquired` (returning `Some` guard) or
    /// `core.busy{owner: <holder>}` (returning `None`) naming whoever
    /// currently holds it. The wait/acquire pair brackets a genuine `.take()`
    /// attempt rather than a real async wait — see [`CoreGuard`]'s own doc
    /// for why that is still exactly what #704 needs made visible.
    pub fn checkout(&self, owner: CoreOwner, now_ms: i64) -> Option<CoreGuard<'_>> {
        let diagnostics = self.diagnostics();
        diagnostics.emit(now_ms, None, DiagnosticEvent::CoreWaitStarted);
        match self.host.borrow_mut().take() {
            Some(host) => {
                self.holder.set(Some(owner));
                diagnostics.emit(now_ms, None, DiagnosticEvent::CoreAcquired);
                Some(CoreGuard {
                    cell: self,
                    host: Some(host),
                    now_ms,
                })
            }
            None => {
                let current_holder = self.holder.get().unwrap_or(owner);
                diagnostics.emit(
                    now_ms,
                    None,
                    DiagnosticEvent::CoreBusy { owner: current_holder },
                );
                None
            }
        }
    }

    /// The read-only getters' own diagnostics twin to
    /// [`TaskCoreCell::checkout`] — a `.borrow()`, never a `.take()`, so it
    /// never contends with itself, only with a write's checkout. Emits the
    /// identical `core.wait_started`/(`core.acquired` xor
    /// `core.busy{owner}`)/`core.released` triad, so a read that finds the
    /// core held behind a sync cycle is visible in the journal exactly the
    /// same way a blocked write is — #704's "a … project read sat waiting
    /// behind [sync]" claim, made checkable. `now_ms` is wasm-boundary
    /// supplied (`lib.rs`'s `js_sys::Date::now()`), never sampled here.
    pub fn read<T>(&self, owner: CoreOwner, now_ms: i64, on_busy: T, f: impl FnOnce(&TaskHostCore) -> T) -> T {
        let diagnostics = self.diagnostics();
        diagnostics.emit(now_ms, None, DiagnosticEvent::CoreWaitStarted);
        match self.host.borrow().as_ref() {
            Some(host) => {
                diagnostics.emit(now_ms, None, DiagnosticEvent::CoreAcquired);
                let result = f(host);
                diagnostics.emit(now_ms, None, DiagnosticEvent::CoreReleased);
                result
            }
            None => {
                let current_holder = self.holder.get().unwrap_or(owner);
                diagnostics.emit(
                    now_ms,
                    None,
                    DiagnosticEvent::CoreBusy { owner: current_holder },
                );
                on_busy
            }
        }
    }

    /// Applies whatever [`PendingApiKeyOp`] a setter deferred while the
    /// host was checked out, then returns it — called from
    /// [`CoreGuard`]'s check-in, the exact moment `ffi-web/src/lib.rs`'s
    /// `TaskShared::check_in` used to.
    fn apply_pending_op(&self, host: &mut TaskHostCore) {
        match self.pending_op.borrow_mut().take() {
            Some(PendingApiKeyOp::Clear) => host.clear_api_key(),
            Some(PendingApiKeyOp::Push(api_key)) => host.push_api_key(api_key),
            Some(PendingApiKeyOp::Rehydrate(api_key)) => host.rehydrate_api_key(api_key),
            None => {}
        }
    }

    /// Pushes immediately if the host is present, or queues for the next
    /// check-in if it is currently checked out — never silently drops the
    /// key either way. A queued push supersedes any other queued op
    /// unconditionally, including a queued rehydration — see
    /// [`PendingApiKeyOp`]'s doc for why that is the correct, not merely
    /// convenient, choice.
    pub fn push_api_key(&self, api_key: String) {
        match self.host.borrow_mut().as_mut() {
            Some(host) => host.push_api_key(api_key),
            None => *self.pending_op.borrow_mut() = Some(PendingApiKeyOp::Push(api_key)),
        }
    }

    /// Issue #196 (shape 2): the rehydration counterpart to
    /// [`TaskCoreCell::push_api_key`] — applies immediately if the host is
    /// present, or queues otherwise, but never resumes a hold either way.
    /// Deliberately does NOT overwrite an already-queued `Push`.
    pub fn rehydrate_api_key(&self, api_key: String) {
        match self.host.borrow_mut().as_mut() {
            Some(host) => host.rehydrate_api_key(api_key),
            None => {
                let mut pending = self.pending_op.borrow_mut();
                if !matches!(*pending, Some(PendingApiKeyOp::Push(_))) {
                    *pending = Some(PendingApiKeyOp::Rehydrate(api_key));
                }
            }
        }
    }

    /// "Forget token" (#106/S8): clears immediately if the host is
    /// present, or queues for the next check-in otherwise. A queued clear
    /// supersedes any other queued op unconditionally, same as a queued
    /// push.
    pub fn clear_api_key(&self) {
        match self.host.borrow_mut().as_mut() {
            Some(host) => host.clear_api_key(),
            None => *self.pending_op.borrow_mut() = Some(PendingApiKeyOp::Clear),
        }
    }
}

/// One checked-out [`TaskHostCore`], released through [`Drop`] rather than
/// a hand-written call at every return — the brief's own warning, verbatim:
/// "A hand-written release call at each `return` is the thing that will be
/// wrong on the one path that matters." `Drop` runs whenever this value
/// goes out of scope for *any* reason Rust's own scoping already handles —
/// a normal `return`, an early `?`/`else` bail, or the enclosing `Future`
/// itself being dropped mid-`.await` (a cancelled or abandoned request) —
/// so a cancelled operation closes its `core.released` span exactly like a
/// completed one, with no second code path to keep in sync.
///
/// The release event's `wall_clock_ms` reuses whatever `now_ms` this guard
/// was checked out with (there is no fresher one to sample at `Drop` time —
/// this crate never samples a clock of its own, the same discipline
/// `hummingbird_core::diagnostics` documents); `elapsed_ms` is real and
/// accurate regardless of when `Drop` runs, since [`now_monotonic_ms`] is
/// callable synchronously at any time, including from inside `Drop`.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub struct CoreGuard<'a> {
    cell: &'a TaskCoreCell,
    host: Option<TaskHostCore>,
    now_ms: i64,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
impl<'a> CoreGuard<'a> {
    pub fn diagnostics(&self) -> OperationDiagnostics<'a> {
        self.cell.diagnostics()
    }

    pub fn mint_operation_id(&self) -> String {
        self.cell.mint_operation_id()
    }
}

impl std::ops::Deref for CoreGuard<'_> {
    type Target = TaskHostCore;
    fn deref(&self) -> &TaskHostCore {
        self.host.as_ref().expect("CoreGuard drops its host exactly once")
    }
}

impl std::ops::DerefMut for CoreGuard<'_> {
    fn deref_mut(&mut self) -> &mut TaskHostCore {
        self.host.as_mut().expect("CoreGuard drops its host exactly once")
    }
}

impl Drop for CoreGuard<'_> {
    fn drop(&mut self) {
        if let Some(mut host) = self.host.take() {
            self.cell.apply_pending_op(&mut host);
            self.cell.holder.set(None);
            *self.cell.host.borrow_mut() = Some(host);
            self.cell
                .diagnostics()
                .emit(self.now_ms, None, DiagnosticEvent::CoreReleased);
        }
    }
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

    /// Items already grilled once and still foggy, per
    /// [`Core::grilling_items`] — the "triage process" queue's second half
    /// (#357, CONTEXT.md). Same per-item `pending` stamp as
    /// [`TaskHostCore::frontier`].
    pub fn grilling_items(&self) -> ItemListResponse {
        ItemListResponse {
            kind: "ok",
            items: self
                .core
                .grilling_items()
                .into_iter()
                .map(|item| self.with_pending(item))
                .collect(),
        }
    }

    /// Items on an **external wait**, per [`Core::externally_blocked`] —
    /// CONTEXT.md's only meaning for `Stage::Blocked`, and a different
    /// fact from [`TaskHostCore::blocked`]'s relation blockers. Same
    /// per-item `pending` stamp as [`TaskHostCore::frontier`].
    ///
    /// No screen lists these; it exists so the standing-question pane
    /// inputs can be the *whole* live partition of the mirror (#675 —
    /// `NowScreen.tsx`'s `realQuestionInputs`).
    pub fn externally_blocked(&self) -> ItemListResponse {
        ItemListResponse {
            kind: "ok",
            items: self
                .core
                .externally_blocked()
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

    /// **Recall** (#478), per [`Core::search`]: re-find one item across the
    /// whole retained roster by remembered words or by handle. `now_ms`
    /// resolves the same alert-liveness read [`TaskHostCore::ledger`] does
    /// (`search` shares its corpus with `ledger`), and `total` is the
    /// un-capped match count. Same per-item `pending` stamp as
    /// [`TaskHostCore::frontier`], through the same single site.
    pub fn search(&self, query: &str, now_ms: i64) -> SearchResponse {
        let outcome = self.core.search(query, now_ms);
        SearchResponse {
            kind: "ok",
            total: outcome.total,
            rows: outcome
                .rows
                .into_iter()
                .map(|row| {
                    let pending = self.core.is_pending(&row.item.id);
                    SearchRowDTO {
                        pending,
                        group: row.group,
                        item: row.item,
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

    /// Every standing question's off switch, per
    /// [`Core::question_switches`] (#715, ADR-0034) — in `QUESTION_ORDER`,
    /// every question present whether it has a row or not.
    ///
    /// A second door beside [`TaskHostCore::bindings`] rather than a field
    /// on it: the two are different vocabularies over the same table
    /// (ADR-0034 decision 2), and `Core::bindings` now subtracts these rows
    /// so the editor never offers a toggle as free text.
    pub fn question_switches(&self) -> QuestionSwitchListResponse {
        QuestionSwitchListResponse {
            kind: "ok",
            switches: self.core.question_switches(),
        }
    }

    /// Switches one standing question on or off (#715), per
    /// [`Core::set_question_enabled`] — which overlays, so the next
    /// [`TaskHostCore::question_switches`] reports the new state and
    /// `pending` immediately.
    ///
    /// `question` is the wire spelling, resolved by name here and never
    /// passed through raw — the same "reject before the seam" discipline
    /// [`TaskHostCore::set_binding`] applies, and load-bearing for the same
    /// second reason: the key this mints lands in a table with no DELETE,
    /// so a question name invented by a caller would leave a permanent row
    /// nothing can ever read again.
    pub async fn set_question_enabled(
        &mut self,
        seed: &str,
        question: &str,
        enabled: bool,
        now_ms: i64,
    ) -> SetQuestionEnabledResponse {
        let Some(question) = QUESTION_ORDER
            .iter()
            .copied()
            .find(|candidate| candidate.as_str() == question)
        else {
            return SetQuestionEnabledResponse {
                kind: "unknown_question",
                error: Some(format!("unrecognised standing question {question:?}")),
            };
        };
        match self
            .core
            .set_question_enabled(seed, question, enabled, now_ms)
            .await
        {
            Ok(()) => SetQuestionEnabledResponse { kind: "ok", error: None },
            Err(error) => SetQuestionEnabledResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
        }
    }

    /// Every live project, per [`Core::projects`] — resolves the frontier's
    /// grouping to real project names (issue #108, PR #200 review) — plus the
    /// archived ones, per [`Core::archived_projects`], which only the
    /// Projects grid reads and only behind its own toggle (#624).
    pub fn projects(&self) -> ProjectListResponse {
        ProjectListResponse {
            kind: "ok",
            projects: self.core.projects(),
            archived: self.core.archived_projects(),
        }
    }

    /// Creates a project, per [`Core::create_project`] (#624). The name is
    /// trimmed and an empty one is refused **before `Core` is reached** —
    /// the authority answers 400 on `name.is_empty()`
    /// (`server/authority/src/handlers/projects.rs`), and every such rule is
    /// checked at this seam, the same discipline capture and triage follow.
    /// Without it a blank name typed into the grid's New-project card would
    /// queue a mutation that dead-letters later, with nothing on screen to
    /// say so.
    pub async fn create_project(&mut self, seed: &str, name: &str, now_ms: i64) -> CreateProjectResponse {
        let name = name.trim();
        if name.is_empty() {
            return CreateProjectResponse {
                kind: "failed",
                id: None,
                error: Some("name must be non-empty".to_string()),
            };
        }
        match self.core.create_project(seed, name, now_ms).await {
            Ok(id) => CreateProjectResponse { kind: "ok", id: Some(id), error: None },
            Err(error) => CreateProjectResponse {
                kind: "failed",
                id: None,
                error: Some(error.to_string()),
            },
        }
    }

    /// Patches a project, per [`Core::patch_project`] (#625) — the dossier's
    /// properties card sets and clears `github_repo`/`default_context`
    /// through this one entry point, alongside renaming and archiving.
    /// `github_repo`, when present and non-null, is checked with
    /// [`is_valid_github_repo`] **before `Core` is reached** — the wasm-seam
    /// half of "validate at the handler and the wasm seam," the authority's
    /// own `handlers/projects.rs` carrying the other half. Without it a
    /// malformed slug typed into the card would queue a mutation that
    /// dead-letters later, with nothing on screen to say why.
    ///
    /// `current` is the caller's own last-known copy of the row (from
    /// [`TaskHostCore::projects`]), the same "caller supplies `base`"
    /// contract every other CAS write here follows.
    /// `name`/`github_repo_touched`+`github_repo`/`default_context_touched`+
    /// `default_context`/`archived_at_touched`+`archived_at` mirror
    /// [`TaskHostCore::patch_rule`]'s `event_kind_touched` shape: `wasm_bindgen`
    /// has no `Option<Option<T>>` argument shape, so each nullable field
    /// arrives as a touched flag plus a value.
    #[allow(clippy::too_many_arguments)]
    pub async fn patch_project(
        &mut self,
        seed: &str,
        current: &Project,
        name: Option<String>,
        github_repo_touched: bool,
        github_repo: Option<String>,
        default_context_touched: bool,
        default_context: Option<String>,
        archived_at_touched: bool,
        archived_at: Option<i64>,
        now_ms: i64,
    ) -> PatchProjectResponse {
        if let Some(repo) = &github_repo {
            if !is_valid_github_repo(repo) {
                return PatchProjectResponse {
                    kind: "failed",
                    error: Some(format!("github_repo must be owner/repo, got {repo:?}")),
                };
            }
        }
        let github_repo = github_repo_touched.then_some(github_repo);
        let default_context = default_context_touched.then_some(default_context);
        let archived_at = archived_at_touched.then_some(archived_at);
        match self
            .core
            .patch_project(seed, current, name, github_repo, default_context, archived_at, now_ms)
            .await
        {
            Ok(()) => PatchProjectResponse { kind: "ok", error: None },
            Err(error) => PatchProjectResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
        }
    }

    /// Every live Link on one project, per [`Core::links_for`] (#626) — the
    /// dossier aside's read.
    pub fn project_links(&self, project_id: &str) -> ProjectLinkListResponse {
        ProjectLinkListResponse {
            kind: "ok",
            links: self.core.links_for(project_id),
        }
    }

    /// Creates a project Link, per [`Core::create_project_link`] (#626).
    /// The url is trimmed and an empty one is refused **before `Core` is
    /// reached** — the authority answers 400 on `url.is_empty()`
    /// (`server/authority/src/handlers/project_links.rs`), same discipline
    /// [`TaskHostCore::create_project`] follows for `name`. `label`, when
    /// present, is trimmed to `None` if it comes out empty.
    pub async fn create_project_link(
        &mut self,
        seed: &str,
        project_id: &str,
        url: &str,
        label: Option<String>,
        position: i64,
        now_ms: i64,
    ) -> CreateProjectLinkResponse {
        let url = url.trim();
        if url.is_empty() {
            return CreateProjectLinkResponse {
                kind: "failed",
                id: None,
                error: Some("url must be non-empty".to_string()),
            };
        }
        let label = label.map(|l| l.trim().to_string()).filter(|l| !l.is_empty());
        match self
            .core
            .create_project_link(seed, project_id, url, label, position, now_ms)
            .await
        {
            Ok(id) => CreateProjectLinkResponse { kind: "ok", id: Some(id), error: None },
            Err(error) => CreateProjectLinkResponse {
                kind: "failed",
                id: None,
                error: Some(error.to_string()),
            },
        }
    }

    /// Patches a project Link, per [`Core::patch_project_link`] (#626) —
    /// editing its url/label, reordering it, or flagging/clearing its
    /// removal, all through this one entry point. `current` is the
    /// caller's own last-known copy of the row (from
    /// [`TaskHostCore::project_links`]), the same "caller supplies `base`"
    /// contract every other CAS write here follows.
    /// `url`/`labelTouched`+`label`/`position`/`removedAtTouched`+
    /// `removedAt` mirror [`TaskHostCore::patch_project`]'s touched-flag
    /// shape: `wasm_bindgen` has no `Option<Option<T>>` argument shape, so
    /// each nullable field arrives as a touched flag plus a value.
    #[allow(clippy::too_many_arguments)]
    pub async fn patch_project_link(
        &mut self,
        seed: &str,
        current: &ProjectLink,
        url: Option<String>,
        label_touched: bool,
        label: Option<String>,
        position: Option<i64>,
        removed_at_touched: bool,
        removed_at: Option<i64>,
        now_ms: i64,
    ) -> PatchProjectLinkResponse {
        if let Some(url) = &url {
            if url.trim().is_empty() {
                return PatchProjectLinkResponse {
                    kind: "failed",
                    error: Some("url must be non-empty".to_string()),
                };
            }
        }
        let label = label_touched.then_some(label);
        let removed_at = removed_at_touched.then_some(removed_at);
        match self
            .core
            .patch_project_link(seed, current, url, label, position, removed_at, now_ms)
            .await
        {
            Ok(()) => PatchProjectLinkResponse { kind: "ok", error: None },
            Err(error) => PatchProjectLinkResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
        }
    }

    /// One project's Route, per [`Core::route`] (#627) — the dossier's
    /// reading column's read.
    pub fn route(&self, project_id: &str) -> RouteResponse {
        RouteResponse {
            kind: "ok",
            route: self.core.route(project_id),
        }
    }

    /// Patches a project's Route, per [`Core::patch_route`] (#627) — the
    /// dossier's reading column, editing `destination`/`notes` through this
    /// one entry point. `current` is the caller's own last-known copy of
    /// the row (from [`TaskHostCore::route`]), the same "caller supplies
    /// `base`" contract every other CAS write here follows.
    /// `destination_touched`+`destination`/`notes_touched`+`notes` mirror
    /// [`TaskHostCore::patch_project`]'s touched-flag shape: `wasm_bindgen`
    /// has no `Option<Option<T>>` argument shape, so each nullable field
    /// arrives as a touched flag plus a value.
    #[allow(clippy::too_many_arguments)]
    pub async fn patch_route(
        &mut self,
        seed: &str,
        current: &Route,
        destination_touched: bool,
        destination: Option<String>,
        notes_touched: bool,
        notes: Option<String>,
        now_ms: i64,
    ) -> PatchRouteResponse {
        let destination = destination_touched.then_some(destination);
        let notes = notes_touched.then_some(notes);
        match self.core.patch_route(seed, current, destination, notes, now_ms).await {
            Ok(()) => PatchRouteResponse { kind: "ok", error: None },
            Err(error) => PatchRouteResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
        }
    }

    /// Every open Fog segment on one project, per [`Core::open_fog_for`]
    /// (#628) — the dossier reading column's read.
    pub fn open_fog(&self, project_id: &str) -> FogListResponse {
        FogListResponse {
            kind: "ok",
            fog: self.core.open_fog_for(project_id),
        }
    }

    /// Creates a Fog segment, per [`Core::create_fog`] (#628). The question
    /// is trimmed and an empty one is refused **before `Core` is
    /// reached** — the authority answers 400 on `question.is_empty()`
    /// (`server/authority/src/handlers/fog.rs`), same discipline
    /// [`TaskHostCore::create_project_link`] follows for `url`.
    pub async fn create_fog(
        &mut self,
        seed: &str,
        project_id: &str,
        question: &str,
        position: i64,
        now_ms: i64,
    ) -> CreateFogResponse {
        let question = question.trim();
        if question.is_empty() {
            return CreateFogResponse {
                kind: "failed",
                id: None,
                error: Some("question must be non-empty".to_string()),
            };
        }
        match self.core.create_fog(seed, project_id, question, position, now_ms).await {
            Ok(id) => CreateFogResponse { kind: "ok", id: Some(id), error: None },
            Err(error) => CreateFogResponse {
                kind: "failed",
                id: None,
                error: Some(error.to_string()),
            },
        }
    }

    /// Patches a Fog segment, per [`Core::patch_fog`] (#628) — rewording
    /// its question, repositioning it, or resolving/reopening it, all
    /// through this one entry point. `current` is the caller's own
    /// last-known copy of the row (from [`TaskHostCore::open_fog`]), the
    /// same "caller supplies `base`" contract every other CAS write here
    /// follows. `question`/`position`/`resolved_at_touched`+`resolved_at`
    /// mirror [`TaskHostCore::patch_project_link`]'s touched-flag shape:
    /// `wasm_bindgen` has no `Option<Option<T>>` argument shape, so the
    /// nullable field arrives as a touched flag plus a value.
    #[allow(clippy::too_many_arguments)]
    pub async fn patch_fog(
        &mut self,
        seed: &str,
        current: &Fog,
        question: Option<String>,
        position: Option<i64>,
        resolved_at_touched: bool,
        resolved_at: Option<i64>,
        now_ms: i64,
    ) -> PatchFogResponse {
        if let Some(question) = &question {
            if question.trim().is_empty() {
                return PatchFogResponse {
                    kind: "failed",
                    error: Some("question must be non-empty".to_string()),
                };
            }
        }
        let resolved_at = resolved_at_touched.then_some(resolved_at);
        match self.core.patch_fog(seed, current, question, position, resolved_at, now_ms).await {
            Ok(()) => PatchFogResponse { kind: "ok", error: None },
            Err(error) => PatchFogResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
        }
    }

    /// Every live Action on a project, per [`Core::actions_for`] (#629) —
    /// the dossier's ordered action list. An Action is an ordinary item, so
    /// this carries the same pending-flag-stamped shape every other item
    /// list here does ([`TaskHostCore::with_pending`]), unlike
    /// [`TaskHostCore::project_links`]/[`TaskHostCore::open_fog`], whose
    /// rows carry no such flag.
    pub fn project_actions(&self, project_id: &str) -> ItemListResponse {
        ItemListResponse {
            kind: "ok",
            items: self
                .core
                .actions_for(project_id)
                .into_iter()
                .map(|item| self.with_pending(item))
                .collect(),
        }
    }

    /// Moves one Action's `project_pos`, per [`Core::patch_action_position`]
    /// (#629) — the dossier's reorder control. `current` is the caller's
    /// own last-known copy of the row (from
    /// [`TaskHostCore::project_actions`]), the same "caller supplies
    /// `base`" contract every other CAS write here follows. A 409 here is
    /// an ordinary outcome (ADR-0030 decision 1: `project_pos` is
    /// shared-owned with `/to-actions`), handled by the same
    /// rebase-and-retry machinery and dead-letter journal every other CAS
    /// write here uses.
    pub async fn patch_action_position(
        &mut self,
        seed: &str,
        current: &Item,
        position: i64,
        now_ms: i64,
    ) -> PatchActionPositionResponse {
        match self.core.patch_action_position(seed, current, position, now_ms).await {
            Ok(()) => PatchActionPositionResponse { kind: "ok", error: None },
            Err(error) => PatchActionPositionResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
        }
    }

    /// Creates a Step on an item's checklist, per [`Core::create_step`]
    /// (#629). The body is trimmed and an empty one is refused **before
    /// `Core` is reached** — the authority answers 400 on
    /// `body.is_empty()` (`server/authority/src/handlers/steps.rs`), same
    /// discipline [`TaskHostCore::create_fog`] follows for `question`.
    pub async fn create_step(
        &mut self,
        seed: &str,
        item_id: &str,
        body: &str,
        position: i64,
        now_ms: i64,
    ) -> CreateStepResponse {
        let body = body.trim();
        if body.is_empty() {
            return CreateStepResponse {
                kind: "failed",
                id: None,
                error: Some("body must be non-empty".to_string()),
            };
        }
        match self.core.create_step(seed, item_id, body, position, now_ms).await {
            Ok(id) => CreateStepResponse { kind: "ok", id: Some(id), error: None },
            Err(error) => CreateStepResponse {
                kind: "failed",
                id: None,
                error: Some(error.to_string()),
            },
        }
    }

    /// Patches a Step, per [`Core::patch_step`] (#629) — ticking, rewording,
    /// repositioning, or flagging/clearing its deletion (ADR-0020), all
    /// through this one entry point. `current` is the caller's own
    /// last-known copy of the row (from [`TaskHostCore::steps`]), the same
    /// "caller supplies `base`" contract every other CAS write here
    /// follows. `deleted_at_touched` distinguishes "leave this field
    /// alone" (`false`) from "set it, possibly to `null`" (`true`, with
    /// the paired value carrying the new value or `None`) — the same
    /// double-`Option` [`hummingbird_domain::StepPatch::deleted_at`]
    /// itself carries, flattened for the wasm boundary exactly like
    /// [`TaskHostCore::patch_fog`]'s `resolved_at_touched`.
    #[allow(clippy::too_many_arguments)]
    pub async fn patch_step(
        &mut self,
        seed: &str,
        current: &Step,
        body: Option<String>,
        done: Option<bool>,
        position: Option<i64>,
        deleted_at_touched: bool,
        deleted_at: Option<i64>,
        now_ms: i64,
    ) -> PatchStepResponse {
        if let Some(body) = &body {
            if body.trim().is_empty() {
                return PatchStepResponse {
                    kind: "failed",
                    error: Some("body must be non-empty".to_string()),
                };
            }
        }
        let deleted_at = deleted_at_touched.then_some(deleted_at);
        match self.core.patch_step(seed, current, body, done, position, deleted_at, now_ms).await {
            Ok(()) => PatchStepResponse { kind: "ok", error: None },
            Err(error) => PatchStepResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
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
            sources: hummingbird_domain::REGISTRY
                .iter()
                .map(|entry| SourceOptionDTO {
                    source: entry.source,
                    retired_as: entry.retired_as,
                })
                .collect(),
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
    /// `event_kind`/`conditions`/`severity`/`tier`/`enabled`/`deleted_at`
    /// are each `None` to mean "leave this field alone." `tier`, when
    /// present, is the wire's snake_case name, resolved through
    /// [`Tier::parse`] before it can reach `Core`.
    ///
    /// **Deleting a rule is this method with `deleted_at` set** — one
    /// flagged column on the one CAS write, per [`Core::patch_rule`]; there
    /// is no delete entry point here either.
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
        // The same flattened double-`Option` as `event_kind` above:
        // `deleted_at_touched` with `deleted_at: None` is the explicit
        // `null` that un-deletes.
        deleted_at_touched: bool,
        deleted_at: Option<i64>,
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
        let deleted_at = deleted_at_touched.then_some(deleted_at);
        match self
            .core
            .patch_rule(
                seed, current, name, event_kind, conditions, severity, tier, enabled, deleted_at,
                now_ms,
            )
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
    /// `fields` (#208, widened here) carries everything the capture box may
    /// set. `size`/`energy` are each resolved by name through
    /// `hummingbird_domain`'s own vocabulary (`Size::parse`/`Energy::parse`),
    /// and `priority`/`deadline`/`scheduled_date` are checked with the same
    /// rules [`TaskHostCore::triage`] applies to its own edits — the same
    /// "reject before the seam" discipline, so an input the authority would
    /// 400 on fails here and never reaches [`Core::capture`] or the queue.
    /// `context` and `description` carry straight through unparsed.
    ///
    /// #708: `diagnostics`/`operation_id` bracket the attempt with
    /// `operation.requested` (emitted here, once the core is already
    /// checked out — a validation rejection below still counts as an
    /// attempt) and `operation.finished{outcome}` (success or failure —
    /// see [`CaptureResponse`]'s own doc for the two failure shapes this
    /// distinguishes). `operation.local_commit` fires only on the success
    /// path, immediately after [`Core::capture`]'s `.await` returns `Ok` —
    /// that call enqueues durably and makes no network call of its own, so
    /// this is provably after the write is durable and provably before any
    /// later cycle's `http.started` for the same mutation. A durability
    /// failure (`Err`) never emits `operation.local_commit` at all, which
    /// is exactly what tells the two failure modes apart in the journal: a
    /// capture blocked before reaching the core never gets an
    /// `operation_id` in the first place (checkout answered `core.busy`),
    /// while a capture whose durable commit failed has `operation.requested`
    /// and `operation.finished{failure}` but no `operation.local_commit`
    /// between them.
    #[allow(clippy::too_many_arguments)]
    pub async fn capture(
        &mut self,
        seed: &str,
        title: &str,
        stage: &str,
        fields: CaptureFields,
        now_ms: i64,
        diagnostics: OperationDiagnostics<'_>,
        operation_id: &str,
    ) -> CaptureResponse {
        diagnostics.emit_operation_requested(now_ms, operation_id);
        let fail = |message: String| CaptureResponse {
            kind: "failed",
            id: None,
            error: Some(message),
        };
        let Some(stage) = Stage::parse(stage) else {
            diagnostics.emit_operation_finished(now_ms, operation_id, OperationOutcome::Failure);
            return fail(format!("unrecognised stage {stage:?}"));
        };
        let size = match &fields.size {
            Some(raw) => match Size::parse(raw) {
                Some(size) => Some(size),
                None => {
                    diagnostics.emit_operation_finished(now_ms, operation_id, OperationOutcome::Failure);
                    return fail(format!("unrecognised size {raw:?}"));
                }
            },
            None => None,
        };
        let energy = match &fields.energy {
            Some(raw) => match Energy::parse(raw) {
                Some(energy) => Some(energy),
                None => {
                    diagnostics.emit_operation_finished(now_ms, operation_id, OperationOutcome::Failure);
                    return fail(format!("unrecognised energy {raw:?}"));
                }
            },
            None => None,
        };
        if let Some(priority) = fields.priority {
            if !(0..=4).contains(&priority) {
                diagnostics.emit_operation_finished(now_ms, operation_id, OperationOutcome::Failure);
                return fail("priority must be between 0 and 4".to_string());
            }
        }
        if let Some(deadline) = &fields.deadline {
            if !is_valid_deadline(deadline) {
                diagnostics.emit_operation_finished(now_ms, operation_id, OperationOutcome::Failure);
                return fail("deadline must be YYYY-MM-DD or YYYY-MM-DDTHH:MM".to_string());
            }
        }
        if let Some(scheduled_date) = &fields.scheduled_date {
            // The length check is what rules out `YYYY-MM-DDTHH:MM` while
            // still borrowing the shared calendar validation (leap years,
            // month lengths) rather than re-deriving it here — `triage`'s own
            // note carries the full argument.
            if scheduled_date.len() != 10 || !is_valid_deadline(scheduled_date) {
                diagnostics.emit_operation_finished(now_ms, operation_id, OperationOutcome::Failure);
                return fail("scheduled date must be YYYY-MM-DD".to_string());
            }
        }
        let options = CaptureOptions {
            size,
            energy,
            context: fields.context,
            description: fields.description,
            priority: fields.priority,
            project_id: fields.project_id,
            deadline: fields.deadline,
            scheduled_date: fields.scheduled_date,
        };
        match self.core.capture(seed, title, stage, now_ms, options).await {
            Ok(id) => {
                diagnostics.emit_operation_local_commit(now_ms, operation_id);
                diagnostics.emit_operation_finished(now_ms, operation_id, OperationOutcome::Success);
                CaptureResponse {
                    kind: "ok",
                    id: Some(id),
                    error: None,
                }
            }
            Err(error) => {
                diagnostics.emit_operation_finished(now_ms, operation_id, OperationOutcome::Failure);
                CaptureResponse {
                    kind: "failed",
                    id: None,
                    error: Some(error.to_string()),
                }
            }
        }
    }

    /// Acts on an already-existing item (S11/#109: start, complete, block,
    /// cancel). `action` is the wire's snake_case action name
    /// ([`ItemAction::parse`]); an unrecognised one fails without ever
    /// touching [`Core::act`], the same "reject before the seam" discipline
    /// [`TaskHostCore::capture`] uses for `stage`.
    pub async fn act(&mut self, seed: &str, item_id: &str, action: &str, now_ms: i64) -> ActResponse {
        let Some(action) = ItemAction::parse(action) else {
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
    /// `edits` sets and, when `destination` is `Some("ready")`, promotes it
    /// to Ready, as one CAS `PATCH` (never one mutation per field —
    /// [`Core::triage`]'s own doc). `"ready"` is the only recognised wire
    /// destination (#360) — an item reaches Grilling exactly one way, a
    /// `fog_remains` verdict from a completed Grill
    /// ([`TaskHostCore::complete_grill`]), never through this seam. Any other
    /// non-`None` string is rejected before [`Core::triage`] is ever called.
    /// `None` (#122) leaves `stage` untouched entirely — the weekend-plans
    /// pane's do-date chip triages an item that may already be `InProgress`,
    /// which a promotion would silently demote back to `Ready`, so a caller
    /// that only wants `edits.scheduled_date` applied passes no destination
    /// at all.
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
    ///
    /// #708: `diagnostics`/`operation_id` bracket the attempt the same way
    /// [`TaskHostCore::capture`]'s own doc describes — `operation.requested`
    /// once checked out, `operation.local_commit` only immediately after
    /// [`Core::triage`]'s `.await` returns `Ok` (its own durable CAS write,
    /// no network call of its own), and `operation.finished{outcome}`
    /// either way, so a rejected-before-the-seam edit, an unknown item id,
    /// and a genuine durability failure are all `Failure` with no
    /// `operation.local_commit` between them — distinguishable from a
    /// success, and from a triage blocked before ever reaching the core
    /// (`core.busy`, no `operation_id` minted at all).
    #[allow(clippy::too_many_arguments)]
    pub async fn triage(
        &mut self,
        seed: &str,
        item_id: &str,
        destination: Option<&str>,
        edits: TriageEdits,
        now_ms: i64,
        diagnostics: OperationDiagnostics<'_>,
        operation_id: &str,
    ) -> TriageResponse {
        diagnostics.emit_operation_requested(now_ms, operation_id);
        let fail_op = |response: TriageResponse| {
            diagnostics.emit_operation_finished(now_ms, operation_id, OperationOutcome::Failure);
            response
        };
        let promote_to_ready = match destination {
            Some("ready") => true,
            Some(raw) => {
                return fail_op(reject(format!("unrecognised triage destination {raw:?}")));
            }
            None => false,
        };
        if edits.title.as_deref() == Some("") {
            // The authority answers 400 on an empty title; a `NOT NULL`
            // column has no "cleared" state to fall back to.
            return fail_op(reject("title must be non-empty".to_string()));
        }
        let size = match parse_named("size", edits.size, Size::parse) {
            Ok(size) => size,
            Err(message) => return fail_op(reject(message)),
        };
        let energy = match parse_named("energy", edits.energy, Energy::parse) {
            Ok(energy) => energy,
            Err(message) => return fail_op(reject(message)),
        };
        if let Some(priority) = edits.priority {
            if !(0..=4).contains(&priority) {
                return fail_op(reject("priority must be between 0 and 4".to_string()));
            }
        }
        if let Some(Some(deadline)) = &edits.deadline {
            if !is_valid_deadline(deadline) {
                return fail_op(reject(
                    "deadline must be YYYY-MM-DD or YYYY-MM-DDTHH:MM".to_string(),
                ));
            }
        }
        if let Some(Some(scheduled_date)) = &edits.scheduled_date {
            // A whole day, so `is_valid_deadline`'s date-time form is not
            // enough on its own — the length check is what rules out
            // `YYYY-MM-DDTHH:MM` while still borrowing the shared calendar
            // validation (leap years, month lengths) rather than re-deriving
            // it here.
            if scheduled_date.len() != 10 || !is_valid_deadline(scheduled_date) {
                return fail_op(reject("scheduled date must be YYYY-MM-DD".to_string()));
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
        match self.core.triage(seed, item_id, promote_to_ready, patch, now_ms).await {
            Ok(()) => {
                diagnostics.emit_operation_local_commit(now_ms, operation_id);
                diagnostics.emit_operation_finished(now_ms, operation_id, OperationOutcome::Success);
                TriageResponse { kind: "ok", error: None }
            }
            Err(ActError::ItemNotFound) => fail_op(TriageResponse {
                kind: "not_found",
                error: Some("item not found".to_string()),
            }),
            Err(error) => fail_op(TriageResponse {
                kind: "failed",
                error: Some(error.to_string()),
            }),
        }
    }

    /// Confirms a completed Grill interview (#355, ADR-0023): the review
    /// card's Confirm button. `verdict` is the wire's snake_case spelling
    /// (`"resolved"`/`"fog_remains"`, [`GrillVerdict::parse`]) and
    /// `session_steps` the review session's own captured Steps as a JSON
    /// array — both rejected here, before [`Core::complete_grill`] is ever
    /// called, the same "reject before the seam" discipline
    /// [`TaskHostCore::capture`]/[`TaskHostCore::triage`] use.
    ///
    /// The completion's own fields arrive as separate scalars rather than one
    /// JSON payload (`triage`'s `edits` shape) because every one of them is a
    /// required scalar the review card always sends — there is no
    /// "untouched vs. cleared" distinction here for a patch object to carry —
    /// so the argument count is the wasm boundary's, not a grouping this side
    /// declined to make.
    #[allow(clippy::too_many_arguments)]
    pub async fn complete_grill(
        &mut self,
        seed: &str,
        item_id: &str,
        session_steps: &str,
        transcript: String,
        summary: String,
        verdict: &str,
        model_proposal: String,
        applied_patch: String,
        delete_unticked_plan: bool,
        now_ms: i64,
    ) -> CompleteGrillResponse {
        let Some(verdict) = GrillVerdict::parse(verdict) else {
            return CompleteGrillResponse {
                kind: "failed",
                id: None,
                error: Some(format!("unrecognised grill verdict {verdict:?}")),
            };
        };
        let session_steps: Vec<Step> = match serde_json::from_str(session_steps) {
            Ok(steps) => steps,
            Err(error) => {
                return CompleteGrillResponse {
                    kind: "failed",
                    id: None,
                    error: Some(format!("unreadable session steps: {error}")),
                };
            }
        };
        let completion = GrillCompletion {
            transcript,
            summary,
            verdict,
            model_proposal,
            applied_patch,
            delete_unticked_plan,
        };
        match self
            .core
            .complete_grill(seed, item_id, &session_steps, completion, now_ms)
            .await
        {
            Ok(id) => CompleteGrillResponse {
                kind: "ok",
                id: Some(id),
                error: None,
            },
            Err(CompleteGrillError::ItemNotFound) => CompleteGrillResponse {
                kind: "not_found",
                id: None,
                error: Some("item not found".to_string()),
            },
            Err(CompleteGrillError::ItemDone) => CompleteGrillResponse {
                kind: "item_done",
                id: None,
                error: Some("item is done".to_string()),
            },
            Err(CompleteGrillError::NeedsReReview) => CompleteGrillResponse {
                kind: "needs_re_review",
                id: None,
                error: Some(
                    "unticked steps changed since this review was last shown".to_string(),
                ),
            },
            Err(error) => CompleteGrillResponse {
                kind: "failed",
                id: None,
                error: Some(error.to_string()),
            },
        }
    }

    /// Saves (or replaces) `item_id`'s Grill draft (#356, ADR-0023) — the
    /// takeover's own continuous "Back or close saves automatically" write,
    /// called after every completed turn, not just on Back. `turns` is the
    /// caller's own opaque JSON array, rejected here — never reaching
    /// [`Core::save_grill_draft`] — if it is not even valid JSON, the same
    /// "reject before the seam" discipline `complete_grill`'s
    /// `session_steps` uses.
    pub async fn save_grill_draft(
        &mut self,
        item_id: &str,
        turns: &str,
        now_ms: i64,
    ) -> SaveGrillDraftResponse {
        let turns: serde_json::Value = match serde_json::from_str(turns) {
            Ok(turns) => turns,
            Err(error) => {
                return SaveGrillDraftResponse {
                    kind: "failed",
                    error: Some(format!("unreadable grill draft turns: {error}")),
                };
            }
        };
        match self.core.save_grill_draft(item_id, turns, now_ms).await {
            Ok(()) => SaveGrillDraftResponse { kind: "ok", error: None },
            Err(error) => SaveGrillDraftResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
        }
    }

    /// Discards `item_id`'s Grill draft (#356) — the takeover's explicit,
    /// confirmed "Discard" gesture, and the one place a completed Grill's
    /// local `"ok"` clears the interview that produced it.
    pub async fn discard_grill_draft(
        &mut self,
        item_id: &str,
        now_ms: i64,
    ) -> DiscardGrillDraftResponse {
        match self.core.discard_grill_draft(item_id, now_ms).await {
            Ok(()) => DiscardGrillDraftResponse { kind: "ok", error: None },
            Err(error) => DiscardGrillDraftResponse {
                kind: "failed",
                error: Some(error.to_string()),
            },
        }
    }

    /// `item_id`'s Grill draft, if any (#356) — the takeover's own "resume"
    /// read, asked for once when opening an item this build already knows
    /// (via `grill_draft_item_ids`) carries a draft.
    pub fn grill_draft(&self, item_id: &str) -> GrillDraftResponse {
        match self.core.grill_draft(item_id) {
            Some(turns) => GrillDraftResponse {
                kind: "ok",
                exists: true,
                turns: Some(turns.clone()),
            },
            None => GrillDraftResponse {
                kind: "ok",
                exists: false,
                turns: None,
            },
        }
    }

    /// Every item id carrying a draft (#356) — one bulk read for the whole
    /// Triage inbox's "Resume grill" labels.
    pub fn grill_draft_item_ids(&self) -> GrillDraftItemIdsResponse {
        GrillDraftItemIdsResponse {
            kind: "ok",
            item_ids: self.core.grill_draft_item_ids(),
        }
    }

    /// Runs one [`Core::run`] cycle against the live `reqwest` transports.
    ///
    /// #708: deliberately still `Core::run`, not `Core::run_observed` — the
    /// latter needs a real [`hummingbird_core::diagnostics::DiagnosticClock`]
    /// (its slow/stalled watchdog `select`s the call against a 5s/30s
    /// sleep), and this crate has no working `sleep_ms` to give it without
    /// a real timer dependency this slice does not add (see this module's
    /// header on why `wasm_bindgen` itself stays out of this file). Wiring
    /// full `sync.*`/`http.*` cycle instrumentation into this call is
    /// tracked as a follow-up, not silently skipped — see this issue's
    /// posted finding. What #708 *does* close here, with no watchdog
    /// needed at all, is [`OperationDiagnostics::emit_sync_outcome`]'s own
    /// gap: `Held`/`NoCredential`/a real cycle's outcome are all now
    /// visible in the journal.
    pub async fn run(
        &mut self,
        now_ms: i64,
        trigger: &str,
        force_full_sweep: bool,
        jitter_unit: f64,
        diagnostics: OperationDiagnostics<'_>,
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
        diagnostics.emit_sync_outcome(now_ms, &outcome);
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

/// #708: throwaway diagnostics rigs for the many pre-existing tests below
/// that exercise [`TaskHostCore::capture`]/[`TaskHostCore::triage`]/
/// [`TaskHostCore::run`] for reasons unrelated to diagnostics at all —
/// letting every one of them keep calling `host.capture_test(...)` etc.
/// with its original argument list, rather than hand-building an
/// [`OperationDiagnostics`] at every one of those call sites. A test that
/// DOES care what gets emitted builds its own
/// [`DiagnosticBuffer`]/[`DiagnosticSession`] and calls the real
/// `capture`/`triage`/`run` directly — see the `core_checkout_tests`/
/// `operation_diagnostics_tests` modules below.
#[cfg(test)]
impl TaskHostCore {
    async fn capture_test(
        &mut self,
        seed: &str,
        title: &str,
        stage: &str,
        fields: CaptureFields,
        now_ms: i64,
    ) -> CaptureResponse {
        let sink = DiagnosticBuffer::default();
        let session = DiagnosticSession::new("test-session", 0);
        let diagnostics = OperationDiagnostics { session: &session, sink: &sink, origin_monotonic_ms: 0 };
        self.capture(seed, title, stage, fields, now_ms, diagnostics, "op-test").await
    }

    async fn triage_test(
        &mut self,
        seed: &str,
        item_id: &str,
        destination: Option<&str>,
        edits: TriageEdits,
        now_ms: i64,
    ) -> TriageResponse {
        let sink = DiagnosticBuffer::default();
        let session = DiagnosticSession::new("test-session", 0);
        let diagnostics = OperationDiagnostics { session: &session, sink: &sink, origin_monotonic_ms: 0 };
        self.triage(seed, item_id, destination, edits, now_ms, diagnostics, "op-test").await
    }

    async fn run_test(
        &mut self,
        now_ms: i64,
        trigger: &str,
        force_full_sweep: bool,
        jitter_unit: f64,
    ) -> RunResponse {
        let sink = DiagnosticBuffer::default();
        let session = DiagnosticSession::new("test-session", 0);
        let diagnostics = OperationDiagnostics { session: &session, sink: &sink, origin_monotonic_ms: 0 };
        self.run(now_ms, trigger, force_full_sweep, jitter_unit, diagnostics).await
    }
}

#[cfg(test)]
mod core_checkout_tests {
    use super::*;

    fn event_names(events: &[DiagnosticEventV1]) -> Vec<String> {
        events
            .iter()
            .map(|e| serde_json::to_value(&e.event).unwrap()["name"].as_str().unwrap().to_string())
            .collect()
    }

    async fn fresh_cell(dir: &tempfile::TempDir, name: &str) -> TaskCoreCell {
        let namespace = dir.path().join(name);
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        TaskCoreCell::new(host, "test-session".to_string())
    }

    /// Acceptance: "Every acquisition of the single `TaskHostCore` emits
    /// `core.wait_started`, then `core.acquired` or `core.busy`, and
    /// finally `core.released`." — the ordinary, uncontended path.
    #[tokio::test]
    async fn an_uncontended_checkout_emits_wait_started_acquired_then_released_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let cell = fresh_cell(&dir, "ns-1").await;

        {
            let _guard = cell.checkout(CoreOwner::Capture, 1_000).unwrap();
        }

        let events = cell.drain_diagnostics();
        assert_eq!(
            event_names(&events),
            vec!["core.wait_started", "core.acquired", "core.released"]
        );
    }

    /// Acceptance: "The owner is recorded as a closed value … and a
    /// `core.busy` event names the owner that was holding the core at the
    /// time." — a second checkout attempted while the first is still held
    /// sees `core.busy{owner: sync}`, naming the *holder*, never the
    /// asker.
    #[tokio::test]
    async fn a_checkout_attempted_while_another_owner_holds_it_is_busy_naming_the_holder() {
        let dir = tempfile::tempdir().unwrap();
        let cell = fresh_cell(&dir, "ns-2").await;

        let guard = cell.checkout(CoreOwner::Sync, 1_000).unwrap();
        assert!(cell.checkout(CoreOwner::Capture, 1_000).is_none());

        let events = cell.drain_diagnostics();
        let busy = events
            .iter()
            .find(|e| matches!(e.event, DiagnosticEvent::CoreBusy { .. }))
            .expect("a busy event was recorded");
        match &busy.event {
            DiagnosticEvent::CoreBusy { owner } => assert_eq!(*owner, CoreOwner::Sync),
            _ => unreachable!(),
        }
        drop(guard);
    }

    /// Same acceptance criterion, exercised through the read-only path
    /// (#704's "a … project read sat waiting behind [sync]"): a read
    /// started while `Sync` holds the checkout is `busy{owner: sync}`
    /// too, not silently blank.
    #[tokio::test]
    async fn a_project_read_started_behind_sync_is_busy_naming_sync() {
        let dir = tempfile::tempdir().unwrap();
        let cell = fresh_cell(&dir, "ns-3").await;

        let guard = cell.checkout(CoreOwner::Sync, 1_000).unwrap();
        cell.drain_diagnostics(); // discard the checkout's own wait/acquire pair
        let result = cell.read(CoreOwner::Projects, 1_000, "busy".to_string(), |host| {
            serde_json::to_string(&host.projects()).unwrap()
        });
        assert_eq!(result, "busy");

        let events = cell.drain_diagnostics();
        assert_eq!(event_names(&events), vec!["core.wait_started", "core.busy"]);
        match &events[1].event {
            DiagnosticEvent::CoreBusy { owner } => assert_eq!(*owner, CoreOwner::Sync),
            _ => unreachable!(),
        }
        drop(guard);
    }

    /// A capture, triage attempted the same way — the brief's three named
    /// cases (capture, triage, project read) all naming `sync`.
    #[tokio::test]
    async fn capture_and_triage_attempted_behind_sync_each_produce_a_wait_span_naming_sync() {
        let dir = tempfile::tempdir().unwrap();
        let cell = fresh_cell(&dir, "ns-4").await;

        let guard = cell.checkout(CoreOwner::Sync, 1_000).unwrap();
        assert!(cell.checkout(CoreOwner::Capture, 1_000).is_none());
        assert!(cell.checkout(CoreOwner::Triage, 1_000).is_none());

        let events = cell.drain_diagnostics();
        let busy_owners: Vec<CoreOwner> = events
            .iter()
            .filter_map(|e| match &e.event {
                DiagnosticEvent::CoreBusy { owner } => Some(*owner),
                _ => None,
            })
            .collect();
        assert_eq!(busy_owners, vec![CoreOwner::Sync, CoreOwner::Sync]);
        drop(guard);
    }

    /// Successful read: wait_started, acquired, released — never busy.
    #[tokio::test]
    async fn an_uncontended_read_emits_wait_started_acquired_released() {
        let dir = tempfile::tempdir().unwrap();
        let cell = fresh_cell(&dir, "ns-5").await;

        let result = cell.read(CoreOwner::Projects, 1_000, "busy".to_string(), |host| {
            serde_json::to_string(&host.projects()).unwrap()
        });
        assert_ne!(result, "busy");

        let events = cell.drain_diagnostics();
        assert_eq!(
            event_names(&events),
            vec!["core.wait_started", "core.acquired", "core.released"]
        );
    }

    /// Acceptance: "Release is recorded via a guard: a test drops or
    /// cancels an in-flight operation mid-checkout and `core.released` is
    /// still present." Dropping the guard *without* it ever completing an
    /// operation — simulating a cancelled future — still checks the host
    /// back in and records `core.released`.
    #[tokio::test]
    async fn dropping_a_guard_mid_checkout_still_records_core_released() {
        let dir = tempfile::tempdir().unwrap();
        let cell = fresh_cell(&dir, "ns-6").await;

        let guard = cell.checkout(CoreOwner::Capture, 1_000).unwrap();
        drop(guard);

        let events = cell.drain_diagnostics();
        assert!(matches!(events.last().unwrap().event, DiagnosticEvent::CoreReleased));
        // The host is genuinely checked back in — a further checkout
        // succeeds rather than seeing a phantom owner forever.
        assert!(cell.checkout(CoreOwner::Sync, 2_000).is_some());
    }

    /// The same drop-based release survives a future that is dropped
    /// mid-`.await`, not just a bare `drop(guard)` — the guard is a local
    /// inside the future's own body, so Rust's ordinary `Drop` semantics
    /// for a cancelled future are exactly what closes the span.
    #[tokio::test]
    async fn a_future_holding_the_guard_dropped_mid_await_still_records_core_released() {
        use std::future::Future;
        let dir = tempfile::tempdir().unwrap();
        let cell = fresh_cell(&dir, "ns-7").await;

        {
            let future = async {
                let _guard = cell.checkout(CoreOwner::Capture, 1_000).unwrap();
                std::future::pending::<()>().await;
            };
            let mut future = Box::pin(future);
            // Poll once: the guard is acquired, then the future parks on
            // `pending()` — never resolving, the same shape a genuinely
            // hung network call leaves behind.
            let waker = std::task::Waker::noop();
            let poll = future.as_mut().poll(&mut std::task::Context::from_waker(waker));
            assert!(poll.is_pending());
            // Dropping the still-pending future drops its local `_guard`.
        }

        let events = cell.drain_diagnostics();
        assert!(matches!(events.last().unwrap().event, DiagnosticEvent::CoreReleased));
        assert!(cell.checkout(CoreOwner::Sync, 2_000).is_some());
    }

    // --------------------------------------------- operation.* (capture/triage)

    /// Acceptance: "A successful capture records `operation.local_commit`
    /// **before** any `http.started` in the same operation." Capture makes
    /// no network call of its own (it only enqueues durably —
    /// [`Core::capture`]'s own doc), so the *complete* ordering claim is:
    /// `operation.requested`, `operation.local_commit`,
    /// `operation.finished{success}`, with no `http.started` anywhere in
    /// between for this operation's id.
    #[tokio::test]
    async fn a_successful_capture_emits_local_commit_before_finished_and_no_http_started() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-op-1");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let sink = DiagnosticBuffer::default();
        let session = DiagnosticSession::new("s", 0);
        let diagnostics = OperationDiagnostics { session: &session, sink: &sink, origin_monotonic_ms: 0 };

        let response = host
            .capture("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000, diagnostics, "op-1")
            .await;
        assert_eq!(response.kind, "ok");

        let events = sink.drain();
        assert_eq!(
            event_names(&events),
            vec!["operation.requested", "operation.local_commit", "operation.finished"]
        );
        assert!(events.iter().all(|e| e.operation_id.as_deref() == Some("op-1")));
        assert!(!events.iter().any(|e| matches!(e.event, DiagnosticEvent::HttpStarted { .. })));
        match &events[2].event {
            DiagnosticEvent::OperationFinished { outcome } => assert_eq!(*outcome, OperationOutcome::Success),
            _ => unreachable!(),
        }
    }

    /// Acceptance: "A capture whose local durable commit fails records
    /// that failure distinctly from a capture blocked before reaching the
    /// core." — forced here by making the namespace directory read-only
    /// after `init` succeeds, so the enqueue's own durable write fails
    /// with a real `io::Error` (`SnapshotError::Store`), never reaching
    /// `operation.local_commit`.
    #[tokio::test]
    #[cfg(unix)]
    async fn a_capture_whose_local_commit_fails_never_emits_local_commit() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-op-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o500)).unwrap();

        let sink = DiagnosticBuffer::default();
        let session = DiagnosticSession::new("s", 0);
        let diagnostics = OperationDiagnostics { session: &session, sink: &sink, origin_monotonic_ms: 0 };

        let response = host
            .capture("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000, diagnostics, "op-2")
            .await;
        assert_eq!(response.kind, "failed", "{response:?}");

        // Restore permissions so `tempfile::TempDir`'s own drop can clean
        // up; failing to do this leaks a read-only directory on disk.
        std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o700)).unwrap();

        let events = sink.drain();
        assert_eq!(event_names(&events), vec!["operation.requested", "operation.finished"]);
        match &events[1].event {
            DiagnosticEvent::OperationFinished { outcome } => assert_eq!(*outcome, OperationOutcome::Failure),
            _ => unreachable!(),
        }
    }

    /// A capture blocked before reaching the core (busy) never mints an
    /// `operation_id`/`operation.*` event at all — the other half of the
    /// same acceptance criterion, distinguishing "never reached the core"
    /// from "reached it and failed to commit."
    #[tokio::test]
    async fn a_capture_blocked_by_busy_never_reaches_operation_requested() {
        let dir = tempfile::tempdir().unwrap();
        let cell = fresh_cell(&dir, "ns-op-3").await;

        let guard = cell.checkout(CoreOwner::Sync, 1_000).unwrap();
        assert!(cell.checkout(CoreOwner::Capture, 1_000).is_none());

        let events = cell.drain_diagnostics();
        assert!(!events
            .iter()
            .any(|e| matches!(e.event, DiagnosticEvent::OperationRequested)));
        drop(guard);
    }

    /// A rejected-before-the-seam triage (an unrecognised destination)
    /// still counts as an attempted operation — `operation.requested` then
    /// `operation.finished{failure}`, no `operation.local_commit`.
    #[tokio::test]
    async fn a_triage_rejected_before_the_seam_emits_requested_then_finished_failure() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-op-4");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let sink = DiagnosticBuffer::default();
        let session = DiagnosticSession::new("s", 0);
        let diagnostics = OperationDiagnostics { session: &session, sink: &sink, origin_monotonic_ms: 0 };

        let response = host
            .triage(
                "seed-1",
                "no-such-item",
                Some("not-a-real-destination"),
                TriageEdits::default(),
                1_000,
                diagnostics,
                "op-3",
            )
            .await;
        assert_eq!(response.kind, "failed");

        let events = sink.drain();
        assert_eq!(event_names(&events), vec!["operation.requested", "operation.finished"]);
        match &events[1].event {
            DiagnosticEvent::OperationFinished { outcome } => assert_eq!(*outcome, OperationOutcome::Failure),
            _ => unreachable!(),
        }
    }

    // --------------------------------------------- sync outcome (credential hold etc.)

    /// Acceptance: "A 401 during sync is followed by a credential-hold
    /// state visible in the journal, with no token value recorded." —
    /// `Core::run` returning `Held` (a credential already known dead from
    /// an earlier 401) used to short-circuit with zero diagnostics; now it
    /// emits `sync.finished{credential_needed}`, and the emitted JSON
    /// carries no token value anywhere (`CoreCycleOutcome::Held`'s own
    /// shape has none to begin with).
    #[tokio::test]
    async fn a_held_credential_emits_a_visible_sync_finished_credential_needed() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-sync-1");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "device-token")
            .await
            .unwrap();
        // Force the credential into `held` without a real 401: this
        // module's own `clear_api_key` test already establishes
        // `clear_api_key` does not reach `held` — a genuine hold needs a
        // real cycle outcome. `CoreCycleOutcome::Held` is exercised
        // directly here instead, matching `map_run_outcome`'s own test
        // style just above.
        let sink = DiagnosticBuffer::default();
        let session = DiagnosticSession::new("s", 0);
        let diagnostics = OperationDiagnostics { session: &session, sink: &sink, origin_monotonic_ms: 0 };
        diagnostics.emit_sync_outcome(1_000, &CoreCycleOutcome::Held);

        let events = sink.drain();
        assert_eq!(events.len(), 1);
        match &events[0].event {
            DiagnosticEvent::SyncFinished { outcome } => assert_eq!(*outcome, SyncOutcome::CredentialNeeded),
            other => panic!("expected sync.finished, got {other:?}"),
        }
        let json = serde_json::to_string(&events[0]).unwrap();
        assert!(!json.contains("device-token"));
        let _ = host.frontier(); // keep `host` alive/used
    }

    /// `NoCredential` (nobody ever pushed a token) is not a "hold" and
    /// emits nothing — unchanged from before this slice.
    #[tokio::test]
    async fn no_credential_emits_nothing() {
        let sink = DiagnosticBuffer::default();
        let session = DiagnosticSession::new("s", 0);
        let diagnostics = OperationDiagnostics { session: &session, sink: &sink, origin_monotonic_ms: 0 };
        diagnostics.emit_sync_outcome(1_000, &CoreCycleOutcome::NoCredential);
        assert_eq!(sink.drain(), Vec::new());
    }

    /// Acceptance: "A connection error and a persistence failure are each
    /// covered by a test asserting the classified event that results." —
    /// exercised through a real cycle: an empty `base_url` forces
    /// `pull_failed` (this file's own established network-free test
    /// pattern, see `map_run_outcome`'s doc just above).
    #[tokio::test]
    async fn a_connection_error_during_run_emits_sync_finished_pull_failed() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-sync-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "device-token")
            .await
            .unwrap();
        let sink = DiagnosticBuffer::default();
        let session = DiagnosticSession::new("s", 0);
        let diagnostics = OperationDiagnostics { session: &session, sink: &sink, origin_monotonic_ms: 0 };

        let response = host.run(1_000, "user", true, 0.0, diagnostics).await;
        assert_eq!(response.kind, "pull_failed");

        let events = sink.drain();
        assert_eq!(events.len(), 1);
        match &events[0].event {
            DiagnosticEvent::SyncFinished { outcome } => assert_eq!(*outcome, SyncOutcome::PullFailed),
            other => panic!("expected sync.finished, got {other:?}"),
        }
    }

    /// The persistence-failure half of the same acceptance criterion,
    /// exercised directly against [`sync_outcome_of`] the same way
    /// `map_run_outcome`'s own tests build a [`CycleOutcome`] by hand
    /// rather than forcing a real store failure through a whole cycle.
    #[test]
    fn a_persistence_failure_outcome_maps_to_sync_finished_persist_failed() {
        let outcome = CycleOutcome::PersistFailed {
            message: "disk full".to_string(),
            retry_after_ms: 1_000,
        };
        assert_eq!(sync_outcome_of(&outcome), SyncOutcome::PersistFailed);
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
        host.capture_test("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000).await;
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
        host.capture_test("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000).await;
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
        host.capture_test("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000).await;
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
        host.capture_test("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000).await;
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
        host.capture_test("seed-1", "someday maybe", "triage", CaptureFields::default(), 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();

        // "grilling" (#360) is now rejected exactly like any other
        // unrecognised spelling — promoting straight into Grilling is no
        // longer a triage gesture at all; an item reaches Grilling only via
        // a `fog_remains` Grill verdict, never through this seam.
        let response = host
            .triage_test("seed-triage-1", &id, Some("grilling"), TriageEdits::default(), 2_000)
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
        host.capture_test("seed-1", "someday maybe", "triage", CaptureFields::default(), 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();

        let response = host
            .triage_test(
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

    /// The capture wire's own contract, which `store/protocol.ts` writes
    /// against. Same camelCase-and-`deny_unknown_fields` discipline as
    /// `TriageEdits` above, and pinned for the same reason: a key spelled one
    /// way here and another way there deserializes to "unset", which is a
    /// capture silently dropping a deadline someone typed rather than an error
    /// anybody sees.
    ///
    /// Where it deliberately differs: `null` is *not* a third instruction.
    /// Nothing exists yet to clear, so a null reads exactly as an absent key.
    #[test]
    fn capture_fields_read_camel_case_keys_and_refuse_unknown_ones() {
        let fields: CaptureFields = serde_json::from_str(
            r#"{"projectId":"p1","scheduledDate":"2026-08-12","deadline":null,"priority":3}"#,
        )
        .unwrap();
        assert_eq!(fields.project_id, Some("p1".to_string()));
        assert_eq!(fields.scheduled_date, Some("2026-08-12".to_string()));
        assert_eq!(fields.deadline, None, "a null is simply not set");
        assert_eq!(fields.priority, Some(3));
        assert_eq!(fields.context, None, "an absent key is not set either");

        assert_eq!(
            serde_json::from_str::<CaptureFields>("{}").unwrap(),
            CaptureFields::default(),
            "an empty object is every field unset — the resting state"
        );
        assert!(
            serde_json::from_str::<CaptureFields>(r#"{"project_id":"p1"}"#).is_err(),
            "snake_case is not this wire's spelling, and a dropped field is worse than an error"
        );
        assert!(
            serde_json::from_str::<CaptureFields>(r#"{"deadlien":"2026-08-12"}"#).is_err(),
            "an unknown key is refused, not read as `nothing was set`"
        );
    }

    /// Every value rule the authority answers 400 on is checked before
    /// `Core::capture` is reached — the same discipline triage has had, now
    /// that capture can carry the same fields. Without it a bad deadline
    /// typed into the capture box would queue a mutation that dead-letters
    /// later, with nothing on screen to say so.
    #[tokio::test]
    async fn capturing_rejects_every_invalid_field_value_before_reaching_core_capture() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-capture-invalid");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let cases = [
            CaptureFields {
                priority: Some(5),
                ..CaptureFields::default()
            },
            CaptureFields {
                deadline: Some("next tuesday".to_string()),
                ..CaptureFields::default()
            },
            CaptureFields {
                // A real date that is not a real day — the shared calendar
                // validation, not a regex.
                deadline: Some("2026-02-30".to_string()),
                ..CaptureFields::default()
            },
            CaptureFields {
                // A scheduled date is a whole civil day and never a date-time.
                scheduled_date: Some("2026-08-12T09:30".to_string()),
                ..CaptureFields::default()
            },
        ];
        for fields in cases {
            let response = host
                .capture_test("seed-1", "buy milk", "ready", fields.clone(), 1_000)
                .await;
            assert_eq!(response.kind, "failed", "{fields:?}");
            assert!(response.error.is_some(), "{fields:?}");
        }
        assert_eq!(host.frontier().items.len(), 0, "nothing was ever queued");
    }

    /// The other half: everything the capture box can set reaches the item,
    /// in one capture mutation.
    #[tokio::test]
    async fn capturing_with_every_field_set_puts_them_all_on_the_item() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-capture-full");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host
            .capture_test(
                "seed-1",
                "buy milk",
                "ready",
                CaptureFields {
                    size: Some("quick".to_string()),
                    energy: Some("low".to_string()),
                    context: Some("@errands".to_string()),
                    description: Some("the oat kind".to_string()),
                    project_id: Some("proj-1".to_string()),
                    priority: Some(2),
                    deadline: Some("2026-09-01T09:30".to_string()),
                    scheduled_date: Some("2026-08-30".to_string()),
                },
                1_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        let item = &host.frontier().items[0].item;
        assert_eq!(item.size, Some(Size::Quick));
        assert_eq!(item.energy, Some(Energy::Low));
        assert_eq!(item.context.as_deref(), Some("@errands"));
        assert_eq!(item.description.as_deref(), Some("the oat kind"));
        assert_eq!(item.project_id.as_deref(), Some("proj-1"));
        assert_eq!(item.priority, 2);
        assert_eq!(item.deadline.as_deref(), Some("2026-09-01T09:30"));
        assert_eq!(item.scheduled_date.as_deref(), Some("2026-08-30"));
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
        host.capture_test("seed-1", "someday maybe", "triage", CaptureFields::default(), 1_000).await;
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
            let response = host.triage_test("seed-triage-1", &id, Some("ready"), edits, 2_000).await;
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
            .triage_test(
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
        host.capture_test("seed-1", "someday maybe", "triage", CaptureFields::default(), 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();
        host.triage_test(
            "seed-triage-1",
            &id,
            Some("ready"),
            TriageEdits {
                context: Some(Some("@computer".to_string())),
                size: Some(Some("deep".to_string())),
                ..Default::default()
            },
            2_000,
        )
        .await;

        let response = host
            .triage_test(
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
            .triage_test("seed-triage-1", "no-such-item", Some("ready"), TriageEdits::default(), 1_000)
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
        host.capture_test("seed-1", "someday maybe", "triage", CaptureFields::default(), 1_000).await;
        let id = host.triage_inbox().items[0].item.id.clone();

        let response = host
            .triage_test(
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

    // Send-to-grilling through this seam is gone (#360): an item reaches
    // Grilling exactly one way now, a `fog_remains` verdict from a completed
    // Grill (`grill_tests::fog_remains_demotes_a_triage_item_to_grilling`).
    // `"grilling"` is simply an unrecognised destination here, re-pinned by
    // `triaging_with_an_unrecognised_destination_never_reaches_core_triage`
    // above.

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
        host.capture_test("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000).await;
        let id = host.frontier().items[0].item.id.clone();
        host.act("seed-act-1", &id, "start", 1_500).await;
        assert_eq!(host.frontier().items[0].item.stage, Stage::InProgress);

        let response = host
            .triage_test(
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
        host.capture_test("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000).await;
        let id = host.frontier().items[0].item.id.clone();
        host.triage_test(
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

        host.triage_test(
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
mod grill_tests {
    use super::*;

    async fn captured_item(host: &mut TaskHostCore) -> String {
        let response = host
            .capture_test("seed-cap", "book flights", "triage", CaptureFields::default(), 1_000)
            .await;
        response.id.expect("capture always mints an id")
    }

    #[tokio::test]
    async fn resolving_a_triage_item_promotes_it_off_the_triage_inbox() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-grill-1");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        let item_id = captured_item(&mut host).await;

        let response = host
            .complete_grill(
                "seed-grill-1",
                &item_id,
                "[]",
                "Q: destination?\nA: Tokyo".to_string(),
                "Settled on Tokyo".to_string(),
                "resolved",
                "{\"title\":\"book flights to Tokyo\"}".to_string(),
                "{\"title\":\"book flights to Tokyo\"}".to_string(),
                false,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        assert!(response.id.is_some());
        assert!(host.triage_inbox().items.is_empty());
        assert_eq!(host.frontier().items[0].item.stage, Stage::Ready);
    }

    #[tokio::test]
    async fn an_unrecognised_verdict_is_rejected_before_reaching_core_complete_grill() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-grill-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        let item_id = captured_item(&mut host).await;

        let response = host
            .complete_grill(
                "seed-grill-2",
                &item_id,
                "[]",
                "transcript".to_string(),
                "summary".to_string(),
                "not-a-real-verdict",
                "{}".to_string(),
                "{}".to_string(),
                false,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "failed");
        assert!(response.id.is_none());
        // Never reached `Core::complete_grill` — the item is exactly where
        // it started.
        assert_eq!(host.triage_inbox().items.len(), 1);
    }

    #[tokio::test]
    async fn unreadable_session_steps_json_is_rejected_before_reaching_core_complete_grill() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-grill-3");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        let item_id = captured_item(&mut host).await;

        let response = host
            .complete_grill(
                "seed-grill-3",
                &item_id,
                "not json",
                "transcript".to_string(),
                "summary".to_string(),
                "resolved",
                "{}".to_string(),
                "{}".to_string(),
                false,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "failed");
        assert_eq!(host.triage_inbox().items.len(), 1);
    }

    #[tokio::test]
    async fn completing_a_grill_on_an_unknown_item_id_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-grill-4");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host
            .complete_grill(
                "seed-grill-4",
                "no-such-item",
                "[]",
                "transcript".to_string(),
                "summary".to_string(),
                "resolved",
                "{}".to_string(),
                "{}".to_string(),
                false,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "not_found");
        assert!(response.error.is_some());
    }

    /// Fog remaining always demotes to Grilling
    /// (`hummingbird_domain::resulting_stage`) — never a UI-side branch on
    /// the item's stage.
    #[tokio::test]
    async fn fog_remains_demotes_a_triage_item_to_grilling() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-grill-5");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        let item_id = captured_item(&mut host).await;

        let response = host
            .complete_grill(
                "seed-grill-5",
                &item_id,
                "[]",
                "transcript".to_string(),
                "still foggy".to_string(),
                "fog_remains",
                "{}".to_string(),
                "{}".to_string(),
                false,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.frontier().items.len(), 0, "Grilling never appears on the frontier");
    }

    /// #357: `grilling_items` is the "triage process" queue's second half —
    /// a demoted item leaves `triage_inbox` and appears here instead, never
    /// in both and never in neither.
    #[tokio::test]
    async fn fog_remains_moves_the_item_from_triage_inbox_to_grilling_items() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-grill-6");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        let item_id = captured_item(&mut host).await;

        let response = host
            .complete_grill(
                "seed-grill-6",
                &item_id,
                "[]",
                "transcript".to_string(),
                "still foggy".to_string(),
                "fog_remains",
                "{}".to_string(),
                "{}".to_string(),
                false,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        assert!(host.triage_inbox().items.is_empty());
        assert_eq!(host.grilling_items().items.len(), 1);
        assert_eq!(host.grilling_items().items[0].item.id, item_id);
    }

    // -------------------------------------------- grill drafts (#356, ADR-0023)

    #[tokio::test]
    async fn a_saved_draft_is_readable_and_listed() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-grill-draft-1");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        let item_id = captured_item(&mut host).await;

        let saved = host
            .save_grill_draft(&item_id, r#"[{"question":{"prompt":"p","recommendedAnswer":"r","choices":[]},"answer":"a"}]"#, 1_000)
            .await;
        assert_eq!(saved.kind, "ok");
        assert!(saved.error.is_none());

        let read = host.grill_draft(&item_id);
        assert_eq!(read.kind, "ok");
        assert!(read.exists);
        assert!(read.turns.is_some());

        let ids = host.grill_draft_item_ids();
        assert_eq!(ids.kind, "ok");
        assert_eq!(ids.item_ids, vec![item_id]);
    }

    #[tokio::test]
    async fn unreadable_turns_json_is_rejected_before_reaching_core_save_grill_draft() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-grill-draft-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        let item_id = captured_item(&mut host).await;

        let saved = host.save_grill_draft(&item_id, "not json", 1_000).await;
        assert_eq!(saved.kind, "failed");
        assert!(saved.error.is_some());
        assert!(!host.grill_draft(&item_id).exists, "a rejected save must not reach the core");
    }

    #[tokio::test]
    async fn discarding_removes_the_draft_and_is_a_no_op_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-grill-draft-3");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        let item_id = captured_item(&mut host).await;
        host.save_grill_draft(&item_id, "[]", 1_000).await;

        let discarded = host.discard_grill_draft(&item_id, 2_000).await;
        assert_eq!(discarded.kind, "ok");
        assert!(!host.grill_draft(&item_id).exists);

        // A second discard, with nothing left to discard, is still "ok".
        let discarded_again = host.discard_grill_draft(&item_id, 3_000).await;
        assert_eq!(discarded_again.kind, "ok");
    }

    #[tokio::test]
    async fn an_item_with_no_draft_reads_as_not_existing() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-grill-draft-4");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let read = host.grill_draft("no-such-item");
        assert_eq!(read.kind, "ok");
        assert!(!read.exists);
        assert!(read.turns.is_none());
        assert!(host.grill_draft_item_ids().item_ids.is_empty());
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
        host.capture_test("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000).await;

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
        host.capture_test("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000).await;
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
mod search_tests {
    use super::*;

    /// The wire contract `task-worker.ts` parses: the item's own snake_case
    /// fields flat at the top level (exactly [`LedgerRowDTO`]'s pattern),
    /// plus `group`, pinned on its keys and its serde string.
    #[tokio::test]
    async fn a_search_row_serializes_item_fields_flat_beside_group() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-search-1");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture_test("seed-1", "buy stamps", "ready", CaptureFields::default(), 1_000).await;

        let response = host.search("stamps", 2_000);
        assert_eq!(response.kind, "ok");
        assert_eq!(response.rows.len(), 1);
        assert_eq!(response.total, 1);

        let json: serde_json::Value = serde_json::to_value(&response.rows[0]).unwrap();
        for key in ["id", "title", "stage", "archived_at", "updated_at", "pending", "group"] {
            assert!(json.get(key).is_some(), "search row must carry {key:?} at the top level");
        }
        assert_eq!(json["group"], serde_json::Value::String("live".to_string()));
        assert_eq!(json["pending"], serde_json::Value::Bool(true));
    }

    /// Recall's corpus reaches archived rows too, and labels them
    /// accordingly — the same roster [`TaskHostCore::ledger`] reads.
    #[tokio::test]
    async fn search_reaches_an_archived_item_and_labels_it() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-search-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture_test("seed-1", "widget report", "ready", CaptureFields::default(), 1_000).await;
        let id = host.frontier().items[0].item.id.clone();
        host.act("seed-act-1", &id, "cancel", 2_000).await;

        let response = host.search("widget", 3_000);
        assert_eq!(response.rows.len(), 1);
        assert_eq!(response.rows[0].item.id, id);
        assert_eq!(response.rows[0].group, Group::Archived);
    }

    /// An empty query never reaches the core matcher with anything to
    /// find — the `"ok"`/empty-list answer this seam promises rather than
    /// `"busy"`.
    #[tokio::test]
    async fn an_empty_query_returns_no_rows_and_zero_total() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-search-3");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();
        host.capture_test("seed-1", "anything at all", "ready", CaptureFields::default(), 1_000).await;

        let response = host.search("", 2_000);
        assert_eq!(response.kind, "ok");
        assert_eq!(response.rows.len(), 0);
        assert_eq!(response.total, 0);
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

        let response = host.capture_test("seed-1", "buy milk", "not-a-stage", CaptureFields::default(), 1_000).await;

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
            .capture_test(
                "seed-1",
                "buy milk",
                "ready",
                CaptureFields {
                    size: Some("giant".to_string()),
                    ..CaptureFields::default()
                },
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
            .capture_test(
                "seed-1",
                "buy milk",
                "ready",
                CaptureFields {
                    energy: Some("blazing".to_string()),
                    ..CaptureFields::default()
                },
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
            .capture_test(
                "seed-1",
                "buy milk",
                "ready",
                CaptureFields {
                    size: Some("deep".to_string()),
                    energy: Some("high".to_string()),
                    context: Some("@errands".to_string()),
                    ..CaptureFields::default()
                },
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

        let response = host.capture_test("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000).await;

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

        host.capture_test("seed-1", "someday maybe", "triage", CaptureFields::default(), 1_000).await;

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
        assert_eq!(
            host.projects(),
            ProjectListResponse { kind: "ok", projects: Vec::new(), archived: Vec::new() }
        );
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

        let response = host.run_test(1_000, "user", true, 0.0).await;

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
        let response = host.run_test(1_000, "user", true, 0.0).await;

        assert_eq!(response.kind, "no_credential");
    }

    #[tokio::test]
    async fn clearing_never_touches_a_pending_capture() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-clear-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "device-token")
            .await
            .unwrap();
        let response = host.capture_test("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000).await;
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
        let timer = host.run_test(1_000, "timer", true, 0.0).await;
        let user = host.run_test(2_000, "anything-else", true, 0.0).await;
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

        host.capture_test("seed-1", "buy milk", "ready", CaptureFields::default(), 1_000).await;

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
                github_repo: Some("JddAndrewLauren/hummingbird".to_string()),
                default_context: Some("@computer".to_string()),
                archived_at: None,
                created_at: 1,
                updated_at: 1,
                version: 1,
            }],
            archived: vec![Project {
                id: "p-9".to_string(),
                name: "Old bike".to_string(),
                github_repo: None,
                default_context: None,
                archived_at: Some(9_000),
                created_at: 1,
                updated_at: 1,
                version: 1,
            }],
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"kind":"ok","projects":[{"id":"p-1","name":"Ship it","github_repo":"JddAndrewLauren/hummingbird","default_context":"@computer","archived_at":null,"created_at":1,"updated_at":1,"version":1}],"archived":[{"id":"p-9","name":"Old bike","github_repo":null,"default_context":null,"archived_at":9000,"created_at":1,"updated_at":1,"version":1}]}"#
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
mod question_switch_tests {
    use super::*;

    #[test]
    fn switch_responses_serialize_with_the_exact_keys_task_worker_ts_parses() {
        let response = QuestionSwitchListResponse {
            kind: "ok",
            switches: vec![
                QuestionSwitch { question: "homework".to_string(), enabled: true, pending: false },
                QuestionSwitch { question: "weekend".to_string(), enabled: false, pending: true },
            ],
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"kind":"ok","switches":[{"question":"homework","enabled":true,"pending":false},{"question":"weekend","enabled":false,"pending":true}]}"#
        );
    }

    #[tokio::test]
    async fn a_fresh_host_reports_all_ten_questions_on() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-switch-fresh");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        let response = host.question_switches();
        assert_eq!(response.kind, "ok");
        assert_eq!(response.switches.len(), 10);
        assert!(response.switches.iter().all(|switch| switch.enabled && !switch.pending));
    }

    #[tokio::test]
    async fn an_unrecognised_question_never_reaches_core_set_question_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-switch-unknown");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        let response = host.set_question_enabled("seed-1", "fantasy", false, 1_000).await;
        assert_eq!(response.kind, "unknown_question");
        // And nothing was written: `settings` has no DELETE, so a key minted
        // from an invented name would be a permanent unreadable row.
        assert!(host.question_switches().switches.iter().all(|switch| !switch.pending));
        assert!(host
            .bindings()
            .bindings
            .iter()
            .all(|binding| !binding.key.starts_with("question-enabled-")));
    }

    #[tokio::test]
    async fn a_question_switched_off_through_the_seam_reads_back_from_the_same_seam() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-switch-roundtrip");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        assert_eq!(
            host.set_question_enabled("seed-1", "weekend", false, 1_000).await.kind,
            "ok"
        );

        let weekend = host
            .question_switches()
            .switches
            .into_iter()
            .find(|switch| switch.question == "weekend")
            .expect("weekend is listed");
        assert!(!weekend.enabled);
        assert!(weekend.pending, "nothing has synced it yet");
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

    /// The source vocabulary crosses whole and in registration order,
    /// **retired entries included** — an existing rule may name one, and a
    /// dropdown that could not render it would show a blank control instead
    /// of the value actually stored. `city-waste/v1` is the registry's real
    /// retired entry (ADR-0014), and it arrives naming its successor, which
    /// is what an editor marks the option unselectable on.
    #[tokio::test]
    async fn kind_registry_carries_every_source_with_the_retired_one_named() {
        let response = TaskHostCore::kind_registry();
        assert_eq!(response.sources.len(), hummingbird_domain::REGISTRY.len());
        assert_eq!(
            response.sources.iter().map(|s| s.source).collect::<Vec<_>>(),
            hummingbird_domain::REGISTRY.iter().map(|e| e.source).collect::<Vec<_>>(),
            "registration order, verbatim",
        );

        let retired = response
            .sources
            .iter()
            .find(|s| s.source == "city-waste/v1")
            .expect("the registry's retired entry still ships");
        assert_eq!(retired.retired_as, Some("city-waste/v2"));

        let live = response
            .sources
            .iter()
            .find(|s| s.source == hummingbird_domain::GMAIL_V1)
            .expect("a live source");
        assert_eq!(live.retired_as, None);

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#"{"source":"city-waste/v1","retired_as":"city-waste/v2"}"#));
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
            deleted_at: None,
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
                None,
                Some(false),
                false,
                None,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
    }

    /// Deleting a rule crosses this seam as one more field on the same
    /// patch — no delete entry point, per `Core::patch_rule`'s own contract.
    #[tokio::test]
    async fn deleting_a_rule_crosses_as_the_same_patch_call() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-delete-rule");
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
            deleted_at: None,
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
                None,
                None,
                true,
                Some(9_000),
                2_000,
            )
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
            deleted_at: None,
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
                false,
                None,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "failed");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 0 });
    }
}

#[cfg(test)]
mod project_tests {
    use super::*;

    /// #624: the authority 400s on an empty project name, so this seam
    /// refuses one — blank and whitespace-only alike — before
    /// `Core::create_project` is reached, and mints no queue entry. Without
    /// it a blank New-project card would queue a write that dead-letters
    /// later with nothing on screen to say so.
    #[tokio::test]
    async fn creating_a_project_rejects_an_empty_name_before_reaching_core() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-create-project-empty");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        for name in ["", "   ", "\t\n"] {
            let response = host.create_project("seed-1", name, 1_000).await;
            assert_eq!(response.kind, "failed", "{name:?} is refused");
            assert!(response.id.is_none());
            assert_eq!(
                host.queue_depth(),
                QueueDepthResponse { kind: "ok", depth: 0 },
                "{name:?} minted no queue entry"
            );
        }
    }

    /// #624: a good name enqueues exactly one create, and the project is
    /// still absent from the read, since there is no optimistic overlay.
    /// (That the name is *trimmed* rather than merely tested trimmed is what
    /// the whitespace-only case above proves; nothing public here can read
    /// the queued body back.)
    #[tokio::test]
    async fn creating_a_project_enqueues_it_and_overlays_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-create-project-ok");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        let response = host.create_project("seed-1", "  Rebuild the deck  ", 1_000).await;

        assert_eq!(response.kind, "ok");
        assert!(response.id.is_some());
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
        assert_eq!(
            host.projects(),
            ProjectListResponse { kind: "ok", projects: Vec::new(), archived: Vec::new() },
            "no overlay: the card appears only once a cycle pulls it back"
        );
    }

    fn fixture_project() -> hummingbird_domain::Project {
        hummingbird_domain::Project {
            id: "p-1".to_string(),
            name: "Rebuild the deck".to_string(),
            github_repo: None,
            default_context: None,
            archived_at: None,
            created_at: 1,
            updated_at: 1,
            version: 3,
        }
    }

    /// #625: the properties card's set gesture enqueues one CAS patch.
    #[tokio::test]
    async fn patching_a_projects_repo_and_context_enqueues_one_patch() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-project");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let project = fixture_project();

        let response = host
            .patch_project(
                "seed-1",
                &project,
                None,
                true,
                Some("JddAndrewLauren/hummingbird".to_string()),
                true,
                Some("@computer".to_string()),
                false,
                None,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
    }

    /// #625: `github_repo` is checked with `is_valid_github_repo` before
    /// `Core::patch_project` is reached — the wasm-seam half of "validate at
    /// the handler and the wasm seam." A malformed slug mints no queue
    /// entry, same discipline as an unrecognised rule tier.
    #[tokio::test]
    async fn patching_a_malformed_github_repo_never_reaches_core_patch_project() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-project-bad-repo");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let project = fixture_project();

        let response = host
            .patch_project(
                "seed-1",
                &project,
                None,
                true,
                Some("not-a-slug".to_string()),
                false,
                None,
                false,
                None,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "failed");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 0 });
    }

    /// The clearing half: touched-but-`None` sends an explicit clear, and a
    /// well-formed value never trips the validation an absent/cleared value
    /// must not.
    #[tokio::test]
    async fn patching_clears_repo_and_context_without_validation_tripping() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-project-clear");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let project = fixture_project();

        let response = host
            .patch_project("seed-1", &project, None, true, None, true, None, false, None, 2_000)
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
    }
}

#[cfg(test)]
mod project_link_tests {
    use super::*;

    /// #626: the authority 400s on an empty url, so this seam refuses one —
    /// blank and whitespace-only alike — before `Core::create_project_link`
    /// is reached, and mints no queue entry. Same discipline
    /// `creating_a_project_rejects_an_empty_name_before_reaching_core`
    /// carries for the project name.
    #[tokio::test]
    async fn creating_a_link_rejects_an_empty_url_before_reaching_core() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-create-link-empty");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        for url in ["", "   ", "\t\n"] {
            let response = host
                .create_project_link("seed-1", "p-1", url, None, 1, 1_000)
                .await;
            assert_eq!(response.kind, "failed", "{url:?} is refused");
            assert!(response.id.is_none());
            assert_eq!(
                host.queue_depth(),
                QueueDepthResponse { kind: "ok", depth: 0 },
                "{url:?} minted no queue entry"
            );
        }
    }

    /// A good url enqueues exactly one create, and the link is still absent
    /// from the read, since there is no optimistic overlay — same "ok means
    /// enqueued, not saved" contract `creating_a_project_enqueues_it_and_
    /// overlays_nothing` carries.
    #[tokio::test]
    async fn creating_a_link_enqueues_it_and_overlays_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-create-link-ok");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        let response = host
            .create_project_link("seed-1", "p-1", "  https://example.com  ", Some("  Docs  ".to_string()), 1, 1_000)
            .await;

        assert_eq!(response.kind, "ok");
        assert!(response.id.is_some());
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
        assert_eq!(
            host.project_links("p-1"),
            ProjectLinkListResponse { kind: "ok", links: Vec::new() },
            "no overlay: the link appears only once a cycle pulls it back"
        );
    }

    /// A whitespace-only label trims to `None`, same as the url trims.
    #[tokio::test]
    async fn creating_a_link_trims_a_blank_label_to_none() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-create-link-blank-label");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        let response = host
            .create_project_link("seed-1", "p-1", "https://example.com", Some("   ".to_string()), 1, 1_000)
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
    }

    fn fixture_link() -> hummingbird_domain::ProjectLink {
        hummingbird_domain::ProjectLink {
            id: "l-1".to_string(),
            project_id: "p-1".to_string(),
            url: "https://example.com".to_string(),
            label: Some("Example".to_string()),
            position: 1,
            removed_at: None,
            version: 3,
        }
    }

    /// Editing, reordering and removing all enqueue one CAS patch each.
    #[tokio::test]
    async fn patching_a_links_url_label_and_position_enqueues_one_patch() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-link");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let link = fixture_link();

        let response = host
            .patch_project_link(
                "seed-1",
                &link,
                Some("https://example.com/docs".to_string()),
                true,
                Some("Docs".to_string()),
                Some(2),
                false,
                None,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
    }

    /// `url` is checked for emptiness before `Core::patch_project_link` is
    /// reached — the wasm-seam half of "validate at the handler and the
    /// wasm seam," same discipline `patching_a_malformed_github_repo_never_
    /// reaches_core_patch_project` follows for `github_repo`.
    #[tokio::test]
    async fn patching_a_link_to_a_blank_url_never_reaches_core() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-link-bad-url");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let link = fixture_link();

        let response = host
            .patch_project_link("seed-1", &link, Some("   ".to_string()), false, None, None, false, None, 2_000)
            .await;

        assert_eq!(response.kind, "failed");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 0 });
    }

    /// The removal gesture: `removedAtTouched` true with a value flags it,
    /// and an explicit clear (touched, `None`) un-removes it — same
    /// touched-flag contract the properties card's clearing test proves.
    #[tokio::test]
    async fn removing_and_un_removing_a_link_each_enqueue_one_patch() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-link-remove");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let link = fixture_link();

        let response = host
            .patch_project_link("seed-1", &link, None, false, None, None, true, Some(9_000), 2_000)
            .await;
        assert_eq!(response.kind, "ok");

        let response = host
            .patch_project_link("seed-2", &link, None, false, None, None, true, None, 3_000)
            .await;
        assert_eq!(response.kind, "ok");

        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 2 });
    }
}

#[cfg(test)]
mod route_tests {
    use super::*;

    /// #627: an unread project's Route answers `None`, not a standing "no
    /// Route" claim — [`Core::route`]'s own doc.
    #[tokio::test]
    async fn an_unread_route_answers_none() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-route-unread");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        assert_eq!(host.route("p-1"), RouteResponse { kind: "ok", route: None });
    }

    fn fixture_route() -> hummingbird_domain::Route {
        hummingbird_domain::Route {
            project_id: "p-1".to_string(),
            destination: None,
            notes: None,
            updated_at: 1,
            version: 3,
        }
    }

    /// #627: the dossier's destination/notes edit enqueues one CAS patch.
    #[tokio::test]
    async fn patching_a_routes_destination_and_notes_enqueues_one_patch() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-route");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let route = fixture_route();

        let response = host
            .patch_route(
                "seed-1",
                &route,
                true,
                Some("Ship the deck".to_string()),
                true,
                Some("Ask the neighbour".to_string()),
                2_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
    }

    /// The clearing half: touched-but-`None` sends an explicit clear, same
    /// discipline `patching_clears_repo_and_context_without_validation_tripping`
    /// follows for the properties card.
    #[tokio::test]
    async fn patching_clears_destination_and_notes() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-route-clear");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let route = fixture_route();

        let response = host.patch_route("seed-1", &route, true, None, true, None, 2_000).await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
    }

    /// Only `destination` is touched when `notes_touched` is `false` — the
    /// wasm-seam half of the "leave this field alone" contract
    /// [`Core::patch_route`] itself carries.
    #[tokio::test]
    async fn leaving_notes_untouched_mints_no_extra_field() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-route-partial");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let route = fixture_route();

        let response = host
            .patch_route("seed-1", &route, true, Some("Ship the deck".to_string()), false, None, 2_000)
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
    }
}

#[cfg(test)]
mod fog_tests {
    use super::*;

    /// #628: an unread project's fog answers empty, not "busy" — same
    /// contract [`TaskHostCore::project_links`] carries.
    #[tokio::test]
    async fn an_unread_projects_fog_answers_empty() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-fog-unread");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        assert_eq!(host.open_fog("p-1"), FogListResponse { kind: "ok", fog: Vec::new() });
    }

    /// #628: the authority 400s on an empty question, so this seam refuses
    /// one — blank and whitespace-only alike — before `Core::create_fog` is
    /// reached, and mints no queue entry. Same discipline
    /// `creating_a_link_rejects_an_empty_url_before_reaching_core` carries
    /// for a link's url.
    #[tokio::test]
    async fn creating_fog_rejects_an_empty_question_before_reaching_core() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-create-fog-empty");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        for question in ["", "   ", "\t\n"] {
            let response = host.create_fog("seed-1", "p-1", question, 0, 1_000).await;
            assert_eq!(response.kind, "failed", "{question:?} is refused");
            assert!(response.id.is_none());
            assert_eq!(
                host.queue_depth(),
                QueueDepthResponse { kind: "ok", depth: 0 },
                "{question:?} minted no queue entry"
            );
        }
    }

    /// A good question enqueues exactly one create, and the segment is
    /// still absent from the read, since there is no optimistic overlay —
    /// same "ok means enqueued, not saved" contract
    /// `creating_a_link_enqueues_it_and_overlays_nothing` carries.
    #[tokio::test]
    async fn creating_fog_enqueues_it_and_overlays_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-create-fog-ok");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();

        let response = host
            .create_fog("seed-1", "p-1", "  What permit does this need?  ", 0, 1_000)
            .await;

        assert_eq!(response.kind, "ok");
        assert!(response.id.is_some());
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
        assert_eq!(
            host.open_fog("p-1"),
            FogListResponse { kind: "ok", fog: Vec::new() },
            "no overlay: the segment appears only once a cycle pulls it back"
        );
    }

    fn fixture_fog() -> hummingbird_domain::Fog {
        hummingbird_domain::Fog {
            id: "f-1".to_string(),
            project_id: "p-1".to_string(),
            question: "What permit does this need?".to_string(),
            position: 0,
            resolved_at: None,
            version: 3,
        }
    }

    /// Rewording and repositioning enqueue one CAS patch.
    #[tokio::test]
    async fn patching_fogs_question_and_position_enqueues_one_patch() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-fog");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let fog = fixture_fog();

        let response = host
            .patch_fog(
                "seed-1",
                &fog,
                Some("What permit does this actually need?".to_string()),
                Some(1),
                false,
                None,
                2_000,
            )
            .await;

        assert_eq!(response.kind, "ok");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 1 });
    }

    /// `question` is checked for emptiness before `Core::patch_fog` is
    /// reached — same discipline `patching_a_link_to_a_blank_url_never_
    /// reaches_core` follows for a link's url.
    #[tokio::test]
    async fn patching_fog_to_a_blank_question_never_reaches_core() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-fog-bad-question");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let fog = fixture_fog();

        let response = host.patch_fog("seed-1", &fog, Some("   ".to_string()), None, false, None, 2_000).await;

        assert_eq!(response.kind, "failed");
        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 0 });
    }

    /// The resolve gesture: `resolvedAtTouched` true with a value stamps
    /// it, and an explicit clear (touched, `None`) reopens it — same
    /// touched-flag contract `removing_and_un_removing_a_link_each_
    /// enqueue_one_patch` carries for a link's removal.
    #[tokio::test]
    async fn resolving_and_reopening_fog_each_enqueue_one_patch() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-patch-fog-resolve");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "").await.unwrap();
        let fog = fixture_fog();

        let response = host.patch_fog("seed-1", &fog, None, None, true, Some(9_000), 2_000).await;
        assert_eq!(response.kind, "ok");

        let response = host.patch_fog("seed-2", &fog, None, None, true, None, 3_000).await;
        assert_eq!(response.kind, "ok");

        assert_eq!(host.queue_depth(), QueueDepthResponse { kind: "ok", depth: 2 });
    }
}
