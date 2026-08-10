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

/** The shape of `currentOrNext`'s `"kind"` field. `"busy"` is the same
 * never-in-practice signal as the poll outcome above, and carries no
 * information about the tile — the worker drops it rather than posting it,
 * so it never reaches the store. */
export type CurrentNextKind =
  | "no_snapshot"
  | "none"
  | "in_progress"
  | "upcoming"
  | "busy";

/** The subset of [`CurrentNextKind`] that can reach the main thread — the
 * worker never forwards `"busy"`, so no consumer downstream has to consider
 * a kind that says nothing about the tile. */
export type RenderableCurrentNextKind = Exclude<CurrentNextKind, "busy">;

/** One selectable calendar offered by the picker — the core's
 * `CalendarListEntry` (`client/core/src/calendar/google/calendar_list.rs`),
 * which fetches it over the core's own `reqwest` path (ADR-0003). Nothing on
 * this side of the boundary ever calls Google directly. */
export interface CalendarListEntryDTO {
  id: string;
  summary: string;
}

/** The event fields the context tile renders — a narrowed mirror of core's
 * `EventRecord` (issue #70), not the full provider-agnostic shape. */
export interface CurrentNextEventDTO {
  title: string;
  startMs: number;
  endMs: number;
  allDay: boolean;
  htmlLink: string | null;
}

// -- main -> worker ---------------------------------------------------------

export type CalendarWorkerRequest =
  | { type: "pushToken"; token: string }
  | { type: "setCalendarIds"; calendarIds: string[] }
  | { type: "pollStart"; nowMs: number }
  | { type: "pollRefresh"; nowMs: number }
  | { type: "pollTimer"; nowMs: number }
  | { type: "getCurrentNext"; nowMs: number }
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
 * both have something to show. */
export interface DeadLetterEntryDTO {
  id: string;
  reason: "permanent" | "conflict";
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
  | { type: "capture"; seed: string; title: string; stage: TaskStageName; nowMs: number }
  | { type: "getFrontier" }
  | { type: "getTriageInbox" }
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
   * views the way the timer is: two tabs focusing near-simultaneously firing
   * two cycles is the same "wasteful but never incorrect" duplicate-gesture
   * case `core.worker.ts`'s calendar wiring already accepts for its own
   * request queue (ADR-0010) — an unattended clock multiplying with tab
   * count is the defect this type exists to close; a human's own actions
   * are not. */
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
  | { type: "frontier"; items: TaskItemDTO[] }
  | { type: "triageInbox"; items: TaskItemDTO[] }
  | { type: "isPendingResult"; itemId: string; pending: boolean }
  /** What one `Core::run` cycle resolved to, broadcast to every connected
   * port. Issue #195: also the last one is cached by `PortRegistry` and
   * replayed to a port that connects after it — see `queueDepth`'s doc
   * below and `ports.ts`'s class doc for the full "latest-state vs
   * one-shot" rule this and its siblings are classified under. */
  | {
      type: "syncOutcome";
      kind: TaskRunOutcomeKind;
      retryAfterMs: number | null;
      activeItemCount: number | null;
      wasFullSweep: boolean | null;
      deadLettered: number | null;
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
  | {
      type: "currentNext";
      kind: RenderableCurrentNextKind;
      event: CurrentNextEventDTO | null;
      asOfMs: number | null;
    }
  | TaskWorkerResponse;
