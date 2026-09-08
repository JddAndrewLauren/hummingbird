import { useCallback, useState } from "react";
import { readDropboxLocalRoot, writeDropboxLocalRoot } from "../dropbox/local-root";

// This device's Dropbox folder (ADR-0036) — the same shape
// `useBackendSelection.ts` gives its preference: read once at mount from
// device-local storage, written back on every change, and never touched by
// anything in `store/` or `worker/`. `useFileLinksWiring.ts` reads it to
// normalize a pasted path; Settings writes it.

export interface DropboxLocalRootControl {
  localRoot: string | null;
  setLocalRoot: (value: string) => void;
}

function deviceStorage(): Storage | undefined {
  return typeof localStorage === "undefined" ? undefined : localStorage;
}

export function useDropboxLocalRoot(storage: Storage | undefined = deviceStorage()): DropboxLocalRootControl {
  const [localRoot, setState] = useState<string | null>(() => readDropboxLocalRoot(storage));

  const setLocalRoot = useCallback(
    (value: string) => {
      writeDropboxLocalRoot(storage, value);
      setState(readDropboxLocalRoot(storage));
    },
    [storage],
  );

  return { localRoot, setLocalRoot };
}
