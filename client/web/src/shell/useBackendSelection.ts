import { useCallback, useState } from "react";
import { BACKEND_REGISTRY } from "../skills/backend-registry";
import { readBackendSelection, writeBackendSelection } from "../skills/backend-selection";

// The picker's own state (#274) — the same shape `useTheme.ts` gives its
// preference: read once at mount from device-local storage, written back on
// every change, and never touched by anything in `store/` or `worker/`. This
// is the one place the app decides which backend a microtask run should
// prefer; `useMicrotaskWiring.ts` only ever reads `selection` from here.

export interface BackendSelectionControl {
  selection: string;
  setSelection: (selection: string) => void;
}

/** The default is resolved lazily, not as a default parameter value: a bare
 * `localStorage` in the signature is evaluated wherever this module is
 * imported, which throws outright in a context that has none (SSR, a worker,
 * a test environment without it). `App.tsx` guards the rail preference the
 * same way. `backend-selection.ts` already treats `undefined` as "no stored
 * preference" and answers Auto, and writing to it is a no-op. */
function deviceStorage(): Storage | undefined {
  return typeof localStorage === "undefined" ? undefined : localStorage;
}

export function useBackendSelection(storage: Storage | undefined = deviceStorage()): BackendSelectionControl {
  const [selection, setSelectionState] = useState<string>(() =>
    readBackendSelection(storage, BACKEND_REGISTRY),
  );

  const setSelection = useCallback(
    (next: string) => {
      writeBackendSelection(storage, next);
      setSelectionState(next);
    },
    [storage],
  );

  return { selection, setSelection };
}
