import { useEffect, useState } from "react";
import {
  clearSignedOutToken,
  forgetTaskToken,
  loadTaskToken,
  submitTaskToken,
  type TaskTokenDeps,
  type TaskTokenSubmitOutcome,
} from "../task/token";
import { createIndexedDbTaskTokenStore, type TaskTokenStoreLike } from "../task/token-store";
import { coreStore, type CoreStatus } from "../store/store";
import {
  clearTaskApiKey,
  initTaskApiKey,
  pushTaskApiKey,
  reportNoTaskApiKey,
  triggerSyncManual,
  type WorkerLike,
} from "../store/worker-client";

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
  handleSubmitToken: (input: string) => Promise<TaskTokenSubmitOutcome>;
  handleForgetToken: () => Promise<void>;
  /** This device signed itself out from Signed-in devices (#840): the core
   * already holds no credential, so only the stored token goes. */
  handleSignedOut: () => Promise<void>;
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
    return {
      store,
      pushApiKey: (token) => pushTaskApiKey(worker, token),
      initApiKey: (token) => initTaskApiKey(worker, token),
      clearApiKey: () => clearTaskApiKey(worker),
      reportNoApiKey: () => reportNoTaskApiKey(worker),
    };
  }

  // Core-start wiring: load whatever token is stored, if any, and rehydrate
  // it into the core immediately (`Core::init` takes no credential and starts
  // `unknown` — #846 — so a real device token only reaches it once this
  // fires, and a device with none reports that absence here too).
  // `loadTaskToken` itself resolves a failed read to the honest "unset"
  // state rather than rejecting, so nothing here needs its own try/catch.
  //
  // This effect keys on THIS VIEW's own `status`, so it fires again on
  // every additional view (tab / PWA window) that reaches `ready` under
  // #126's one-shared-core-per-origin — that used to matter, because
  // `loadTaskToken` called `pushApiKey`, which unconditionally resumes a
  // held credential: opening a second tab silently retried a token the
  // first tab's rejected cycle had just held on (issue #196). Nothing here
  // changed to fix that — `loadTaskToken` now calls `deps.initApiKey`
  // instead, which the worker protocol (`initTaskApiKey`, `protocol.ts`)
  // never resumes a hold for. This effect firing per-view is fine; it is
  // the rehydration it triggers that had to stop resuming.
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
  // the 401 re-prompt use this. Only updates local state on `"ok"`: a
  // `"blank"` or `"storeError"` outcome must never be reported as a
  // resting token, and the caller (the form) is what turns the outcome
  // into a validation message. A successful submit clears the local
  // `needsReconnect` flag optimistically: submitting is the recovery
  // action, and a still-bad token will simply flip it back on the next
  // `taskEvents` broadcast.
  async function handleSubmitToken(input: string): Promise<TaskTokenSubmitOutcome> {
    const nowMs = now();
    const outcome = await submitTaskToken(deps(), input, nowMs);
    if (outcome === "ok") {
      setHasToken(true);
      setEnteredAtMs(nowMs);
      coreStore.setTaskState({ needsReconnect: false });
    }
    return outcome;
  }

  // "Forget token" (Agent Brief): clears the stored credential AND the live
  // key the core holds (`clearTaskApiKey` — see `token.ts`'s doc for why
  // clearing storage alone is not enough under #126's SharedWorker). The
  // mirror and every queued capture are untouched either way.
  //
  // #889: both ways this device loses its token then run one cycle. The Sync
  // card reads the last cycle's outcome, which until the next tick is still
  // the "completed" one the old token earned; a cycle with no credential
  // sends no request and answers `no_credential`, so every view's card says
  // held at once. Queued behind the clear, so it never runs with the old key.
  async function handleForgetToken(): Promise<void> {
    const result = await forgetTaskToken(deps());
    setHasToken(result.hasToken);
    setEnteredAtMs(result.enteredAtMs);
    triggerSyncManual(worker);
  }

  async function handleSignedOut(): Promise<void> {
    const result = await clearSignedOutToken(deps());
    setHasToken(result.hasToken);
    setEnteredAtMs(result.enteredAtMs);
    triggerSyncManual(worker);
  }

  return { hasToken, enteredAtMs, handleSubmitToken, handleForgetToken, handleSignedOut };
}

let cachedStore: TaskTokenStoreLike | null = null;
function defaultStore(): TaskTokenStoreLike {
  cachedStore ??= createIndexedDbTaskTokenStore();
  return cachedStore;
}
