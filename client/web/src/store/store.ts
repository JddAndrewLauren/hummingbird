// The single React <-> core surface (#69's Key interfaces). `useStore`
// (in useStore.ts) reads this store through `useSyncExternalStore`; there
// is no second state channel — the worker client (worker-client.ts) is the
// only writer.

import type {
  CalendarListEntryDTO,
  CurrentNextEventDTO,
  DeadLetterEntryDTO,
  PollOutcomeName,
  RenderableCurrentNextKind,
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

/** Issue #105/S7's task read-model slice: the owned-schema counterpart to
 * [`CalendarState`], fed by `worker/task-worker.ts`'s broadcasts. */
export interface TaskState {
  frontier: TaskItemDTO[];
  triageInbox: TaskItemDTO[];
  /** Keyed by item id — only ever grows entries this view actually asked
   * about via `isPending`, never a full mirror of every pending item. */
  pending: Record<string, boolean>;
  lastCapture: TaskCaptureResult | null;
  lastSyncOutcome: TaskSyncOutcome | null;
  /** When this view learned the last `Core::run` cycle happened (any
   * trigger, any outcome) — S9's "last sweep" readout. Sampled by
   * `worker-client.ts` at the moment it processes the `syncOutcome`
   * broadcast, using this view's own clock — the wire message carries no
   * `nowMs` of its own (the cycle's real invocation time, sampled inside
   * `core.worker.ts`, is never round-tripped back), and every view's
   * broadcast-processing clock reads within a few milliseconds of every
   * other's regardless, so this is an accurate-enough proxy for "just now"
   * without needing to widen the protocol. */
  lastSyncAtMs: number | null;
  /** Monotonic count of `syncOutcome` broadcasts this view has processed —
   * incremented by `worker-client.ts` on EVERY cycle, whatever its `kind`.
   * This is what `useSyncWiring.ts` keys its per-cycle queue-depth /
   * dead-letter refresh on (round-2 review of PR #181): keying on the
   * outcome's `kind` froze the refresh after the first cycle (steady state
   * is `"completed"` forever, and a dead letter arrives INSIDE a completed
   * outcome — `deadLettered` is a separate field), and keying on the outcome
   * object's identity works today but is one memoisation away from the same
   * freeze. A counter changes on every cycle by construction. */
  syncOutcomeSeq: number;
  /** The outbound queue's current depth — S9's sync-status "queued"
   * figure. `null` until the first answer arrives (this view has not asked,
   * or the core is still loading). */
  queueDepth: number | null;
  /** The whole dead-letter journal, as of the last `getDeadLetters`
   * request — S9's "1 edit didn't apply" affordance. Never pruned
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
  pending: {},
  lastCapture: null,
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

  // A stable reference: this closure is created once, when the store is
  // created, and never reallocated per call. useSyncExternalStore relies on
  // that stability to avoid resubscribing every render.
  function subscribe(listener: Listener): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }

  return { getSnapshot, setState, setCalendarState, setTaskState, setTaskPending, subscribe };
}

// The one module-level singleton the app renders from.
export const coreStore = createCoreStore();
