// Per-device opt-in state (issue #73) — NEVER a credential. Only a boolean
// "this device has connected Google Calendar before" flag and the picker's
// calendar-id selection are persisted; the access token itself only ever
// lives in the core's in-memory `CredentialState` (#72/ADR-0005) and is
// never written here. `storage` is injectable (defaults to `localStorage`)
// so the read/write logic is unit-testable without a DOM.

const CONNECTED_KEY = "hb.calendar.connected";
const SELECTED_CALENDAR_IDS_KEY = "hb.calendar.selectedCalendarIds";

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export function readConnected(storage: StorageLike): boolean {
  return storage.getItem(CONNECTED_KEY) === "true";
}

export function writeConnected(storage: StorageLike, connected: boolean): void {
  if (connected) {
    storage.setItem(CONNECTED_KEY, "true");
  } else {
    storage.removeItem(CONNECTED_KEY);
  }
}

export function readSelectedCalendarIds(storage: StorageLike): string[] {
  const raw = storage.getItem(SELECTED_CALENDAR_IDS_KEY);
  if (!raw) {
    return [];
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((id): id is string => typeof id === "string") : [];
  } catch {
    return [];
  }
}

export function writeSelectedCalendarIds(storage: StorageLike, ids: string[]): void {
  storage.setItem(SELECTED_CALENDAR_IDS_KEY, JSON.stringify(ids));
}
