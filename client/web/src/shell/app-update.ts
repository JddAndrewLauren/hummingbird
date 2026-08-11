// The service-worker update signal: one fact, held outside React, in
// `store/store.ts`'s own listener-set idiom.
//
// Deliberately NOT a field on `CoreState`. That store pins `worker-client.ts`
// as its only writer and every field there is fed by a `protocol.ts` message
// (store.ts:1-4); a waiting service worker is a browser fact, not a core
// fact, and putting it there would need a fake protocol message to write it
// and a new default in `taskState()` to read it.
//
// `main.tsx` is the only writer — it owns the `virtual:pwa-register` import,
// so nothing else in `src/` (vitest included, which runs without the plugin
// and could not resolve that module at all) has to know the module exists.

type Listener = () => void;

export interface AppUpdateSnapshot {
  /** A new version is precached and waiting. */
  ready: boolean;
  /** Applies the waiting worker and reloads. Null until one is waiting. */
  apply: (() => void) | null;
}

const IDLE: AppUpdateSnapshot = Object.freeze({ ready: false, apply: null });

export interface AppUpdateSignal {
  subscribe(listener: Listener): () => void;
  getSnapshot(): AppUpdateSnapshot;
  /** A new worker is waiting; `apply` swaps to it and reloads the page. */
  markReady(apply: () => void): void;
}

export function createAppUpdateSignal(): AppUpdateSignal {
  // One frozen snapshot object, replaced only in `markReady`.
  // `useSyncExternalStore` compares by reference and re-renders forever if
  // `getSnapshot` mints a fresh object per call.
  let snapshot: AppUpdateSnapshot = IDLE;
  const listeners = new Set<Listener>();

  return {
    subscribe(listener: Listener): () => void {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    getSnapshot(): AppUpdateSnapshot {
      return snapshot;
    },
    markReady(apply: () => void): void {
      // A second call is a third deploy landing while the strip is already
      // up: keep the strip, but point the button at the freshest waiting
      // worker rather than the one it was minted with.
      snapshot = Object.freeze({ ready: true, apply });
      for (const listener of listeners) {
        listener();
      }
    },
  };
}

/** The one signal the app reads, written by `main.tsx`. */
export const appUpdateSignal = createAppUpdateSignal();
