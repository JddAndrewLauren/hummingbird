import {
  parseWasteBodyFromCore,
  wasteAnswerFromCore,
  wasteFactsFromCore,
  wasteSetupFromCore,
  wasteZoneQueries,
  type PaneInputsSource,
  type WasteFacts,
  type WasteGap,
  type ZoneFacts,
} from "../../decisions/seam";
import type { FreshnessDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";
import { isStaleFreshness } from "../questions/freshness";
import { resolveZoneFacts } from "../questions/zone-bridge";

// **The which-cans question** (#120), answered over #245's pane shell — and
// since #533/M4, **the web's rendering half of it only**.
//
// Every rule this file used to hold is now
// `hummingbird_core::decisions::panes::waste`: the payload parser, the
// setup arm, days-away/holiday/stale, the band and its 26h threshold, and
// the gap kinds. Read that module for the reasoning behind any of them —
// the prototype verdicts it carries (one collection day, a holiday IS the
// answer, the pane is furniture most of the week) are stated there once.
//
// What stayed here is what ADR-0025 leaves per-client: **the words and the
// bins**. `wasteHeadline`, `wasteCollapsedHeadline`, `wasteGapReason`,
// `BIN`/`wasteGlyphs` and the weekday word. Two clients disagreeing about
// the band would be a bug; two clients wording "Trash tonight" differently
// is a design choice.
//
// **Every export below keeps its name and signature.** The pane's own
// components, `question.ts` and `registry.ts` are untouched by the sink,
// which is what makes this a probe slice rather than a rewrite: only the
// bodies moved.
//
// **Self-resolving per call.** Each entry point runs the whole bridge —
// core names its `(zone, civil-date)` queries, `zone-bridge.ts` resolves
// them with `Intl`, core decides — rather than taking a resolved table as
// an argument. That is deliberate for this slice: threading facts through
// `QuestionDef.answer` would change `contract.ts`, `registry.ts`,
// `RankedRegion.tsx` and all seven other panes. The batched, surface-level
// path (`rankPanesFromCore`) exists in the core and is exercised by tests;
// whether the web should hoist onto it is #533's stated ergonomics
// question.

// **The four constants below stay literal TS**, deliberately, and are
// pinned against `waste_constants_json()` by `seam.test.ts` rather than
// read through the seam at runtime. `question.ts` builds `sources:
// [SOURCE]` at module evaluation and `registry.ts` imports it statically,
// so a seam call here would run before `initDecisions()` resolves and throw
// the seam's "used before ready" guard on every page load. This is exactly
// `field-vocabulary.ts`'s existing arrangement (#500's PR records the
// constraint) — and a *fallback* is not the alternative: `seam.ts`'s "not
// ready is a throw, never a fallback" rule means the choice is a pinned
// literal or nothing.

/** Both the snapshot's source and every alert's — ADR-0009's join
 * constraint is that these are one string. `/v2` because `city-waste/v1` is
 * retired. `hummingbird_core::decisions::panes::waste::SOURCE` is the
 * authority; this is the pinned copy. */
export const SOURCE = "city-waste/v2";

/** The one `context_snapshots.key` this question reads, and its subject key. */
export const SNAPSHOT_KEY = "collection";

/** The binding that has to be set before this question can be asked at all. */
export const BINDING_KEY = "city-waste-page";

/** How old an answer may be before it is worth saying so — 26 hours, and
 * `waste.rs` states why it is not `2 ×` the daily cadence. */
export const STALE_AFTER_MS = 26 * 60 * 60 * 1000;

export type Stream = "trash" | "recycling" | "yard";

/** Kerb order — pinned the same way. */
export const STREAM_ORDER: readonly Stream[] = ["trash", "recycling", "yard"];

/** The bins' own colours, as they are on the kerb: grey, light blue, green.
 *
 * A deliberate departure from the design system's "colour always encodes
 * status, never decoration" rule, documented here at its point of use:
 * these encode **object identity** — the one thing on this pane a person
 * matches against the real world before walking outside. Literal hex rather
 * than brand tokens for the same reason: the bins are not part of the
 * palette, and both values are picked to hold up on the sand and the ink
 * surfaces without a per-theme variant. Every glyph still carries a label,
 * because colour alone is not a label to a screen reader.
 *
 * Stayed in TS by ADR-0025's verdict table: a second client rendering these
 * differently is a design choice, not a bug. */
export const BIN: Record<Stream, { fill: string; edge: string; label: string }> = {
  trash: { fill: "#9aa3ab73", edge: "#79838b", label: "trash" },
  recycling: { fill: "#7fc4e873", edge: "#3f93c4", label: "recycling" },
  yard: { fill: "#6aa84f73", edge: "#4d8a3a", label: "yard" },
};

/** The day names, indexed by `WasteFacts.weekdayIndex` (`0` = Sunday).
 *
 * The *word* is per-client and the *day* is the core's: reading it off the
 * index rather than re-deriving it with `Intl` is what stops the sentence
 * and the decision disagreeing about which day it is.
 * `zoned-day.ts`'s `weekdayInZone` remains the resolver module's own
 * export — it is no longer this pane's source of the day. */
const WEEKDAYS: readonly string[] = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
];

/** The `city-waste/v2` payload body. The shape is pinned by
 * `waste.rs`'s `parse_waste_body`; this is its wire form. */
export interface WasteBody {
  zone: string;
  scheduled: string;
  collectedOn: string;
  streams: Stream[];
}

/** A body that could be read, or the reason it could not — the "gap, not
 * absence" split in one type. `reason` is words a pane can render, composed
 * here from the core's gap **kind**. */
export type WasteParse =
  | { kind: "ok"; body: WasteBody }
  | { kind: "gap"; reason: string };

/** Reads one snapshot row into a body, or says why it could not —
 * `waste.rs`'s `parse_waste_body` with this client's wording put back on.
 *
 * **Shape only, since #533.** An unusable time zone used to be refused
 * here, by calling `zonedMidnightMs`; the core cannot, so that refusal
 * moved to [`wasteView`]/[`wasteGapReason`], where the bridge has come back
 * without the fact. A payload naming `Mars/Olympus_Mons` therefore parses
 * and then has no answer, rather than failing to parse. */
export function parseWasteBody(snapshot: PaneSnapshotDTO | undefined): WasteParse {
  const parsed = parseWasteBodyFromCore(snapshot);
  return parsed.kind === "ok"
    ? { kind: "ok", body: parsed.body as WasteBody }
    : { kind: "gap", reason: gapReason(parsed.gap) };
}

/** Whether an answer is old enough to say so, against this pane's own
 * threshold — re-exported from the shell (`questions/freshness.ts`) since
 * #119's race pane needs the identical reading at its own 12h threshold. */
export { isStaleFreshness };

/** Kerb order, never the order the payload happened to list. */
export function orderedStreams(streams: readonly Stream[]): Stream[] {
  return STREAM_ORDER.filter((stream) => streams.includes(stream));
}

/** One dot per bin going out, in kerb order — the collapsed row's whole
 * content besides its sentence. */
export function wasteGlyphs(streams: readonly Stream[]): PaneGlyph[] {
  return orderedStreams(streams).map((stream) => ({
    kind: "dot" as const,
    fill: BIN[stream].fill,
    edge: BIN[stream].edge,
    label: BIN[stream].label,
  }));
}

/** The words for an answered pane's expanded rendering. Deliberately tiny:
 * the coloured bins already say *which* cans, so the sentence only has to
 * say *when*.
 *
 * A holiday names its actual day even when that day is tomorrow — on the
 * one week the day is unusual, "Tonight" would hide the very thing that
 * changed.
 *
 * "Today" is `daysAway === 0` and **never `<= 0`**: a negative distance is
 * a collection that has already happened, which the core refuses outright
 * (`WasteGap::PastCollection`) rather than describe. Written as equality
 * anyway, because `<=` here is the exact line that rendered yesterday's
 * collection as "Trash Today" for every hour between the address's midnight
 * and the next daily poll. */
export function wasteHeadline(daysAway: number, weekday: string, holiday: boolean): string {
  if (daysAway === 0) {
    return "Trash Today";
  }
  if (holiday) {
    return `Trash ${weekday}`;
  }
  if (daysAway === 1) {
    return "Trash Tonight";
  }
  return `Trash ${weekday}`;
}

/** The one-line collapsed form, which is also how a dormant pane renders —
 * there is no separate dormant card, because most of the week this is
 * furniture and furniture that takes a card is competing with the
 * frontier. */
export function wasteCollapsedHeadline(
  daysAway: number,
  weekday: string,
  holiday: boolean,
): string {
  if (daysAway === 0) {
    return "Trash today";
  }
  if (daysAway === 1 && !holiday) {
    return "Trash tonight";
  }
  return `${weekday} · ${daysAway}d`;
}

/** Everything an answered pane needs, read by both the answer and the
 * expanded rendering — `WasteFacts` (`waste.rs`) with this client's weekday
 * word attached. `null` when this question has no answer. */
export interface WasteView {
  body: WasteBody;
  today: string;
  /** Whole days from today at the address to the collection. **Never
   * negative** — a collection already in the past is not an answer this
   * pane will render, and the core refuses it. */
  daysAway: number;
  /** The city moved this week's collection off its cadence day. Read
   * straight off the snapshot, never from the alert lane. */
  holiday: boolean;
  weekday: string;
  stale: boolean;
  freshness: FreshnessDTO;
}

/** Whether the question has been asked at all — **four answers, not a
 * boolean**, for the same reason `BindingValueDTO` is not `string | null`.
 * `waste.rs`'s `WasteSetup` verbatim. */
export type WasteSetup =
  | { kind: "bound"; page: string }
  | { kind: "unread" }
  | { kind: "unusable" }
  | { kind: "unset" };

export function wasteSetup(inputs: QuestionInputs): WasteSetup {
  return wasteSetupFromCore(paneInputs(inputs));
}

/** The three fields the core's rules read, out of the shell's whole
 * `QuestionInputs`. */
function paneInputs(inputs: QuestionInputs): PaneInputsSource {
  return { nowMs: inputs.nowMs, bindings: inputs.bindings, paneReads: inputs.paneReads };
}

/** The whole bridge, for one call: the core names its queries, `Intl`
 * resolves what it can, and the core decides against the result. */
function resolve(inputs: QuestionInputs): { source: PaneInputsSource; facts: ZoneFacts } {
  const source = paneInputs(inputs);
  return { source, facts: resolveZoneFacts(wasteZoneQueries(source)) };
}

function toView(facts: WasteFacts): WasteView {
  return {
    body: {
      zone: facts.zone,
      scheduled: facts.scheduled,
      collectedOn: facts.collectedOn,
      streams: facts.streams,
    },
    today: facts.today,
    daysAway: facts.daysAway,
    holiday: facts.holiday,
    weekday: WEEKDAYS[facts.weekdayIndex],
    stale: facts.stale,
    freshness: facts.freshness,
  };
}

/** The whole answered view, or `null` when there is nothing to answer with
 * (no snapshot, a body that could not be read, a zone this build cannot
 * resolve, a collection already past). The caller turns `null` into the
 * right gap — see [`wasteGapReason`] for its words. */
export function wasteView(inputs: QuestionInputs): WasteView | null {
  const { source, facts } = resolve(inputs);
  const resolved = wasteFactsFromCore(source, facts);
  return resolved.kind === "facts" ? toView(resolved) : null;
}

/** Why this pane has no answer, in words — read only when [`wasteView`]
 * returned `null`.
 *
 * **The one place a gap kind becomes a sentence.** Every arm below is this
 * client's wording for one `WasteGap` variant; a second client writes its
 * own, and neither is a bug. The only text that arrives from the core is
 * `malformed`'s `reason`, which is the domain's own `EnvelopeProblem`
 * wording rather than a sentence composed there. */
export function wasteGapReason(inputs: QuestionInputs): string {
  const { source, facts } = resolve(inputs);
  const resolved = wasteFactsFromCore(source, facts);
  return resolved.kind === "gap" ? gapReason(resolved.gap) : UNRESOLVABLE_DAY;
}

/** A gap kind this build has never heard of — a newer core with a new arm.
 * Named rather than inlined so it reads as the deliberate catch-all it is
 * (the same "visibly broken, never quietly empty" rule as everywhere else),
 * and so nothing renders an empty string. */
const UNRESOLVABLE_DAY = "The collection schedule couldn't be resolved to a day.";

function gapReason(gap: WasteGap): string {
  switch (gap.gap) {
    case "notFetched":
      return "No collection schedule has been fetched yet.";
    case "malformed":
      return `The collection payload couldn't be read: ${gap.reason}`;
    case "unknownSchema":
      return `This device doesn't know how to read ${gap.schema} yet. Update the app.`;
    case "notJson":
      return "The collection payload isn't JSON.";
    case "notAnObject":
      return "The collection payload isn't an object.";
    case "noZone":
      return "The collection payload names no time zone.";
    case "badDates":
      return "The collection payload's dates aren't whole days.";
    case "unknownStream":
      return "The collection payload lists an unknown kind of bin.";
    case "unresolvableZone":
      return `The collection payload names an unknown time zone (${gap.zone}).`;
    case "pastCollection":
      return `The collection schedule is out of date: it still names ${WEEKDAYS[gap.weekdayIndex]} ${gap.collectedOn}, which has passed.`;
    default:
      return UNRESOLVABLE_DAY;
  }
}

/** This question's answer for the shell (#245).
 *
 * The three decided fields come from `waste.rs`'s `waste_answer`; the
 * headline and the glyphs are composed here, which is exactly the cut
 * ADR-0025 draws through `PaneAnswer`. */
export function wasteAnswer(inputs: QuestionInputs): PaneAnswer {
  const { source, facts } = resolve(inputs);
  const answer = wasteAnswerFromCore(source, facts);

  if (answer.answerState === "unbound") {
    return {
      ...answer,
      collapsedHeadline: "Not set up",
      icon: [{ kind: "icon", name: "help-circle", label: "not set up" }],
    };
  }
  if (answer.answerState === "bound-but-unacquired") {
    // Which gap it is decides the words: the table has not been read yet,
    // it holds something this pane cannot use, or it is set and there is
    // simply no answer. Saying "Not set up" for any of them would claim a
    // fact about the reader's configuration this device has not
    // established.
    const setup = wasteSetupFromCore(source);
    if (setup.kind === "unread") {
      return {
        ...answer,
        collapsedHeadline: "Checking setup",
        icon: [{ kind: "icon", name: "cloud-fog", label: "checking setup" }],
      };
    }
    if (setup.kind === "unusable") {
      return {
        ...answer,
        collapsedHeadline: "Setup needs a look",
        icon: [{ kind: "icon", name: "help-circle", label: "setup needs a look" }],
      };
    }
    return {
      ...answer,
      collapsedHeadline: "No answer yet",
      icon: [{ kind: "icon", name: "cloud-fog", label: "no answer yet" }],
    };
  }

  const resolved = wasteFactsFromCore(source, facts);
  if (resolved.kind !== "facts") {
    // Unreachable: the core answers `answered` only when it produced facts.
    return { ...answer, collapsedHeadline: "No answer yet" };
  }
  const view = toView(resolved);
  return {
    ...answer,
    collapsedHeadline: wasteCollapsedHeadline(view.daysAway, view.weekday, view.holiday),
    icon: wasteGlyphs(view.body.streams),
  };
}
