// The single React <-> core surface (#69's Key interfaces). `useStore`
// (in useStore.ts) reads this store through `useSyncExternalStore`; there
// is no second state channel — the worker client (worker-client.ts) is the
// only writer.

export type CoreStatus = "loading" | "ready" | "error";

export interface CoreState {
  status: CoreStatus;
  apiVersion: number | null;
  error: string | null;
}

type Listener = () => void;

const initialState: CoreState = {
  status: "loading",
  apiVersion: null,
  error: null,
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

  // A stable reference: this closure is created once, when the store is
  // created, and never reallocated per call. useSyncExternalStore relies on
  // that stability to avoid resubscribing every render.
  function subscribe(listener: Listener): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }

  return { getSnapshot, setState, subscribe };
}

// The one module-level singleton the app renders from.
export const coreStore = createCoreStore();
