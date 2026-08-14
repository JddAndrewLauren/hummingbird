// Device/origin-local sync history for the reachability question (#316).
// This is a timestamp, not a credential or an authority-owned fact: clearing
// site data deliberately clears it. The narrow storage seam keeps worker
// attachment deterministic in tests and avoids coupling this module to a
// browser global.

const LAST_SUCCESSFUL_SYNC_KEY = "hb.sync.lastSuccessfulAtMs";

export interface SyncStorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function validTimestamp(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

export function readLastSuccessfulSyncAtMs(storage: SyncStorageLike): number | null {
  try {
    const raw = storage.getItem(LAST_SUCCESSFUL_SYNC_KEY);
    if (raw === null) {
      return null;
    }
    const parsed: unknown = JSON.parse(raw);
    return validTimestamp(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

/** Persist `candidate` when it is a valid advance, or when the persisted
 * value is ahead of the clock that observed this completed cycle. The
 * latter is a wall-clock correction, not an old view trying to overwrite a
 * newer success. Returns the newest usable value even when storage itself
 * is unavailable. */
export function advanceLastSuccessfulSyncAtMs(
  storage: SyncStorageLike,
  current: number | null,
  candidate: number,
  observedAtMs: number,
): number | null {
  const persisted = readLastSuccessfulSyncAtMs(storage);
  const newestKnown =
    current === null ? persisted : persisted === null ? current : Math.max(current, persisted);
  if (!validTimestamp(candidate)) {
    return newestKnown;
  }
  const correctingBackwardClock =
    validTimestamp(observedAtMs) && newestKnown !== null && newestKnown > observedAtMs;
  if (newestKnown !== null && candidate <= newestKnown && !correctingBackwardClock) {
    return newestKnown;
  }
  try {
    storage.setItem(LAST_SUCCESSFUL_SYNC_KEY, JSON.stringify(candidate));
  } catch {
    // The in-memory answer is still valid. Persistence failure must not turn
    // a completed authority request into a missing success for this view.
  }
  return candidate;
}
