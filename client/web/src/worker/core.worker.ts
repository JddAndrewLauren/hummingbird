/// <reference lib="webworker" />

// The core SharedWorker (ADR-0010, #126): loads the wasm-bindgen core
// (hummingbird-ffi-web, #67) off the main thread, and off every tab's own
// thread — there is exactly one of these per origin, not one per app
// instance. `vite-plugin-wasm` + `vite-plugin-top-level-await`
// (vite.config.ts) let this import the wasm-pack `--target bundler` output
// directly as an ES module, and those plugins apply to shared workers the
// same as dedicated ones.
//
// This is not the only instantiation of that module any more (ADR-0025,
// #499): the main thread instantiates it a second time behind
// `src/decisions/seam.ts`, for the pure decision functions a React render
// has to call synchronously. That instance holds no core, no storage and no
// queue, which is the whole reason it does not make this a second sync
// engine — the seam's header carries the argument. What matters HERE is the
// direction of the boundary: nothing under `src/decisions/` may appear in
// this file's static import graph, because a static wasm import is exactly
// the top-level `await` the next paragraph forbids.
// `worker/worker-import-graph.test.ts` fails the build if one appears.
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
// `online` event, and `onOpen` on the first `initTaskApiKey`/`pushTaskApiKey`
// any view sends (`dispatch.ts`, which owns that rule: at core activation no
// credential is known yet, which is not what `onOpen`'s contract asks for).
// The one thing this global scope genuinely cannot
// observe on its own is page visibility — no `document` exists here — so
// `VisibilityTracker` aggregates each view's own `setViewVisibility` report
// (`protocol.ts`) instead: one visible tab keeps the cycle running even
// while its siblings are backgrounded. A view's own `focus` event similarly
// has no worker-side equivalent and is forwarded per view as
// `syncFocusTrigger` — deliberately NOT deduplicated the way the timer is.
// This is safe not because a focus is a harmless human gesture (issue #190's
// ruling: it is not the gesture ADR-0007's backoff-reset sentence is about
// at all — see `sync-cadence.ts`'s `toCoreTrigger`, which maps `"focus"`
// onto the core's `"timer"` spelling and so never resets backoff), but
// because two tabs focusing near-simultaneously is now just two cycles that
// each land as a no-op during backoff, the same "wasteful but never
// incorrect" duplicate-trigger case the calendar wiring above already
// accepts.

import type {
  DiagnosticsWorkerRequest,
  TaskWorkerRequest,
  TaskWorkerResponse,
} from "../store/protocol";
import {
  createSyncCadence,
  mergePendingSyncTrigger,
  SYNC_TIMER_MS,
  toCoreTrigger,
} from "../shell/sync-cadence";
import { createRequestQueue } from "./calendar-worker";
import { createDispatch, type DispatchDiagnostics, type DispatchVisibility } from "./dispatch";
import { mintCoreId } from "./core-id";
import { createDiagnosticsJournal } from "./diagnostics-journal";
import { PortRegistry, type PortLike } from "./ports";
import { isDiagnosticsWorkerRequest } from "./request-router";
import { createSyncRunGuard } from "./sync-run-guard";
import {
  createTaskRequestQueue,
  TASK_REQUEST_TIMEOUT_MS,
  type TaskDiagnostics,
  type TaskHostLike,
} from "./task-worker";
import { VisibilityTracker } from "./visibility-tracker";

// The IndexedDB database name (ADR-0003: the host contributes exactly one
// thing at init — a storage path/namespace). No calendars are selected
// until the picker (a view) calls `setCalendarSelections` — `"[]"` is the
// empty selection, in the JSON text that seam takes (#121).
const CALENDAR_NAMESPACE = "hummingbird-calendar";

// The owned-schema task binding's own namespace (#105/S7) — a sibling
// IndexedDB database, not the calendar one above; `Core::init`'s queue and
// mirror stores live under it.
const TASK_NAMESPACE = "hummingbird-task";

// The authority's origin (ADR-0003: `core` invents no deployment address of
// its own — the *host* supplies one, and this is the host).
//
// ADR-0006/0008 put the API same-origin with the shell at
// `hb.twinion.net/api/*`, so the origin the host should supply is, always
// and by definition, its own — which a `SharedWorker` knows at runtime
// without anything being configured or baked in. `self.location.origin` is
// `https://hb.twinion.net` in the deployed bundle and
// `http://localhost:5173` under `vite dev` (where `vite.config.ts` proxies
// `/api` on to `wrangler dev`), so dev and production exercise the same
// same-origin path rather than two different shapes.
//
// It must not be `""`: the transports build `{base}/api/...` verbatim
// (`client/core/src/sync/reqwest_transport.rs`), and an empty base yields a
// *relative* URL, which `reqwest` rejects before opening a socket — every
// cycle would fail as `"pull_failed"` forever. That is the correct
// never-configured state for a host with nowhere to point, but it is not
// this host, which always has an origin.
//
// `VITE_API_BASE_URL` stays as a build-time override for the one case the
// rule above does not cover — pointing a bundle at an authority that is
// deliberately not its own origin (a staging DO, a second workspace). It is
// unset in every checked-in configuration.
const TASK_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? self.location.origin;

// #707's SharedWorker diagnostic journal: one IndexedDB-backed journal for
// the whole origin, declared BEFORE `registry` below (which now needs it —
// see `diagnosticsPortHandler`) — for the same reason `visibility` is
// declared alongside `registry`: a `getDiagnostics`/`clearDiagnostics`
// request could in principle arrive as soon as any port is wired, well
// before the async IIFE below resolves. `Date.now()` anchors the session's
// `elapsed_ms` origin — bare wasm32 has no clock of its own, but this
// global scope is a real JS runtime (the same reasoning the module doc
// above gives for calling `Math.random()`/`Date.now()` directly in the
// cadence wiring).
const diagnosticsJournal = createDiagnosticsJournal(Date.now());

/** #707 review round 1: a core that fails to initialize could not, until
 * this, ever answer `getDiagnostics`/`clearDiagnostics` — `PortRegistry`'s
 * "failed" branch posted `{type: "error"}` and never assigned the port's
 * `onmessage` at all, so those two messages sat queued in the port
 * forever. The journal itself lives in this module's scope regardless of
 * whether the wasm import below ever resolves, so it has always been
 * reachable in principle — this is what makes it reachable in practice,
 * passed into `PortRegistry`'s constructor so it applies on the "failed"
 * branch too (see `ports.ts`'s `DiagnosticsPortHandler` doc). */
const diagnosticsPortHandler = {
  isDiagnosticsRequest: isDiagnosticsWorkerRequest,
  handle: (request: DiagnosticsWorkerRequest, port: PortLike) => {
    if (request.type === "getDiagnostics") {
      void diagnosticsJournal.export().then(({ events, droppedCount }) => {
        port.postMessage({ type: "diagnosticsExport", events, droppedCount });
      });
      return;
    }
    void diagnosticsJournal.clear();
  },
};

// Issue #172: the id every view's handshake carries, minted once per
// `SharedWorker` global scope — this module is evaluated exactly once per
// core, so the constant IS the core instance. `mintCoreId` owns the
// secure-context guard, and owns it in a sibling module precisely so a
// missing `crypto.randomUUID` can never throw here, where a throw would
// happen before `self.onconnect` is assigned and hang every view (see
// `core-id.ts`). A plain static call either way: this file's invariant is
// no top-level `await` in its static import graph, which that module does
// not touch.
const registry = new PortRegistry(mintCoreId(), diagnosticsPortHandler);

// The shared cadence's own view-visibility aggregate (S9 round-1 review) —
// see the module doc above and `visibility-tracker.ts`. Declared alongside
// `registry` rather than inside the async IIFE below: `dispatch` (defined
// once wasm loads) closes over it, but a `setViewVisibility` request could
// in principle arrive as soon as any port is wired, and this must already
// exist by then.
const visibility = new VisibilityTracker<PortLike>();

// The narrow adapter `worker/task-worker.ts`'s `createTaskRequestQueue`
// needs (`TaskDiagnostics`) — every method there is clock-free by design
// (see that interface's own doc), so this is the one place `Date.now()` is
// actually sampled for the task queue's own lifecycle events.
const taskDiagnostics: TaskDiagnostics = {
  recordEnqueued: () => diagnosticsJournal.recordEnqueued(Date.now()),
  recordDequeued: () => diagnosticsJournal.recordDequeued(Date.now()),
  recordAbandoned: () => diagnosticsJournal.recordAbandoned(Date.now()),
  recordBusy: () => diagnosticsJournal.recordBusy(Date.now()),
  drainAroundRequest: (run, drainHost) => diagnosticsJournal.drainAroundRequest(run, drainHost, Date.now),
};

/** Wraps the shared `VisibilityTracker` so #707's journal also learns about
 * the ONE visibility fact this global scope can observe: the aggregate
 * `isHidden()` flipping (`worker/visibility-tracker.ts`'s own doc — no
 * per-view report matters here, only the origin-wide answer the cadence
 * itself consults). Recorded as `network.changed`, re-checking
 * `navigator.onLine` at the moment it flips: `DiagnosticEvent::NetworkChanged`
 * carries only `{online}` (`client/core/src/diagnostics/mod.rs`), with no
 * field for visibility itself, so a visibility transition is folded into
 * the one shared payload shape rather than left unrecorded — see issue
 * #707's own posted finding on that gap. */
function wrapVisibilityForDiagnostics(
  tracker: VisibilityTracker<PortLike>,
  journal: typeof diagnosticsJournal,
): DispatchVisibility<PortLike> {
  let lastHidden = tracker.isHidden();
  return {
    setHidden: (port, hidden) => {
      tracker.setHidden(port, hidden);
      const nowHidden = tracker.isHidden();
      if (nowHidden !== lastHidden) {
        lastHidden = nowHidden;
        journal.recordNetworkChanged(Date.now(), self.navigator.onLine);
      }
    },
  };
}

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
    .then((taskHost) => createTaskRequestQueue(taskHost, broadcast, taskDiagnostics))
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

    const calendarHost = new CalendarHost(CALENDAR_NAMESPACE, "[]");
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
    // "focus" | "manual" | "timer"); this is the one place that both maps it
    // to the spelling `Core::run`'s own `Trigger` expects (`toCoreTrigger` —
    // "open"/"reconnect"/"manual" -> "user", but "focus" -> "timer" per
    // issue #190's ruling: a focus event never resets backoff) AND decides
    // `forceFullSweep`
    // from it directly (#193: ADR-0008's "on app open" backstop is only
    // `"open"` — an already-warm core's `onFocus`/`onReconnect`/timer ticks
    // stay delta-only; a NEW VIEW connecting to a live core does not re-fire
    // `onOpen` at all, deliberately, per #193's triage: "a new view is not a
    // new core, and the core it connects to has already swept" (ADR-0010)).
    // `Math.random()`/`Date.now()` are the caller-injected clock/jitter
    // `Core::run` requires (this global scope is a real JS runtime, unlike
    // bare wasm32, so both are safe to call directly here). Issue #184's
    // in-flight guard (`sync-run-guard.ts`) wraps this `run` sink rather
    // than living inside `sync-cadence.ts` itself: at most one `runSync` is
    // in flight at a time, any triggers arriving meanwhile coalesce into
    // exactly one follow-up run that starts the instant the in-flight one
    // resolves, and the guard's own release bound — reused from
    // `TASK_REQUEST_TIMEOUT_MS`, the same bound the underlying task queue
    // already uses to abandon a hung request — keeps a `runSync` whose
    // promise never settles from wedging the cadence forever. `mergePending`
    // is `mergePendingSyncTrigger` rather than the guard's bare last-wins
    // default: trigger identity survives the guard into this very callback
    // (`forceFullSweep`/`toCoreTrigger` above both read it), so a later,
    // lower-priority trigger arriving while e.g. an `"open"` is still
    // waiting in the guard's pending slot must not silently overwrite it.
    const cadence = createSyncCadence(
      createSyncRunGuard(
        (trigger) =>
          taskEnqueueReady.then((enqueue) =>
            enqueue({
              type: "runSync",
              nowMs: Date.now(),
              trigger: toCoreTrigger(trigger),
              forceFullSweep: trigger === "open",
              jitterUnit: Math.random(),
            }),
          ),
        { releaseMs: TASK_REQUEST_TIMEOUT_MS, mergePending: mergePendingSyncTrigger },
      ),
    );

    // The three-way routing (shared cadence / task queue / calendar queue)
    // and ADR-0007's "on app open" trigger both live in `dispatch.ts` as
    // pure logic a node test can execute — see that module's doc. In
    // particular the open sweep fires on the FIRST `initTaskApiKey` or
    // `pushTaskApiKey`, not here at activation: `onOpen` is documented as
    // "call once the core is ready and a task credential is known", and at
    // activation no view has had the chance to supply one yet, so firing it
    // here made every session's first cycle a spurious `no_credential`.
    // #707's diagnostics-journal doors — neither reaches a wasm host (see
    // `dispatch.ts`'s `DispatchDiagnostics`), so both are answered straight
    // from `diagnosticsJournal` here.
    const diagnosticsDispatch: DispatchDiagnostics = {
      exportJournal: async () => {
        const { events, droppedCount } = await diagnosticsJournal.export();
        registry.broadcast({ type: "diagnosticsExport", events, droppedCount });
      },
      clear: () => diagnosticsJournal.clear(),
    };

    const dispatch = createDispatch<PortLike>({
      cadence,
      visibility: wrapVisibilityForDiagnostics(visibility, diagnosticsJournal),
      taskEnqueueReady,
      calendarEnqueue,
      diagnostics: diagnosticsDispatch,
    });

    registry.activate(dispatch, core_api_version);

    // ADR-0007: "on reconnect" — the worker's own connectivity signal.
    // `online`/`offline` fire on whatever global scope implements
    // `WindowOrWorkerGlobalScope` per the HTML spec, a `SharedWorker`
    // included, so this needs no per-view forwarding.
    self.addEventListener("online", () => {
      cadence.onReconnect();
      diagnosticsJournal.recordNetworkChanged(Date.now(), true);
    });
    // #707: the offline counterpart. ADR-0007 has no trigger for going
    // offline (there is nothing to sweep), so this exists purely for the
    // journal — the shared cadence itself learns about connectivity loss
    // the ordinary way, its next `runSync` failing.
    self.addEventListener("offline", () => {
      diagnosticsJournal.recordNetworkChanged(Date.now(), false);
    });

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
