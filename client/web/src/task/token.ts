import type { TaskTokenStoreLike } from "./token-store";

// Issue #106/S8's entry/rest/re-prompt orchestration for the owned-server
// device token (ADR-0004, amended by ADR-0008), kept free of IndexedDB and
// the wasm worker so it is unit-testable against a fake `TaskTokenStoreLike`
// and spyable `pushApiKey`/`clearApiKey` callbacks — the same discipline
// `calendar/connection.ts` uses for the calendar credential.
//
// There is no rotation timer here (unlike the calendar's GIS-driven
// `connection.ts`): a device token is long-lived and entered by hand, so
// the only lifecycle events are "load whatever is stored, once, at
// startup", "a fresh one was typed in", and "forget the one that's here".

export interface TaskTokenDeps {
  store: TaskTokenStoreLike;
  /** `worker-client.ts`'s `pushTaskApiKey`, bound to the live worker. Never
   * called with anything but the token itself — nothing here logs it or
   * wraps it in an error. */
  pushApiKey: (token: string) => void;
  /** `worker-client.ts`'s `clearTaskApiKey`, bound to the live worker.
   * Round-1 review finding: #126 put the core in a SharedWorker that
   * outlives any single tab, so clearing storage alone leaves the live key
   * resident in the core's `AccessTokenSlot` for as long as any tab stays
   * open — the device would keep draining and pulling with a "forgotten"
   * token. This is the wire message (`clearTaskApiKey`) that closes that
   * gap by reaching `Core::clear_api_key` (client/core/src/lib.rs). */
  clearApiKey: () => void;
}

export interface TaskTokenLoadResult {
  hasToken: boolean;
  /** When the current token was entered, or `null` when there is none.
   * Metadata only — rendered via the design system's mono meta style, never
   * the token's own value. */
  enteredAtMs: number | null;
}

/** Result of a submit attempt: `"ok"`, `"blank"` (rejected before touching
 * storage or the worker), or `"storeError"` (the underlying `store.write`
 * rejected — an IndexedDB failure, say). Callers must not treat anything
 * but `"ok"` as a successful save. */
export type TaskTokenSubmitOutcome = "ok" | "blank" | "storeError";

const NO_TOKEN: TaskTokenLoadResult = { hasToken: false, enteredAtMs: null };

/** Core-start wiring: reads whatever device token this browser has stored
 * and, if present, pushes it into the core immediately — mirroring
 * `calendar/connection.ts`'s `initConnection`, but for a token the user
 * typed in rather than one GIS silently re-mints. A never-entered device
 * does nothing here and stays in the "unset" state.
 *
 * A failed read (`store.read()` rejects — a corrupt or blocked IndexedDB)
 * is reported as `null` rather than left to reject this promise: a load
 * failure must resolve to the same honest "unset" state a never-entered
 * device shows, not an unhandled rejection that leaves the UI stuck on
 * whatever it last rendered. */
export async function loadTaskToken(deps: TaskTokenDeps): Promise<TaskTokenLoadResult> {
  let record;
  try {
    record = await deps.store.read();
  } catch (error) {
    console.error("failed to read the stored device token", error);
    return NO_TOKEN;
  }
  if (record === null) {
    return NO_TOKEN;
  }
  deps.pushApiKey(record.token);
  return { hasToken: true, enteredAtMs: record.enteredAtMs };
}

/** Whether `input` is worth submitting at all — a blank or whitespace-only
 * entry is rejected before it ever reaches storage or the worker. Callers
 * (the token-entry form) check this before calling `submitTaskToken`. */
export function isBlankTokenInput(input: string): boolean {
  return input.trim().length === 0;
}

/** The token-entry form's submit handler: trims `input` (a pasted trailing
 * newline or leading space is never part of the token a server minted —
 * left untrimmed it would silently become a wrong bearer value and read
 * back as an unexplained 401), persists it, and pushes it into the core.
 * Also the recovery path for a 401 re-prompt: a fresh token submitted here
 * is exactly what `Core::push_api_key` needs to resume a held cycle.
 *
 * Returns `"blank"` without touching storage or the worker if `input` is
 * blank after trimming, and `"storeError"` without pushing anything into
 * the core if `store.write` rejects — a failed write must never be
 * reported as `"ok"`, or the UI would claim a token is resting that the
 * device never actually saved. */
export async function submitTaskToken(
  deps: TaskTokenDeps,
  input: string,
  nowMs: number,
): Promise<TaskTokenSubmitOutcome> {
  const token = input.trim();
  if (token.length === 0) {
    return "blank";
  }
  try {
    await deps.store.write({ token, enteredAtMs: nowMs });
  } catch (error) {
    console.error("failed to save the device token", error);
    return "storeError";
  }
  deps.pushApiKey(token);
  return "ok";
}

/** "Forget token" (Agent Brief): clears the stored credential AND the live
 * key the core is holding in memory (`deps.clearApiKey`, wired to
 * `Core::clear_api_key`) — #126's SharedWorker outlives any one tab, so
 * clearing storage alone would leave a "forgotten" token still draining and
 * pulling for as long as any tab stays open. The mirror and every queued
 * capture are untouched either way: only the credential slot is cleared. */
export async function forgetTaskToken(deps: TaskTokenDeps): Promise<TaskTokenLoadResult> {
  await deps.store.clear();
  deps.clearApiKey();
  return NO_TOKEN;
}
