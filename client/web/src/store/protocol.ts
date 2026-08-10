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
  dueDate: string | null;
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
  /** The host calls this once a device token is known (startup, or a
   * rotation) — never in response to anything the worker posted back,
   * because nothing the worker posts back ever carries the key. */
  | { type: "pushTaskApiKey"; apiKey: string }
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
    };

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
  | { type: "taskEvents"; events: TaskEventDTO[] };

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
