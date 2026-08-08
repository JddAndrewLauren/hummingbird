// The message protocol between the main thread and the core Web Worker
// (#69). Shared by worker-client.ts (main thread) and worker/core.worker.ts
// (the worker itself) so both sides stay in sync on the wire shape.
//
// The worker->main direction is push-only, unprompted at module evaluation
// (the "ready"/"error" handshake) — see the comment below for why. The
// main->worker direction (issue #73's calendar requests) is different: the
// main thread only ever sends a `CalendarWorkerRequest` after observing
// "ready", so there is no handshake to race — by the time "ready" is
// observed, the worker has already synchronously attached its `onmessage`
// listener (see core.worker.ts).
//
// This is what makes the *ready* handshake immune to bundler transforms (PR
// #79 round-2 blocker): vite-plugin-top-level-await wraps the worker module
// in an async IIFE, so nothing here is guaranteed to run before the main
// thread's messages arrive; a request/response handshake AT CONSTRUCTION
// TIME therefore drops the request. In the worker->main direction there is
// no such race: the main thread attaches its listener synchronously in the
// same task that constructs the Worker, before any worker message can be
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
  | { type: "getCurrentNext"; nowMs: number };

// -- worker -> main -----------------------------------------------------

export type WorkerResponse =
  | { type: "ready"; apiVersion: number }
  | { type: "error"; message: string }
  | { type: "pollOutcome"; outcome: PollOutcomeName }
  | { type: "credentialEvents"; events: CredentialEventDTO[] }
  | {
      type: "currentNext";
      kind: RenderableCurrentNextKind;
      event: CurrentNextEventDTO | null;
      asOfMs: number | null;
    };
