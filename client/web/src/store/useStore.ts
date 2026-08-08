import { useSyncExternalStore } from "react";
import { type CoreState, coreStore } from "./store";

// The only React <-> core surface (#69). `coreStore.subscribe` is a stable,
// module-level reference (see store.ts), so React never resubscribes on
// re-render.
export function useStore<T>(selector: (state: CoreState) => T): T {
  return useSyncExternalStore(coreStore.subscribe, () =>
    selector(coreStore.getSnapshot()),
  );
}
