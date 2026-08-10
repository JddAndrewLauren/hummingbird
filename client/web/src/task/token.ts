import type { TaskTokenStoreLike } from "./token-store";

// Issue #106/S8's entry/rest/re-prompt orchestration for the owned-server
// device token (ADR-0004, amended by ADR-0008), kept free of IndexedDB and
// the wasm worker so it is unit-testable against a fake `TaskTokenStoreLike`
// and a spyable `pushApiKey` — the same discipline `calendar/connection.ts`
// uses for the calendar credential.
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
}

export interface TaskTokenLoadResult {
  hasToken: boolean;
  /** When the current token was entered, or `null` when there is none.
   * Metadata only — rendered via the design system's mono meta style, never
   * the token's own value. */
  enteredAtMs: number | null;
}

const NO_TOKEN: TaskTokenLoadResult = { hasToken: false, enteredAtMs: null };

/** Core-start wiring: reads whatever device token this browser has stored
 * and, if present, pushes it into the core immediately — mirroring
 * `calendar/connection.ts`'s `initConnection`, but for a token the user
 * typed in rather than one GIS silently re-mints. A never-entered device
 * does nothing here and stays in the "unset" state. */
export async function loadTaskToken(deps: TaskTokenDeps): Promise<TaskTokenLoadResult> {
  const record = await deps.store.read();
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

/** The token-entry form's submit handler: persists `input` verbatim (no
 * trimming the token itself — whatever was pasted round-trips exactly) and
 * pushes it into the core. Callers are expected to have already rejected a
 * blank `input` via [`isBlankTokenInput`]. Also the recovery path for a 401
 * re-prompt: a fresh token submitted here is exactly what
 * `Core::push_api_key` needs to resume a held cycle. */
export async function submitTaskToken(
  deps: TaskTokenDeps,
  input: string,
  nowMs: number,
): Promise<TaskTokenLoadResult> {
  await deps.store.write({ token: input, enteredAtMs: nowMs });
  deps.pushApiKey(input);
  return { hasToken: true, enteredAtMs: nowMs };
}

/** "Forget token" (Agent Brief): clears the stored credential only. The
 * mirror and every queued capture are untouched — this never talks to the
 * worker at all, since there is no "unset the key" message in the wire
 * protocol (`store/protocol.ts`) and none is needed: the running core
 * simply keeps whatever key it last held in memory until the tab reloads,
 * at which point `loadTaskToken` finds nothing stored and starts the
 * "unset" state fresh. */
export async function forgetTaskToken(store: TaskTokenStoreLike): Promise<TaskTokenLoadResult> {
  await store.clear();
  return NO_TOKEN;
}
