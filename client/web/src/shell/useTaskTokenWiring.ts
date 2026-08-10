import { useEffect, useState } from "react";
import {
  forgetTaskToken,
  isBlankTokenInput,
  loadTaskToken,
  submitTaskToken,
  type TaskTokenDeps,
} from "../task/token";
import { createIndexedDbTaskTokenStore, type TaskTokenStoreLike } from "../task/token-store";
import { coreStore, type CoreStatus } from "../store/store";
import { pushTaskApiKey, type WorkerLike } from "../store/worker-client";

// The web host's device-token lifecycle (#106/S8): entry, rest, and
// re-prompt against the owned server's bearer token (ADR-0004, amended by
// ADR-0008). Mirrors `useCalendarWiring.ts`'s shape — core-start load, an
// interactive submit affordance, `needsReconnect` recovery — but simpler:
// a device token is typed in by hand and long-lived, so there is no
// silent-reconnect or proactive-rotation timer to run, only "load whatever
// is stored" and "a fresh one was just typed in".

export interface TaskTokenWiring {
  hasToken: boolean;
  enteredAtMs: number | null;
  handleSubmitToken: (input: string) => Promise<boolean>;
  handleForgetToken: () => Promise<void>;
}

export function useTaskTokenWiring(
  worker: WorkerLike,
  status: CoreStatus,
  store: TaskTokenStoreLike = defaultStore(),
  now: () => number = Date.now,
): TaskTokenWiring {
  const [hasToken, setHasToken] = useState(false);
  const [enteredAtMs, setEnteredAtMs] = useState<number | null>(null);

  function deps(): TaskTokenDeps {
    return { store, pushApiKey: (token) => pushTaskApiKey(worker, token) };
  }

  // Core-start wiring: load whatever token is stored, if any, and push it
  // into the core immediately (`Core::init` starts with an empty key —
  // `core.worker.ts`'s provisional `""` — so a real device token only
  // reaches it once this fires).
  useEffect(() => {
    if (status !== "ready") {
      return;
    }
    let cancelled = false;
    void (async () => {
      const result = await loadTaskToken(deps());
      if (cancelled) {
        return;
      }
      setHasToken(result.hasToken);
      setEnteredAtMs(result.enteredAtMs);
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status]);

  // The token-entry form's submit handler — both the first-run entry and
  // the 401 re-prompt use this. Returns `false` without touching storage or
  // the worker for a blank submission, so the form can show its own
  // validation message. A successful submit clears the local
  // `needsReconnect` flag optimistically: submitting is the recovery
  // action, and a still-bad token will simply flip it back on the next
  // `taskEvents` broadcast.
  async function handleSubmitToken(input: string): Promise<boolean> {
    if (isBlankTokenInput(input)) {
      return false;
    }
    const result = await submitTaskToken(deps(), input, now());
    setHasToken(result.hasToken);
    setEnteredAtMs(result.enteredAtMs);
    coreStore.setTaskState({ needsReconnect: false });
    return true;
  }

  // "Forget token" (Agent Brief): clears the stored credential and leaves
  // the mirror and every queued capture untouched — this never posts
  // anything to the worker (see `forgetTaskToken`'s own doc).
  async function handleForgetToken(): Promise<void> {
    const result = await forgetTaskToken(store);
    setHasToken(result.hasToken);
    setEnteredAtMs(result.enteredAtMs);
  }

  return { hasToken, enteredAtMs, handleSubmitToken, handleForgetToken };
}

let cachedStore: TaskTokenStoreLike | null = null;
function defaultStore(): TaskTokenStoreLike {
  cachedStore ??= createIndexedDbTaskTokenStore();
  return cachedStore;
}
