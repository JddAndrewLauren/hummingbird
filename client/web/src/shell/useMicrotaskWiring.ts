import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AUTO_SELECTION, BACKEND_REGISTRY, fallbackEntry, type BackendEntry } from "../skills/backend-registry";
import { NO_TOKEN } from "../skills/decline";
import { microtaskRunBody } from "../skills/microtask-args";
import { EMPTY_MEMO, type ReachabilityMemo } from "../skills/reachability-memo";
import { runRouted, type ReachabilityMemoStore } from "../skills/route-run";
import type { RunSkillDeps } from "../skills/run-skill";
import { IDLE, reduceRun, type SkillRunState } from "../skills/run-state";
import { createIndexedDbTaskTokenStore, type TaskTokenStoreLike } from "../task/token-store";
import { triggerSyncManual, type WorkerLike } from "../store/worker-client";

// The microtask button's wiring (#273): tap, stream, then ask for one sync
// cycle so the steps arrive through the normal read path.
//
// **This runs on the main thread, deliberately — not in the SharedWorker.**
// ADR-0010 makes that worker shared across every tab, so a run hosted there
// would need progress fan-out to other tabs, a per-run registry keyed by
// the initiating port, ownership arbitration, and new `protocol.ts` variants
// with branches in `dispatch.ts` and `request-router.ts` — all to serve a
// rare, foreground, single-view gesture. Worse: a worker-hosted run
// survives the tab that asked for it, and its terminal envelope then has
// nowhere to go but a slot held for a future view to read. **That is a
// queue in all but name, and #269 bans it.**
//
// On the main thread the lane cannot reach the sync queue at all — an
// invariant checkable from the import graph rather than promised in review,
// and `skills/no-queue.test.ts` checks it. It needs no abandon timer either
// (an `AbortController` on unmount is not a clock, so ADR-0007's single
// interval stays the only one), and the device token is already
// main-thread-readable through `task/token-store.ts`.
//
// **Closing the tab mid-run loses the narration, never the work.** The
// runner writes to the authority, so the steps land and appear at the next
// pull from any device.
//
// **Cross-tab duplicate taps are not prevented, and that is correct.** #307
// point 6 makes the *seam* the safety property: `apply` re-asserts the
// guard and declines any run that sees a live undone step `prepare` did
// not. The in-flight lock below is an affordance — it keeps one person from
// double-tapping one button — not the thing that protects a checklist.

export interface MicrotaskRunRequest {
  itemId: string;
  replace?: boolean;
  grain?: number;
}

export interface MicrotaskWiring {
  /** The open item's run, or `IDLE`. Keyed by item so a run started on one
   * item never renders under another. */
  run: SkillRunState;
  onRun: (request: MicrotaskRunRequest) => void;
  /** #274's one-tap offer beside a decline **that evidences an unreachable
   * backend**. `null` whenever there is nothing to offer: the run is not
   * declined, the current selection is Auto (nothing to fall back FROM —
   * Auto already tried everything it could), the registry has no other
   * entry, or the decline is not about reachability at all — no token was
   * sent, or a backend *answered* (`run.answered`), in which case switching
   * tiers changes nothing about what it said. Still never an *automatic*
   * fallback — nothing here fires without the user pressing the button.
   *
   * **Switching and re-running are one call on purpose.** They cannot be
   * two: `onRun` is a `useCallback` closed over the render's `selection`,
   * so a caller doing `onSwitch(); onRun(request)` in one tick re-attempts
   * the very pin that just declined — and, freshly memoized dead, gets an
   * instant decline instead of the retry it asked for. Handing back one
   * function makes that miswiring unexpressible. */
  declinedFallback: { label: string; onSwitchAndRun: (request: MicrotaskRunRequest) => void } | null;
}

export interface MicrotaskWiringDeps {
  fetch?: typeof globalThis.fetch;
  tokenStore?: TaskTokenStoreLike;
  registry?: BackendEntry[];
  /** Device-local, from `useBackendSelection.ts`'s `setSelection` — how the
   * fallback button actually switches. Omitted (e.g. in a test with no
   * picker wired up) simply means the fallback is never offered. */
  onSelectBackend?: (backendId: string) => void;
}

export function useMicrotaskWiring(
  worker: WorkerLike,
  selectedItemId: string | null,
  /** #274's picker choice — `AUTO_SELECTION` or a registered entry's id.
   * Device-local (`useBackendSelection.ts`), never a synced fact, and this
   * hook only ever reads it — the selection itself is owned one level up. */
  selection: string = AUTO_SELECTION,
  deps: MicrotaskWiringDeps = {},
): MicrotaskWiring {
  const [runs, setRuns] = useState<Record<string, SkillRunState>>({});
  const registry = deps.registry ?? BACKEND_REGISTRY;
  // The reachability memo (#274): brief, in-memory, and scoped to this
  // hook's own lifetime — never persisted, never read on a schedule. It is
  // written only as the side effect of a real attempt inside `runRouted`.
  const memo = useRef<ReachabilityMemo>(EMPTY_MEMO);
  const memoStore: ReachabilityMemoStore = useRef<ReachabilityMemoStore>({
    get: () => memo.current,
    set: (next) => {
      memo.current = next;
    },
  }).current;
  // **One controller per item**, not one for the hook. A single shared
  // controller would make starting a run on item B abort item A's — and A
  // would then be left reading "The run ended without an answer.", which is
  // a lie: the runner writes to the authority, so A's checklist is very
  // likely landing while the app says it did not. Runs on different items
  // are independent, and the map is what says so.
  //
  // This doubles as the in-flight lock, which has to be a ref rather than a
  // read off `runs`: two clicks in one React batch would both see the same
  // pre-render state and both start. The reducer's own duplicate-tap rule
  // and the button's `disabled` are the other two expressions of the same
  // thing; this is the one that holds between a click and the render it
  // causes.
  const inFlight = useRef<Map<string, AbortController>>(new Map());

  // Abort every live run when the view goes away. Not a timeout and not a
  // retry: the runs simply stop being narrated, and the runner's own writes
  // still land.
  // The map is read inside the effect body rather than during render: this
  // repo's lint config forbids touching a ref while rendering
  // (`react-hooks/refs`), and an effect is not a render.
  useEffect(() => {
    const running = inFlight.current;
    return () => {
      for (const controller of running.values()) controller.abort();
      running.clear();
    };
  }, []);

  // Takes the selection to route with as an argument rather than reading
  // the closed-over one, so the fallback button can run as the backend it
  // just switched to — in the same tick, before any re-render has handed
  // this hook the new selection.
  const startRun = useCallback(
    (request: MicrotaskRunRequest, runAs: string) => {
      const itemId = request.itemId;
      if (inFlight.current.has(itemId)) return;
      const controller = new AbortController();
      inFlight.current.set(itemId, controller);

      const runDeps: RunSkillDeps = {
        fetch: deps.fetch ?? globalThis.fetch.bind(globalThis),
        readToken: async () => (await store(deps.tokenStore).read())?.token ?? null,
        signal: controller.signal,
      };

      void (async () => {
        // The run's state is folded HERE, not inside the `setRuns` updater:
        // React does not promise to run an updater before the next
        // statement, so reading the folded value back out of one would be a
        // race — the terminal phase would still be `idle` by the time the
        // loop ended, and the sync cycle below would never fire.
        let state: SkillRunState = IDLE;
        try {
          for await (const event of runRouted(
            runAs,
            (entry) => microtaskRunBody({ ...request, model: entry.model ?? undefined }),
            { ...runDeps, registry, memoStore, now: Date.now },
          )) {
            state = reduceRun(state, event);
            const next = state;
            setRuns((current) => ({ ...current, [itemId]: next }));
          }
        } finally {
          inFlight.current.delete(itemId);
        }
        // **Only on a terminal ok.** #307 point 4 puts the drop after
        // validation, so a decline leaves the plan intact and there is
        // nothing new to pull. Never on progress, and never on a schedule:
        // this asks the shared cadence for one cycle, which
        // `sync-run-guard.ts` coalesces into the pending slot if one is
        // already mid-flight, so it cannot be lost.
        //
        // There is a window of up to one cycle between "the envelope said
        // ok" and "the checklist is on screen", and it is accepted rather
        // than papered over: a spinner that waited for the steps to change
        // would be a second reader for steps with a private notion of
        // freshness, which is exactly what #273 forbids. The completed
        // block says what happened; the checklist fills when it fills.
        if (state.phase === "done") triggerSyncManual(worker);
      })();
    },
    [worker, deps.fetch, deps.tokenStore, registry, memoStore],
  );

  const onRun = useCallback(
    (request: MicrotaskRunRequest) => startRun(request, selection),
    [startRun, selection],
  );

  const run = (selectedItemId && runs[selectedItemId]) || IDLE;

  const declinedFallback = useMemo(() => {
    if (run.phase !== "declined" || selection === AUTO_SELECTION) return null;
    // NO_TOKEN means no request was ever made — it says nothing about
    // whether the pinned backend itself is reachable, so offering "switch
    // to X" here would suggest a fix for a problem that was never observed.
    if (run.reason === NO_TOKEN) return null;
    // A decline a backend ANSWERED says nothing about reachability, so
    // nothing may offer switching away from a backend on the strength of it:
    // "That item already has live steps." is true of every tier, because the
    // guard is the seam's (#307), not any backend's. Same principle as the
    // NO_TOKEN case above, for the other way a decline arrives without
    // evidencing unreachability.
    //
    // Read off the routed fact, never off the stamp: a 401 and a 403 are
    // empty-bodied by design (ADR-0018) and so arrive unstamped, yet the
    // backend plainly answered — and switching tiers cannot help, since
    // every tier is reached through the same proxy with the same device
    // token.
    if (run.answered) return null;
    const fallback = fallbackEntry(registry, selection);
    const onSelectBackend = deps.onSelectBackend;
    if (fallback === null || onSelectBackend === undefined) return null;
    return {
      label: fallback.label,
      onSwitchAndRun: (request: MicrotaskRunRequest) => {
        // The preference change is device-local and takes effect on the
        // next render; the run cannot wait for it, so it is told which
        // backend to use outright.
        onSelectBackend(fallback.id);
        startRun(request, fallback.id);
      },
    };
  }, [run, selection, registry, deps.onSelectBackend, startRun]);

  return { run, onRun, declinedFallback };
}

let cachedStore: TaskTokenStoreLike | null = null;
function store(injected?: TaskTokenStoreLike): TaskTokenStoreLike {
  if (injected) return injected;
  cachedStore ??= createIndexedDbTaskTokenStore();
  return cachedStore;
}
