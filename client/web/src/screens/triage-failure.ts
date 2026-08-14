import type { TaskItemDTO } from "../store/protocol";
import type { TaskTriageResult } from "../store/store";

/** What a failed triage says — the one spelling, for both surfaces that say it.
 *
 * `TaskState.lastTriage` holds the most recent result and nothing else, so at
 * most one failure exists at a time on any screen. Everything here is a pure
 * read over that single result; the `.tsx` files only place the string.
 *
 * Split out of `TriageRow` when Now grew a second place to say this (#418):
 * two components deriving the same sentence from the same broadcast is the
 * shape that drifts, and the fallback wording in particular is a decision
 * ("didn't apply", never "failed") rather than an implementation detail.
 */

/** The failure belonging to one item, matched by the id the result itself
 * carries — the broadcast-recognition contract `NowScreen`'s `actError` uses
 * for `lastAct`. A failure belongs to whichever item it names, never to
 * "whichever row is open". `null` when the last result was an `"ok"`, named a
 * different item, or has not arrived yet. */
export function triageFailureFor(
  result: TaskTriageResult | null | undefined,
  itemId: string,
): string | null {
  return result && result.itemId === itemId && result.kind !== "ok"
    ? (result.error ?? "That triage didn't apply.")
    : null;
}

/** The same failure when there is no row on screen wearing it (#418).
 *
 * On the Triage screen the rows stay mounted in a list, so a result that lands
 * after the reader collapses one still has its row to render into. In Now the
 * row **is** the slot above the columns: closing the slot unmounts `TriageRow`
 * outright, and a failure arriving afterwards was displayed nowhere at all —
 * the capture's card returned to the board (correctly, a failed triage leaves
 * the item in `triageInbox`) carrying no sign anything had gone wrong.
 *
 * So the screen states it instead, and names the item, because "a triage
 * failed" without saying which is not worth the line. Suppressed while that
 * capture is the open one — `TriageRow` is mounted then and owns the message,
 * and doubling it onto both would be two alerts for one failure.
 *
 * The title is read from the live inbox rather than remembered from the click:
 * a failed triage puts the capture back, so it is there to be named, and if it
 * is *not* (a `"not_found"`, or an item that has since left) the honest
 * sentence is the un-named one rather than a stale title.
 */
export function strandedTriageFailure(
  result: TaskTriageResult | null | undefined,
  openCaptureId: string | null,
  inbox: readonly TaskItemDTO[],
): string | null {
  if (!result || result.kind === "ok" || result.itemId === openCaptureId) {
    return null;
  }
  const title = inbox.find((item) => item.id === result.itemId)?.title;
  if (title === undefined) {
    return result.error ?? "That triage didn't apply.";
  }
  // Em dash for the honest aside, the house punctuation for exactly this —
  // the sentence states what is true, then hands over the server's own words.
  return result.error
    ? `Triage didn't apply to "${title}" — ${result.error}`
    : `Triage didn't apply to "${title}".`;
}
