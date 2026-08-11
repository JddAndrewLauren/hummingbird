import type { TaskItemDTO } from "../store/protocol";

/** The Done screen's order: most recently touched first. The schema has no
 * `done_at` (declined in the grilling that shaped this screen — a later
 * additive column if the caveat grates), so `updatedAt` is the only stamp
 * available, and a later edit to a done item honestly re-sorts it. Ties
 * break on id so the order is total. Pure: returns a new array. */
export function orderDone(items: readonly TaskItemDTO[]): TaskItemDTO[] {
  return [...items].sort((a, b) => b.updatedAt - a.updatedAt || a.id.localeCompare(b.id));
}
