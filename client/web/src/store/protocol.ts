// The message protocol between a view (main thread) and the core
// SharedWorker (ADR-0010, #126 — one core per origin, formerly a dedicated
// Worker per app instance, #69). Shared by worker-client.ts (a view) and
// worker/core.worker.ts (the worker itself) so both sides stay in sync on
// the wire shape.
//
// The worker->view direction is push-only, unprompted per connecting port
// (the "ready"/"error" handshake) — see the comment below for why. The
// view->worker direction (issue #73's calendar requests) is different: a
// view only ever sends a `CalendarWorkerRequest` after observing "ready" on
// its own port, so there is no handshake to race — by the time "ready" is
// observed, the worker has already synchronously attached that port's
// `onmessage` listener (see core.worker.ts / ports.ts).
//
// This is what makes the *ready* handshake immune to bundler transforms (PR
// #79 round-2 blocker): vite-plugin-top-level-await wraps the worker module
// in an async IIFE, so nothing here is guaranteed to run before a view's
// messages arrive; a request/response handshake AT CONNECTION TIME therefore
// drops the request. In the worker->view direction there is no such race:
// each view attaches its port's listener synchronously in the same task
// that constructs the SharedWorker, before any message on that port can be
// dispatched.

/** One host-visible signal that a provider's credential no longer works. */
export interface CredentialEventDTO {
  provider: string;
  atMs: number;
}

/** The stable outcome names `hummingbird-ffi-web`'s `CalendarHost` resolves
 * poll triggers to (`client/ffi-web/src/calendar_host.rs::outcome_name`). */
export type PollOutcomeName =
  | "no_credential"
  | "held"
  | "succeeded"
  | "transient_failure"
  | "unauthorized"
  /** The host was already inside another call and nothing was attempted.
   * The worker's request queue means this should never be observed; the
   * wasm binding reports it rather than panicking if it ever is. */
  | "busy";

/** One selectable calendar offered by the picker — the core's
 * `CalendarListEntry` (`client/core/src/calendar/google/calendar_list.rs`),
 * which fetches it over the core's own `reqwest` path (ADR-0003). Nothing on
 * this side of the boundary ever calls Google directly. */
export interface CalendarListEntryDTO {
  id: string;
  summary: string;
}

// -- main -> worker ---------------------------------------------------------

export type CalendarWorkerRequest =
  | { type: "pushToken"; token: string }
  | { type: "setCalendarIds"; calendarIds: string[] }
  | { type: "pollStart"; nowMs: number }
  | { type: "pollRefresh"; nowMs: number }
  | { type: "pollTimer"; nowMs: number }
  /** Carries no token: the core lists with the credential it was already
   * pushed, so the picker's lookup costs the host nothing extra. */
  | { type: "listCalendars" };

// -- task binding (#105/S7) -------------------------------------------------
//
// The owned-schema counterpart to the calendar wiring above, one door into
// #104's `Core` instead of #72's `ContextPoller`. It gets its own single-file
// request queue (`worker/task-worker.ts`'s `createTaskRequestQueue`) rather
// than sharing the calendar one: the two wrap independent Rust objects with
// independent re-entrancy guards, so serialising them together would only
// buy incidental ordering neither side depends on. `PortRegistry` (ports.ts)
// does not care which queue a request lands in — `core.worker.ts`'s combined
// dispatcher (`worker/request-router.ts`) is what tells them apart, by
// `type`.

/** The six-stage lifecycle name the owned schema's `Stage` enum serializes
 * to (`server/domain/src/item.rs`; snake_case, byte-for-byte the DDL `CHECK`
 * literal). */
export type TaskStageName =
  | "triage"
  | "grilling"
  | "ready"
  | "in_progress"
  | "blocked"
  | "done";

/** S11/#109's closed act vocabulary — every affordance the UI offers on an
 * already-existing item (`client/ffi-web/src/task_host.rs`'s `parse_action`,
 * which maps this exact string set onto `hummingbird_core::ItemAction`).
 * Deliberately NOT a `TaskStageName`: this is the one place a UI action
 * crosses into the core, and it never carries a raw stage id — the state
 * `Core::act` sets is resolved from this name, server-side vocabulary
 * (Stage's own enum), never hardcoded here. There is no `"pick"`/`"depend"`
 * action: `"block"` sets `Stage::Blocked`, which means an external wait and
 * nothing else — expressing one action's dependency on another is a
 * `blocked_by` relation edge (S10's `getBlocked`), never this. */
export type TaskActionName = "start" | "complete" | "block" | "cancel";

/** S13/#111's triage promotion vocabulary — the only two stages a triage
 * mutation may promote a captured item into (`TriageDestination`,
 * `client/core/src/lib.rs`). Deliberately not a `TaskStageName`, same
 * "reject before the seam" discipline `TaskActionName` documents for its
 * own vocabulary — and deliberately no `"backlog"` spelling: the owned
 * schema's six-stage vocabulary has no such stage, so an item not yet ready
 * to promote simply stays in `"triage"` rather than sending a destination
 * the server cannot express. */
export type TriageDestinationName = "grilling" | "ready";

/** One `steps` row (ADR-0009), as the web host's JSON/DTO shape — a 1:1
 * field mirror of `hummingbird_domain::Step`, camelCased. Item detail's
 * checklist (issue #96, S10) — read-only from this binding; ticking one is
 * S11's concern. */
export interface StepDTO {
  id: string;
  itemId: string;
  body: string;
  done: boolean;
  position: number;
  deletedAt: number | null;
  version: number;
}

/** One [`TaskHostCore::blocked`] entry: an item and the open blockers
 * excluding it from the frontier — S10's "relation-blocked … the reason
 * visible" (issue #108). */
export interface BlockedFrontierEntryDTO {
  item: TaskItemDTO;
  blockedBy: TaskItemDTO[];
}

/** One `items` row (ADR-0009), as the web host's JSON/DTO shape — a 1:1
 * field mirror of `hummingbird_domain::Item`, camelCased. */
export interface TaskItemDTO {
  id: string;
  seq: number | null;
  title: string;
  description: string | null;
  stage: TaskStageName;
  size: "quick" | "short" | "deep" | null;
  energy: "low" | "medium" | "high" | null;
  context: string | null;
  priority: number;
  projectId: string | null;
  projectPos: number | null;
  deadline: string | null;
  scheduledDate: string | null;
  source: string | null;
  sourceKey: string | null;
  sourceUrl: string | null;
  archivedAt: number | null;
  createdAt: number;
  updatedAt: number;
  version: number;
  /** Whether this item is currently overlaid by an unconfirmed local
   * mutation (`Core::is_pending`, `ffi-web`'s `FrontierItemDTO`) — S10's "a
   * pending item is marked as such" (issue #108). Stamped by the host on
   * every item this DTO shape carries (frontier, triage inbox, blocked and
   * its blockers), not left to a separate `isPending` request per row. */
  pending: boolean;
}

/** What one binding is currently set to (#118, ADR-0015) — the wire shape of
 * `hummingbird_core::bindings::BindingValue`. A tagged union, deliberately
 * not `string | null`: "nobody has set this" and "this holds something that
 * is not text" are different facts, and collapsing the second into the first
 * would let the editor offer to overwrite a value it never showed anyone.
 * `"other"` carries the stored JSON verbatim so it can be displayed —
 * visibly odd, never quietly empty. */
export type BindingValueDTO =
  | { state: "unset" }
  | { state: "text"; text: string }
  | { state: "other"; raw: string };

/** One `settings` row as the bindings editor sees it (#118). `known` is
 * whether this build can WRITE the key (`BindingKey::parse` resolved it) —
 * an unknown key is a real row a newer build wrote and is shown read-only,
 * since `settings` has no DELETE and a key minted by mistake can never be
 * taken back out. `pending` is the same read-time overlay fact
 * `TaskItemDTO.pending` carries for an item, never a stored column. */
export interface BindingDTO {
  key: string;
  known: boolean;
  pending: boolean;
  value: BindingValueDTO;
}

// -- rules (#140, ADR-0012/0013) --------------------------------------------
//
// The rules screen: condition rows, a per-row "not" toggle, the
// enable/disable CAS toggle, and backtest. The kind registry
// (`hummingbird_domain::kind_registry_json`, #133) is the single source for
// which kinds/fields/operators exist — never a hand-maintained list here.

/** One ANDed field test — the wire shape of `hummingbird_domain::Condition`.
 * `value` is carried as `unknown`: its shape depends on the field's declared
 * type (a string, a string array, a number, a bool, or a duration string for
 * `within_next`/`within_last`), decided by `screens/rules/registry.ts`
 * against the kind registry, never by this type. */
export interface ConditionDTO {
  field: string;
  op: string;
  value: unknown;
  negate: boolean;
}

/** The two notification tiers (ADR-0012) — `hummingbird_domain::Tier`'s wire
 * spelling. */
export type TierName = "urgent" | "normal";

/** One `rules` row — a 1:1 field mirror of `hummingbird_domain::Rule`,
 * camelCased. `eventKind: null` is ADR-0013's "any kind" — narrows the
 * editor's field list to the Event core rather than naming no fields at
 * all. */
export interface RuleDTO {
  id: string;
  name: string;
  eventKind: string | null;
  conditions: ConditionDTO[];
  severity: string;
  tier: TierName;
  enabled: boolean;
  updatedAt: number;
  version: number;
}

/** The typed catalogue ADR-0013 gates operators by —
 * `hummingbird_domain::FieldType`'s wire spelling. `"dynamic"` is
 * `snapshot_change.value`/`.previous` only; the rules screen resolves its
 * legal operators against the concrete value present, never this type
 * alone (`screens/rules/operators.ts`). */
export type FieldTypeName =
  | "string"
  | "string_list"
  | "number"
  | "bool"
  | "timestamp"
  | "date"
  | "dynamic";

/** One field a kind (or the Event core) declares. */
export interface KindFieldDTO {
  name: string;
  fieldType: FieldTypeName;
}

/** One registry entry — `hummingbird_domain::EventKindEntry`'s wire shape.
 * `mints` is ADR-0013's raw-stream-vs-already-an-alert flag; a `mints:
 * false` kind (`alert_raised`) is still selectable, since a rule can react
 * to an alert even though it never re-mints one. */
export interface KindEntryDTO {
  key: string;
  mints: boolean;
  fields: KindFieldDTO[];
}

/** The kind registry export (#133/#140, ADR-0013): the exact catalogue the
 * rule engine evaluates against, so a kind added there surfaces here with
 * no UI-side change. `alarmIntervalMs` is the DO alarm interval (#138) —
 * what a `within_next`/`within_last` duration shorter than this should warn
 * about, never reject. */
export interface KindRegistryDTO {
  kinds: KindEntryDTO[];
  coreFields: KindFieldDTO[];
  alarmIntervalMs: number;
}

// -- the pane read (#245, ADR-0015) ----------------------------------------
//
// The generic read every standing question's pane starts from:
// `Core::pane_read` (`client/core/src/pane.rs`) for one source, camelCased by
// `worker/task-worker.ts`'s `mapPaneRead`. Two things are already decided by
// the time they arrive here and must never be re-derived on this side —
// each row's measured age (`Freshness::measure`'s clock rule) and which
// alerts are still live (ADR-0014's predicate). Everything else — the answer,
// the band, the threshold, the `(source, subjectKey)` join onto a pane — is
// this side's, per ADR-0015's carve-out.

/** How old one answer is (`hummingbird_core::freshness::Freshness`).
 *
 * A tagged union, not a nullable number, because two different unknowns
 * exist: `"unknown"` is *we do not know the age at all*, while `"age"` with
 * `declaredCadenceMs: null` is *we know the age but not what normal looks
 * like*. The invariant the Rust half exists to hold is that `"unknown"` must
 * never render as fresh — so a pane's own stale check has to consider this
 * arm explicitly (`waste.ts`'s `isStaleFreshness`), which a boolean or a
 * zero age would let it skip. */
export type FreshnessDTO =
  | { kind: "unknown" }
  | { kind: "age"; ageMs: number; declaredCadenceMs: number | null };

/** One snapshot row's envelope, read shallowly (`SnapshotEnvelope`).
 * `"malformed"` carries the reason in words a pane can render — "visibly
 * broken, never quietly empty". `schema` is passed through untouched and is
 * never checked against a source registry: a source this build has not heard
 * of is a fact about the build, and the pane says so itself. */
export type PaneEnvelopeDTO =
  | { kind: "ok"; schema: string; polledEveryMs: number | null; body: string }
  | { kind: "malformed"; reason: string };

/** One `context_snapshots` row as a pane reads it. `body` is a **string
 * containing JSON** all the way across the seam — opaque to everything but
 * the pane that owns the payload shape. */
export interface PaneSnapshotDTO {
  key: string;
  fetchedAtMs: number;
  envelope: PaneEnvelopeDTO;
  freshness: FreshnessDTO;
}

/** One live alert from a pane's own source. `subjectKey` is the pane join's
 * client half — `(source, subjectKey)` matches `(source, key)` — and it is
 * *additive*: an alert matching no pane is not dropped, it simply appears in
 * `AlertsScreen` alone. `null` is a legitimate value (an alert naming no
 * subject), not a missing field. */
export interface PaneAlertDTO {
  id: string;
  subjectKey: string | null;
  title: string;
  body: string | null;
  raisedAtMs: number;
  expiresAtMs: number | null;
}

/** One source's whole pane-facing read. `liveAlerts` is named for what it
 * is: the liveness filter already ran, core-side, against the `nowMs` the
 * request carried. */
export interface PaneReadDTO {
  source: string;
  snapshots: PaneSnapshotDTO[];
  liveAlerts: PaneAlertDTO[];
}

/** One `projects` row (ADR-0009), as the web host's JSON/DTO shape — a 1:1
 * field mirror of `hummingbird_domain::Project`, camelCased. Resolves a
 * `TaskItemDTO.projectId` to a real name for the frontier's "grouped by
 * project" display (issue #108, PR #200 review). */
export interface ProjectDTO {
  id: string;
  name: string;
  archivedAt: number | null;
  createdAt: number;
  updatedAt: number;
  version: number;
}

/** One drained `CoreEvent` (`client/core/src/lib.rs`), as the web host's
 * JSON shape. `"credential_needed"` is the only kind `Core` produces today. */
export interface TaskEventDTO {
  kind: "credential_needed";
  atMs: number;
}

/** One field a dead-lettered conflict disagreed on — S9's "1 edit didn't
 * apply" affordance (`client/ffi-web/src/task_host.rs`'s
 * `DeadLetterFieldDTO`). `local`/`server` are whatever JSON value that field
 * held on each side; deliberately `unknown` rather than a narrower type —
 * this is a display-only diff over an arbitrary entity's fields, not a
 * value this app ever reads structurally. */
export interface DeadLetterFieldDTO {
  field: string;
  local: unknown;
  server: unknown;
}

/** One dead-lettered outbound mutation, as the web host's JSON shape
 * (`DeadLetterEntryDTO`). `"permanent"` carries `message` and an empty
 * `fields`; `"conflict"` carries `fields` and no `message` — the two never
 * both have something to show. `"contention"` (#163) carries NEITHER: a
 * second 409 that stayed genuinely disjoint after the one bounded rebase
 * retry has no colliding field name and no server message, only repeated
 * churn. This union is closed on the TS side but the value arrives across a
 * JSON boundary, so typecheck cannot see a Rust variant that is missing
 * here — `client/ffi-web/src/task_host.rs`'s `map_dead_letter` is the one
 * place that mints these strings, and `protocol.test.ts` pins each of them
 * as accepted. */
export interface DeadLetterEntryDTO {
  id: string;
  reason: "permanent" | "conflict" | "contention";
  message: string | null;
  fields: DeadLetterFieldDTO[];
  atMs: number;
}

/** What one `Core::run` cycle resolved to — the stable string names
 * `hummingbird-ffi-web`'s `TaskHost::runSync` (`client/ffi-web/src/lib.rs`)
 * resolves to, plus whatever payload S9's sync-status / "1 edit didn't
 * apply" affordance reads. `"busy"` is the same never-in-practice signal the
 * calendar binding's `PollOutcomeName` documents — the task binding's own
 * single-file queue means it should never be observed either. */
export type TaskRunOutcomeKind =
  | "no_credential"
  | "held"
  | "skipped"
  | "blocked"
  | "credential_needed"
  | "persist_failed"
  | "pull_failed"
  | "completed"
  | "busy";

export type TaskWorkerRequest =
  /** The host calls this on a genuinely new or re-entered token — a
   * first-run entry, or a deliberate re-submit through the 401 re-prompt —
   * never in response to anything the worker posted back, because nothing
   * the worker posts back ever carries the key. This is the only message
   * that resumes a held credential (issue #196: shape 2 splits rehydration
   * from rotation at this protocol level precisely so that only THIS one
   * can). */
  | { type: "pushTaskApiKey"; apiKey: string }
  /** Issue #196 (shape 2): the rehydration counterpart to `pushTaskApiKey`
   * — the host calls this at core start, and every time a view reaches
   * `ready` under #126's one-shared-core-per-origin
   * (`useTaskTokenWiring.ts`'s core-start effect), to re-supply whatever
   * device token is already stored. Deliberately NOT `pushTaskApiKey`: a
   * later view rehydrating the very token that just got 401'd must not be
   * able to resume the hold that rejection set, or the client silently
   * retries a credential already known to be dead and drops the pending
   * re-auth prompt. Still reaches the core's credential slot when nothing
   * is held — a first-run device's stored token must still become usable. */
  | { type: "initTaskApiKey"; apiKey: string }
  /** "Forget token" (#106/S8): clears the core's in-memory credential.
   * Carries nothing — there is no key to carry — and gets no reply; the
   * host fires this and moves on. Never touches the mirror or the queue. */
  | { type: "clearTaskApiKey" }
  /** `seed` mints the deterministic id `Core::capture` derives from it
   * (`client/core/src/sync/write/id.rs`) — caller-supplied so the view that
   * issued the capture can match its own seed back against the
   * `captureResult` broadcast (see `TaskWorkerResponse`), since the
   * worker->view direction never replies to just one sender. */
  /** `size`/`energy`/`context` (#208) carry the capture box's own optional
   * selections through to the wire, riding this single create mutation —
   * never a follow-up patch. `size`/`energy` are the wire's snake_case
   * vocabulary names, resolved by name through
   * `hummingbird_domain::Size`/`Energy::parse` at the seam, same discipline
   * `"triage"` already uses for its own edit fields; `null` (the resting
   * state every one of these controls starts at) means "not set", never a
   * default value reaching `Core`. */
  | {
      type: "capture";
      seed: string;
      title: string;
      stage: TaskStageName;
      size: "quick" | "short" | "deep" | null;
      energy: "low" | "medium" | "high" | null;
      context: string | null;
      nowMs: number;
    }
  /** S11/#109's act mutation: start, complete, block, cancel. `seed` is the
   * same "caller-mints, matches its own broadcast back" contract
   * `"capture"` documents above — `Core::act`'s own queue-entry id derives
   * from it. */
  | { type: "act"; seed: string; itemId: string; action: TaskActionName; nowMs: number }
  /** S13/#111's triage mutation: edits whatever `title`/`projectId`/`size`/
   * `energy`/`context` set (each `null` meaning "leave this field alone",
   * never "clear it" — `TriagePatch`'s own contract) and promotes to
   * `destination`, as one CAS `PATCH` — never four separate mutations for
   * four separate fields. `size`/`energy` are the wire's snake_case
   * vocabulary names, resolved by name through
   * `hummingbird_domain::Size`/`Energy::parse` on the way in, never a raw
   * index. Same caller-mints-`seed` contract as `"act"`. */
  | {
      type: "triage";
      seed: string;
      itemId: string;
      destination: TriageDestinationName;
      title: string | null;
      projectId: string | null;
      size: "quick" | "short" | "deep" | null;
      energy: "low" | "medium" | "high" | null;
      context: string | null;
      nowMs: number;
    }
  /** #118's binding write: one absolute-value CAS `PUT /api/settings/:key`,
   * enqueued durably like every other mutation. `key` is the kebab-case,
   * unversioned binding name (ADR-0015), resolved by name in
   * `client/ffi-web/src/task_host.rs`'s `set_binding` before it can reach
   * `Core` — an unrecognised one is refused at the seam rather than minting
   * a row into a table that has no DELETE. Same caller-mints-`seed`
   * contract as `"act"`. */
  | { type: "setBinding"; seed: string; key: string; value: string; nowMs: number }
  | { type: "getBindings" }
  /** #140: the kind registry export. Carries no argument and needs no
   * `Core` state — see `TaskHostCore::kind_registry`'s own doc. */
  | { type: "getKindRegistry" }
  | { type: "getRules" }
  /** #140's rule create: one `POST /api/rules`, enqueued durably like every
   * other mutation. `tier` is the wire's snake_case name, resolved by name
   * in `client/ffi-web/src/task_host.rs`'s `create_rule` before it can
   * reach `Core`. Same caller-mints-`seed` contract as `"capture"`. */
  | {
      type: "createRule";
      seed: string;
      name: string;
      eventKind: string | null;
      conditions: ConditionDTO[];
      severity: string;
      tier: TierName;
      enabled: boolean;
      nowMs: number;
    }
  /** #140's rule patch — the enable/disable toggle and every other rule
   * edit share this one message, exactly the "one CAS field" acceptance
   * criterion for the toggle: `null` on every optional field but `enabled`
   * IS the toggle. `current` is the caller's own last-known copy of the
   * row (from the `rules` push) — the CAS `base` a 409 is diffed against.
   * `eventKindTouched` distinguishes "leave `eventKind` alone" from "set
   * it, possibly to `null` for any kind" — the same double-`Option`
   * `RulePatch` itself carries, flattened for the wire. */
  | {
      type: "patchRule";
      seed: string;
      current: RuleDTO;
      name: string | null;
      eventKindTouched: boolean;
      eventKind: string | null;
      conditions: ConditionDTO[] | null;
      severity: string | null;
      tier: TierName | null;
      enabled: boolean | null;
      nowMs: number;
    }
  /** #245's generic pane read: one source's snapshot rows and its live
   * alerts. `nowMs` is the request's own clock — it decides both the
   * measured ages and the liveness filter, core-side, so nothing on this
   * side re-derives either. */
  | { type: "getPaneRead"; source: string; nowMs: number }
  | { type: "getFrontier" }
  | { type: "getTriageInbox" }
  /** Relation-blocked items with the reason visible — S10 (issue #108). */
  | { type: "getBlocked" }
  /** One item's Steps — item detail (issue #96, S10). */
  | { type: "getSteps"; itemId: string }
  /** Resolves the frontier's "grouped by project" display to real names
   * (issue #108, PR #200 review). */
  | { type: "getProjects" }
  | { type: "isPending"; itemId: string }
  | {
      type: "runSync";
      nowMs: number;
      trigger: "user" | "timer";
      forceFullSweep: boolean;
      jitterUnit: number;
    }
  /** S9's sync-status "queued" figure. As of issue #191, sent only once —
   * on becoming ready — rather than after every cycle: a view connecting
   * mid-session still needs an initial answer, but the per-cycle refresh
   * this used to also drive is now the worker's own unsolicited `queueDepth`
   * push at the tail of every `runSync` (`worker/task-worker.ts`'s `runSync`
   * branch), not a re-request from each view. */
  | { type: "getQueueDepth" }
  /** S9's "1 edit didn't apply" affordance. As of issue #191, sent only
   * once — on becoming ready, same as `getQueueDepth` above — since the
   * per-cycle case is now the worker's own unsolicited `deadLetters` push at
   * the tail of every cycle. */
  | { type: "getDeadLetters" }
  /** S9's mirror download button. */
  | { type: "getMirrorSnapshot" };

// -- shared cadence coordination (S9 round-1 review, PR #181) --------------
//
// ADR-0010's core lives in exactly one `SharedWorker` per origin, so
// ADR-0007's 60-second cadence must be ONE clock for the whole origin, not
// one per connected view — `core.worker.ts` owns the timer and the
// open/reconnect triggers itself (see that file). The one thing it
// genuinely cannot observe on its own is page visibility: a
// `SharedWorker`'s global scope has no `document`. These two messages are
// the only cadence-related traffic that still crosses the view->worker
// boundary, and neither one reaches either wasm host — `core.worker.ts`'s
// dispatch intercepts both before routing anything to the calendar or task
// queues.
export type SyncCadenceRequest =
  /** Sent on mount and on every `visibilitychange`. `hidden` is this one
   * view's own `document.hidden` — the worker aggregates every connected
   * view's report itself (`worker/visibility-tracker.ts`) so one visible
   * tab keeps the shared cycle running even while its siblings are
   * backgrounded. */
  | { type: "setViewVisibility"; hidden: boolean }
  /** Sent on a `window` `focus` event. Deliberately not deduplicated across
   * views the way the timer is — but not because a human's own actions are
   * exempt from that concern. Issue #190's ruling: a focus event maps onto
   * `sync-cadence.ts`'s `toCoreTrigger("focus") === "timer"`, so it never
   * resets ADR-0007's backoff, which is what makes duplicate focus triggers
   * across tabs harmless rather than the human-gesture argument this type
   * used to lean on. Two tabs focusing near-simultaneously still firing two
   * cycles is the same "wasteful but never incorrect" duplicate-trigger case
   * `core.worker.ts`'s calendar wiring already accepts for its own request
   * queue (ADR-0010) — it just no longer needs the gesture framing to get
   * there. */
  | { type: "syncFocusTrigger" }
  /** Issue #194: the header refresh control's task leg. Sent on a manual
   * refresh press, and routed through the shared cadence the same as every
   * other trigger — never posted straight to the task queue as a bespoke
   * `runSync` — so it stays ADR-0007's "same cycle, user-invoked; no special
   * path", and any future in-flight coalescing (#184) covers it too. */
  | { type: "manualSyncTrigger" };

export type TaskWorkerResponse =
  | {
      type: "captureResult";
      seed: string;
      kind: "ok" | "failed" | "busy";
      id: string | null;
      error: string | null;
    }
  /** S11/#109's act result, matched back by `seed` — same broadcast-not-reply
   * contract as `captureResult`. `"not_found"` is a caller mistake (no such
   * item); `"failed"` is everything else (an unrecognised action, or a
   * durability failure enqueueing the mutation). A successful act needs no
   * item payload here: `Core::act`'s overlay already updated, so the
   * existing per-cycle `frontier`/`blocked` refresh (`useFrontierWiring.ts`)
   * is what a caller re-reads to see it. */
  | {
      type: "actResult";
      seed: string;
      itemId: string;
      action: TaskActionName;
      kind: "ok" | "not_found" | "failed" | "busy";
      error: string | null;
    }
  /** S13/#111's triage result, matched back by `seed` — same
   * broadcast-not-reply contract as `actResult`. `"not_found"` is a caller
   * mistake (no such item); `"failed"` is everything else (an unrecognised
   * `destination`/`size`/`energy` name, or a durability failure enqueueing
   * the mutation). A successful triage needs no item payload here either:
   * `Core::triage`'s overlay already updated, so the existing
   * `triageInbox`/`frontier` refresh is what a caller re-reads to see it. */
  | {
      type: "triageResult";
      seed: string;
      itemId: string;
      kind: "ok" | "not_found" | "failed" | "busy";
      error: string | null;
    }
  /** #118's binding write result, matched back by `seed` — same
   * broadcast-not-reply contract as `actResult`. `"unknown_key"` is a caller
   * mistake the seam refused (not in ADR-0015's closed vocabulary);
   * `"failed"` is a durability failure enqueueing the write. A successful
   * write needs no payload: `Core::set_binding`'s overlay already updated,
   * and `store/worker-client.ts` re-requests `getBindings` behind an `ok`
   * exactly as it re-reads the frontier behind an `actResult`. */
  | {
      type: "setBindingResult";
      seed: string;
      key: string;
      kind: "ok" | "unknown_key" | "failed" | "busy";
      error: string | null;
    }
  | { type: "bindings"; bindings: BindingDTO[] }
  /** #140's rule create result, matched back by `seed` — same
   * broadcast-not-reply contract as `captureResult`. `"failed"` covers both
   * a rejected wire vocabulary (an unrecognised `tier`) and a durability
   * failure enqueueing the create; a *save-time* rule problem (#186 — an
   * unknown field, an illegal operator, a malformed duration) is discovered
   * later, at drain time, and surfaces through the ordinary dead-letter
   * journal rather than this result. */
  | {
      type: "createRuleResult";
      seed: string;
      kind: "ok" | "failed" | "busy";
      id: string | null;
      error: string | null;
    }
  /** #140's rule patch result (the enable/disable toggle and every other
   * edit), matched back by `seed`. A 409 is **handled, not swallowed**: it
   * is enqueued through the ordinary CAS path and either auto-rebases or
   * lands in the dead-letter journal, same as any other conflicted write —
   * this result only reports whether the enqueue itself succeeded. */
  | {
      type: "patchRuleResult";
      seed: string;
      ruleId: string;
      kind: "ok" | "failed" | "busy";
      error: string | null;
    }
  | { type: "rules"; rules: RuleDTO[] }
  /** Answers `getKindRegistry` (#140). Never `"busy"` — see
   * `TaskHostCore::kind_registry`'s own doc. */
  | { type: "kindRegistry"; registry: KindRegistryDTO }
  /** Answers `getPaneRead` (#245). Never posted for a `"busy"` read: an
   * empty pane read renders as "nothing is due", which a core that has not
   * loaded has no standing to claim — no answer, not an empty one. */
  | { type: "paneRead"; read: PaneReadDTO }
  | { type: "frontier"; items: TaskItemDTO[] }
  | { type: "triageInbox"; items: TaskItemDTO[] }
  | { type: "blocked"; entries: BlockedFrontierEntryDTO[] }
  | { type: "steps"; itemId: string; steps: StepDTO[] }
  | { type: "projects"; projects: ProjectDTO[] }
  | { type: "isPendingResult"; itemId: string; pending: boolean }
  /** What one `Core::run` cycle resolved to, broadcast to every connected
   * port. Issue #195: also the last one is cached by `PortRegistry` and
   * replayed to a port that connects after it — see `queueDepth`'s doc
   * below and `ports.ts`'s class doc for the full "latest-state vs
   * one-shot" rule this and its siblings are classified under.
   *
   * `atMs` is the cycle's OWN time (`worker/task-worker.ts`'s `runSync`
   * branch posts `request.nowMs`, the same clock value `Core::run` was
   * invoked with) — deliberately not left for the receiving view to infer
   * from its own message-receipt clock. Issue #195 round-1 review: a live
   * broadcast's receipt time is a safe stand-in for the cycle's real time
   * (sub-second apart), but a REPLAYED broadcast can reach a newly
   * connecting port arbitrarily long after the cycle it describes — a view
   * stamping its own `now()` on receipt would render a five-hour-old cycle
   * as "as of just now", exactly the false-freshness the badge exists to
   * avoid (`shell/sync-status.ts`). `store/worker-client.ts` reads this
   * field for `lastSyncAtMs` rather than calling its own clock. */
  | {
      type: "syncOutcome";
      kind: TaskRunOutcomeKind;
      retryAfterMs: number | null;
      activeItemCount: number | null;
      wasFullSweep: boolean | null;
      deadLettered: number | null;
      atMs: number;
    }
  /** Drained from `Core::take_events` and broadcast to every connected port
   * (`PortRegistry.broadcast`, not a reply to whichever port triggered the
   * drain) — the fix for #104's review finding that a destructive
   * single-reader drain would let the first tab to poll swallow an event
   * every other tab needed too. */
  | { type: "taskEvents"; events: TaskEventDTO[] }
  /** Answers `getQueueDepth`, AND — issue #191 — arrives unsolicited at the
   * tail of every completed `runSync`, whether or not any view asked this
   * cycle: `worker/task-worker.ts`'s `runSync` branch reads and posts it
   * once per cycle (dropped, not posted, on a `"busy"` read), broadcast to
   * every connected port the same as `syncOutcome`. This is what keeps N
   * connected views from each re-requesting it every cycle. `syncOutcome`,
   * this, `deadLetters` and `taskHostUnavailable` are also — issue #195 —
   * cached by `PortRegistry` and replayed to a port that connects after the
   * fact, so a view whose port wires up between cycles still starts from
   * the current figure instead of `null` (see `ports.ts`'s class doc for
   * which message types get this treatment and why). */
  | { type: "queueDepth"; depth: number }
  /** Answers `getDeadLetters`, AND — issue #191 — arrives unsolicited at the
   * tail of every completed `runSync`, same contract as `queueDepth` above:
   * read and posted at most once per cycle regardless of view count, never
   * on a `"busy"` read. Replayed to a late-connecting port the same as
   * `queueDepth` — see that field's doc. */
  | { type: "deadLetters"; entries: DeadLetterEntryDTO[] }
  | { type: "mirrorSnapshot"; mirror: unknown }
  /** The task host itself failed to construct (a corrupt durable snapshot,
   * say), so every task request — every capture, every sync, every pushed
   * device token — is being dropped for this core's whole lifetime.
   *
   * This is deliberately NOT `{type: "error"}`: #171 decoupled task-host
   * construction from calendar activation on purpose, so the calendar side
   * is genuinely still working and its views are genuinely still ready.
   * Without a signal of its own, though, that decoupling meant a view saw a
   * perfectly healthy `ready` while everything task-shaped vanished into a
   * `console.error` nobody reads (post-batch review of PR #185). Broadcast
   * once when construction fails AND again per dropped request, since a
   * broadcast reaches only the views connected at the time and a view that
   * connects later must still learn — its first task request tells it, and
   * — issue #195 — so does `PortRegistry`'s replay on connect, without
   * waiting for that view to send one. */
  | { type: "taskHostUnavailable"; message: string };

// -- worker -> main -----------------------------------------------------

export type WorkerResponse =
  | { type: "ready"; apiVersion: number }
  | { type: "error"; message: string }
  | { type: "pollOutcome"; outcome: PollOutcomeName }
  | { type: "credentialEvents"; events: CredentialEventDTO[] }
  /** Only posted for a successful listing. A held credential, a failed
   * lookup or a busy core say nothing about which calendars exist, and the
   * worker drops them rather than emptying a picker that is showing real
   * options. */
  | { type: "calendarList"; calendars: CalendarListEntryDTO[] }
  | TaskWorkerResponse;
