import type { CoreStatus } from "../store/store";

// Issue #194: the header refresh control's gate, as pure decision logic
// (this repo's own pattern for anything decidable — `status-label.ts`,
// `task/token-ui.ts`). It used to be inline in `App.tsx` and gated on the
// calendar alone, which hid the button entirely on a task-only device — the
// default, and the state the owned-stack path actually cares about
// (`App.tsx`'s old `canRefresh` comment, preserved in spirit below: "no
// usable refresh, no button").

/** Whether the header refresh control should render at all, and — since
 * `App.tsx` wires the same boolean straight to `onRefresh` — whether firing
 * it does anything. True whenever there is at least one leg to refresh: a
 * task token (the ADR-0007 cycle), a healthy calendar connection (the
 * context poll), or both. `worker-client.ts`'s postMessage wrappers may only
 * be called once the core reports `ready`, so that gates everything else. */
export function canRefresh(
  status: CoreStatus,
  hasTaskToken: boolean,
  calendarConnected: boolean,
  calendarNeedsReconnect: boolean,
): boolean {
  if (status !== "ready") {
    return false;
  }
  const calendarRefreshable = calendarConnected && !calendarNeedsReconnect;
  return hasTaskToken || calendarRefreshable;
}
