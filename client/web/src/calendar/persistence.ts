// The old per-device calendar choices (issue #73) — read ONCE, for
// ADR-0040 decision 8's migration, and then deleted. Until #843 this browser
// kept a "connected" flag and the picker's ticked ids in `localStorage`;
// both are synced `settings` rows now (`hummingbird_core::calendar_prefs`),
// read and written through the core. Nothing here writes either key any
// more: `useCalendarWiring.ts` hands what it finds to the core's one-time
// seed and, once the core answers anything but "not yet", clears them.
// Never a credential — the access token only ever lives in the core's
// in-memory `CredentialState` (#72/ADR-0005). `storage` is injectable
// (defaults to `localStorage`) so this is unit-testable without a DOM.

const CONNECTED_KEY = "hb.calendar.connected";
const SELECTED_CALENDAR_IDS_KEY = "hb.calendar.selectedCalendarIds";

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/** This browser's old calendar choices, or `null` when it holds none —
 * a browser that never connected, or one already migrated. */
export interface LocalCalendarPrefs {
  connected: boolean;
  selectedIds: string[];
}

export function readLocalCalendarPrefs(storage: StorageLike): LocalCalendarPrefs | null {
  if (storage.getItem(CONNECTED_KEY) === null && storage.getItem(SELECTED_CALENDAR_IDS_KEY) === null) {
    return null;
  }
  return {
    connected: storage.getItem(CONNECTED_KEY) === "true",
    selectedIds: readSelectedCalendarIds(storage),
  };
}

/** Deletes both old keys — the migration's last step, on any settled seed. */
export function clearLocalCalendarPrefs(storage: StorageLike): void {
  storage.removeItem(CONNECTED_KEY);
  storage.removeItem(SELECTED_CALENDAR_IDS_KEY);
}

function readSelectedCalendarIds(storage: StorageLike): string[] {
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
