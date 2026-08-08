// The calendar picker's pure selection logic (issue #73), separated from
// CalendarPicker.tsx so it is unit-testable without a DOM (this repo's
// vitest config runs in the "node" environment — see vitest.config.ts).

/** Toggles `id` in `selected`, preserving the existing order of the
 * untouched entries. */
export function toggleCalendarId(selected: string[], id: string): string[] {
  return selected.includes(id)
    ? selected.filter((existing) => existing !== id)
    : [...selected, id];
}
