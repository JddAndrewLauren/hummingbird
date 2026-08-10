// The single React <-> core surface (#69's Key interfaces). `useStore`
// (in useStore.ts) reads this store through `useSyncExternalStore`; there
// is no second state channel — the worker client (worker-client.ts) is the
// only writer.

import type {
  BlockedFrontierEntryDTO,
  CalendarListEntryDTO,
  CurrentNextEventDTO,
  DeadLetterEntryDTO,
  PollOutcomeName,
  ProjectDTO,
  RenderableCurrentNextKind,
  StepDTO,
  TaskActionName,
  TaskItemDTO,
  TaskRunOutcomeKind,
} from "./protocol";

export type CoreStatus = "loading" | "ready" | "error";

/** Issue #73's calendar opt-in/tile state. A never-opted-in device starts
 * and stays at `connected: false` — the context tile renders nothing and
 * `/next-up-personal`-style ranking stays unconstrained (see the Agent
 * Brief's "opt-in is per-device" note). */
export interface CalendarState {
  connected: boolean;
  needsReconnect: boolean;
  selectedCalendarIds: string[];
  /** The picker's options, as last listed by the core. Distinct from
   * `selectedCalendarIds`, which is persisted and survives a listing that
   * never happened (offline start, held credential) — a selected id absent
   * from here is rendered as unavailable rather than silently dropped. */
  availableCalendars: CalendarListEntryDTO[];
  lastPollOutcome: PollOutcomeName | null;
  tileKind: RenderableCurrentNextKind;
  tileEvent: CurrentNextEventDTO | null;
  asOfMs: number | null;
}

/** The last `Core::run` cycle's outcome, as far as the shell needs it —
 * S9's sync-status affordance reads the whole thing; `useStore` consumers
 * before then only need `kind`. */
export interface TaskSyncOutcome {
  kind: TaskRunOutcomeKind;
  retryAfterMs: number | null;
  activeItemCount: number | null;
  wasFullSweep: boolean | null;
  deadLettered: number | null;
}

/** The result of the most recent `capture` request this view issued,
 * matched back by `seed` — the worker->view direction is a broadcast to
 * every connected port (protocol.ts), so a view only knows a `captureResult`
 * is "its own" by recognising the seed it itself minted. */
export interface TaskCaptureResult {
  seed: string;
  kind: "ok" | "failed" | "busy";
  id: string | null;
  error: string | null;
}

/** The result of the most recent `act` request this view issued (S11/#109),
 * matched back by `seed` — same broadcast-recognition contract as
 * [`TaskCaptureResult`]. */
export interface TaskActResult {
  seed: string;
  itemId: string;
  action: TaskActionName;
  kind: "ok" | "not_found" | "failed" | "busy";
  error: string | null;
}

/** The result of the most recent `triage` request this view issued
 * (S13/#111), matched back by `seed` — same broadcast-recognition contract
 * as [`TaskActResult`]. */
export interface TaskTriageResult {
  seed: string;
  itemId: string;
  kind: "ok" | "not_found" | "failed" | "busy";
  error: string | null;
}

/** Issue #105/S7's task read-model slice: the owned-schema counterpart to
 * [`CalendarState`], fed by `worker/task-worker.ts`'s broadcasts. */
export interface TaskState {
  frontier: TaskItemDTO[];
  triageInbox: TaskItemDTO[];
  /** Relation-blocked items with the reason visible — S10's frontier list
   * (issue #108). Populated by `getBlocked`, same "last full answer wins"
   * contract as `frontier`. */
  blocked: BlockedFrontierEntryDTO[];
  /** Item detail's checklist (issue #96, S10), keyed by item id — only ever
   * grows entries a view actually asked about via `getSteps`, the same
   * "only what was asked for" shape `pending` already uses. */
  stepsByItem: Record<string, StepDTO[]>;
  /** Every live project — resolves the frontier's "grouped by project"
   * display to real names (issue #108, PR #200 review). */
  projects: ProjectDTO[];
  /** Keyed by item id — only ever grows entries this view actually asked
   * about via `isPending`, never a full mirror of every pending item. */
  pending: Record<string, boolean>;
  lastCapture: TaskCaptureResult | null;
  /** The result of the most recent `act` request this view issued (S11/
   * #109) — `null` until the first one resolves. */
  lastAct: TaskActResult | null;
  /** The result of the most recent `triage` request this view issued
   * (S13/#111) — `null` until the first one resolves. */
  lastTriage: TaskTriageResult | null;
  lastSyncOutcome: TaskSyncOutcome | null;
  /** When the last `Core::run` cycle actually happened (any trigger, any
   * outcome) — S9's "last sweep" readout. Copied by `worker-client.ts`
   * straight from the `syncOutcome` message's own `atMs`, which
   * `task-worker.ts` stamps from the cycle's clock (`atMs: request.nowMs`,
   * the same value `core.worker.ts` passes to `host.runSync`) — NOT sampled
   * from this view's clock at broadcast-processing time. That distinction is
   * what issue #195's cache-and-replay depends on: a `syncOutcome` replayed
   * to a late-connecting port must read at its true age, and a view stamping
   * its own clock on a replay would launder a stale outcome into a fresh
   * success. */
  lastSyncAtMs: number | null;
  /** Monotonic count of `syncOutcome` broadcasts this view has processed —
   * incremented by `worker-client.ts` on EVERY cycle, whatever its `kind`.
   * Until issue #191, this was what `useSyncWiring.ts` keyed its per-cycle
   * queue-depth / dead-letter refresh effect on (round-2 review of PR #181):
   * keying on the outcome's `kind` froze the refresh after the first cycle
   * (steady state is `"completed"` forever, and a dead letter arrives
   * INSIDE a completed outcome — `deadLettered` is a separate field), and
   * keying on the outcome object's identity works today but is one
   * memoisation away from the same freeze. A counter changes on every cycle
   * by construction.
   *
   * Issue #191 moved the per-cycle queue-depth/dead-letter refresh itself
   * into the worker (an unsolicited push at the cycle tail, protocol.ts's
   * `queueDepth`/`deadLetters` docs), which removed `useSyncWiring.ts`'s
   * only consumer of this field — it is deliberately kept anyway, still
   * bumped every cycle and still asserted by `worker-client.test.ts`,
   * because a per-cycle "a cycle just completed" signal is generically
   * useful even with no current reader; see the PR that closed #191 for the
   * explicit call-out this is not a silent, unexplained deletion. */
  syncOutcomeSeq: number;
  /** The outbound queue's current depth — S9's sync-status "queued"
   * figure. `null` until the first answer arrives: an explicit
   * `getQueueDepth` reply, the worker's own unsolicited per-cycle push
   * (issue #191), or — issue #195 — the replay a newly connecting port gets
   * of whatever `queueDepth` last broadcast this session
   * (`worker/ports.ts`'s `PortRegistry`). `null` still means exactly one
   * thing regardless of origin: nothing has answered yet, because the core
   * is still loading or has never completed a cycle. */
  queueDepth: number | null;
  /** The whole dead-letter journal, as of the last `deadLetters` broadcast
   * — S9's "1 edit didn't apply" affordance. Kept fresh by the worker's own
   * unsolicited push at the tail of every cycle as well as by an explicit
   * `getDeadLetters` request (issue #191). Never pruned
   * client-side either (mirrors the core's own journal, `sync::queue`'s own
   * doc), so this only ever grows until a re-apply flow exists to shrink
   * it. */
  deadLetters: DeadLetterEntryDTO[];
  /** Set once a `taskEvents` broadcast carries a `credential_needed` event;
   * mirrors `CalendarState.needsReconnect`'s own contract. */
  needsReconnect: boolean;
  /** The task host failed to construct in the shared worker, so every task
   * request this core ever receives is dropped (`taskHostUnavailable`,
   * protocol.ts). `null` in the normal case. Not recoverable in-session —
   * the core does not retry construction — so the one honest thing a view
   * can do with it is say so and tell the user to reload. */
  hostError: string | null;
}

export interface CoreState {
  status: CoreStatus;
  apiVersion: number | null;
  error: string | null;
  calendar: CalendarState;
  task: TaskState;
}

type Listener = () => void;

const initialCalendarState: CalendarState = {
  connected: false,
  needsReconnect: false,
  selectedCalendarIds: [],
  availableCalendars: [],
  lastPollOutcome: null,
  tileKind: "no_snapshot",
  tileEvent: null,
  asOfMs: null,
};

const initialTaskState: TaskState = {
  frontier: [],
  triageInbox: [],
  blocked: [],
  stepsByItem: {},
  projects: [],
  pending: {},
  lastCapture: null,
  lastAct: null,
  lastTriage: null,
  lastSyncOutcome: null,
  lastSyncAtMs: null,
  syncOutcomeSeq: 0,
  queueDepth: null,
  deadLetters: [],
  needsReconnect: false,
  hostError: null,
};

const initialState: CoreState = {
  status: "loading",
  apiVersion: null,
  error: null,
  calendar: initialCalendarState,
  task: initialTaskState,
};

export function createCoreStore() {
  let state: CoreState = initialState;
  const listeners = new Set<Listener>();

  function getSnapshot(): CoreState {
    return state;
  }

  function setState(patch: Partial<CoreState>): void {
    state = { ...state, ...patch };
    for (const listener of listeners) {
      listener();
    }
  }

  // A narrower merge for the calendar slice, so callers don't have to
  // spread `state.calendar` themselves at every call site.
  function setCalendarState(patch: Partial<CalendarState>): void {
    setState({ calendar: { ...state.calendar, ...patch } });
  }

  // Same idea for the task slice (#105/S7).
  function setTaskState(patch: Partial<TaskState>): void {
    setState({ task: { ...state.task, ...patch } });
  }

  // `pending` is itself a map, so a plain `setTaskState` merge would
  // require every caller to spread `state.task.pending` — this is that one
  // extra level, kept here rather than duplicated at each call site.
  function setTaskPending(itemId: string, pending: boolean): void {
    setTaskState({ pending: { ...state.task.pending, [itemId]: pending } });
  }

  // Same idea for `stepsByItem` (item detail, issue #96/S10).
  function setTaskSteps(itemId: string, steps: StepDTO[]): void {
    setTaskState({ stepsByItem: { ...state.task.stepsByItem, [itemId]: steps } });
  }

  // A stable reference: this closure is created once, when the store is
  // created, and never reallocated per call. useSyncExternalStore relies on
  // that stability to avoid resubscribing every render.
  function subscribe(listener: Listener): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }

  return {
    getSnapshot,
    setState,
    setCalendarState,
    setTaskState,
    setTaskPending,
    setTaskSteps,
    subscribe,
  };
}

// The one module-level singleton the app renders from.
export const coreStore = createCoreStore();
