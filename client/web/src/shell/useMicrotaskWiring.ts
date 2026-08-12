import { useCallback, useEffect, useRef, useState } from "react";
import { microtaskRunBody } from "../skills/microtask-args";
import { runSkill, type RunSkillDeps, type SkillBackend } from "../skills/run-skill";
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

/** #274 replaces this one expression with a picker. */
const CLOUD_RUNNER: SkillBackend = { id: "cloud", endpoint: "/api/skills/run" };

export interface MicrotaskRunRequest {
  itemId: string;
  replace?: boolean;
  grain?: number;
  model?: string;
}

export interface MicrotaskWiring {
  /** The open item's run, or `IDLE`. Keyed by item so a run started on one
   * item never renders under another. */
  run: SkillRunState;
  onRun: (request: MicrotaskRunRequest) => void;
}

export interface MicrotaskWiringDeps {
  fetch?: typeof globalThis.fetch;
  tokenStore?: TaskTokenStoreLike;
}

export function useMicrotaskWiring(
  worker: WorkerLike,
  selectedItemId: string | null,
  deps: MicrotaskWiringDeps = {},
): MicrotaskWiring {
  const [runs, setRuns] = useState<Record<string, SkillRunState>>({});
  const abortRef = useRef<AbortController | null>(null);
  // The in-flight lock, in a ref rather than read off `runs`: two clicks in
  // one React batch would both see the same pre-render `runs` and both
  // start. The reducer's own duplicate-tap rule and the button's `disabled`
  // are the other two expressions of the same thing; this is the one that
  // holds between a click and the render it causes.
  const inFlight = useRef<Set<string>>(new Set());

  // Abort whatever is in flight when the view goes away. Not a timeout and
  // not a retry: the run simply stops being narrated, and the runner's own
  // write still lands.
  useEffect(() => () => abortRef.current?.abort(), []);

  const onRun = useCallback(
    (request: MicrotaskRunRequest) => {
      const itemId = request.itemId;
      if (inFlight.current.has(itemId)) return;
      inFlight.current.add(itemId);

      const controller = new AbortController();
      abortRef.current?.abort();
      abortRef.current = controller;

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
          for await (const event of runSkill(CLOUD_RUNNER, microtaskRunBody(request), runDeps)) {
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
    [worker, deps.fetch, deps.tokenStore],
  );

  return { run: (selectedItemId && runs[selectedItemId]) || IDLE, onRun };
}

let cachedStore: TaskTokenStoreLike | null = null;
function store(injected?: TaskTokenStoreLike): TaskTokenStoreLike {
  if (injected) return injected;
  cachedStore ??= createIndexedDbTaskTokenStore();
  return cachedStore;
}
