// The picker's own choice, persisted device-locally (#274). Same shape as
// `theme/theme.ts`'s preference: a narrow `StorageLike` (defaults to
// `localStorage`) so the read/write logic is unit-testable without a DOM,
// and — the point of this module — never anywhere near the sync engine.
// `localStorage` is per-origin and per-device; nothing here ever reaches
// the outbound queue, the mirror, or a `Core` mutation entry point, which is
// what makes "device preference, not a synced fact" true by construction.

import { resolveBackendSelectionFromCore } from "../decisions/seam";
import { AUTO_SELECTION, type BackendEntry } from "./backend-registry";

const SELECTION_KEY = "hb.backend-selection";

/** Same shape as `calendar/persistence.ts`'s `StorageLike`, defined again
 * rather than imported: this module lives in `src/skills/`, which
 * `no-queue.test.ts` pins to importing nothing outside itself (plus one
 * named type exception) — the whole point being that the lane's import
 * graph alone proves it cannot reach the sync engine, without a reviewer
 * having to trust a promise. */
export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/**
 * Auto when nothing is stored, and Auto when the stored value no longer
 * names a registered entry — the same "a stale binding degrades to the
 * safe default" rule `route-plan.ts`'s `planRoute` applies at routing time,
 * kept here too so a picker never renders a selection it cannot label.
 *
 * The degrade rule itself is `hummingbird_core::decisions::skills::
 * resolve_backend_selection` (#539); this reads the raw stored value and
 * hands it to the seam.
 */
export function readBackendSelection(storage: StorageLike | undefined, registry: BackendEntry[]): string {
  // No storage at all (a context without `localStorage`) reads the same as
  // nothing stored — Auto. `rail-collapse.ts` takes its preference the same
  // way, for the same reason.
  if (!storage) return AUTO_SELECTION;
  const raw = storage.getItem(SELECTION_KEY);
  return resolveBackendSelectionFromCore(
    raw ?? undefined,
    registry.map((entry) => entry.id),
  );
}

export function writeBackendSelection(storage: StorageLike | undefined, selection: string): void {
  if (!storage) return;
  storage.setItem(SELECTION_KEY, selection);
}
