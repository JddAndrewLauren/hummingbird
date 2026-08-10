/// <reference lib="webworker" />

// The core SharedWorker (ADR-0010, #126): loads the wasm-bindgen core
// (hummingbird-ffi-web, #67) off the main thread, and off every tab's own
// thread — there is exactly one of these per origin, not one per app
// instance. `vite-plugin-wasm` + `vite-plugin-top-level-await`
// (vite.config.ts) let this import the wasm-pack `--target bundler` output
// directly as an ES module, and those plugins apply to shared workers the
// same as dedicated ones.
//
// Every tab and the installed PWA window is a VIEW: it connects a
// `MessagePort` via `onconnect` below, and `PortRegistry` (ports.ts) is what
// turns that into "one core, N views" — it announces the "ready"/"error"
// handshake to each newly connecting port (never assume the first view is
// the only one; a view connecting after the core is already running must
// still see its own handshake) and broadcasts every published event
// (poll outcomes, credential events, the tile, the picker list) to every
// port currently connected. The one-at-a-time `createRequestQueue` from
// #73's calendar wiring is unchanged and now serialises requests arriving
// from every view, which is what makes duplicate triggers (e.g. two tabs
// both firing a focus poll) wasteful but never incorrect (ADR-0010).
//
// `self.onconnect` MUST be assigned synchronously, in this module's first
// turn, before anything below is awaited — this is why the wasm core is
// loaded with a dynamic `import()` inside an async IIFE rather than a
// static top-level import (PR #167 round-1 review, blocker 1). A `connect`
// event has no platform buffering: the connect queued by the very view that
// STARTS this SharedWorker fires as soon as the worker's event loop begins,
// which is immediately after this module's synchronous top level finishes
// running. If `onconnect` were assigned only after an awaited import
// resolved (as a static top-level import forces, since
// vite-plugin-top-level-await wraps the whole module in an async IIFE —
// see protocol.ts and PR #79's round-2 blocker for the same fact biting the
// view->worker direction), that first connect would be dropped and its view
// would sit on "Loading core…" forever, having never gotten a wired port or
// a `ready`. `PortRegistry` is built to be safe to `connect` against before
// the core exists: it queues the port and delivers its handshake
// retroactively once `activate` (or `activateError`, if the wasm import
// itself fails — e.g. a CSP rejecting WebAssembly compilation) resolves the
// race. See ports.ts's class doc for the full account.
//
// The handshake itself stays push-only, unprompted per port (never a
// request/response), for the same reason as the original dedicated-worker
// wiring: nothing here is guaranteed to run before a view's messages
// arrive, so a request/response handshake would race it and drop the
// request. Pushing worker -> view cannot race per connection: `onconnect`
// receives the port before anything else can post to it, and
// `PortRegistry`'s wiring attaches `onmessage` (which arms the port's
// incoming queue) before announcing.
//
// Issue #73's calendar wiring is the one case that goes the other direction
// (view -> worker): a view only ever sends a `CalendarWorkerRequest` after
// observing "ready" on its own port (see protocol.ts), and by the time any
// such request could arrive, that port's `onmessage` listener has already
// been attached, synchronously, before announcing ready.
//
// The core is not torn down when one view disconnects — SharedWorker's own
// lifetime already gives ADR-0010's rule for free: the browser keeps this
// global scope alive as long as any port is connected, and terminates it
// only once the last one is. Nothing here needs to detect a disconnect.

import type { CalendarWorkerRequest, TaskWorkerRequest } from "../store/protocol";
import { createRequestQueue } from "./calendar-worker";
import { PortRegistry, type PortLike } from "./ports";
import { isTaskWorkerRequest } from "./request-router";
import { createTaskRequestQueue, type TaskHostLike } from "./task-worker";

// The IndexedDB database name (ADR-0003: the host contributes exactly one
// thing at init — a storage path/namespace). No calendars are selected
// until the picker (a view) calls `setCalendarIds`.
const CALENDAR_NAMESPACE = "hummingbird-calendar";

// The owned-schema task binding's own namespace (#105/S7) — a sibling
// IndexedDB database, not the calendar one above; `Core::init`'s queue and
// mirror stores live under it.
const TASK_NAMESPACE = "hummingbird-task";

// The authority's origin (ADR-0003: `core` invents no deployment address of
// its own). Unset in dev by default, same posture as `VITE_GOOGLE_CLIENT_ID`
// (App.tsx) — an empty string builds a relative, `reqwest`-rejected URL, so
// every `runSync` request fails fast (network-free) as `"pull_failed"`
// rather than the task binding silently doing nothing.
const TASK_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? "";

const registry = new PortRegistry();

declare const self: SharedWorkerGlobalScope;

// Assigned before anything below is awaited — see the header comment for
// why this ordering is load-bearing.
self.onconnect = (event: MessageEvent) => {
  const port = event.ports[0] as unknown as PortLike;
  registry.connect(port);
};

/** A `TaskWorkerRequest` handler used only when the task host itself failed
 * to construct (a corrupt durable snapshot) — reported per request rather
 * than failing calendar activation too, since the two bindings are
 * otherwise fully independent (see the module doc). */
function failedTaskEnqueue(message: string): (request: TaskWorkerRequest) => Promise<void> {
  return async (request) => {
    console.error("task host unavailable, dropping request", request.type, message);
  };
}

void (async () => {
  try {
    const wasmModule = await import("../wasm/pkg/hummingbird_ffi_web");
    const { CalendarHost, core_api_version, createTaskHost } = wasmModule;

    const calendarHost = new CalendarHost(CALENDAR_NAMESPACE, []);
    // Every request goes through one at-a-time queue: `CalendarHost` is not
    // re-entrant (see createRequestQueue), and per-port `onmessage` handlers
    // on their own would run a fresh handler per message with no regard for
    // one already suspended on a network await — across ports, not just
    // within one.
    const calendarEnqueue = createRequestQueue(calendarHost, (response) =>
      registry.broadcast(response),
    );

    // The task binding (#105/S7) gets its own host and its own queue — see
    // `protocol.ts`'s note on why it is not merged with the calendar one.
    // Its failure to construct must not take calendar activation down with
    // it: the two are otherwise fully independent doors into the wasm
    // module, so a corrupt task snapshot is this binding's problem alone.
    let taskEnqueue: (request: TaskWorkerRequest) => Promise<void>;
    try {
      const taskHost: TaskHostLike = await createTaskHost(TASK_NAMESPACE, TASK_BASE_URL, "");
      taskEnqueue = createTaskRequestQueue(taskHost, (response) => registry.broadcast(response));
    } catch (taskErr) {
      taskEnqueue = failedTaskEnqueue(
        taskErr instanceof Error ? taskErr.message : String(taskErr),
      );
    }

    const dispatch = (request: CalendarWorkerRequest | TaskWorkerRequest): Promise<void> =>
      isTaskWorkerRequest(request) ? taskEnqueue(request) : calendarEnqueue(request);

    registry.activate(dispatch, core_api_version);
  } catch (err) {
    // Reaches every view as `{type: "error"}` via `PortRegistry` instead of
    // silently hanging them on "Loading core…" — see main.tsx for why a
    // SharedWorker's own `onerror` cannot be relied on to catch this.
    registry.activateError(err instanceof Error ? err.message : String(err));
  }
})();
