// The single React <-> core surface (#69's Key interfaces). `useStore`
// (in useStore.ts) reads this store through `useSyncExternalStore`; there
// is no second state channel — the worker client (worker-client.ts) is the
// only writer.

import type {
  CalendarListEntryDTO,
  CurrentNextEventDTO,
  PollOutcomeName,
  RenderableCurrentNextKind,
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

export interface CoreState {
  status: CoreStatus;
  apiVersion: number | null;
  error: string | null;
  calendar: CalendarState;
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

const initialState: CoreState = {
  status: "loading",
  apiVersion: null,
  error: null,
  calendar: initialCalendarState,
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

  // A stable reference: this closure is created once, when the store is
  // created, and never reallocated per call. useSyncExternalStore relies on
  // that stability to avoid resubscribing every render.
  function subscribe(listener: Listener): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }

  return { getSnapshot, setState, setCalendarState, subscribe };
}

// The one module-level singleton the app renders from.
export const coreStore = createCoreStore();
