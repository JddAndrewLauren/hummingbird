import type { DeadLetterEntryDTO, LedgerRowDTO } from "../store/protocol";

/** What a dead-lettered entry was *about*, said in words.
 *
 * The journal's own `id` is the queue entry's — it names the attempt, not the
 * thing — so "1 edit didn't apply" could state that a change had been
 * abandoned without ever saying whose. `entity`/`entityId` (derived in the
 * core from the queued intent, `MutationIntent::subject`) is what fixed that,
 * and this is the one place that turns the pair into a sentence.
 *
 * Pure, and separate from `SettingsScreen.tsx` for the reason every
 * `screens/*` module is: the naming rule below is a decision with three
 * branches and it is worth a test each, none of which needs React.
 */

/** Display words for the entities a person can recognise. Deliberately
 * partial: an entity absent here renders as its own path segment, which is
 * honest and readable (`"projects"`, `"rules"`), and inventing a display word
 * per entity ahead of a surface that shows it would be vocabulary nobody
 * asked for. */
const ENTITY_WORD: Record<string, string> = {
  items: "item",
  steps: "step",
  settings: "setting",
};

/** The sentence naming what didn't apply.
 *
 * Three branches, in order of how much is known:
 *
 * 1. The row is an item the ledger knows — name it by **title**, which is the
 *    only spelling a person can act on. (`ledger` is the complete retained
 *    roster, archived rows included, so a dead-lettered edit to an item that
 *    has since been archived still names it.)
 * 2. The row is known only by id — say the entity and the id. Half an answer
 *    is still the difference between "an edit failed" and "*that* edit
 *    failed".
 * 3. The intent named no row at all (`entityId` is `null`) — say the entity
 *    alone rather than inventing an identity, the same discipline
 *    `write-failure.ts` keeps for a title it cannot find.
 *
 * `ledger` is `null` before the first `getLedger` answer arrives — a real
 * "not read yet" state (`TaskState.ledger`), which lands on branch 2 rather
 * than being treated as "no such item".
 */
export function deadLetterSubject(
  entry: Pick<DeadLetterEntryDTO, "entity" | "entityId">,
  ledger: readonly LedgerRowDTO[] | null,
): string {
  const word = ENTITY_WORD[entry.entity] ?? entry.entity;
  if (entry.entityId === null) {
    return word;
  }
  const title =
    entry.entity === "items"
      ? ledger?.find((row) => row.id === entry.entityId)?.title
      : undefined;
  return title === undefined ? `${word} ${entry.entityId}` : `${word} "${title}"`;
}
