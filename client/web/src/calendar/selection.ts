// The calendar picker's pure selection logic (issue #73), separated from
// CalendarPicker.tsx so it is unit-testable without a DOM (this repo's
// vitest config runs in the "node" environment — see vitest.config.ts).

import type { CalendarListEntryDTO } from "../store/protocol";

/** Toggles `id` in `selected`, preserving the existing order of the
 * untouched entries. */
export function toggleCalendarId(selected: string[], id: string): string[] {
  return selected.includes(id)
    ? selected.filter((existing) => existing !== id)
    : [...selected, id];
}

/** The selected ids the core's last listing did not offer.
 *
 * A calendar that is deleted, or whose access is revoked, simply stops
 * appearing in `calendarList` — but the id stays in the persisted selection,
 * and polling is all-or-nothing: its 403/404 aborts the whole snapshot, so
 * every calendar's context goes stale and stays stale. Without a row in the
 * picker for it there is nothing to uncheck and no way out of that state.
 *
 * These are *not* dropped automatically. `calendarList` also omits calendars
 * the user has merely hidden in Google's UI, and silently deselecting one
 * would quietly stop polling a calendar the user still wants — so this
 * surfaces them for the user to remove instead. An empty `available` (never
 * listed yet: offline start, held credential) therefore returns nothing:
 * "we haven't looked" must not read as "none of these exist". */
export function unavailableSelectedIds(
  selected: string[],
  available: CalendarListEntryDTO[],
): string[] {
  if (available.length === 0) {
    return [];
  }
  const availableIds = new Set(available.map((calendar) => calendar.id));
  return selected.filter((id) => !availableIds.has(id));
}
