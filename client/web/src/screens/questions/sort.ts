import { BAND_ORDER, type AnswerState, type RankedPane } from "./contract";

// ADR-0015's cross-pane sort (#245) — the shell's whole opinion about which
// standing question the eye should reach first. Mirrors
// `frontier-order.ts`'s discipline exactly: pure, clock-free, copies its
// input, and total (no two distinct panes ever compare equal), so ranking
// the same snapshot twice is byte-identical.
//
// Five axes, in this order:
//
//   1. answerState — answered first, then a gap, then unbound. An answer
//      beats a promise: a pane that knows something is more use than one
//      that is waiting on data, and a question nobody has bound yet is
//      setup, not context, so it settles at the bottom.
//   2. band — ADR-0015's salience vocabulary, live -> dormant.
//   3. withinBand — soonest first inside one band; `null` after every
//      non-null (`compareDeadlines`' own shape in `frontier-order.ts`).
//   4. declared question order — the registry's, not alphabetical.
//   5. subject key — the last resort, so the order is total.

const ANSWER_STATE_ORDER: readonly AnswerState[] = ["answered", "bound-but-unacquired", "unbound"];

function rankIn<T>(order: readonly T[], value: T): number {
  const index = order.indexOf(value);
  // An unrecognised value sorts last rather than first. It cannot happen
  // through the type system, but the alternative default (-1, i.e. first)
  // would promote a broken pane to the top of the region.
  return index === -1 ? order.length : index;
}

/** `null` sorts after every real number — "nothing to order by" is not
 * "infinitely soon". */
function compareWithinBand(a: number | null, b: number | null): number {
  if (a !== null && b !== null) {
    return a - b;
  }
  if (a !== null) {
    return -1;
  }
  if (b !== null) {
    return 1;
  }
  return 0;
}

/** Orders the ranked panes for display. Never mutates `panes`. */
export function orderPanes(
  panes: readonly RankedPane[],
  questionOrder: readonly string[],
): RankedPane[] {
  return [...panes].sort((a, b) => {
    const byAnswerState =
      rankIn(ANSWER_STATE_ORDER, a.answer.answerState) -
      rankIn(ANSWER_STATE_ORDER, b.answer.answerState);
    if (byAnswerState !== 0) {
      return byAnswerState;
    }
    const byBand = rankIn(BAND_ORDER, a.answer.band) - rankIn(BAND_ORDER, b.answer.band);
    if (byBand !== 0) {
      return byBand;
    }
    const byWithinBand = compareWithinBand(a.answer.withinBand, b.answer.withinBand);
    if (byWithinBand !== 0) {
      return byWithinBand;
    }
    const byQuestion = rankIn(questionOrder, a.question) - rankIn(questionOrder, b.question);
    if (byQuestion !== 0) {
      return byQuestion;
    }
    return a.subjectKey < b.subjectKey ? -1 : a.subjectKey > b.subjectKey ? 1 : 0;
  });
}

/** Whether two ranked lists describe the same set of panes in the same
 * answer states — the one comparison `RankedRegion` re-samples its captured
 * order on, beyond a sync cycle.
 *
 * Deliberately **not** a full equality: band and `withinBand` move on their
 * own with the clock, and re-sorting on those would be exactly the
 * under-the-cursor reordering ADR-0015 captures the order to prevent. A pane
 * appearing, disappearing, or flipping between an answer and a gap is a
 * different kind of change — the region is showing something that is no
 * longer true — and is worth a re-sort at once. */
export function samePaneIdentity(a: readonly RankedPane[], b: readonly RankedPane[]): boolean {
  if (a.length !== b.length) {
    return false;
  }
  const byKey = new Map(a.map((pane) => [pane.paneKey, pane.answer.answerState]));
  return b.every((pane) => byKey.get(pane.paneKey) === pane.answer.answerState);
}
