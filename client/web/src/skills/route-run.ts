// The impure half of #274's routing: `route-plan.ts` decides *what* to try,
// this module is what actually tries it, on top of #273's `runSkill`.
//
// **Auto's fallback is discovered live, never guessed.** `planRoute` only
// ever skips a tier the memo already knows is dead; whether an *attempted*
// tier answers is found out by really calling it, with a short connect
// timeout (`entry.connectTimeoutMs`) so a sleeping home machine cannot make
// the tap hang. That timeout bounds only the connect phase — the wait for
// `fetch` itself to settle — and is disarmed the moment it does, whether
// `fetch` resolved or rejected; it never reaches the body reader, because
// the runner's own 20s "still running" heartbeat (`run-state.ts`) means a
// timeout armed for the whole run would abort every real streamed run. The
// same category as the unmount `AbortController` `useMicrotaskWiring.ts`
// already uses, not a second interval against ADR-0007.
//
// **Exactly one `started` and one terminal event reach the caller**, same
// contract as `runSkill` itself: a multi-tier fallback is invisible at that
// level, narrated instead as `progress` lines in between.
//
// **The memo is read and written only as the side effect of a real
// attempt.** Nothing here polls it on a schedule — there is no schedule.

import { AUTO_SELECTION, type BackendEntry } from "./backend-registry";
import { NO_TERMINAL_LINE, NO_TOKEN } from "./decline";
import { markDead, markReachable, type ReachabilityMemo } from "./reachability-memo";
import { planRoute, type RouteStep } from "./route-plan";
import { runSkill, type RunSkillDeps } from "./run-skill";
import type { SkillEvent } from "./run-state";

/** The reachability memo's resting place for the run's lifetime — a plain
 * get/set pair so the caller can hold it in a `useRef` (or a test can hold
 * it in a local variable) without this module knowing which. */
export interface ReachabilityMemoStore {
  get(): ReachabilityMemo;
  set(next: ReachabilityMemo): void;
}

export interface RouteRunDeps extends RunSkillDeps {
  registry: BackendEntry[];
  memoStore: ReachabilityMemoStore;
  /** The clock reading, passed in rather than read — `Date.now` in
   * production, a fixed number in a test. */
  now: () => number;
}

/**
 * `selection` is `AUTO_SELECTION` or a registered entry's id (`route-plan.ts`
 * degrades an unknown one to Auto). `buildBody` is called once per attempt,
 * with the entry about to be tried, so a multi-model registry can send each
 * tier's own `model` rather than baking one model into the request before
 * routing has picked anyone.
 */
export async function* runRouted(
  selection: string,
  buildBody: (entry: BackendEntry) => unknown,
  deps: RouteRunDeps,
): AsyncGenerator<SkillEvent> {
  yield { kind: "started" };

  const plan = planRoute(selection, deps.registry, deps.memoStore.get(), deps.now());

  if (plan.kind === "declined") {
    yield failure(declineForPinnedDead(plan.entry, plan.fallback));
    return;
  }

  const steps = plan.steps;
  for (let index = 0; index < steps.length; index++) {
    const step = steps[index] as RouteStep;

    if (step.kind === "skip") {
      yield { kind: "progress", message: `Skipping ${step.entry.label}: recently unreachable.` };
      continue;
    }

    const entry = step.entry;
    const terminal = yield* attempt(entry, buildBody(entry), deps);

    if (terminal.kind === "ok") {
      deps.memoStore.set(markReachable(deps.memoStore.get(), entry.id, deps.now()));
      yield terminal;
      return;
    }

    // `NO_TOKEN` is not evidence the backend is unreachable — `fetch` was
    // never even called — so it must not memoize the entry as dead. Doing
    // so would make the very next tap's plan all-skip (every entry
    // memoized dead), which is exactly the stuck-forever bug below.
    if (terminal.error !== NO_TOKEN) {
      deps.memoStore.set(markDead(deps.memoStore.get(), entry.id, deps.now()));
    }
    const moreToTry = steps.slice(index + 1).some((s) => s.kind === "attempt");
    if (!moreToTry) {
      // NO_TOKEN already names the actionable problem on its own — wrapping
      // it in "<entry> did not answer: …" would suggest the pinned backend
      // was unreachable, when no request was ever sent to it.
      yield failure(
        terminal.error === NO_TOKEN
          ? terminal.error
          : selection === AUTO_SELECTION
            ? terminal.error
            : declineForPinnedFailure(entry, terminal.error),
      );
      return;
    }
    yield { kind: "progress", message: `${entry.label} unreachable, trying the next backend.` };
  }

  // Every step was a `skip` — the plan was exhausted before a single
  // attempt was made (every registered entry memoized dead). The module
  // contract is exactly one terminal event per run; without this, the loop
  // above yields nothing but `progress` and returns, leaving the caller's
  // reducer at `phase: "running"` forever.
  yield failure(NO_REACHABLE_BACKEND);
}

/** One tier's attempt: `runSkill`'s own `started` is swallowed (this
 * generator already emitted its own), its `progress` lines are forwarded,
 * and its terminal event is returned to the caller rather than yielded
 * directly — the caller decides whether that terminal is *this run's*
 * terminal or a fallback trigger. */
async function* attempt(
  entry: BackendEntry,
  body: unknown,
  deps: RouteRunDeps,
): AsyncGenerator<SkillEvent, { kind: "ok"; result: unknown; backend: string | null; model: string | null } | { kind: "failed"; error: string; backend: string | null; model: string | null }> {
  // Wraps `deps.fetch` so the connect timeout bounds only the wait for
  // `fetch` to settle. A real `fetch`'s `AbortSignal` stays tied to the
  // whole request, headers AND body — so a signal that is still armed when
  // the body starts streaming would abort it just the same as one shared
  // outright, no better than the bug this replaces. Instead a fresh
  // `AbortSignal.timeout(entry.connectTimeoutMs)` only ever *arms*
  // `cancelSignal` (via `addEventListener`), and that arming listener is
  // detached — `removeEventListener`, not a cleared timer — the moment
  // `fetch` settles, resolved or rejected. From then on `cancelSignal` can
  // no longer fire from the connect timeout, so a backend that answered
  // within the connect window is never aborted mid-stream no matter how
  // long the runner's own 20s heartbeats make the stream run.
  // `AbortSignal.timeout` owns its own platform timer; nothing here starts
  // one (ADR-0007 — the 60s sync interval stays the only clock in this
  // lane).
  const connectBoundedFetch: typeof globalThis.fetch = async (input, init) => {
    const cancelController = new AbortController();
    const connectTimeoutSignal = AbortSignal.timeout(entry.connectTimeoutMs);
    const onConnectTimeout = () => cancelController.abort();
    connectTimeoutSignal.addEventListener("abort", onConnectTimeout, { once: true });
    const signal = init?.signal
      ? AbortSignal.any([init.signal, cancelController.signal])
      : cancelController.signal;
    try {
      return await deps.fetch(input, { ...init, signal });
    } finally {
      connectTimeoutSignal.removeEventListener("abort", onConnectTimeout);
    }
  };

  let terminal:
    | { kind: "ok"; result: unknown; backend: string | null; model: string | null }
    | { kind: "failed"; error: string; backend: string | null; model: string | null }
    | null = null;

  for await (const event of runSkill(entry, body, {
    fetch: connectBoundedFetch,
    readToken: deps.readToken,
    signal: deps.signal,
  })) {
    if (event.kind === "started") continue;
    if (event.kind === "progress") {
      yield event;
      continue;
    }
    // `runSkill`'s own contract: `unreadable` is dropped inside it and
    // never escapes as a terminal event — this guard exists only so the
    // type narrows to `ok | failed` without a cast; it is unreachable at
    // runtime.
    if (event.kind === "unreadable") continue;
    terminal = event;
  }

  return terminal ?? { kind: "failed", error: NO_TERMINAL_LINE, backend: null, model: null };
}

function failure(error: string): SkillEvent {
  return { kind: "failed", error, backend: null, model: null };
}

/** Every registered entry was memoized dead, so nothing was even attempted. */
const NO_REACHABLE_BACKEND = "No backend is currently reachable.";

function declineForPinnedDead(entry: BackendEntry, fallback: BackendEntry | null): string {
  const base = `${entry.label} is not answering right now.`;
  return fallback ? `${base} Try ${fallback.label} instead.` : base;
}

function declineForPinnedFailure(entry: BackendEntry, detail: string): string {
  return `${entry.label} did not answer: ${detail}`;
}
