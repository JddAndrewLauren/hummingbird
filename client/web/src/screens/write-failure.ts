import type { TaskItemDTO } from "../store/protocol";
import type { TaskActResult, TaskGrillCompletionResult, TaskTriageResult } from "../store/store";

/** What a failed write says — the one spelling, for every surface that says it.
 *
 * `TaskState.lastTriage` and `TaskState.lastAct` each hold the most recent
 * result and nothing else, so at most one failure of each kind exists at a
 * time. That is honest rather than lossy here: both are posted synchronously,
 * one per request, off the task host's own serial queue (`task-worker.ts`) —
 * a `"failed"` means the mutation never reached the outbound queue at all. The
 * lane that genuinely holds many failures at once is the **dead-letter
 * journal**, which is where a write rejected by the authority lands, and it
 * has its own surface (`SettingsScreen`).
 *
 * Everything here is a pure read over one of those single results; the `.tsx`
 * files only place the string.
 *
 * Split out of `TriageRow` when Now grew a second place to say this (#418):
 * two components deriving the same sentence from the same broadcast is the
 * shape that drifts, and the fallback wording in particular is a decision
 * ("didn't apply", never "failed") rather than an implementation detail.
 * Generalised from triage to `lastAct` when Now grew the same pair of surfaces
 * for acts — one algorithm with the subject as its parameter, never a second
 * copy of it under another noun.
 */

/** The two fields every mutation result shares — enough to decide whether it
 * is a failure and whose it is. Structural on purpose: `TaskActResult` and
 * `TaskTriageResult` have different `kind` unions and different extra fields,
 * and nothing here reads either. */
interface ItemWriteResult {
  itemId: string;
  kind: string;
  error: string | null;
}

function failureFor(
  result: ItemWriteResult | null | undefined,
  itemId: string,
  fallback: string,
): string | null {
  return result && result.itemId === itemId && result.kind !== "ok"
    ? (result.error ?? fallback)
    : null;
}

/** The same failure when there is no component on screen wearing it (#418).
 *
 * On the Triage screen the rows stay mounted in a list, so a result that lands
 * after the reader collapses one still has its row to render into. In Now the
 * row **is** the slot above the columns: closing the slot unmounts the editor
 * outright, and a failure arriving afterwards was displayed nowhere at all —
 * the item returned to the board (correctly: a failed write leaves it exactly
 * where it was) carrying no sign anything had gone wrong.
 *
 * So the screen states it instead, and names the item, because "a write
 * failed" without saying which is not worth the line. `openItemId` is
 * whichever item the *editor that owns this kind of failure* currently holds —
 * the caller passes `null` when no such editor is open, so the two surfaces
 * never both speak for one result.
 *
 * The title is read from the live board rather than remembered from the click:
 * a failed write leaves the item where it was, so it is there to be named, and
 * if it is *not* (a `"not_found"`, or an item that has since left) the honest
 * sentence is the un-named one rather than a stale title.
 */
function strandedFailure(
  result: ItemWriteResult | null | undefined,
  openItemId: string | null,
  items: readonly TaskItemDTO[],
  subject: string,
  fallback: string,
): string | null {
  if (!result || result.kind === "ok" || result.itemId === openItemId) {
    return null;
  }
  const title = items.find((item) => item.id === result.itemId)?.title;
  if (title === undefined) {
    return result.error ?? fallback;
  }
  // Em dash for the honest aside, the house punctuation for exactly this —
  // the sentence states what is true, then hands over the server's own words.
  return result.error
    ? `${subject} didn't apply to "${title}" — ${result.error}`
    : `${subject} didn't apply to "${title}".`;
}

const TRIAGE_FALLBACK = "That triage didn't apply.";
const ACT_FALLBACK = "That action didn't apply.";
const GRILL_COMPLETION_FALLBACK = "That Grill didn't apply.";

/** The triage failure belonging to one item, matched by the id the result
 * itself carries. A failure belongs to whichever item it names, never to
 * "whichever row is open". `null` when the last result was an `"ok"`, named a
 * different item, or has not arrived yet. */
export function triageFailureFor(
  result: TaskTriageResult | null | undefined,
  itemId: string,
): string | null {
  return failureFor(result, itemId, TRIAGE_FALLBACK);
}

/** The act failure belonging to one item — `lastAct`'s twin of
 * [`triageFailureFor`], and the same contract. This is what `ItemDetailPanel`
 * renders as its `actError`. */
export function actFailureFor(
  result: TaskActResult | null | undefined,
  itemId: string,
): string | null {
  return failureFor(result, itemId, ACT_FALLBACK);
}

/** The Grill-completion failure belonging to one **confirm**, matched by the
 * `seed` that confirm minted — the review card's `triageFailureFor` twin, but
 * scoped one level tighter than by item.
 *
 * By item would be wrong here in a way it is not for triage. `lastGrillCompletion`
 * holds only the most recent result, and a Grill session is *re-openable on the
 * same item*: a confirm refused with `"needs_re_review"` leaves the item exactly
 * where it was, in Triage, so the next thing the reader does is grill it again —
 * and an item-matched read would greet that fresh interview with the previous
 * session's failure, attached to a proposal it does not describe. The seed names
 * one request, so a stale result simply does not match. `seed` is `null` when
 * this session has no confirm outstanding, which no result can match either.
 *
 * `"needs_re_review"` and every other non-`"ok"` kind read the same: the
 * server's own words, or the fallback sentence. */
export function grillCompletionFailureFor(
  result: TaskGrillCompletionResult | null | undefined,
  seed: string | null,
): string | null {
  if (!result || seed === null || result.seed !== seed || result.kind === "ok") {
    return null;
  }
  return result.error ?? GRILL_COMPLETION_FALLBACK;
}

/** A triage failure with no editor left to wear it — see [`strandedFailure`].
 *
 * `openEditorItemId` is whichever item has an editor open on it, and since
 * item detail gained an Edit mode that is **either** of the two things Now's
 * slot can hold: a capture in `TriageRow`, or a minted item in `ItemPanel`,
 * both of which render this failure themselves. Passing only the capture
 * would say a failed edit twice, once in the panel and once above the
 * columns.
 *
 * `inbox` stays the inbox: it is only consulted to *name* the item a stranded
 * failure belongs to, and a minted item's failure is never stranded — its
 * panel is the thing that was open when the edit was sent. */
export function strandedTriageFailure(
  result: TaskTriageResult | null | undefined,
  openEditorItemId: string | null,
  inbox: readonly TaskItemDTO[],
): string | null {
  return strandedFailure(result, openEditorItemId, inbox, "Triage", TRIAGE_FALLBACK);
}

/** An act failure with no `ItemPanel` left to wear it — the twin bug,
 * on the other mutation.
 *
 * `openItemId` is the item whose **detail panel** is open, which is not the
 * same as "the selected item": Now's slot holds either an `ItemPanel` or
 * a `TriageRow`, and a capture in that slot still acts (its one-click
 * checkmark completes the item, `canMarkDone`). A failed act on the open
 * *capture* therefore has no `actError` anywhere — `TriageRow` renders triage
 * failures, not act ones — so the caller passes `null` there and this line
 * speaks for it.
 *
 * `items` is every item that could be named: the frontier and blocked lists
 * plus the triage inbox, for exactly that reason.
 */
export function strandedActFailure(
  result: TaskActResult | null | undefined,
  openItemId: string | null,
  items: readonly TaskItemDTO[],
): string | null {
  return strandedFailure(result, openItemId, items, "That action", ACT_FALLBACK);
}
