import type { FreshnessDTO, PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";
import { isStaleFreshness } from "../questions/freshness";
import {
  civilDaysBetween,
  civilTodayInZone,
  isCivilDate,
  weekdayInZone,
  zonedMidnightMs,
  type CivilDate,
} from "./zoned-day";

// **The which-cans question** (#120), answered over #245's pane shell — and
// the shell's proof, since a shell with no pane is exactly the exported,
// unit-tested, never-wired UI this repo keeps having to reject.
//
// Rewritten from `prototype-waste-pane/`, carrying its settled verdicts (see
// that directory's NOTES.md, deleted with it):
//
//   * there is only ever ONE collection day — everything going out that week
//     goes out together, so a per-stream next date modelled a variation that
//     does not exist;
//   * a holiday is not an interruption laid over the answer, it IS the
//     answer, so it changes the words and there is nothing to acknowledge
//     away;
//   * the pane is furniture most of the week, and awake on the eve, the day
//     itself, and every day of a week whose day has moved.
//
// The one thing the prototype left open and this closes is the time zone:
// the payload body carries an IANA `zone` and every day-shaped question is
// resolved in it (`zoned-day.ts`).

/** Both the snapshot's source and every alert's — ADR-0009's join constraint
 * is that these are one string. `/v2` because `city-waste/v1` is retired.
 *
 * `city-waste/v2` is now in the frozen source registry
 * (`server/domain/src/sources.rs`, #120), so an ingest token can be bound to
 * it and the poller can mint. Nothing on this side checks that registry —
 * ADR-0015 forbids checking a snapshot's `schema` against it — so this
 * constant is this file's own, and the two agree by review, not by import. */
export const SOURCE = "city-waste/v2";

/** The one `context_snapshots.key` this question reads, and its subject key. */
export const SNAPSHOT_KEY = "collection";

/** The binding that has to be set before this question can be asked at all. */
export const BINDING_KEY = "city-waste-page";

/** How old an answer may be before it is worth saying so — beside the band
 * function, where ADR-0015 puts every threshold.
 *
 * **26 hours, not `2 ×` the daily cadence.** The driver is the cost of a
 * wrong answer, not the poll interval: a 47-hour-old waste answer can be a
 * whole collection cycle out of date and would render "Trash Tonight" on the
 * wrong night, which is worse than saying nothing. A slightly-late daily poll
 * (25h) is routine and must stay quiet, so the line sits just past a day. */
export const STALE_AFTER_MS = 26 * 60 * 60 * 1000;

export type Stream = "trash" | "recycling" | "yard";

export const STREAM_ORDER: readonly Stream[] = ["trash", "recycling", "yard"];

/** The bins' own colours, as they are on the kerb: grey, light blue, green.
 *
 * A deliberate departure from the design system's "colour always encodes
 * status, never decoration" rule, documented here at its point of use: these
 * encode **object identity** — the one thing on this pane a person matches
 * against the real world before walking outside. Literal hex rather than
 * brand tokens for the same reason: the bins are not part of the palette,
 * and both values are picked to hold up on the sand and the ink surfaces
 * without a per-theme variant. Every glyph still carries a label, because
 * colour alone is not a label to a screen reader. */
export const BIN: Record<Stream, { fill: string; edge: string; label: string }> = {
  trash: { fill: "#9aa3ab73", edge: "#79838b", label: "trash" },
  recycling: { fill: "#7fc4e873", edge: "#3f93c4", label: "recycling" },
  yard: { fill: "#6aa84f73", edge: "#4d8a3a", label: "yard" },
};

/** The `city-waste/v2` payload body — **this pane's parser is what pins the
 * shape**, since the body inside ADR-0015's envelope is deliberately
 * unfrozen and opaque to everything else.
 *
 * `scheduled` is where the cadence puts this week's collection and
 * `collectedOn` is where it actually happens; they differ exactly on a
 * holiday week, which is how a holiday is read — off the snapshot, never off
 * an alert. */
export interface WasteBody {
  /** IANA zone the two dates are civil in. */
  zone: string;
  scheduled: CivilDate;
  collectedOn: CivilDate;
  streams: Stream[];
}

/** A body that could be read, or the reason it could not — the "gap, not
 * absence" split in one type. `reason` is words a pane can render. */
export type WasteParse =
  | { kind: "ok"; body: WasteBody }
  | { kind: "gap"; reason: string };

function isStream(value: unknown): value is Stream {
  return value === "trash" || value === "recycling" || value === "yard";
}

/** Reads one snapshot row into a body, or says why it could not.
 *
 * Every arm names what was wrong. An **unrecognised `schema`** gets its own
 * wording — "this device is behind" — because it is not a broken payload at
 * all: a newer build wrote a shape this one has never heard of, which is a
 * fact about this device and is fixed by updating it, not by looking at the
 * council's website. */
export function parseWasteBody(snapshot: PaneSnapshotDTO | undefined): WasteParse {
  if (snapshot === undefined) {
    return { kind: "gap", reason: "No collection schedule has been fetched yet." };
  }
  if (snapshot.envelope.kind === "malformed") {
    return { kind: "gap", reason: `The collection payload couldn't be read: ${snapshot.envelope.reason}` };
  }
  if (snapshot.envelope.schema !== SOURCE) {
    return {
      kind: "gap",
      reason: `This device doesn't know how to read ${snapshot.envelope.schema} yet. Update the app.`,
    };
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(snapshot.envelope.body);
  } catch {
    return { kind: "gap", reason: "The collection payload isn't JSON." };
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { kind: "gap", reason: "The collection payload isn't an object." };
  }

  const body = parsed as {
    zone?: unknown;
    scheduled?: unknown;
    collected_on?: unknown;
    streams?: unknown;
  };
  if (typeof body.zone !== "string" || body.zone === "") {
    return { kind: "gap", reason: "The collection payload names no time zone." };
  }
  if (!isCivilDate(body.scheduled) || !isCivilDate(body.collected_on)) {
    return { kind: "gap", reason: "The collection payload's dates aren't whole days." };
  }
  if (!Array.isArray(body.streams) || !body.streams.every(isStream)) {
    return { kind: "gap", reason: "The collection payload lists an unknown kind of bin." };
  }
  // An unusable zone is a malformed payload, never a crash: `zoned-day.ts`
  // answers `null` for one, and every day-shaped question below depends on
  // it, so it is refused once, here.
  if (zonedMidnightMs(body.collected_on, body.zone) === null) {
    return { kind: "gap", reason: `The collection payload names an unknown time zone (${body.zone}).` };
  }

  return {
    kind: "ok",
    body: {
      zone: body.zone,
      scheduled: body.scheduled,
      collectedOn: body.collected_on,
      streams: body.streams,
    },
  };
}

/** Whether an answer is old enough to say so, against this pane's own
 * threshold — re-exported from the shell (`questions/freshness.ts`) since
 * #119's race pane needs the identical reading at its own 12h threshold.
 * The threshold stays here, per ADR-0015; only the `"unknown"` arm is
 * shared, so no pane can lose it by copying the comparison alone. */
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
 * the coloured bins already say *which* cans, so the sentence only has to say
 * *when*.
 *
 * A holiday names its actual day even when that day is tomorrow — on the one
 * week the day is unusual, "Tonight" would hide the very thing that
 * changed.
 *
 * "Today" is `daysAway === 0` and **never `<= 0`**: a negative distance is a
 * collection that has already happened, which `wasteView` refuses outright
 * rather than describe. Written as equality anyway, because `<=` here is the
 * exact line that rendered yesterday's collection as "Trash Today" for every
 * hour between the address's midnight and the next daily poll. */
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

/** Everything an answered pane needs, computed once and read by both the
 * answer and the expanded rendering. `null` when this question has no answer
 * — see [`wasteAnswer`] for which arm that becomes. */
export interface WasteView {
  body: WasteBody;
  snapshot: PaneSnapshotDTO;
  today: CivilDate;
  /** Whole days from today at the address to the collection. **Never
   * negative** — a collection already in the past is not an answer this pane
   * will render, and is refused by [`wasteView`]. */
  daysAway: number;
  /** The city moved this week's collection off its cadence day. Read
   * straight off the snapshot (`collectedOn !== scheduled`) — never from the
   * alert lane: with no card to acknowledge there is nothing to quiet, and
   * the changed day IS the answer. */
  holiday: boolean;
  weekday: string;
  stale: boolean;
  freshness: FreshnessDTO;
}

function snapshotFor(read: PaneReadDTO | undefined): PaneSnapshotDTO | undefined {
  return read?.snapshots.find((snapshot) => snapshot.key === SNAPSHOT_KEY);
}

/** Whether the question has been asked at all — **four answers, not a
 * boolean**, for the same reason `BindingValueDTO` is not `string | null`.
 *
 *   * `unread` — `bindings` is `null`, so nobody has read the table on this
 *     device yet. This is *not* "nobody has set the page": reading it as one
 *     showed every configured device the setup prompt for the whole
 *     round-trip between mount and the first `bindings` answer.
 *   * `unusable` — a row exists but holds something that is not text. A gap
 *     the reader can act on, never an absence.
 *   * `unset` — no row, or one holding nothing. The only arm that is
 *     genuinely `unbound`.
 */
export type WasteSetup =
  | { kind: "bound"; page: string }
  | { kind: "unread" }
  | { kind: "unusable" }
  | { kind: "unset" };

export function wasteSetup(inputs: QuestionInputs): WasteSetup {
  if (inputs.bindings === null) {
    return { kind: "unread" };
  }
  const binding = inputs.bindings.find((candidate) => candidate.key === BINDING_KEY);
  if (binding === undefined || binding.value.state === "unset") {
    return { kind: "unset" };
  }
  if (binding.value.state !== "text") {
    return { kind: "unusable" };
  }
  const page = binding.value.text.trim();
  // A row blanked to whitespace is the nearest thing `settings` has to a
  // DELETE, and reads as never having been set.
  return page === "" ? { kind: "unset" } : { kind: "bound", page };
}

/** The whole answered view, or the reason there is none. One function, so
 * the words a gap renders and the decision to be a gap cannot disagree. */
type WasteResolve = { kind: "view"; view: WasteView } | { kind: "gap"; reason: string };

function resolveWaste(inputs: QuestionInputs): WasteResolve {
  const snapshot = snapshotFor(inputs.paneReads[SOURCE]);
  const parsed = parseWasteBody(snapshot);
  if (parsed.kind !== "ok" || snapshot === undefined) {
    return {
      kind: "gap",
      reason: parsed.kind === "gap" ? parsed.reason : UNRESOLVABLE_DAY,
    };
  }
  const today = civilTodayInZone(inputs.nowMs, parsed.body.zone);
  const weekday = weekdayInZone(parsed.body.collectedOn, parsed.body.zone);
  const daysAway = today === null ? null : civilDaysBetween(today, parsed.body.collectedOn);
  if (today === null || weekday === null || daysAway === null) {
    return { kind: "gap", reason: UNRESOLVABLE_DAY };
  }
  // **A collection in the past is not an answer.** The poll is daily, so
  // between the address's midnight and the day's fetch the snapshot still
  // names the collection that has already happened — comfortably inside the
  // 26h stale line, so freshness says nothing about it. Rendering it would
  // put "Trash today" on screen about yesterday; the honest reading is that
  // this device's schedule is out of date, which is a gap with words.
  if (daysAway < 0) {
    return {
      kind: "gap",
      reason: `The collection schedule is out of date: it still names ${weekday} ${parsed.body.collectedOn}, which has passed.`,
    };
  }
  return {
    kind: "view",
    view: {
      body: parsed.body,
      snapshot,
      today,
      daysAway,
      holiday: parsed.body.collectedOn !== parsed.body.scheduled,
      weekday,
      stale: isStaleFreshness(snapshot.freshness, STALE_AFTER_MS),
      freshness: snapshot.freshness,
    },
  };
}

/** A body that parses but whose zone or dates cannot be resolved into a civil
 * day — refused rather than rendered wrong. */
const UNRESOLVABLE_DAY = "The collection schedule couldn't be resolved to a day.";

/** The whole answered view, or `null` when there is nothing to answer with
 * (no snapshot, a body that could not be read, a zone this build cannot
 * resolve, a collection already past). The caller turns `null` into the
 * right gap — see [`wasteGapReason`] for its words. */
export function wasteView(inputs: QuestionInputs): WasteView | null {
  const resolved = resolveWaste(inputs);
  return resolved.kind === "view" ? resolved.view : null;
}

/** Why this pane has no answer, in words — read only when [`wasteView`]
 * returned `null`. */
export function wasteGapReason(inputs: QuestionInputs): string {
  const resolved = resolveWaste(inputs);
  return resolved.kind === "gap" ? resolved.reason : UNRESOLVABLE_DAY;
}

/** This question's answer for the shell (#245).
 *
 * The band is deliberately **binary** — `dormant` or `imminent` — because
 * the real question only has two settings: it is either the night the cans
 * go out (or the day itself, or any day of a week whose day has moved), or
 * it is furniture. A `withinBand` is produced in *every* case, including
 * while dormant, so two dormant panes still order meaningfully against each
 * other rather than alphabetically.
 *
 * `liveAlerts` is deliberately never read here. The lane still raises a
 * holiday alert and the notification lane still delivers it (ADR-0012) — but
 * a holiday *is* the answer on this pane, so joining it would render the
 * same fact twice and give the reader something to dismiss that would not
 * change what they have to do. */
export function wasteAnswer(inputs: QuestionInputs): PaneAnswer {
  const setup = wasteSetup(inputs);
  if (setup.kind === "unset") {
    return {
      answerState: "unbound",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: "Not set up",
      icon: [{ kind: "icon", name: "help-circle", label: "not set up" }],
    };
  }
  if (setup.kind !== "bound") {
    // Neither answered nor unbound: the table has not been read yet, or it
    // holds something this pane cannot use. Both are gaps — saying "Not set
    // up" here would claim a fact about the reader's configuration that this
    // device has not established.
    return {
      answerState: "bound-but-unacquired",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: setup.kind === "unread" ? "Checking setup" : "Setup needs a look",
      icon: [
        setup.kind === "unread"
          ? { kind: "icon", name: "cloud-fog", label: "checking setup" }
          : { kind: "icon", name: "help-circle", label: "setup needs a look" },
      ],
    };
  }

  const view = wasteView(inputs);
  if (view === null) {
    return {
      answerState: "bound-but-unacquired",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: "No answer yet",
      icon: [{ kind: "icon", name: "cloud-fog", label: "no answer yet" }],
    };
  }

  // **Epoch ms of the collection day's start at the address**, not a
  // duration: `withinBand` is an absolute instant by contract, so the sort
  // reads no clock and the value cannot age between renders. Already in the
  // past on the day itself, which is what sorts today ahead of tomorrow. A
  // real number in every band, per the contract's own note.
  const startsAtMs = zonedMidnightMs(view.body.collectedOn, view.body.zone);

  return {
    answerState: "answered",
    band: view.holiday || view.daysAway <= 1 ? "imminent" : "dormant",
    withinBand: startsAtMs,
    collapsedHeadline: wasteCollapsedHeadline(view.daysAway, view.weekday, view.holiday),
    icon: wasteGlyphs(view.body.streams),
  };
}
