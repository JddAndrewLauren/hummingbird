// PROTOTYPE — throwaway. Delete with the rest of this directory (#122).
//
// The stub behind the `scheduled_date` affordance. Setting a do-date in the
// real app is a `Core::triage` CAS PATCH through the SharedWorker; here it is
// a module-level map of overrides applied over the fixture at read time, so
// the merge visibly re-runs and the reader can judge the AFFORDANCE without
// the prototype touching production wiring (UI.md: "point it at a stub").
//
// A module-level store with a window event rather than context, for the same
// reason `prototype-race-pane` used one: threading a provider through `App`
// would edit production code the prototype is meant to leave alone.

import { useEffect, useState } from "react";
import type { WeekendItem } from "./weekend";

const CHANGED = "prototype-weekend-pane:plans";

/** itemId -> `YYYY-MM-DD` or null (explicitly cleared). Absent = untouched,
 * which is NOT the same as cleared — that difference is exactly what the
 * "clear" affordance has to be able to express. */
let overrides: Record<string, string | null> = {};
// Bumped on every change. The subscriber keys on THIS, not on the edit
// count — re-planning an item from Saturday to Sunday leaves the count
// identical, and a component keyed on the count would never re-render.
let version = 0;

export function setPlan(itemId: string, dayKey: string | null) {
  overrides = { ...overrides, [itemId]: dayKey };
  version += 1;
  window.dispatchEvent(new Event(CHANGED));
}

export function resetPlans() {
  overrides = {};
  version += 1;
  window.dispatchEvent(new Event(CHANGED));
}

export function planEditCount(): number {
  return Object.keys(overrides).length;
}

/** The fixture items with every stub edit applied. */
export function applyPlans(items: WeekendItem[]): WeekendItem[] {
  return items.map((item) =>
    item.id in overrides ? { ...item, scheduledDate: overrides[item.id] } : item,
  );
}

/** Re-renders whichever slot is mounted whenever a plan changes. Returns the
 * store version — the data itself flows through `applyPlans`. */
export function usePlanVersion(): number {
  const [seen, setSeen] = useState(version);
  useEffect(() => {
    function onChanged() {
      setSeen(version);
    }
    window.addEventListener(CHANGED, onChanged);
    return () => window.removeEventListener(CHANGED, onChanged);
  }, []);
  return seen;
}
