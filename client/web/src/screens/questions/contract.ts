import type { ComponentType } from "react";
import type { IconName } from "../../components/core/Icon";
import type { BindingDTO, CalendarReadDTO, PaneReadDTO } from "../../store/protocol";

// ADR-0015's **pane shell contract** (#245): the one thing every standing
// question answers, and the one shape the shell knows how to rank, collapse
// and draw.
//
// The split this file exists to hold: a question owns its *answer* — what
// state it is in, how soon it matters, the sentence it collapses to, its
// glyphs, and its whole expanded rendering. The shell owns everything
// *between* panes — the cross-pane order, the collapsed row's chrome, the
// device-local collapse state, the setup prompt an unbound question gets.
// Neither reaches into the other: the shell never parses a payload, and no
// pane draws its own collapsed form or decides its own position.
//
// Everything here is pure and clock-free by construction. The clock arrives
// as `QuestionInputs.nowMs`, exactly as it does in Rust — see
// `client/core/src/pane.rs` for the other half of this seam.

/** Whether this question has an answer at all, and if not, why not.
 *
 * Three states rather than "answered or not", for the same reason
 * `BindingValueDTO` is not `string | null`: **a gap is not an absence**. A
 * question whose binding is set but whose data has not landed (never polled,
 * a payload this build cannot read, a source it has not heard of) is
 * something the reader can act on — wait, or look at Settings — while a
 * question nobody has bound yet is a question that was never asked. Rendering
 * both as "nothing to show" is the quietly-empty answer ADR-0015 rules out. */
export type AnswerState = "answered" | "bound-but-unacquired" | "unbound";

/** ADR-0015's five-band salience vocabulary — how soon this answer matters,
 * declared by the pane and read by the shell for both the cross-pane sort and
 * the default collapse.
 *
 * Deliberately a closed word list rather than a number: the bands are what
 * make two questions with entirely unrelated units — a race in 3 days, a bin
 * collection tonight, a trip in 6 weeks — comparable at all. A pane that
 * needs finer resolution than a band expresses it in `withinBand` — an
 * instant, not a unit of its own — which only ever breaks ties *inside* one
 * band and can never move a pane across one. */
export type Band = "live" | "imminent" | "near" | "distant" | "dormant";

/** Bands in salience order, most pressing first. The sort reads this; so
 * does `collapse.ts`'s default rule. */
export const BAND_ORDER: readonly Band[] = ["live", "imminent", "near", "distant", "dormant"];

/** One small mark a pane puts on its collapsed row — a coloured dot (a
 * kerbside bin, a session colour) or a named icon.
 *
 * `label` is mandatory on both arms and is not decoration: a glyph carries
 * meaning colour alone cannot convey to a screen reader, and a dot with no
 * accessible name is a blank box. The shell renders these; the pane only
 * says which. */
export type PaneGlyph =
  | { kind: "dot"; fill: string; edge: string; label: string }
  | { kind: "icon"; name: IconName; label: string };

/** How many glyphs a collapsed row may carry. A cap rather than a scroll or
 * a wrap: the collapsed row is one line of furniture, and a pane that
 * produced a dozen glyphs would silently reflow every other pane's row. */
export const MAX_GLYPHS = 4;

/** The shell's own bound on a pane's glyphs — applied by the shell, never
 * trusted to the pane, since the cap exists to protect the row from the pane
 * rather than to be honoured by it. */
export function boundedGlyphs(glyphs: readonly PaneGlyph[] | undefined): PaneGlyph[] {
  return glyphs ? glyphs.slice(0, MAX_GLYPHS) : [];
}

/** One question's answer about one subject, as the shell reads it. Computed
 * fresh on every rank — nothing here is stored, and nothing here is written
 * back onto a DTO (the same read-time-only rule `urgency.ts` documents). */
export interface PaneAnswer {
  answerState: AnswerState;
  band: Band;
  /** **Epoch ms of this answer's next relevant moment**, device clock —
   * smaller = sooner, and the tie-break *within* a band.
   *
   * An absolute instant, never a duration, and not a unit each pane picks
   * for itself: the sort then reads no clock at all, and a captured value
   * cannot go stale between renders. A pane that subtracted "now" here would
   * be the second place in the app that does that arithmetic, which is
   * exactly the drift `Freshness`'s Rust carve-out exists to stop.
   *
   * `null` is "nothing to order by", and sorts after every non-null — the
   * same reading `frontier-order.ts` gives a missing deadline. A pane that
   * can always produce a real number should: it is what keeps two dormant
   * panes in a stable, meaningful order rather than an alphabetical one. */
  withinBand: number | null;
  /** The whole answer in one line, for the collapsed row. The pane writes
   * the words; the shell owns the chrome and the type they are set in, which
   * is why a pane ships no compact component of its own. */
  collapsedHeadline: string;
  /** Up to [`MAX_GLYPHS`] marks for the collapsed row, bounded by the shell. */
  icon?: PaneGlyph[];
}

/** Every standing question this build can render. A closed vocabulary, on
 * the same reasoning as `BindingKey` in Rust: the registry is what makes
 * "every question, in a declared order" a fact the type system checks, not a
 * list someone has to remember to update. */
export type StandingQuestion = "waste";

/** Declared display order — the last axis of the cross-pane sort, and the
 * order the wiring unions its sources in. Declaration order, not
 * alphabetical, so a question's place does not move when another is
 * renamed. */
export const QUESTION_ORDER: readonly StandingQuestion[] = ["waste"];

/** Everything a question needs to answer, and nothing else: the bindings
 * table, whatever pane reads have landed, and the clock.
 *
 * A plain value on purpose — no hooks, no store access, no fetching. That is
 * what lets the demo fixture (`fixtures/demo-questions.ts`) hand the *real*
 * region a hand-authored world and photograph the real shell, rather than a
 * second demo-only rendering that can drift from it. */
export interface QuestionInputs {
  /** `null` until the first `bindings` answer arrives — "nobody has read the
   * table yet", which is not the same as "the table is empty". */
  bindings: BindingDTO[] | null;
  /** Keyed by source, only what was actually requested (the `stepsByItem`
   * shape). A missing entry is "not read yet", never "no rows". */
  paneReads: Record<string, PaneReadDTO | undefined>;
  /** Issue #267's calendar-reads arm — the core-to-view seam #122's
   * weekend-plans pane (and any future calendar-lane question) reads
   * through, never a second read of its own (the Agent Brief's "do not
   * build a second read"). Keyed by the caller-chosen request `key` (never
   * a source — the calendar mirror has no source vocabulary), same "only
   * what was actually requested" shape as `paneReads`: a missing entry is
   * "not requested yet", and `CalendarReadDTO`'s own `"not_read"` state is
   * the further, core-answered distinction "requested, but this device has
   * never synced its calendar at all". */
  calendarReads: Record<string, CalendarReadDTO | undefined>;
  nowMs: number;
}

/** One question's whole implementation, as the shell sees it. */
export interface QuestionDef {
  /** The question itself, in the reader's words — "Which cans go out". The
   * shell draws it (on the collapsed row and above the expanded content), so
   * every pane in the region is named in the same type at the same place;
   * `collapsedHeadline` is the *answer*, and a pane that put its own
   * question in there would say it twice. */
  label: string;
  /** Which `context_snapshots` sources the wiring must request a pane read
   * for. Empty for a question that reads no snapshot lane at all (the
   * calendar-lane questions, #117/#121/#122). */
  sources: readonly string[];
  /** Which subjects this question currently has — 0..N, so one question can
   * answer for several things (several bins, several race series).
   *
   * An **unbound** question still returns one sentinel subject: an unbound
   * pane must render its setup prompt, and a question that vanished when
   * nobody had bound it would be a question nobody could ever discover. */
  subjects(inputs: QuestionInputs): string[];
  answer(subjectKey: string, inputs: QuestionInputs): PaneAnswer;
  /** The pane's own expanded rendering. Rendered with the **live** inputs on
   * every render — only position and band chrome come from the shell's
   * sample, so an expanded pane is never a frame behind what it knows. */
  Expanded: ComponentType<{
    subjectKey: string;
    inputs: QuestionInputs;
    onSetupNavigate?: () => void;
  }>;
}

/** One pane, ranked: which question, which subject, and the answer that
 * placed it. `paneKey` is the stable identity the collapse map and React
 * key both use. */
export interface RankedPane {
  question: StandingQuestion;
  subjectKey: string;
  paneKey: string;
  answer: PaneAnswer;
}

/** The stable per-pane identity — question and subject, never position. */
export function paneKey(question: StandingQuestion, subjectKey: string): string {
  return `${question}:${subjectKey}`;
}
