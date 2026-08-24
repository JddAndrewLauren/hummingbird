//! `DiagnosticEventV1` (#706): the shared diagnostic vocabulary every host
//! writes into — the PWA's SharedWorker journal (#707), Android's
//! process-wide recorder (#709), and the authority's request boundary
//! (#711) all serialize *this* envelope, never a host-local shape.
//!
//! **Why this lives in `hummingbird-domain` and not `hummingbird-core`
//! (review round 1 on #711).** The envelope was first drafted in
//! `client/core/src/diagnostics/mod.rs` (#706), a member of the *client*
//! Cargo workspace. `hummingbird-authority` (the authority's request
//! boundary, #711) is a member of the *server* workspace and cannot depend
//! on a client-workspace crate without dragging its dependency tree along.
//! `hummingbird-domain` is the one crate both workspaces already compile —
//! `hummingbird-core`, `hummingbird-ffi-web` and `hummingbird-ffi-mobile`
//! on the client side, `hummingbird-authority` and (transitively)
//! `hummingbird-authority-worker` on the server side — and it carries
//! nothing but `serde`/`serde_json`, so moving the envelope here costs the
//! wasm32 worker build nothing it didn't already pay for `hummingbird-domain`
//! itself. This module is the sole owner of [`DiagnosticEventV1`],
//! [`DiagnosticEvent`] (the closed event-family enum) and [`Source`] —
//! #707/#708/#709/#710 are explicitly forbidden from redefining an owner
//! enum of their own, per their own briefs, and #711 does not either: it
//! constructs real [`DiagnosticEventV1`] values from here, through
//! `hummingbird_authority::diagnostics`'s constructors.
//!
//! **Payloads are closed types, never a string-keyed metadata map.** That is
//! what makes the redaction rule in this module's own tests checkable — by
//! scanning [`DiagnosticEvent`]'s own declaration and by grepping serialized
//! fixtures — rather than by review habit: a map could carry anything; a
//! fixed set of typed fields cannot silently grow a `title` or a `token`.
//!
//! **This module has no clock, RNG or session concept of its own.** Every
//! timestamp, id and delay in here is a plain value a caller already had;
//! `client/core/src/diagnostics/context.rs` supplies the client's
//! session/clock seam, and `hummingbird_authority::diagnostics` supplies the
//! authority's (a fixed session id — one Durable Object instance is ADR-0008's
//! "one workspace singleton" — plus a per-instance `seq` counter and
//! elapsed-time origin held in the `wasm32` shim's own state).
//!
//! **The cross-language payload rule for the `core.*` quad (#708 review
//! round 2).** [`DiagnosticEvent`] is *adjacently* tagged
//! (`#[serde(tag = "name", content = "payload")]`), so the instant a
//! payload-free variant grows a field, every writer of that family must
//! start emitting a `payload` object — and one class of writer is not the
//! Rust compiler's to check: `client/web/src/worker/diagnostics-events.ts`
//! serializes this same envelope from TypeScript. **Not Kotlin** — Android's
//! rows are minted in Rust
//! (`client/ffi-mobile/src/lib.rs`'s `diagnostic_event_json` over the
//! UniFFI `MobileDiagnosticEvent` enum), so the compiler already checks
//! that half; Kotlin only hand-builds the export wrapper
//! (`DiagnosticJournal.kt`). Making [`DiagnosticEvent::CoreBusy`] a struct variant in
//! #708 parted this enum from that live TypeScript writer, which kept
//! emitting a bare `{"name":"core.busy"}` that
//! `serde_json::from_str::<DiagnosticEvent>` rejects — invisible to every
//! gate, because the TS side's own DTO for `event` was `{name: string;
//! payload?: unknown}`. The rule adopted so the next such amendment does
//! not rediscover it, binding all four `core.*` members
//! ([`DiagnosticEvent::CoreWaitStarted`], [`DiagnosticEvent::CoreAcquired`],
//! [`DiagnosticEvent::CoreBusy`], [`DiagnosticEvent::CoreReleased`]) and
//! anything else a non-Rust host writes:
//!
//! 1. **A payload field is `Option<T>` when a live non-Core writer of that
//!    family structurally cannot observe the fact**, and required when
//!    every possible producer can. `None`/`null` means "this producer
//!    could not see it," never "no owner" — a reader treats a null `owner`
//!    as *unknown*, and must not fabricate one. This is the same encoding
//!    [`DiagnosticEvent::HttpFinished`]'s `status`/`failure` already use
//!    for "present only when there was one to record."
//! 2. **A non-Rust writer emits the `payload` key explicitly**, with every
//!    field present (`{"owner": null}`, not an absent `payload`). Serde's
//!    adjacent tagging needs the `content` key to exist once the variant is
//!    a struct variant, so an omitted `payload` is a hard deserialization
//!    failure rather than a defaulted one.
//! 3. **Every non-Rust writer's exact emitted shape is pinned by a Rust
//!    test in this module** —
//!    `every_web_worker_row_the_shared_worker_writes_deserializes` holds the
//!    literal JSON `diagnostics-events.ts` produces and parses it through
//!    `serde_json::from_str::<DiagnosticEventV1>`. A field added to a
//!    `core.*` variant without updating that writer fails *there*, loudly,
//!    which is the only place in either language the drift is detectable.
//!
//! Applied state of the quad: `core.busy` carries
//! `owner: Option<CoreOwner>` (TS writes `null` — the holder lives in a
//! private `Cell` inside `client/ffi-web/src/task_host.rs`'s `TaskCoreCell`
//! and reaches no worker response DTO); `core.released` carries a required
//! `owner` (only a Rust guard can produce one — the TS layer deliberately
//! emits no `core.released` at all); `core.wait_started`/`core.acquired`
//! **also carry `owner: Option<CoreOwner>`, per #710**: both have live TS
//! writers (`diagnostics-events.ts`'s `requestEnqueuedEvent`/
//! `requestDequeuedEvent`) whose own layer — the SharedWorker's serial
//! queue — cannot see a `CoreOwner` any more than `requestBusyEvent` can,
//! so rule 1 gives them `Option<CoreOwner>` too, with the TS side emitting
//! `{"owner": null}` explicitly (rule 2). Every Rust writer of these two
//! (the web host's `TaskCoreCell::checkout`/`read`/`read_mut`, the mobile
//! host's `lock_with_diagnostics`) always knows its owner and wraps it in
//! `Some`.
//!
//! [`FailureClass`] and [`route_template`] live here for the identical
//! cross-workspace reason: [`DiagnosticEvent::HttpFinished`]'s `failure`
//! field needs [`FailureClass`] to be nameable from both sides, and
//! [`route_template`] is the one pure function both
//! `client/core/src/diagnostics/route.rs` (which re-exports it) and
//! `hummingbird_authority::diagnostics` (which calls it directly) must agree
//! on byte-for-byte — a second, hand-copied implementation on either side is
//! exactly the drift a shared home exists to rule out.

use serde::{Deserialize, Serialize};

/// `DiagnosticEventV1`'s schema version — bumped only on a breaking change
/// to the envelope shape (never the event families; a new family or payload
/// field is additive and does not need this to move).
pub const DIAGNOSTIC_EVENT_SCHEMA_VERSION: u32 = 1;

/// Which host produced an event. `Core` is what `hummingbird-core` stamps;
/// `WebWorker`/`Android` are stamped by hosts that generate events without
/// going through `hummingbird-core`'s `DiagnosticSink` at all (a browser
/// SharedWorker or an Android process has no reason to round-trip through
/// Rust to record its own span); `Authority` is stamped by
/// `hummingbird_authority::diagnostics`, the only one of the four that
/// constructs a [`DiagnosticEventV1`] from a different Cargo workspace than
/// the one this module was first drafted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Core,
    WebWorker,
    Android,
    Authority,
}

/// The envelope every diagnostic event is wrapped in, whatever host or
/// family produced it. `seq` and `elapsed_ms` are monotonic *within one
/// session* — for `hummingbird-core`, a session outlives any single sync
/// cycle (`client/core/src/diagnostics/context.rs::DiagnosticSession`); for
/// the authority, "session" is the Durable Object instance's own lifetime
/// (ADR-0008's one workspace singleton), and `seq`/the elapsed-time origin
/// live in the `wasm32` shim's own `Cell` state rather than a type in this
/// crate — this module holds no clock, RNG or session state of its own.
/// `wall_clock_ms` is caller-supplied and exists for human correlation
/// against real-world time, not for ordering — `seq` is what orders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticEventV1 {
    pub schema_version: u32,
    pub seq: u64,
    pub wall_clock_ms: i64,
    /// Milliseconds since the emitting session's own origin — never sampled
    /// by this module itself.
    pub elapsed_ms: u64,
    pub session_id: String,
    pub source: Source,
    /// Which sync cycle this event belongs to, when it belongs to one.
    pub cycle_id: Option<String>,
    /// Which logical operation (a Core mutation, a background work item)
    /// this event belongs to, when it belongs to one. Independent of
    /// `cycle_id` — an operation can span cycles (queued offline, sent on a
    /// later one) and a cycle carries operations from more than one caller.
    pub operation_id: Option<String>,
    /// Which single HTTP round trip this event belongs to, when it belongs
    /// to one — `<cycle_id>-<ordinal>` on the client side, the authority's
    /// own accepted-or-generated request id on the server side.
    pub request_id: Option<String>,
    pub event: DiagnosticEvent,
}

/// One phase of a sync cycle (ADR-0007/ADR-0008) — the four boundaries
/// `client/core/src/sync/cycle.rs`'s own docs already name: drain, pull,
/// and the two persists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPhase {
    QueueDrain,
    Pull,
    QueuePersist,
    MirrorPersist,
}

/// How a whole sync cycle ended — `client::sync::cycle::CycleOutcome`
/// collapsed to its discriminant, deliberately: the outcome's own fields
/// (`drain`, `message`, ...) can carry more than this redacted vocabulary
/// wants recorded (`PersistFailed`'s `message` is a raw store error string).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutcome {
    Skipped,
    Blocked,
    CredentialNeeded,
    PersistFailed,
    PullFailed,
    Completed,
}

/// The closed transport vocabulary `network.changed` records —
/// `ConnectivityManager`'s `NetworkCapabilities.getTransportInfo`/
/// `hasTransport` collapsed to one value per reading (a real network can
/// carry more than one transport bit; the mobile host picks the single most
/// specific one — cellular/wifi/vpn before falling back to `Other`/`None`).
/// `None` is itself a reading, not an absent value: "no active network" is
/// exactly as informative as which transport is active, so it is a variant
/// here rather than folding into `Option<NetworkTransport>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkTransport {
    Cellular,
    Wifi,
    Vpn,
    Other,
    None,
}

/// Which trigger started one WorkManager run of `SyncWorker` — the app's
/// own two-member vocabulary (`SyncWorker.TRIGGER_TIMER`/`TRIGGER_PUSH`),
/// closed here for the same reason every other payload field is: a free
/// string would let the redaction rule's own guarantee slip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerTrigger {
    Timer,
    Push,
}

/// The HTTP verb one `http.*`/`request.*` event's call used. `Delete` was
/// added for #711: the authority's request boundary sees every verb its own
/// route table answers (`DELETE /api/admin/tokens/:id`,
/// `DELETE /api/push_targets/:id`), unlike a sync cycle's own transports
/// (`client::sync::write::transport::HttpMethod` has no `Get`; nothing on
/// the client ever `DELETE`s).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiagnosticHttpMethod {
    Get,
    Post,
    Patch,
    Put,
    Delete,
}

/// Who currently holds the single web/mobile-host task-core checkout
/// (#708's amendment to this shared enum, promised by #706 and #707's own
/// docs) — a payload field of `DiagnosticEvent::CoreBusy` (as
/// `Option<CoreOwner>`) and `DiagnosticEvent::CoreReleased` (required).
/// The asker in a `core.busy` answer
/// already knows who *it* is; the holder is the one fact a bare
/// re-entrancy guard (a `RefCell::take()` returning `None`) could never
/// answer on its own, so this is what makes that answer nameable rather
/// than a bare "no." **Deliberately plain prose, no intra-doc links, in
/// every variant's doc below**: this is `hummingbird-domain`, the one
/// crate shared by both the client and server Cargo workspaces (this
/// module's own header), and every concrete method this enum names —
/// `TaskHostCore::capture`, `Core::run`, and the rest — lives in
/// `hummingbird-core` or `hummingbird-ffi-web`, both *client*-workspace
/// crates this crate cannot depend on or link into. A `[text]` link to one
/// from here does not resolve; `cargo doc -p hummingbird-domain --no-deps`
/// is the check that would have caught it.
///
/// Deliberately coarser than "one variant per wasm-host entry point" —
/// the web host's `Projects` owner covers every dossier-card write
/// (create_project, patch_project, project links, Route, fog, actions,
/// Steps) and `Grill` covers the Grill-completion trio (complete_grill,
/// save_grill_draft, discard_grill_draft): a caller waiting behind the
/// core cares *which area* is holding it, not which of a dozen near-
/// identical CAS-patch methods inside that area happened to be the one.
///
/// **Base enum for #708/#710's reconciliation, and what #710 inherits.**
/// #708 (the web host) and
/// #710 (the mobile host) each need this vocabulary; #708 landed first, so
/// this is the base #710 rebases its own call sites onto rather than
/// forking a second enum. As of #708, `core.busy` and `core.released`
/// **both carry an owner** — any statement that `core.busy` "does not
/// carry an owner in this tree" describes the pre-#708 enum and is false
/// once this landed, so #710's rebase updates its own docs to match rather
/// than the other way round. #710's own two families
/// (`core.wait_started`/`core.acquired`) **also carry `owner: Option<CoreOwner>`**
/// now — see this module's header's "Applied state of the quad" paragraph
/// for why `Option` rather than a required field, and each variant's own
/// doc below for the rest. See this module's own header on why there is
/// exactly one owner enum, ever. No `Other`/catch-all variant: an
/// unnameable owner would defeat #712's whole interpretation table, whose
/// job is telling an operator who held the core, so every call site on
/// every host must be nameable by one of the members below (add a member
/// rather than reach for a catch-all).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreOwner {
    /// A sync cycle — the web host's `Core::run`/`Core::run_observed`
    /// checkout.
    Sync,
    /// The web host's capture (new-item) write.
    Capture,
    /// The web host's act write (start/complete/block/cancel an item).
    Act,
    /// The web host's triage write (edit-and-promote).
    Triage,
    /// The Grill-completion trio: complete_grill, save_grill_draft,
    /// discard_grill_draft — plus, on the mobile host (#710), that same
    /// trio's own reads (grill_draft, has_grill_draft), kept under this
    /// area rather than [`Self::Read`] since they are Grill-specific state,
    /// not a generic getter.
    Grill,
    /// Every project-dossier write: create_project, patch_project,
    /// project links, Route, fog, project actions, Steps — plus, on the
    /// mobile host (#710), that area's own read (`projects()`), for the
    /// identical reason [`Self::Grill`]'s doc states.
    Projects,
    /// create_rule/patch_rule (#140/ADR-0013), plus the mobile host's own
    /// `rules`/`rule` reads (#710), same reasoning as [`Self::Grill`].
    Rules,
    /// set_binding/set_question_enabled (#118/#715), plus the mobile
    /// host's own `api_version`/`dead_letters`/`bindings`/
    /// `question_switches` reads (#710) — device/config state this area
    /// already owns, kept off [`Self::Read`] for the same reason as
    /// [`Self::Grill`]'s doc.
    Settings,
    /// A read-only getter's own acquisition (#708 review round 1,
    /// finding 1) — every one of the web host's read-only accessors
    /// (frontier/ledger/search/bindings/pane_read/etc.) shares this one
    /// identity rather than borrowing a write category that does not
    /// describe them. Also the defensive fallback a read's `core.busy`
    /// answer uses in the (should-be-unreachable) case where the checkout
    /// slot is empty but no holder was recorded — an invariant violation
    /// this vocabulary still needs a legal value for, rather than a panic.
    Read,
    /// #710: the mobile host's own calendar-lane reads of the shared
    /// `inner` lock (`MobileTaskHost::device_token`/`trips_calendar_id`,
    /// both called from `calendar_on_timer`) -- kept distinct from `Read`
    /// so contention the calendar poll causes (or waits behind) is legible
    /// as calendar-lane activity in the journal, not folded into the
    /// generic read identity every other getter shares. Never the
    /// calendar's own mutex (`MobileTaskHost::calendar`, a second lock
    /// entirely -- see `core_lock`'s module doc on that lock's ordering);
    /// only the brief acquisitions of `inner` the calendar lane makes
    /// around it.
    Calendar,
}

/// How one `operation.*`-family unit of work ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOutcome {
    Success,
    Failure,
}

/// The closed transport-failure classification `http.finished` records.
/// Exactly one of these seven — never the underlying message. HTTP status
/// codes are retained (`Http`'s `status`); no response content is, which is
/// the redaction rule this type exists to make checkable: there is no field
/// here a raw exception string or a response body could hide inside.
/// `client/core/src/diagnostics/failure.rs` owns the *classification*
/// (`classify_transport_error`, `from_adapter_error`) — those functions
/// take client-only types (`TransportError`, `AdapterError`) this crate has
/// no business naming; only the closed output vocabulary lives here, so
/// [`DiagnosticEvent::HttpFinished`] can name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FailureClass {
    Timeout,
    Connect,
    Http { status: u16 },
    Body,
    Decode,
    Cancelled,
    Unknown,
}

/// The authority's closed auth-result vocabulary (#711's acceptance list,
/// verbatim) — a payload field of [`DiagnosticEvent::RequestFinished`].
/// Lives here rather than in `hummingbird-authority` because it is part of
/// the wire shape this envelope commits to, the same reason
/// [`FailureClass`] does for `http.finished`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthResult {
    Accepted,
    Rejected,
    Forbidden,
    Admin,
}

/// The closed event vocabulary (#706's acceptance list, extended by #711's
/// `request.received`/`request.finished`). A variant's payload carries only
/// what is unique to that moment — every correlation id already lives on
/// [`DiagnosticEventV1`] itself, so no payload repeats
/// `cycle_id`/`operation_id`/`request_id`.
///
/// **Every family in this enum exists here, whether or not `hummingbird-core`
/// ever constructs one.** `core.*`, `operation.*`, `network.changed`,
/// `worker.*` and `push.received` are defined for #708/#709/#710 to emit;
/// `request.received`/`request.finished` are defined for, and constructed
/// only by, `hummingbird_authority::diagnostics` (#711) — `hummingbird-core`
/// never builds one of those two variants, the same way it never builds
/// `worker.started`.
///
/// `operation.abandoned` is #707's own addition to this owner enum (the
/// PWA SharedWorker's serial queue giving up *waiting*, distinct from
/// `operation.stalled`'s "still running past 30s"). No Rust crate in
/// either workspace constructs it: the only writer is
/// `client/web/src/worker/diagnostics-events.ts`, which serializes this
/// same envelope from TypeScript. It lives here anyway because this
/// module is the sole owner of the vocabulary — see that variant's own
/// doc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "payload")]
pub enum DiagnosticEvent {
    #[serde(rename = "session.started")]
    SessionStarted,

    #[serde(rename = "sync.started")]
    SyncStarted { force_full_sweep: bool },
    #[serde(rename = "sync.phase_started")]
    SyncPhaseStarted { phase: SyncPhase },
    #[serde(rename = "sync.phase_finished")]
    SyncPhaseFinished { phase: SyncPhase },
    #[serde(rename = "sync.finished")]
    SyncFinished { outcome: SyncOutcome },

    #[serde(rename = "http.started")]
    HttpStarted {
        method: DiagnosticHttpMethod,
        /// A route template (`/api/items/:id`) — never a concrete path.
        route: String,
    },
    #[serde(rename = "http.finished")]
    HttpFinished {
        method: DiagnosticHttpMethod,
        route: String,
        /// Present only when a response actually arrived.
        status: Option<u16>,
        /// Present only on failure — see [`FailureClass`] for why this,
        /// and not the underlying error's message, is what gets kept.
        failure: Option<FailureClass>,
    },

    /// #710: `owner` is the identity the waiter observed at the moment it
    /// started waiting — the current holder's, when the mutex was already
    /// held, or the waiter's own otherwise (see [`CoreOwner`]'s own doc).
    ///
    /// **`Option`, for the same structural reason as [`DiagnosticEvent::CoreBusy`]
    /// (this module's header rule 1).** `client/web/src/worker/diagnostics-events.ts`'s
    /// `requestEnqueuedEvent` also writes this family, from the
    /// SharedWorker's serial queue — a layer that cannot see a
    /// `CoreOwner` any more than `requestBusyEvent` can, so it emits
    /// `None`. Every Rust writer (the web host's `TaskCoreCell::checkout`/
    /// `read`/`read_mut`, the mobile host's `lock_with_diagnostics`) always
    /// knows its owner and wraps it in `Some`.
    #[serde(rename = "core.wait_started")]
    CoreWaitStarted { owner: Option<CoreOwner> },
    /// #710: `owner` is the caller that just acquired the mutex —
    /// `Option`, but for a narrower reason than [`DiagnosticEvent::CoreWaitStarted`]'s.
    /// `diagnostics-events.ts`'s `requestDequeuedEvent` writes this family
    /// too, from the SharedWorker's serial queue, and that queue layer is
    /// **not** structurally blind here the way it is for `core.busy`/
    /// `core.wait_started`: `onDequeue` runs with the dequeued
    /// `TaskWorkerRequest` in scope (`task-worker.ts`'s
    /// `createTaskRequestQueue`), so `request.type` is available and could
    /// be mapped to a `CoreOwner`. `owner: null` here is a deliberate
    /// choice not to — that mapping would be a second, independently
    /// maintained copy of the identity the wasm host's own
    /// `Source::Core` `core.acquired` row already names authoritatively,
    /// and a second copy is exactly the kind of thing that drifts. `null`
    /// stays "this writer did not name one," not "unreachable."
    #[serde(rename = "core.acquired")]
    CoreAcquired { owner: Option<CoreOwner> },
    /// #708's amendment: names the [`CoreOwner`] holding the checkout —
    /// the asker already knows who *it* is, so this is the fact only the
    /// holder can supply.
    ///
    /// **`Option`, because one live writer of this family structurally
    /// cannot see the holder (#708 review round 2).** `Some(owner)` is what
    /// `client/ffi-web/src/task_host.rs`'s `TaskCoreCell` emits: it owns the
    /// holder slot, so it always names one. `None` is what
    /// `client/web/src/worker/diagnostics-events.ts`'s `requestBusyEvent`
    /// emits — the SharedWorker's serial queue learns "busy" only from a
    /// worker response's `kind: "busy"`, and not one of those response DTOs
    /// carries an owner; the holder never leaves that private `Cell`. That
    /// layer's row is therefore "a queue-level observer saw a request
    /// refused, holder unknown", which is a weaker but true fact, and the
    /// authoritative `Some(owner)` row for the *same* checkout is in the
    /// same journal under `source: Source::Core`. A reader (#712's
    /// interpretation table) reads `null` as **unknown**, never as "nobody
    /// held it", and joins to the `source: core` row by
    /// `(source, session_id)` scoping — see `TaskCoreCell`'s own doc on why
    /// spans never pair across sources. See this module's header for the
    /// rule this follows and what it binds #710 to.
    #[serde(rename = "core.busy")]
    CoreBusy { owner: Option<CoreOwner> },
    /// #708 review round 1: also carries [`CoreOwner`] — a checkout's own
    /// guard is the one thing that still knows which owner it was checked
    /// out as by the time it releases (the shared re-entrancy slot itself
    /// is cleared first), so the release event is where that fact would
    /// otherwise be lost rather than merely redundant with `core.acquired`
    /// (the two can be arbitrarily far apart in the stream once a hold
    /// runs long, which is exactly the case this whole slice exists to
    /// make legible). #710: also true of the mobile host — `owner` is the
    /// caller that just released the mutex, always recorded via a `Drop`
    /// guard, so a cancelled in-flight operation still records this.
    #[serde(rename = "core.released")]
    CoreReleased { owner: CoreOwner },

    #[serde(rename = "operation.requested")]
    OperationRequested,
    #[serde(rename = "operation.local_commit")]
    OperationLocalCommit,
    #[serde(rename = "operation.finished")]
    OperationFinished { outcome: OperationOutcome },
    #[serde(rename = "operation.slow")]
    OperationSlow,
    #[serde(rename = "operation.stalled")]
    OperationStalled,
    /// A queue gave up WAITING on a request rather than watching one still
    /// running — the terminal fact "the queue moved on without knowing if
    /// this ever finished," as opposed to [`DiagnosticEvent::OperationStalled`]'s
    /// "still running past 30s." Added by #707's SharedWorker journal (its
    /// serial queue's own 30s abandon-on-timeout, `worker/serial-queue.ts`)
    /// after a review caught the two being folded into one name with no
    /// discriminating field — #708's wasm-level `core.busy{owner}` amendment
    /// is expected to need a similar enum touch, so this is the first of
    /// what may be more than one #707/#708 amendment to this file.
    #[serde(rename = "operation.abandoned")]
    OperationAbandoned,

    /// `online` is #707's original field (a browser's `navigator.onLine`,
    /// still all the PWA ever supplies). The five fields below are #710's
    /// addition, all `Option` so #707's own construction sites (which
    /// build this variant's JSON by hand in TypeScript, never through this
    /// Rust type) go on emitting a bare `{"online": ...}` payload without
    /// having to name them. Android supplies every one of them from
    /// `ConnectivityManager`'s capabilities; no IP address or SSID is ever
    /// recorded (an SSID is as much a location fingerprint as an address,
    /// even though the acceptance list only names the address).
    #[serde(rename = "network.changed")]
    NetworkChanged {
        online: bool,
        transport: Option<NetworkTransport>,
        internet_capable: Option<bool>,
        validated: Option<bool>,
        metered: Option<bool>,
        roaming: Option<bool>,
    },

    /// #710: `trigger`/`attempt_count` are read off the `WorkManager`
    /// worker itself (`SyncWorker`'s own `runAttemptCount`) — the one place
    /// that distinguishes a first failure from a backoff loop.
    #[serde(rename = "worker.started")]
    WorkerStarted {
        trigger: WorkerTrigger,
        attempt_count: u32,
    },
    /// #710: same `trigger`/`attempt_count` as [`DiagnosticEvent::WorkerStarted`]'s
    /// own run, plus the outcome that run ended with.
    #[serde(rename = "worker.finished")]
    WorkerFinished {
        trigger: WorkerTrigger,
        attempt_count: u32,
        outcome: OperationOutcome,
    },

    #[serde(rename = "push.received")]
    PushReceived,

    /// #711: written before `handle()` runs (before schema init, alarm
    /// scheduling, or reading the body), so an incomplete span survives a
    /// hang — the same reason `http.started` above is emitted before its
    /// awaited call.
    #[serde(rename = "request.received")]
    RequestReceived {
        method: DiagnosticHttpMethod,
        /// A route template (`/api/items/:id`), never a concrete path —
        /// the same rule [`route_template`] enforces for `http.started`.
        route: String,
    },
    /// #711: written after the response is built (or, for a request the DO
    /// itself fails on before ever calling `handle()`, immediately before
    /// that failure propagates — see `hummingbird-authority-worker`'s own
    /// `fetch` docs). Never carries a token value, an `authorization`
    /// header, or a response body — only the non-secret token id and the
    /// closed [`AuthResult`].
    #[serde(rename = "request.finished")]
    RequestFinished {
        method: DiagnosticHttpMethod,
        route: String,
        status: u16,
        duration_ms: i64,
        response_bytes: usize,
        token_id: Option<String>,
        auth_result: AuthResult,
    },
}

/// Reduces a concrete request path to its route template — every path
/// segment that is not made up entirely of ASCII letters and underscores
/// becomes `:id`, except the segment immediately after `settings` (a
/// settings key, drawn from a small fixed non-secret vocabulary, some of
/// whose entries are hyphenated — e.g. `race-series` — so it must stay
/// concrete rather than being treated as an entity id).
///
/// Shared verbatim by `client/core/src/diagnostics/route.rs` (a thin
/// re-export) and `hummingbird_authority::diagnostics` (a direct call) —
/// see this module's own header for why a second, hand-copied
/// implementation on either side would be exactly the drift a shared home
/// exists to rule out.
///
/// **The known limit:** a purely alphabetic id would be left concrete,
/// indistinguishable from a static segment. No id either side of this
/// contract mints is purely alphabetic — `sweep.py`'s `deterministic_v4`,
/// the authority's own uuids, and its hex-encoded generated request ids all
/// contain a digit or a hyphen — so the limit is unreachable today; a
/// future id format without a digit or a hyphen would need a route table
/// here rather than a tweak to this rule.
pub fn route_template(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').collect();
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let previous_is_settings = index > 0 && segments[index - 1] == "settings";
            if previous_is_settings
                || segment.is_empty()
                || segment.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
            {
                segment.to_string()
            } else {
                ":id".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// `[A-Za-z0-9_-]{1,80}` — the shape every `X-Hummingbird-*` correlation
/// header value must satisfy client-side
/// (`client/core/src/diagnostics/route.rs::sanitize_header_value`), and the
/// shape the authority's request boundary re-checks server-side
/// (`hummingbird_authority::diagnostics::accept_cycle_id`/
/// `accept_request_id`) rather than trusting either header value —
/// correlation ids are attacker-supplied strings, so the client is not a
/// trust boundary. A pure predicate; each side's own enforcement point
/// calls it.
pub fn is_valid_header_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Field names a payload must never carry (#706's redaction rule) —
/// checked structurally, not by review habit, by two tests that cover
/// different halves of the claim: `no_variant_declares_a_forbidden_field_name`
/// scans this enum's whole declaration (every variant, including future
/// ones), and `no_payload_ever_carries_a_forbidden_field_name` checks real
/// serialized JSON for the variants that have a fixture. Each test's docs
/// state what it does not cover. Exact JSON key matches, case-insensitive, so a
/// legitimate `cycle_id`/`request_id`/`session_id`/`operation_id` (whose
/// key is not literally one of these words) never false-positives.
#[cfg(test)]
const FORBIDDEN_FIELD_NAMES: &[&str] = &[
    "authorization",
    "access_token",
    "api_key",
    "token",
    "credential",
    "password",
    "body",
    "request_body",
    "response_body",
    "title",
    "description",
    "url",
    "ip",
    "ip_address",
    "exception",
    "stack_trace",
    "message",
];

#[cfg(test)]
fn forbidden_keys_in(value: &serde_json::Value, found: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, nested) in map {
                if FORBIDDEN_FIELD_NAMES
                    .iter()
                    .any(|forbidden| forbidden.eq_ignore_ascii_case(key))
                {
                    found.push(key.clone());
                }
                forbidden_keys_in(nested, found);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                forbidden_keys_in(item, found);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `DiagnosticEventV1` must serialize stably — a round trip through
    /// `serde_json` is the whole claim, and pins the envelope's field names
    /// (a rename here breaks every host's stored history).
    #[test]
    fn a_diagnostic_event_v1_serializes_and_round_trips_stably() {
        let event = DiagnosticEventV1 {
            schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
            seq: 1,
            wall_clock_ms: 1_700_000_000_000,
            elapsed_ms: 42,
            session_id: "s-1".to_string(),
            source: Source::Core,
            cycle_id: Some("c-1".to_string()),
            operation_id: None,
            request_id: Some("c-1-0".to_string()),
            event: DiagnosticEvent::HttpStarted {
                method: DiagnosticHttpMethod::Get,
                route: "/api/changes".to_string(),
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"name\":\"http.started\""));
        assert!(json.contains("\"schema_version\":1"));

        let round_tripped: DiagnosticEventV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped, event);
    }

    /// #708's amendment: `core.busy` carries the closed [`CoreOwner`]
    /// naming who currently holds the checkout, not the asker — and it
    /// round-trips.
    #[test]
    fn a_core_busy_event_names_the_holder_and_round_trips() {
        let event = DiagnosticEventV1 {
            schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
            seq: 5,
            wall_clock_ms: 1_700_000_000_000,
            elapsed_ms: 3,
            session_id: "web-1".to_string(),
            source: Source::Core,
            cycle_id: None,
            operation_id: Some("op-1".to_string()),
            request_id: None,
            event: DiagnosticEvent::CoreBusy {
                owner: Some(CoreOwner::Sync),
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"name\":\"core.busy\""));
        assert!(json.contains("\"owner\":\"sync\""));
        let round_tripped: DiagnosticEventV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped, event);
    }

    /// #708 review round 1: `core.released` carries the same closed
    /// [`CoreOwner`] — the checkout's own guard is the only thing left
    /// that still knows which owner it was by the time it releases, since
    /// the shared holder slot is cleared before this fires.
    #[test]
    fn a_core_released_event_names_its_own_owner_and_round_trips() {
        let event = DiagnosticEventV1 {
            schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
            seq: 6,
            wall_clock_ms: 1_700_000_030_000,
            elapsed_ms: 30_000,
            session_id: "web-1".to_string(),
            source: Source::Core,
            cycle_id: None,
            operation_id: None,
            request_id: None,
            event: DiagnosticEvent::CoreReleased { owner: CoreOwner::Sync },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"name\":\"core.released\""));
        assert!(json.contains("\"owner\":\"sync\""));
        let round_tripped: DiagnosticEventV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped, event);
    }

    /// **The cross-language gate (#708 review round 2).** Every row
    /// `client/web/src/worker/diagnostics-events.ts` writes, as literal
    /// JSON, parsed back through this module's own `Deserialize` — the only
    /// check in either language that catches a `core.*` variant growing a
    /// field while that TypeScript writer keeps emitting the old shape. It
    /// caught exactly that: #708 made `core.busy` a struct variant and,
    /// because `DiagnosticEventNamePayload` typed `payload` as
    /// `unknown | undefined`, `pnpm run typecheck`, `pnpm run test`,
    /// `cargo test` and `cargo clippy` all stayed green while
    /// `requestBusyEvent` emitted a bare `{"name":"core.busy"}` that
    /// `serde_json` rejects. The last assertion below pins that old shape
    /// as an error, so this test is not vacuous. **Mutation-tested**:
    /// replacing the `core.busy` literal below with the pre-fix
    /// `{"name":"core.busy"}` fails the first block with "web-worker row
    /// must deserialize: missing field payload at line 1 column 198".
    /// Reverted before landing. The same mutation applied to the TypeScript
    /// writer (dropping `payload` from `requestBusyEvent`) is now a
    /// `pnpm run typecheck` error too — `WebWorkerDiagnosticEvent` in
    /// `client/web/src/store/protocol.ts` is that half of the gate, since a
    /// hand-copied literal here cannot notice the TS side drifting.
    ///
    /// Keep these strings byte-identical to what `diagnostics-events.ts`
    /// serializes (`envelope`'s field order and its `null`s included) —
    /// this module's header states the rule, and
    /// `client/web/src/worker/diagnostics-events.test.ts` pins the same
    /// shapes from the TypeScript side.
    #[test]
    fn every_web_worker_row_the_shared_worker_writes_deserializes() {
        // `requestEnqueuedEvent`, `requestDequeuedEvent`,
        // `requestAbandonedEvent`, `requestBusyEvent`,
        // `networkChangedEvent` — the whole of that module's public surface.
        let rows = [
            r#"{"schema_version":1,"seq":1,"wall_clock_ms":1700000000000,"elapsed_ms":0,"session_id":"ww-1","source":"web-worker","cycle_id":null,"operation_id":null,"request_id":null,"event":{"name":"core.wait_started","payload":{"owner":null}}}"#,
            r#"{"schema_version":1,"seq":2,"wall_clock_ms":1700000000010,"elapsed_ms":10,"session_id":"ww-1","source":"web-worker","cycle_id":null,"operation_id":null,"request_id":null,"event":{"name":"core.acquired","payload":{"owner":null}}}"#,
            r#"{"schema_version":1,"seq":3,"wall_clock_ms":1700000030000,"elapsed_ms":30000,"session_id":"ww-1","source":"web-worker","cycle_id":null,"operation_id":null,"request_id":null,"event":{"name":"operation.abandoned"}}"#,
            r#"{"schema_version":1,"seq":4,"wall_clock_ms":1700000000020,"elapsed_ms":20,"session_id":"ww-1","source":"web-worker","cycle_id":null,"operation_id":null,"request_id":null,"event":{"name":"core.busy","payload":{"owner":null}}}"#,
            r#"{"schema_version":1,"seq":5,"wall_clock_ms":1700000000030,"elapsed_ms":30,"session_id":"ww-1","source":"web-worker","cycle_id":null,"operation_id":null,"request_id":null,"event":{"name":"network.changed","payload":{"online":false}}}"#,
        ];
        for row in rows {
            let parsed: DiagnosticEventV1 = serde_json::from_str(row)
                .unwrap_or_else(|e| panic!("web-worker row must deserialize: {e}\n  row: {row}"));
            assert_eq!(parsed.source, Source::WebWorker);
        }

        // The wait_started/acquired/busy rows' `owner` really is read back
        // as "unknown", not as some default owner — the distinction this
        // module's header rule 1 rests on.
        let wait_started: DiagnosticEventV1 = serde_json::from_str(rows[0]).unwrap();
        assert_eq!(wait_started.event, DiagnosticEvent::CoreWaitStarted { owner: None });
        let acquired: DiagnosticEventV1 = serde_json::from_str(rows[1]).unwrap();
        assert_eq!(acquired.event, DiagnosticEvent::CoreAcquired { owner: None });
        let busy: DiagnosticEventV1 = serde_json::from_str(rows[3]).unwrap();
        assert_eq!(busy.event, DiagnosticEvent::CoreBusy { owner: None });

        // And the shape each of these writers emitted before their own fix
        // is a hard failure, which is what makes the drift a real defect
        // rather than a cosmetic one.
        assert!(
            serde_json::from_str::<DiagnosticEvent>(r#"{"name":"core.wait_started"}"#).is_err(),
            "a bare core.wait_started with no payload must not deserialize"
        );
        assert!(
            serde_json::from_str::<DiagnosticEvent>(r#"{"name":"core.acquired"}"#).is_err(),
            "a bare core.acquired with no payload must not deserialize"
        );
        assert!(
            serde_json::from_str::<DiagnosticEvent>(r#"{"name":"core.busy"}"#).is_err(),
            "a bare core.busy with no payload must not deserialize"
        );
    }

    /// The authority's own two families round-trip too, with `Source::Authority`
    /// and no `cycle_id` — the common case, since most authority traffic
    /// (settings reads, admin operations) carries no client sync cycle at
    /// all.
    #[test]
    fn an_authority_event_serializes_with_source_authority_and_round_trips() {
        let event = DiagnosticEventV1 {
            schema_version: DIAGNOSTIC_EVENT_SCHEMA_VERSION,
            seq: 3,
            wall_clock_ms: 1_700_000_000_000,
            elapsed_ms: 12,
            session_id: "authority".to_string(),
            source: Source::Authority,
            cycle_id: None,
            operation_id: None,
            request_id: Some("a1b2c3".to_string()),
            event: DiagnosticEvent::RequestFinished {
                method: DiagnosticHttpMethod::Delete,
                route: "/api/admin/tokens/:id".to_string(),
                status: 204,
                duration_ms: 7,
                response_bytes: 0,
                token_id: None,
                auth_result: AuthResult::Admin,
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"source\":\"authority\""));
        assert!(json.contains("\"name\":\"request.finished\""));
        assert!(json.contains("\"auth_result\":\"admin\""));
        let round_tripped: DiagnosticEventV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped, event);
    }

    /// One instance of every [`DiagnosticEvent`] family known when this was
    /// written, so [`no_payload_ever_carries_a_forbidden_field_name`] checks
    /// real serialized JSON — including keys that come from a *nested* type
    /// (a payload field whose own struct grows a bad field name) rather than
    /// from this enum's own declaration.
    ///
    /// **What the compiler does and does not force here.** `canonical`
    /// re-matches each fixture against `DiagnosticEvent` with **no wildcard
    /// arm**, so adding a variant fails to compile — `error[E0004]:
    /// non-exhaustive patterns` — until an arm exists for it. That forces an
    /// *arm*; it does **not** force a *fixture*. Review round 2 (on #706)
    /// disproved the earlier claim that it did: adding `BrandNewFamily {
    /// title: String }` plus only the arm the compiler demanded left this
    /// array untouched and the redaction test still green. Stable Rust
    /// cannot count an enum's variants (`std::mem::variant_count` is
    /// nightly), so there is no way to assert this array is complete
    /// without either a derive dependency or macro-generating the enum —
    /// and the enum's declaration is the wire contract, which stays
    /// hand-written and readable.
    ///
    /// So the whole-enum guarantee lives in
    /// [`no_variant_declares_a_forbidden_field_name`] instead, which reads
    /// this enum's own source text and therefore covers every variant that
    /// exists, including ones nobody added a fixture for. This array carries
    /// the value-level half; that test carries the coverage half. **Neither
    /// one alone closes the rule** — see that test's docs for the residual
    /// hole it leaves.
    ///
    /// **Mutation-tested**: commenting out the `PushReceived` arm below
    /// reproduces `error[E0004]` at this function, pinning that the match
    /// really is exhaustive rather than accidentally carrying a stray
    /// wildcard. Reverted before landing.
    fn one_of_every_event_variant() -> Vec<DiagnosticEvent> {
        fn canonical(event: DiagnosticEvent) -> DiagnosticEvent {
            match event {
                DiagnosticEvent::SessionStarted => DiagnosticEvent::SessionStarted,
                DiagnosticEvent::SyncStarted { force_full_sweep } => {
                    DiagnosticEvent::SyncStarted { force_full_sweep }
                }
                DiagnosticEvent::SyncPhaseStarted { phase } => DiagnosticEvent::SyncPhaseStarted { phase },
                DiagnosticEvent::SyncPhaseFinished { phase } => DiagnosticEvent::SyncPhaseFinished { phase },
                DiagnosticEvent::SyncFinished { outcome } => DiagnosticEvent::SyncFinished { outcome },
                DiagnosticEvent::HttpStarted { method, route } => {
                    DiagnosticEvent::HttpStarted { method, route }
                }
                DiagnosticEvent::HttpFinished {
                    method,
                    route,
                    status,
                    failure,
                } => DiagnosticEvent::HttpFinished {
                    method,
                    route,
                    status,
                    failure,
                },
                DiagnosticEvent::CoreWaitStarted { owner } => DiagnosticEvent::CoreWaitStarted { owner },
                DiagnosticEvent::CoreAcquired { owner } => DiagnosticEvent::CoreAcquired { owner },
                DiagnosticEvent::CoreBusy { owner } => DiagnosticEvent::CoreBusy { owner },
                DiagnosticEvent::CoreReleased { owner } => DiagnosticEvent::CoreReleased { owner },
                DiagnosticEvent::OperationRequested => DiagnosticEvent::OperationRequested,
                DiagnosticEvent::OperationLocalCommit => DiagnosticEvent::OperationLocalCommit,
                DiagnosticEvent::OperationFinished { outcome } => {
                    DiagnosticEvent::OperationFinished { outcome }
                }
                DiagnosticEvent::OperationSlow => DiagnosticEvent::OperationSlow,
                DiagnosticEvent::OperationStalled => DiagnosticEvent::OperationStalled,
                DiagnosticEvent::OperationAbandoned => DiagnosticEvent::OperationAbandoned,
                DiagnosticEvent::NetworkChanged {
                    online,
                    transport,
                    internet_capable,
                    validated,
                    metered,
                    roaming,
                } => DiagnosticEvent::NetworkChanged {
                    online,
                    transport,
                    internet_capable,
                    validated,
                    metered,
                    roaming,
                },
                DiagnosticEvent::WorkerStarted { trigger, attempt_count } => {
                    DiagnosticEvent::WorkerStarted { trigger, attempt_count }
                }
                DiagnosticEvent::WorkerFinished {
                    trigger,
                    attempt_count,
                    outcome,
                } => DiagnosticEvent::WorkerFinished {
                    trigger,
                    attempt_count,
                    outcome,
                },
                DiagnosticEvent::PushReceived => DiagnosticEvent::PushReceived,
                DiagnosticEvent::RequestReceived { method, route } => {
                    DiagnosticEvent::RequestReceived { method, route }
                }
                DiagnosticEvent::RequestFinished {
                    method,
                    route,
                    status,
                    duration_ms,
                    response_bytes,
                    token_id,
                    auth_result,
                } => DiagnosticEvent::RequestFinished {
                    method,
                    route,
                    status,
                    duration_ms,
                    response_bytes,
                    token_id,
                    auth_result,
                },
                // No `_` arm — see this function's doc comment.
            }
        }

        [
            DiagnosticEvent::SessionStarted,
            DiagnosticEvent::SyncStarted { force_full_sweep: true },
            DiagnosticEvent::SyncPhaseStarted { phase: SyncPhase::QueueDrain },
            DiagnosticEvent::SyncPhaseFinished { phase: SyncPhase::Pull },
            DiagnosticEvent::SyncFinished { outcome: SyncOutcome::Completed },
            DiagnosticEvent::HttpStarted {
                method: DiagnosticHttpMethod::Get,
                route: "/api/items/:id".to_string(),
            },
            DiagnosticEvent::HttpFinished {
                method: DiagnosticHttpMethod::Patch,
                route: "/api/items/:id".to_string(),
                status: Some(200),
                failure: None,
            },
            DiagnosticEvent::CoreWaitStarted { owner: Some(CoreOwner::Sync) },
            DiagnosticEvent::CoreAcquired { owner: Some(CoreOwner::Capture) },
            DiagnosticEvent::CoreBusy {
                owner: Some(CoreOwner::Sync),
            },
            DiagnosticEvent::CoreReleased { owner: CoreOwner::Triage },
            DiagnosticEvent::OperationRequested,
            DiagnosticEvent::OperationLocalCommit,
            DiagnosticEvent::OperationFinished { outcome: OperationOutcome::Success },
            DiagnosticEvent::OperationSlow,
            DiagnosticEvent::OperationStalled,
            DiagnosticEvent::OperationAbandoned,
            DiagnosticEvent::NetworkChanged {
                online: true,
                transport: Some(NetworkTransport::Wifi),
                internet_capable: Some(true),
                validated: Some(true),
                metered: Some(false),
                roaming: Some(false),
            },
            DiagnosticEvent::WorkerStarted {
                trigger: WorkerTrigger::Timer,
                attempt_count: 1,
            },
            DiagnosticEvent::WorkerFinished {
                trigger: WorkerTrigger::Push,
                attempt_count: 2,
                outcome: OperationOutcome::Success,
            },
            DiagnosticEvent::PushReceived,
            DiagnosticEvent::RequestReceived {
                method: DiagnosticHttpMethod::Get,
                route: "/api/items/:id".to_string(),
            },
            DiagnosticEvent::RequestFinished {
                method: DiagnosticHttpMethod::Get,
                route: "/api/items/:id".to_string(),
                status: 200,
                duration_ms: 10,
                response_bytes: 128,
                token_id: Some("device-mac".to_string()),
                auth_result: AuthResult::Accepted,
            },
        ]
        .into_iter()
        .map(canonical)
        .collect()
    }

    /// The `DiagnosticEvent` declaration's own source text, from `pub enum
    /// DiagnosticEvent {` to the column-0 `}` that closes it. Deliberately
    /// *not* the whole file: [`FORBIDDEN_FIELD_NAMES`] itself lists every
    /// forbidden word as a literal, so a whole-file scan would always fail.
    fn diagnostic_event_declaration() -> &'static str {
        let source = include_str!("diagnostics.rs");
        let start = source
            .find("pub enum DiagnosticEvent {")
            .expect("DiagnosticEvent's declaration is in this module's own source");
        let body = &source[start..];
        let end = body
            .find("\n}\n")
            .expect("DiagnosticEvent's declaration closes with a column-0 brace");
        &body[..end]
    }

    /// **The whole-enum half of #706's redaction rule.** Scans
    /// `DiagnosticEvent`'s own declaration and rejects any forbidden word
    /// appearing as a bare token on a non-comment line — which catches both
    /// a field named `title` and a `#[serde(rename = "title")]` on a
    /// differently-named one. Unlike
    /// [`no_payload_ever_carries_a_forbidden_field_name`] this enumerates no
    /// variants, so it covers every variant that exists by construction: a
    /// new family cannot slip past it by having no fixture, which is exactly
    /// how the earlier version of this rule was disproved in review round 2
    /// (on #706).
    ///
    /// **Mutation-tested:** adding `BrandNewFamily { title: String }` to the
    /// enum, with only the arm the compiler demands in
    /// [`one_of_every_event_variant`] and no fixture, fails *this* test
    /// (`forbidden field name(s) declared on DiagnosticEvent: ["title"]`)
    /// where the fixture test passed. Reverted before landing.
    ///
    /// **The residual hole, stated honestly.** This is a source-text check,
    /// so it sees only names written literally in *this* declaration. A
    /// forbidden key reaching the wire from a *nested* type — a payload
    /// field like `failure: Option<FailureClass>` whose own struct grows a
    /// `message` field in another module — is invisible here; that case is
    /// the fixture test's job, and only for variants that have a fixture. A
    /// new variant carrying a *new* nested type with a bad field inside it
    /// is covered by neither, and nothing in stable Rust closes that without
    /// generating the enum from a macro. All payload types today are the
    /// closed local enums above plus scalars, so the gap is unreachable at
    /// present.
    #[test]
    fn no_variant_declares_a_forbidden_field_name() {
        let mut offending = Vec::new();
        for line in diagnostic_event_declaration().lines() {
            let line = line.trim();
            if line.starts_with("//") {
                continue;
            }
            for token in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                if FORBIDDEN_FIELD_NAMES
                    .iter()
                    .any(|forbidden| forbidden.eq_ignore_ascii_case(token))
                {
                    offending.push(token.to_string());
                }
            }
        }
        assert!(
            offending.is_empty(),
            "forbidden field name(s) declared on DiagnosticEvent: {offending:?}"
        );
    }

    /// The scan above is only as good as the block it reads — pin that the
    /// extraction really found the enum body and stopped at its end, so a
    /// future edit that moves the declaration cannot silently reduce
    /// [`no_variant_declares_a_forbidden_field_name`] to scanning nothing.
    #[test]
    fn the_scanned_declaration_is_the_whole_enum_body_and_no_more() {
        let declaration = diagnostic_event_declaration();
        assert!(declaration.contains("SessionStarted"));
        assert!(declaration.contains("PushReceived"));
        assert!(declaration.contains("RequestFinished"));
        assert!(declaration.contains("force_full_sweep"));
        // Stops at the enum's own closing brace — the next item in the file
        // is not in scope.
        assert!(!declaration.contains("pub fn route_template"));
        assert!(!declaration.contains("FORBIDDEN_FIELD_NAMES"));
    }

    /// #706 acceptance: "A redaction test rejects forbidden field names ...
    /// a future payload field called `title` or `token` fails a test
    /// rather than a review." The value-level half — see
    /// [`one_of_every_event_variant`] for what this does and does not
    /// cover, and [`no_variant_declares_a_forbidden_field_name`] for the
    /// whole-enum half.
    ///
    /// **Mutation-tested:** adding a field named `title` to `SyncStarted`
    /// made this fail. Reverted before landing.
    #[test]
    fn no_payload_ever_carries_a_forbidden_field_name() {
        let mut offending = Vec::new();
        for event in one_of_every_event_variant() {
            let value = serde_json::to_value(&event).unwrap();
            forbidden_keys_in(&value, &mut offending);
        }
        assert!(
            offending.is_empty(),
            "forbidden field name(s) found on a payload: {offending:?}"
        );
    }


    // ------------------------------------------------- route templating

    #[test]
    fn a_bare_collection_path_is_unchanged() {
        assert_eq!(route_template("/api/items"), "/api/items");
    }

    #[test]
    fn a_single_entity_id_is_templated() {
        assert_eq!(route_template("/api/items/a-1"), "/api/items/:id");
    }

    #[test]
    fn two_entity_ids_in_one_path_are_both_templated() {
        assert_eq!(
            route_template("/api/blocked_by/a-1/a-2"),
            "/api/blocked_by/:id/:id"
        );
    }

    #[test]
    fn a_recorded_route_never_contains_the_concrete_entity_id() {
        let template = route_template("/api/items/9f1c2e40-aaaa-4b2b-8c3d-000000000001");
        assert!(!template.contains("9f1c2e40"));
        assert_eq!(template, "/api/items/:id");
    }

    #[test]
    fn purely_alphabetic_route_words_survive_untouched() {
        assert_eq!(route_template("/api/sweep"), "/api/sweep");
        assert_eq!(route_template("/api/changes"), "/api/changes");
    }

    /// Pins the exact case review round 1 (on #706) found broken: a
    /// hyphenated settings key must survive concrete, the same as an
    /// unhyphenated one — `race-series` and `question-enabled-race` are
    /// both real keys (`client::sync::write::paths::setting`,
    /// `hummingbird-authority`'s settings handlers).
    #[test]
    fn a_hyphenated_settings_key_survives_concrete_same_as_an_unhyphenated_one() {
        assert_eq!(route_template("/api/settings/race-series"), "/api/settings/race-series");
        assert_eq!(
            route_template("/api/settings/question-enabled-race"),
            "/api/settings/question-enabled-race"
        );
        assert_eq!(route_template("/api/settings/theme"), "/api/settings/theme");
    }

    /// An entity id one level *past* the settings key is still templated —
    /// the exemption is exactly one segment wide, not "everything under
    /// `/api/settings`".
    #[test]
    fn only_the_segment_immediately_after_settings_is_exempt() {
        assert_eq!(
            route_template("/api/settings/race-series/a-1"),
            "/api/settings/race-series/:id"
        );
    }

    /// An authority-only route (an admin token id, never seen client-side)
    /// templates the same way — the function makes no reference to which
    /// side's routes it is fed, which is the whole point of sharing it.
    #[test]
    fn an_authority_only_route_templates_by_the_same_rule() {
        assert_eq!(
            route_template("/api/admin/tokens/t-diag"),
            "/api/admin/tokens/:id"
        );
        assert_eq!(route_template("/api/google/calendar_token"), "/api/google/calendar_token");
    }

    // ------------------------------------------------- header/id validation

    #[test]
    fn a_valid_header_value_accepts_letters_digits_underscore_and_hyphen() {
        assert!(is_valid_header_value("cycle-1_ABC123"));
    }

    #[test]
    fn an_empty_header_value_is_rejected() {
        assert!(!is_valid_header_value(""));
    }

    #[test]
    fn a_header_value_over_eighty_characters_is_rejected() {
        let too_long = "a".repeat(81);
        assert!(!is_valid_header_value(&too_long));
    }

    #[test]
    fn an_eighty_character_header_value_is_accepted() {
        let boundary = "a".repeat(80);
        assert!(is_valid_header_value(&boundary));
    }

    #[test]
    fn a_header_value_with_a_disallowed_character_is_rejected() {
        assert!(!is_valid_header_value("has a space"));
        assert!(!is_valid_header_value("has/a/slash"));
    }

    /// Percent-decoding never happens before this check on either side
    /// (the authority's `ApiRequest::path` doc, and the client's own
    /// headers are never percent-encoded to begin with) — pins that the
    /// validator sees raw bytes and rejects an encoded payload rather than
    /// a decoded form that might validate.
    #[test]
    fn a_percent_encoded_payload_is_rejected() {
        assert!(!is_valid_header_value("cycle%2F1"));
    }
}
