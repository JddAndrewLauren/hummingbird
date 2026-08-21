import type { ComponentType } from "react";
import type { IconName } from "../../components/core/Icon";
import type { BindingDTO, CalendarReadDTO, PaneReadDTO, TaskItemDTO } from "../../store/protocol";
import type { TaskSyncOutcome } from "../../store/store";

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

/** ADR-0017's second axis: which ranked region a question renders into.
 * `NowScreen`'s aside ("now") or the Status screen ("status") — a *declared*
 * value, not a set. A pane wanting both surfaces is the recorded tripwire
 * (ADR-0017 decision 1), not something this contract accommodates today: it
 * would need its own decision about how it sorts and collapses on each
 * surface, which is exactly the kind of quiet expansion a closed union is
 * meant to force into the open. */
export type Surface = "now" | "status";

/** Every standing question this build can render. A closed vocabulary, on
 * the same reasoning as `BindingKey` in Rust: the registry is what makes
 * "every question, in a declared order" a fact the type system checks, not a
 * list someone has to remember to update. */
export type StandingQuestion =
  | "homework"
  | "waste"
  | "weekend"
  | "vacation"
  | "race"
  | "kimi"
  | "github"
  | "uptime"
  | "reachability";

/** Declared display order — the last axis of the cross-pane sort, and the
 * order the wiring unions its sources in. Declaration order, not
 * alphabetical, so a question's place does not move when another is
 * renamed. The four infra questions (ADR-0017, #311) are declared after
 * Now's own so neither surface's existing order moves when the other
 * surface's questions are filtered in or out.
 *
 * `homework` is declared **first** (#675): it is the only question about
 * work the reader personally owes, so when two panes tie on band and
 * `withinBand` it is the one that should be read first. Pinned against
 * `hummingbird_core::decisions::panes::contract::QUESTION_ORDER` by
 * `seam.test.ts`, which is what stops the two lists drifting. */
export const QUESTION_ORDER: readonly StandingQuestion[] = [
  "homework",
  "waste",
  "weekend",
  "vacation",
  "race",
  "kimi",
  "github",
  "uptime",
  "reachability",
];

/** Everything a question needs to answer, and nothing else: the bindings
 * table, whatever pane reads have landed, and the clock.
 *
 * A plain value on purpose — no hooks, no store access, no fetching. That is
 * what lets the board world's seed (`fixtures/demo-task-state.ts`, built from
 * `fixtures/demo-pane-reads.ts`) hand the *real* region a hand-authored world
 * and photograph the real shell, rather than a second demo-only rendering
 * that can drift from it. (`fixtures/demo-questions.ts` did this before #452
 * folded its content into the seed and #455 deleted the module.) */
export interface QuestionSyncSnapshot {
  latestOutcome: TaskSyncOutcome | null;
  latestInformativeAtMs: number | null;
  lastSuccessfulAtMs: number | null;
}

/** The honest pre-sync snapshot, shared by fixtures that are not about the
 * sync lane. Kept in the contract so every `QuestionInputs` remains
 * structurally complete rather than making sync optional. */
export const EMPTY_QUESTION_SYNC: QuestionSyncSnapshot = {
  latestOutcome: null,
  latestInformativeAtMs: null,
  lastSuccessfulAtMs: null,
};

export interface QuestionInputs {
  /** The device's latest authority-sync facts. Questions receive the same
   * immutable snapshot shape in real and demo worlds; they never sample a
   * clock, read storage, or reach into the store themselves. */
  sync: QuestionSyncSnapshot;
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
  /** Issue #122 review fix: whether this device has ever connected a
   * calendar at all (`CalendarState.connected`, `store/store.ts`) —
   * distinct from whether a *snapshot* has landed. A calendar-lane
   * question's `not_read` read state is the core's "no snapshot at all",
   * which also covers a connected-but-unpolled device, an offline one, or
   * one sitting on `needsReconnect` — none of those are "never set up".
   * Only `!calendarConnected` may render the setup prompt; `not_read` while
   * connected is `bound-but-unacquired`, the same "the table hasn't
   * answered yet" reading `waste.ts`'s unread pane read gives. */
  calendarConnected: boolean;
  /** Every actionable item this device currently knows about — `task.frontier`
   * and `task.blocked`'s items, unioned (#122). The weekend-plans pane is
   * the first question to read items directly rather than through a
   * snapshot lane: unlike the pane-read/calendar arms it needs no separate
   * "not read yet" state, because `frontier`/`blocked` already answer that
   * (an empty list before the first sync and a genuinely empty mirror are
   * indistinguishable here on purpose — the same steady state `frontier`
   * itself renders while offline on a fresh device). Never filtered or
   * re-derived by a question; each one reads only the fields it needs
   * (`deadline`/`scheduledDate`/`stage`) off the same list `NowScreen`
   * already renders. */
  items: TaskItemDTO[];
  nowMs: number;
}

/** One `getCalendarEvents` request a question declares it needs — #267's
 * calendar arm's own request shape, named here (rather than in `registry.ts`,
 * which only unions these) because `QuestionDef.calendarRequests` has to
 * reference it too. */
export interface CalendarEventsRequest {
  /** The caller's own identity for this request — becomes the
   * `QuestionInputs.calendarReads` map key. */
  key: string;
  startMs: number;
  endMs: number;
  /** The same window in civil dates (`YYYY-MM-DD`, exclusive end),
   * resolved in the device's own zone — the arm all-day events are asked
   * about, since they carry no instant. A question computes both halves
   * itself: the core owns no tzdb and derives neither from the other. */
  startDate: string;
  endDate: string;
}

/** One question's whole implementation, as the shell sees it. */
export interface QuestionDef {
  /** The question itself, in the reader's words — "Which cans go out". The
   * shell draws it (on the collapsed row and above the expanded content), so
   * every pane in the region is named in the same type at the same place;
   * `collapsedHeadline` is the *answer*, and a pane that put its own
   * question in there would say it twice. */
  label: string;
  /** ADR-0017's surface axis: which ranked region (`NowScreen`'s aside or
   * the Status screen) this question renders into. Read by `registry.ts`'s
   * `rankPanes`/`requiredSources` to filter `QUESTION_ORDER` per view — the
   * sort, the band vocabulary and the collapse rule downstream of that
   * filter are entirely unaware a second surface exists. */
  surface: Surface;
  /** Which `context_snapshots` sources the wiring must request a pane read
   * for. Empty for a question that reads no snapshot lane at all (the
   * calendar-lane questions, #117/#121/#122, and client-only questions such
   * as device reachability, #316). */
  sources: readonly string[];
  /** Which subjects this question currently has — 0..N, so one question can
   * answer for several things (several bins, several race series).
   *
   * An **unbound** question still returns one sentinel subject: an unbound
   * pane must render its setup prompt, and a question that vanished when
   * nobody had bound it would be a question nobody could ever discover.
   *
   * **The same rule extends to the never-polled case (ADR-0017, #311).** A
   * question whose subjects have never been acquired — no poller has run
   * yet, so there is no row in `context_snapshots` and (unlike the waste
   * pane) no binding to be unset either — must still return one sentinel
   * subject, answered `bound-but-unacquired`. A platform with no rows yet
   * renders as a gap, never as nothing: a question that renders nothing is
   * the quietly-empty answer ADR-0015 rules out, and it is exactly the state
   * every infra question is in until #313-#316 wire its poller. */
  subjects(inputs: QuestionInputs): string[];
  answer(subjectKey: string, inputs: QuestionInputs): PaneAnswer;
  /** Every #267 calendar-arm interval this question needs, given the clock
   * — `undefined` for a question that reads no calendar lane at all (the
   * waste and race panes; both calendar-lane questions declare one). A function
   * of `nowMs`
   * rather than a fixed interval because a window like #122's rolls
   * forward as the clock advances; `registry.ts`'s `requiredCalendarRequests`
   * is the union of every registered question's answer here, exactly the
   * way `requiredSources()` unions `sources`. */
  calendarRequests?(nowMs: number): CalendarEventsRequest[];
  /** The pane's own expanded rendering. Rendered with the **live** inputs on
   * every render — only position and band chrome come from the shell's
   * sample, so an expanded pane is never a frame behind what it knows. */
  Expanded: ComponentType<{
    subjectKey: string;
    inputs: QuestionInputs;
    onSetupNavigate?: () => void;
    /** #122: set (a day) or clear (`null`) an item's do-date, through the
     * triage mutation entry point (`Core::triage`, `destination: None`).
     * `undefined` for every pane with no write affordance — every one but
     * the weekend-plans pane, today. Threaded the same way
     * `onSetupNavigate` is: a plain callback prop, never something read out
     * of `QuestionInputs`, which stays a plain, writable-nothing value. */
    onSetScheduledDate?: (itemId: string, date: string | null) => void;
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
