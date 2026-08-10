// The frontier's display order (issue #108's "Ordering is a pure function
// and is unit-tested" acceptance criterion): most urgent priority label
// first, then soonest deadline, then id as a stable tie-break. The Rust
// twin of this is `client/core/src/task/query.rs`'s `by_priority_then_due`
// (the S1/Linear-era mirror's own version of the identical rule); this is
// the owned-schema web host's copy, since ADR-0002 leaves ranking to
// consumers and `hummingbird_domain::Item.priority` stays a bare `i64`.

import type { TaskItemDTO } from "../store/protocol";
import { priorityRank } from "./priority";
import { deadlineSortKey } from "./urgency";

/** A pure sort: never mutates `items`, and reading it twice with the same
 * input yields the same output — no clock, no randomness, nothing ambient. */
export function orderFrontier(items: readonly TaskItemDTO[]): TaskItemDTO[] {
  return [...items].sort((a, b) => {
    const byPriority = priorityRank(a.priority) - priorityRank(b.priority);
    if (byPriority !== 0) {
      return byPriority;
    }
    const byDeadline = compareDeadlines(a.deadline, b.deadline);
    if (byDeadline !== 0) {
      return byDeadline;
    }
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
  });
}

/** `None` (no deadline) sorts last — an item with nothing pressing it is
 * not the same as one due infinitely soon. */
function compareDeadlines(a: string | null, b: string | null): number {
  if (a !== null && b !== null) {
    const keyA = deadlineSortKey(a);
    const keyB = deadlineSortKey(b);
    return keyA < keyB ? -1 : keyA > keyB ? 1 : 0;
  }
  if (a !== null) {
    return -1;
  }
  if (b !== null) {
    return 1;
  }
  return 0;
}
