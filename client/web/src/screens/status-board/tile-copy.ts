import type { Band, PaneAnswer } from "../questions/contract";

// The Status board's compact form: how one already-decided answer becomes a
// tile's two lines, and which of four treatments it gets.
//
// Nothing here writes words. `collapsedHeadline` is the pane's own sentence
// (ADR-0015: the pane owns the words, the shell owns the chrome), and every
// pane on this surface happens to write it as `subject · what happened` —
// `authority · 401 as expected`, `gmail-poll · healthy`,
// `runner · unreachable — connect timeout`. A tile is that sentence set in
// two types instead of one, so `tileParts` splits it and rewrites neither
// half. A headline with no separator (the reachability pane's
// `Synced 12m ago`, every gap sentence) keeps the question's own label as
// its name and the whole sentence as its fact.
//
// Splitting on the *first* separator matters: an uptime error can carry its
// own ` · `, and the subject is always the head.

export interface TileParts {
  /** The tile's bold line — the subject, or the question's label. */
  name: string;
  /** The tile's mono line: what this pane currently says. */
  fact: string;
}

const SEPARATOR = " · ";

export function tileParts(label: string, collapsedHeadline: string): TileParts {
  const at = collapsedHeadline.indexOf(SEPARATOR);
  if (at === -1) return { name: label, fact: collapsedHeadline };
  const name = collapsedHeadline.slice(0, at).trim();
  const fact = collapsedHeadline.slice(at + SEPARATOR.length);
  // An empty head is reachable from real data: both separator-bearing
  // families interpolate a server-supplied string there (a workflow's
  // `display_name`, a probe's `serviceId`), and an absent one would leave a
  // blank bold line and an accessible name reading "GitHub workflows —  ·
  // never run". The question's own label is always a true thing to say.
  if (name === "")
    return { name: label, fact: fact === "" ? collapsedHeadline : fact };
  return { name, fact };
}

/** A tile's treatment. Four arms rather than the handoff's healthy/problem
 * two, because `answerState` already draws a third distinction the board
 * must not flatten: **a gap is not "as expected"**. A pane that has never
 * been polled gets no green dot — it gets its gap sentence and a muted
 * fact, which is the honest reading (`contract.ts`'s `AnswerState`, and
 * ADR-0015's rule against the quietly-empty answer). */
export type TileTone = "quiet" | "warn" | "danger" | "gap";

/** The band→treatment mapping, deliberately the same one
 * `GithubPaneExpanded` and `UptimePaneExpanded` already apply to their own
 * headlines: live/imminent read as danger, near/distant as warn. Keeping it
 * identical is what stops a tile and the body it expands into disagreeing
 * about how bad the same answer is. */
export function bandTone(band: Band): TileTone {
  switch (band) {
    case "live":
    case "imminent":
      return "danger";
    case "near":
    case "distant":
      return "warn";
    case "dormant":
      return "quiet";
  }
}

export function tileTone(answer: PaneAnswer): TileTone {
  return answer.answerState === "answered" ? bandTone(answer.band) : "gap";
}

/** The band, as the expanded tile's mono right-hand word. The band
 * vocabulary itself (`contract.ts`'s `Band`), not a synonym for it. */
export function bandWord(band: Band): string {
  return `band:${band}`;
}

/** A group label's tail — `5 subjects`, `1 subject`. The count is of panes
 * actually ranked into that group, never a fixed number: the board draws
 * what `rankPanes` returned. */
export function subjectCount(panes: number): string {
  return `${panes} ${panes === 1 ? "subject" : "subjects"}`;
}
