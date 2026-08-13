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

export function useBackendSelection(storage: Storage = localStorage): BackendSelectionControl {
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
