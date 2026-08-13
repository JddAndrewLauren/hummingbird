// The impure half of #274's routing: `route-plan.ts` decides *what* to try,
// this module is what actually tries it, on top of #273's `runSkill`.
//
// **Auto's fallback is discovered live, never guessed.** `planRoute` only
// ever skips a tier the memo already knows is dead; whether an *attempted*
// tier answers is found out by really calling it, with a short connect
// timeout (`entry.connectTimeoutMs`) so a sleeping home machine cannot make
// the tap hang. That timeout is `AbortSignal.timeout`, scoped to one
// attempt's own lifetime and cleared the moment the attempt settles — the
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
import { NO_TERMINAL_LINE } from "./decline";
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

    deps.memoStore.set(markDead(deps.memoStore.get(), entry.id, deps.now()));
    const moreToTry = steps.slice(index + 1).some((s) => s.kind === "attempt");
    if (!moreToTry) {
      yield failure(
        selection === AUTO_SELECTION ? terminal.error : declineForPinnedFailure(entry, terminal.error),
      );
      return;
    }
    yield { kind: "progress", message: `${entry.label} unreachable, trying the next backend.` };
  }
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
  const timeoutSignal = AbortSignal.timeout(entry.connectTimeoutMs);
  const signal = deps.signal ? AbortSignal.any([deps.signal, timeoutSignal]) : timeoutSignal;

  let terminal:
    | { kind: "ok"; result: unknown; backend: string | null; model: string | null }
    | { kind: "failed"; error: string; backend: string | null; model: string | null }
    | null = null;

  for await (const event of runSkill(entry, body, { fetch: deps.fetch, readToken: deps.readToken, signal })) {
    if (event.kind === "started") continue;
    if (event.kind === "progress") {
      yield event;
      continue;
    }
    // `runSkill`'s own contract: `unreadable` is dropped inside it and never
    // escapes as a terminal event — this guard exists only so the type
    // narrows to `ok | failed` without a cast; it is unreachable at runtime.
    if (event.kind === "unreadable") continue;
    terminal = event;
  }

  return terminal ?? { kind: "failed", error: NO_TERMINAL_LINE, backend: null, model: null };
}

function failure(error: string): SkillEvent {
  return { kind: "failed", error, backend: null, model: null };
}

function declineForPinnedDead(entry: BackendEntry, fallback: BackendEntry | null): string {
  const base = `${entry.label} is not answering right now.`;
  return fallback ? `${base} Try ${fallback.label} instead.` : base;
}

function declineForPinnedFailure(entry: BackendEntry, detail: string): string {
  return `${entry.label} did not answer: ${detail}`;
}
