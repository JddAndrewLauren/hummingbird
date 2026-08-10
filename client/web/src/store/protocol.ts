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
    };
