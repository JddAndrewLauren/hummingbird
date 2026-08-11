import type { LedgerRowDTO } from "../store/protocol";

// The Ledger's deciding logic, pure and node-testable — the `.tsx` only
// threads state through it, the same split every screens/* module keeps.
//
// The Ledger derives, it does not record (the grilling that shaped it): no
// transition history exists anywhere, so ordering and labels can only read
// each row's *current* facts — `updatedAt` (clobbered by any later edit, the
// accepted caveat), `archivedAt`, and the mirror's retention stamp.

/** What one row's state label says beside its stage. `"archived"` covers
 * both an explicit `archivedAt` flag and a row the mirror demoted because a
 * complete sweep stopped carrying it — either way the row is no longer live
 * anywhere else in the app, and the Ledger is the one read that still shows
 * it, labelled rather than hidden. */
export type LedgerRowState = { kind: "live" } | { kind: "archived"; sinceMs: number };

export function ledgerRowState(row: LedgerRowDTO): LedgerRowState {
  // The row's own flag wins over the mirror's demotion stamp when both
  // exist: `archivedAt` is when the archive actually happened, while
  // `absentSinceMs` for a flagged row is derived from it anyway (the mirror
  // stamps an absence from the flag's own timestamp when it has one).
  if (row.archivedAt !== null) {
    return { kind: "archived", sinceMs: row.archivedAt };
  }
  if (row.absentSinceMs !== null) {
    return { kind: "archived", sinceMs: row.absentSinceMs };
  }
  return { kind: "live" };
}

/** The roster's sort key: when this row was last touched, as far as the
 * derivable facts can say. `updatedAt` covers every server-stamped edit;
 * `archivedAt`/`absentSinceMs` can postdate it (archiving is a flag write,
 * and a sweep demotion happens with no row write at all), so the latest of
 * the three is the honest "something last happened here" instant. */
export function lastTouchedMs(row: LedgerRowDTO): number {
  return Math.max(row.updatedAt, row.archivedAt ?? 0, row.absentSinceMs ?? 0);
}

/** Last touched first — the log-like spine the grilling picked, with the
 * known caveat that any edit re-sorts a row (`updatedAt` is all-purpose).
 * Ties break on id, so the order is total and a re-render can never shuffle
 * equal rows. Pure: returns a new array, never mutates. */
export function orderLedger(rows: readonly LedgerRowDTO[]): LedgerRowDTO[] {
  return [...rows].sort(
    (a, b) => lastTouchedMs(b) - lastTouchedMs(a) || a.id.localeCompare(b.id),
  );
}
