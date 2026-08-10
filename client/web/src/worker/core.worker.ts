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
//
// **ADR-0007's 60-second cadence is owned HERE, not per view** (S9 round-1
// review of PR #181): a `setInterval` inside a view's own hook multiplies
// with open-tab count — N tabs, N timers, N cycles/minute — which directly
// contradicts ADR-0010's amendment to ADR-0007 ("a second tab is a view,
// not a second cycle") and blows the ADR's explicit ~60 req/hr budget. This
// module constructs exactly one `sync-cadence.ts` cadence and exactly one
// `setInterval` for the whole origin below; `onOpen`/`onReconnect` fire
// once each, for the same reason — `onReconnect` on this worker's own
// `online` event, and `onOpen` on the first `pushTaskApiKey` any view sends
// (`dispatch.ts`, which owns that rule: at core activation no credential is
// known yet, which is not what `onOpen`'s contract asks for). The one thing
// this global scope genuinely cannot
// observe on its own is page visibility — no `document` exists here — so
// `VisibilityTracker` aggregates each view's own `setViewVisibility` report
// (`protocol.ts`) instead: one visible tab keeps the cycle running even
// while its siblings are backgrounded. A view's own `focus` event similarly
// has no worker-side equivalent and is forwarded per view as
// `syncFocusTrigger` — deliberately NOT deduplicated the way the timer is,
// since two tabs focusing near-simultaneously is the same "wasteful but
// never incorrect" duplicate-user-gesture case the calendar wiring above
// already accepts, not the unattended-clock defect this fix closes.

import type { TaskWorkerRequest, TaskWorkerResponse } from "../store/protocol";
import { createSyncCadence, SYNC_TIMER_MS, toCoreTrigger } from "../shell/sync-cadence";
import { createRequestQueue } from "./calendar-worker";
import { createDispatch } from "./dispatch";
import { PortRegistry, type PortLike } from "./ports";
import { createTaskRequestQueue, type TaskHostLike } from "./task-worker";
import { VisibilityTracker } from "./visibility-tracker";

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

// The shared cadence's own view-visibility aggregate (S9 round-1 review) —
// see the module doc above and `visibility-tracker.ts`. Declared alongside
// `registry` rather than inside the async IIFE below: `dispatch` (defined
// once wasm loads) closes over it, but a `setViewVisibility` request could
// in principle arrive as soon as any port is wired, and this must already
// exist by then.
const visibility = new VisibilityTracker<PortLike>();

declare const self: SharedWorkerGlobalScope;

// Assigned before anything below is awaited — see the header comment for
// why this ordering is load-bearing.
self.onconnect = (event: MessageEvent) => {
  const port = event.ports[0] as unknown as PortLike;
  registry.connect(port);
};

type TaskEnqueue = (request: TaskWorkerRequest) => Promise<void>;

/** A `TaskWorkerRequest` handler used only when the task host itself failed
 * to construct (a corrupt durable snapshot) — reported per request rather
 * than failing calendar activation too, since the two bindings are
 * otherwise fully independent (see the module doc).
 *
 * Post-batch review of PR #185: this used to `console.error` and nothing
 * else, so every view kept rendering a healthy `ready` (the calendar side's,
 * which really is fine) while every capture, every sync and every pushed
 * device token was dropped forever, with no retry and no user-visible
 * signal. It now also broadcasts `taskHostUnavailable` per dropped request —
 * per request rather than only once at construction, because a view that
 * connects after the failure was announced would otherwise never hear about
 * it (`PortRegistry.broadcast` reaches the views connected at the time). */
function failedTaskEnqueue(
  message: string,
  broadcast: (response: TaskWorkerResponse) => void,
): TaskEnqueue {
  return async (request) => {
    console.error("task host unavailable, dropping request", request.type, message);
    broadcast({ type: "taskHostUnavailable", message });
  };
}

/** Constructs the task host and its queue in the background, without making
 * `registry.activate` (and therefore every view's calendar-side "ready")
 * wait on it (PR #171 round-1 review) — `createTaskHost` only touches local
 * storage today, but the two bindings are otherwise fully independent doors
 * into the same wasm module, and #126 deliberately decoupled their failure
 * paths for exactly this reason: a slow or hung task host must not delay
 * calendar activation for every connected view.
 *
 * Requests arriving before this resolves queue on the returned promise
 * itself (`.then` callbacks fire in attachment order), so ordering across
 * task requests is preserved either way — see `dispatch` below, the one
 * caller. */
function createTaskEnqueueDeferred(
  createTaskHost: (namespace: string, baseUrl: string, apiKey: string) => Promise<TaskHostLike>,
): Promise<TaskEnqueue> {
  const broadcast = (response: TaskWorkerResponse) => registry.broadcast(response);
  return createTaskHost(TASK_NAMESPACE, TASK_BASE_URL, "")
    .then((taskHost) => createTaskRequestQueue(taskHost, broadcast))
    .catch((taskErr: unknown) => {
      const message = taskErr instanceof Error ? taskErr.message : String(taskErr);
      // Announce it once immediately, for the views already connected —
      // they may never issue another task request on their own.
      broadcast({ type: "taskHostUnavailable", message });
      return failedTaskEnqueue(message, broadcast);
    });
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
    // Deliberately not awaited here — see `createTaskEnqueueDeferred`'s doc.
    const taskEnqueueReady = createTaskEnqueueDeferred(createTaskHost);

    // The shared ADR-0007 cadence — see the module doc above for why this
    // is constructed exactly once here rather than once per view. `trigger`
    // is the un-collapsed `SyncCadenceTrigger` ("open" | "reconnect" |
    // "focus" | "timer"); this is the one place that both maps it to the
    // spelling `Core::run`'s own `Trigger` expects (`toCoreTrigger`, still
    // "open"/"reconnect"/"focus" -> "user") AND decides `forceFullSweep`
    // from it directly (#193: ADR-0008's "on app open" backstop is only
    // `"open"` — an already-warm core's `onFocus`/`onReconnect`/timer ticks
    // stay delta-only; a NEW VIEW connecting to a live core does not re-fire
    // `onOpen` at all, deliberately, per #193's triage: "a new view is not a
    // new core, and the core it connects to has already swept" (ADR-0010)).
    // `Math.random()`/`Date.now()` are the caller-injected clock/jitter
    // `Core::run` requires (this global scope is a real JS runtime, unlike
    // bare wasm32, so both are safe to call directly here). No in-flight
    // guard yet: a cycle still running when `task-worker.ts`'s 30s abandon
    // fires can overlap the next 60s tick's `runSync` and surface as
    // `"busy"` — tracked as issue #184.
    const cadence = createSyncCadence((trigger) => {
      void taskEnqueueReady.then((enqueue) =>
        enqueue({
          type: "runSync",
          nowMs: Date.now(),
          trigger: toCoreTrigger(trigger),
          forceFullSweep: trigger === "open",
          jitterUnit: Math.random(),
        }),
      );
    });

    // The three-way routing (shared cadence / task queue / calendar queue)
    // and ADR-0007's "on app open" trigger both live in `dispatch.ts` as
    // pure logic a node test can execute — see that module's doc. In
    // particular the open sweep fires on the FIRST `pushTaskApiKey`, not
    // here at activation: `onOpen` is documented as "call once the core is
    // ready and a task credential is known", and at activation no view has
    // had the chance to push one yet, so firing it here made every
    // session's first cycle a spurious `no_credential`.
    const dispatch = createDispatch<PortLike>({
      cadence,
      visibility,
      taskEnqueueReady,
      calendarEnqueue,
    });

    registry.activate(dispatch, core_api_version);

    // ADR-0007: "on reconnect" — the worker's own connectivity signal.
    // `online`/`offline` fire on whatever global scope implements
    // `WindowOrWorkerGlobalScope` per the HTML spec, a `SharedWorker`
    // included, so this needs no per-view forwarding.
    self.addEventListener("online", () => cadence.onReconnect());

    // ADR-0007's 60-second foreground timer: the ONE interval for the whole
    // origin (see the module doc above), paused while every connected view
    // reports hidden (`VisibilityTracker`) or this worker itself is
    // offline — both proven as pure logic in `sync-cadence.test.ts` and
    // `visibility-tracker.test.ts`.
    self.setInterval(() => {
      cadence.onTimerTick(visibility.isHidden(), self.navigator.onLine);
    }, SYNC_TIMER_MS);
  } catch (err) {
    // Reaches every view as `{type: "error"}` via `PortRegistry` instead of
    // silently hanging them on "Loading core…" — see main.tsx for why a
    // SharedWorker's own `onerror` cannot be relied on to catch this.
    registry.activateError(err instanceof Error ? err.message : String(err));
  }
})();
