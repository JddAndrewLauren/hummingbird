// Display-only derivations for the device token surface (#106/S8) — no
// storage, no worker, just the mapping from state to what a screen shows.
// Kept separate from `token.ts` (the load/submit/forget orchestration) so
// the copy can be unit-tested on its own, the same split the calendar
// binding uses between `connection.ts` (behaviour) and `tile-props.ts`
// (display).

export type TaskTokenUiState = "unset" | "resting" | "reprompt";

/** Derives the surface's state from only what this device knows locally:
 * whether a token is stored, and whether the core has flagged it as no
 * longer working (a `credential_needed` event — `store.ts`'s
 * `TaskState.needsReconnect`). `hasToken` wins over a stale reconnect flag:
 * forgetting a token during a 401 hold moves the surface back to "unset",
 * not "reprompt" — there is nothing left to re-enter *against*, just a
 * fresh first-run entry. */
export function taskTokenUiState(hasToken: boolean, needsReconnect: boolean): TaskTokenUiState {
  if (!hasToken) {
    return "unset";
  }
  return needsReconnect ? "reprompt" : "resting";
}

/** The queue-status sentence for each state (Agent Brief: "a device with no
 * token still captures — queued and pending — and that state is visibly
 * distinct from a 401 hold"). The `reprompt` copy deliberately never says
 * "expired": these tokens are long-lived, so a rejected one has been
 * revoked, not timed out. */
export function taskQueueStatusCopy(state: TaskTokenUiState): string {
  switch (state) {
    case "unset":
      return "No device token yet. Captures are still queued and pending — they will sync once a token is entered.";
    case "reprompt":
      return "This device token no longer works. Captures are queued and held, not draining, until a fresh token is entered.";
    case "resting":
      return "Captures sync normally.";
  }
}

/** Formats an `enteredAtMs` timestamp for the mono meta line next to a
 * resting token — machine values use the design system's `.hb-meta` style,
 * never the token itself. */
export function formatEnteredAt(enteredAtMs: number): string {
  return new Date(enteredAtMs).toISOString();
}
