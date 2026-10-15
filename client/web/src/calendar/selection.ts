// The calendar picker's pure selection logic (issue #73), separated from
// the calendar picker so it is unit-testable without a DOM (this repo's
// vitest config runs in the "node" environment — see vitest.config.ts).

import type { BindingDTO, CalendarListEntryDTO } from "../store/protocol";

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

// **The polled set is derived, not purely chosen** (#121, ADR-0005's
// amendment). The ticked calendars are the synced `calendar-selection` row
// (ADR-0040 decision 8, `TaskState.calendarPrefs`), and the `trips-calendar`
// binding is another synced row: designating a Trips calendar opts every
// device into polling it, on the long horizon, whatever the picker says.
// The union is the core's (`decisions/seam.ts`'s
// `effectiveCalendarSelection`) and is computed at every push site rather
// than written back into the selection row — deriving is what makes a
// re-binding re-compute cleanly. The refusal that keeps the bound row from
// being unticked is the core's too (`Core::toggle_calendar`); the picker
// draws the row locked so the refusal is never reached by a tap.

/** ADR-0015's binding key for the Trips calendar. Resolved by name here the
 * same way `waste.ts` resolves its own — the vocabulary itself lives in
 * `hummingbird_core::bindings`, and `Core::bindings` is what says which keys
 * exist. */
export const TRIPS_CALENDAR_BINDING_KEY = "trips-calendar";

/** The designated Trips calendar id, or `null` when there isn't one.
 *
 * Four inputs collapse to `null` deliberately: an unread bindings table, an
 * unset row, a row holding something that is not text, and a row blanked to
 * whitespace (the nearest thing `settings` has to a DELETE). None of them
 * names a calendar, and a caller that has to poll something cannot act on the
 * difference — the *pane* is where those states read differently. */
export function tripsCalendarId(bindings: BindingDTO[] | null): string | null {
  if (bindings === null) {
    return null;
  }
  const binding = bindings.find((candidate) => candidate.key === TRIPS_CALENDAR_BINDING_KEY);
  if (binding === undefined || binding.value.state !== "text") {
    return null;
  }
  const id = binding.value.text.trim();
  return id === "" ? null : id;
}
