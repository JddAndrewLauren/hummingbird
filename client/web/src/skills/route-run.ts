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
// **"Did not answer" and "answered with an error" are different things**,
// and only the first is about reachability. A tier that sent a response —
// a seam decline (#307), a 400, a 401 — has demonstrably been reached: it
// is never memoized dead, never fallen through, and never reworded; its
// terminal event reaches the caller exactly as the envelope wrote it,
// stamp included. Only a `fetch` that rejected or timed out connecting is
// evidence of unreachability, and only that memoizes a tier dead.
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
    const { terminal, answered } = yield* attempt(entry, buildBody(entry), deps);

    if (terminal.kind === "ok") {
      deps.memoStore.set(markReachable(deps.memoStore.get(), entry.id, deps.now()));
      yield terminal;
      return;
    }

    // **The tier answered — with an error, but it answered.** A 200 whose
    // terminal line says `ok:false` is the seam declining, which #307 made a
    // first-class outcome; a 400/413 is forwarded verbatim by the proxy the
    // same way. None of that is evidence of unreachability, so it must not
    // memoize the entry dead (a false decline would then block a legitimate
    // retry for the memo's whole TTL, with no request sent at all), must not
    // fall through to another tier, and must reach the caller EXACTLY as the
    // envelope wrote it — its own prose (`decline.ts`: prefixing it here
    // would be the same mistake in a different place) and its own stamp
    // (which `failure()` below would discard, so a declined run would lose
    // the badge naming who answered — and comparing tiers is the point).
    if (answered) {
      yield terminal;
      return;
    }

    // `NO_TOKEN` is not evidence the backend is unreachable either — `fetch`
    // was never even called — so it must not memoize the entry as dead.
    // Doing so would make the very next tap's plan all-skip (every entry
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

/** A run's last event: what `runSkill` narrows to once `started`,
 * `progress` and `unreadable` are accounted for. */
type TerminalEvent =
  | { kind: "ok"; result: unknown; backend: string | null; model: string | null }
  | { kind: "failed"; error: string; backend: string | null; model: string | null };

/** One tier's attempt: `runSkill`'s own `started` is swallowed (this
 * generator already emitted its own), its `progress` lines are forwarded,
 * and its terminal event is returned to the caller rather than yielded
 * directly — the caller decides whether that terminal is *this run's*
 * terminal or a fallback trigger.
 *
 * **`answered` is what makes that decision possible**, and it is observed
 * rather than inferred: it is true once `deps.fetch` has resolved — the
 * backend sent a response — and false when it rejected, timed out
 * connecting, or was never called at all. Reading it off the terminal event
 * instead is not possible and must not be attempted: a synthesized decline
 * and a stamp-less envelope line are indistinguishable by shape, and
 * matching on the prose is exactly what `decline.ts` forbids. */
async function* attempt(
  entry: BackendEntry,
  body: unknown,
  deps: RouteRunDeps,
): AsyncGenerator<SkillEvent, { terminal: TerminalEvent; answered: boolean }> {
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
  let answered = false;
  const connectBoundedFetch: typeof globalThis.fetch = async (input, init) => {
    const cancelController = new AbortController();
    const connectTimeoutSignal = AbortSignal.timeout(entry.connectTimeoutMs);
    const onConnectTimeout = () => cancelController.abort();
    connectTimeoutSignal.addEventListener("abort", onConnectTimeout, { once: true });
    const signal = init?.signal
      ? AbortSignal.any([init.signal, cancelController.signal])
      : cancelController.signal;
    try {
      const response = await deps.fetch(input, { ...init, signal });
      // Set on the resolve path only — a rejected `fetch` leaves it false,
      // and so does a `runSkill` that never calls `fetch` at all (no token).
      // A body that later tears mid-stream does not unset it: the backend
      // did answer, and the run is this tier's to finish or lose.
      answered = true;
      return response;
    } finally {
      connectTimeoutSignal.removeEventListener("abort", onConnectTimeout);
    }
  };

  let terminal: TerminalEvent | null = null;

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

  return {
    terminal: terminal ?? { kind: "failed", error: NO_TERMINAL_LINE, backend: null, model: null },
    answered,
  };
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
