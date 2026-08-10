import type { CalendarWorkerRequest, SyncCadenceRequest, TaskWorkerRequest } from "../store/protocol";
import { isSyncCadenceRequest, isTaskWorkerRequest } from "./request-router";

// `core.worker.ts`'s routing decision, as pure logic — the three-way split
// between the shared cadence, the task binding's queue and the calendar
// binding's queue, plus the one ordering rule that hangs off it (ADR-0007's
// "app open" trigger, see `openTrigger` below).
//
// It lives here rather than inline in `core.worker.ts` for the reason that
// file's own module doc gives for `request-router.ts`, `ports.ts` and
// `sync-cadence.ts`: `core.worker.ts` statically imports the wasm module's
// wrapper and is a `SharedWorkerGlobalScope` script, so nothing in it can be
// imported by a vitest (node) test — decision logic left in there is
// untestable by construction. This is the one place a `setViewVisibility`
// could be mis-routed into a wasm queue, so it is exactly the kind of
// decision that belongs in a `.ts` module a test can execute.

type AnyWorkerRequest = CalendarWorkerRequest | TaskWorkerRequest | SyncCadenceRequest;

/** The slice of `sync-cadence.ts`'s controller this routing needs. */
export interface DispatchCadence {
  onOpen: () => void;
  onFocus: () => void;
}

/** The slice of `visibility-tracker.ts` this routing needs, generic in the
 * port type so a test can key on plain strings. */
export interface DispatchVisibility<Port> {
  setHidden: (port: Port, hidden: boolean) => void;
}

export interface DispatchDeps<Port> {
  cadence: DispatchCadence;
  visibility: DispatchVisibility<Port>;
  /** The task binding's queue, still being constructed when the first
   * requests can arrive (`core.worker.ts`'s `createTaskEnqueueDeferred`) —
   * `.then` callbacks fire in attachment order, so task requests stay
   * ordered whether they arrive before or after it resolves. */
  taskEnqueueReady: Promise<(request: TaskWorkerRequest) => Promise<void>>;
  calendarEnqueue: (request: CalendarWorkerRequest) => Promise<void>;
}

export type Dispatch<Port> = (request: AnyWorkerRequest, port: Port) => Promise<void>;

/** Builds the dispatcher `PortRegistry.activate` wires every port's
 * `onmessage` to. One instance per core, not per port — `port` is a
 * parameter precisely because the shared cadence needs to know WHICH view
 * reported its visibility. */
export function createDispatch<Port>(deps: DispatchDeps<Port>): Dispatch<Port> {
  const open = openTrigger(deps.cadence);
  return (request, port) => {
    if (isSyncCadenceRequest(request)) {
      if (request.type === "setViewVisibility") {
        deps.visibility.setHidden(port, request.hidden);
      } else {
        deps.cadence.onFocus();
      }
      return Promise.resolve();
    }
    if (!isTaskWorkerRequest(request)) {
      return deps.calendarEnqueue(request);
    }
    return deps.taskEnqueueReady.then(async (enqueue) => {
      await enqueue(request);
      if (request.type === "pushTaskApiKey") {
        open.credentialPushed();
      }
    });
  };
}

/** ADR-0007's "sweep on app open / core start", fired exactly once per core
 * — but NOT at core activation, which is what PR #181 shipped and a
 * post-batch review caught: `sync-cadence.ts` documents `onOpen` as "call
 * once the core is ready **and a task credential is known**", and at
 * activation no view has had a chance to `pushTaskApiKey` yet
 * (`useTaskTokenWiring.ts` only loads the stored token once its own view
 * observes `ready`). Every session's first cycle therefore returned
 * `no_credential` and Settings showed "Held — device token needed" until
 * the next 60s tick, on a device with a perfectly good stored token.
 *
 * Hanging it off the first `pushTaskApiKey` instead honours the contract
 * literally: the open sweep fires the moment a credential is actually
 * known, which is also the moment a first-run token entry completes (so
 * entering a token starts syncing immediately rather than up to a minute
 * later). A never-entered device pushes no key and gets no open sweep — the
 * 60s timer surfaces its `no_credential` within the minute, and its
 * Settings screen already knows it has no token from its own local state,
 * so nothing waits on the badge. */
function openTrigger(cadence: DispatchCadence): { credentialPushed: () => void } {
  let fired = false;
  return {
    credentialPushed: () => {
      if (fired) {
        return;
      }
      fired = true;
      cadence.onOpen();
    },
  };
}
