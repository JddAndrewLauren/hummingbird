import type { FreshnessDTO, PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";
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
 * NOTE: `city-waste/v2` is **not yet in the frozen source registry**
 * (`server/domain/src/sources.rs` still carries only the retired `v1`).
 * Nothing on this side checks the registry — ADR-0015 forbids checking a
 * snapshot's `schema` against it — so this pane is correct either way; the
 * registration is #135–137's and is still open. */
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
 * threshold.
 *
 * `"unknown"` is **never fresh** — the whole reason `Freshness` is a tagged
 * union rather than a nullable age. A pane that had no stamp to measure has
 * not measured anything, and "we don't know how old this is" must never
 * render as "this is current". */
export function isStaleFreshness(freshness: FreshnessDTO, thresholdMs: number): boolean {
  return freshness.kind === "unknown" || freshness.ageMs > thresholdMs;
}

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
 * changed. */
export function wasteHeadline(daysAway: number, weekday: string, holiday: boolean): string {
  if (daysAway <= 0) {
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
  if (daysAway <= 0) {
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

/** Whether the question has been asked at all: the `city-waste-page` binding
 * holding real text. An unset one — or one holding something that is not
 * text — is `unbound`, and the pane renders its setup prompt. */
export function wasteBound(inputs: QuestionInputs): boolean {
  const binding = inputs.bindings?.find((candidate) => candidate.key === BINDING_KEY);
  return binding?.value.state === "text" && binding.value.text.trim() !== "";
}

/** The whole answered view, or `null` when there is nothing to answer with
 * (no snapshot, a body that could not be read, a zone this build cannot
 * resolve). The caller turns `null` into the right gap. */
export function wasteView(inputs: QuestionInputs): WasteView | null {
  const snapshot = snapshotFor(inputs.paneReads[SOURCE]);
  const parsed = parseWasteBody(snapshot);
  if (parsed.kind !== "ok" || snapshot === undefined) {
    return null;
  }
  const today = civilTodayInZone(inputs.nowMs, parsed.body.zone);
  const weekday = weekdayInZone(parsed.body.collectedOn, parsed.body.zone);
  const daysAway = today === null ? null : civilDaysBetween(today, parsed.body.collectedOn);
  if (today === null || weekday === null || daysAway === null) {
    return null;
  }
  return {
    body: parsed.body,
    snapshot,
    today,
    daysAway,
    holiday: parsed.body.collectedOn !== parsed.body.scheduled,
    weekday,
    stale: isStaleFreshness(snapshot.freshness, STALE_AFTER_MS),
    freshness: snapshot.freshness,
  };
}

/** Why this pane has no answer, in words — read only when [`wasteView`]
 * returned `null`. */
export function wasteGapReason(inputs: QuestionInputs): string {
  const read = inputs.paneReads[SOURCE];
  const parsed = parseWasteBody(snapshotFor(read));
  if (parsed.kind === "gap") {
    return parsed.reason;
  }
  // A body that parses but whose zone or dates cannot be resolved into a
  // civil day — refused by `wasteView` rather than rendered wrong.
  return "The collection schedule couldn't be resolved to a day.";
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
  if (!wasteBound(inputs)) {
    return {
      answerState: "unbound",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: "Not set up",
      icon: [{ kind: "icon", name: "help-circle", label: "not set up" }],
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

  // Milliseconds until the collection day *starts* at the address — negative
  // on the day itself, which is what sorts today ahead of tomorrow. A real
  // number in every band, per the contract's own note.
  const startsAtMs = zonedMidnightMs(view.body.collectedOn, view.body.zone);

  return {
    answerState: "answered",
    band: view.holiday || view.daysAway <= 1 ? "imminent" : "dormant",
    withinBand: startsAtMs === null ? null : startsAtMs - inputs.nowMs,
    collapsedHeadline: wasteCollapsedHeadline(view.daysAway, view.weekday, view.holiday),
    icon: wasteGlyphs(view.body.streams),
  };
}
