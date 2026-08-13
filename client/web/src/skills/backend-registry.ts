// The backend registry #274 asks for: an ordered collection of entries Auto
// walks in declared order. This slice registers exactly one — the cloud
// runner from #272/#273 — deliberately: #275 (on-device) and #276 (home
// runner) each add their own entry by pushing onto this array, never by
// touching the routing logic in `route-plan.ts`/`route-run.ts`.
//
// **A `BackendEntry` is what a picker lists and what a run attempts.** Its
// `model` is fixed per entry (never chosen separately, the way
// `skills/models.ts` used to offer): a future entry with a different model
// is a different entry, not a variant of this one.

export interface BackendEntry {
  /** Stable id, persisted as the device's selection and read back on
   * reload — never re-derived from `label`, which is free to reword. */
  id: string;
  /** What the picker shows. */
  label: string;
  /** Sent as `microtask`'s own `model` arg. `null` leaves the runner's
   * configured default, the same rule `microtask-args.ts` already applies. */
  model: string | null;
  /** Same-origin path (ADR-0018): every entry proxies through the
   * authority, never a runner URL a device could reach directly. */
  endpoint: string;
  /** Auto's per-tier connect timeout — short, because a dead tier must not
   * make the tap wait. */
  connectTimeoutMs: number;
}

export const CLOUD_RUNNER_ENTRY: BackendEntry = {
  id: "cloud",
  label: "Cloud runner",
  model: null,
  endpoint: "/api/skills/run",
  connectTimeoutMs: 2500,
};

/** Declared order is the order Auto walks. #275/#276 append here. */
export const BACKEND_REGISTRY: BackendEntry[] = [CLOUD_RUNNER_ENTRY];

/** The sentinel selection value. Not a `BackendEntry` id — no registry entry
 * may ever be named `"auto"`, which is why picker code checks this constant
 * rather than `registry.length === 0`. */
export const AUTO_SELECTION = "auto";

export function findBackendEntry(registry: BackendEntry[], id: string): BackendEntry | null {
  return registry.find((entry) => entry.id === id) ?? null;
}

/** The one-tap fallback offered when a pin is declined: the next registered
 * entry that is not the dead one, in registry order. `null` when there is
 * none — this slice's single-entry registry means that is always true today,
 * and is exactly why the picker renders no fallback button yet. */
export function fallbackEntry(registry: BackendEntry[], deadId: string): BackendEntry | null {
  return registry.find((entry) => entry.id !== deadId) ?? null;
}
