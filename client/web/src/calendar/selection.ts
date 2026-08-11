// The calendar picker's pure selection logic (issue #73), separated from
// the calendar picker so it is unit-testable without a DOM (this repo's
// vitest config runs in the "node" environment — see vitest.config.ts).

import type { BindingDTO, CalendarListEntryDTO, CalendarSelectionDTO } from "../store/protocol";

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
// amendment).
//
// Two facts decide which calendars this device polls, and they live in
// different places on purpose: `selectedCalendarIds` is per-device,
// `localStorage`-persisted and host-owned (ADR-0005's "context is per-device
// opt-in"), while the `trips-calendar` binding is a **synced** `settings` row
// that every device sees (#118, ADR-0015). Designating a Trips calendar is
// what makes the vacation countdown answerable at all, so it opts every
// device into polling that calendar — on the long horizon, since a trip a
// year out is outside the ordinary 90-day window entirely.
//
// The union is computed at **every** push site rather than written back into
// `localStorage`, and that is the whole point: deriving is what makes a
// re-binding re-compute cleanly. Persisting it would leave the old calendar
// polled forever with nothing on screen that knows why — the consent
// surprise ADR-0005 guarded against.

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

/** What this device actually polls: the ticked calendars ∪ the bound Trips
 * calendar, each with its horizon. Order is the stored order, with a bound
 * calendar nobody ticked appended — stable, so a re-push is byte-identical
 * while nothing moved. */
export function effectiveSelection(
  storedIds: readonly string[],
  tripsId: string | null,
): CalendarSelectionDTO[] {
  const selection: CalendarSelectionDTO[] = storedIds.map((id) => ({
    id,
    horizon: id === tripsId ? "long" : "standard",
  }));
  if (tripsId !== null && !storedIds.includes(tripsId)) {
    selection.push({ id: tripsId, horizon: "long" });
  }
  return selection;
}

/** The same set as ids alone — what the picker renders as checked, so the
 * locked row is visibly part of the polled set rather than a calendar being
 * fetched with nothing on screen to say so. */
export function effectiveCalendarIds(
  storedIds: readonly string[],
  tripsId: string | null,
): string[] {
  return effectiveSelection(storedIds, tripsId).map((entry) => entry.id);
}

/** What a picker change becomes, or `null` when it must be **refused**.
 *
 * `requestedIds` is a toggle over the *effective* set (what the picker
 * displays), so a change that omits the bound Trips calendar is an attempt to
 * untick the locked row. That is refused outright rather than accepted and
 * silently re-added: a control that springs back is a control that lied about
 * what it does, and the reason the row is locked has to be visible instead.
 *
 * An accepted change is stripped of the derived id before it is persisted —
 * the binding contributes to the polled set at read time, never to
 * `localStorage`. */
export function acceptSelectionChange(
  requestedIds: readonly string[],
  tripsId: string | null,
): string[] | null {
  if (tripsId !== null && !requestedIds.includes(tripsId)) {
    return null;
  }
  return requestedIds.filter((id) => id !== tripsId);
}
