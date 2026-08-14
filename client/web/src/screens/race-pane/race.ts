import type {
  FreshnessDTO,
  PaneAlertDTO,
  PaneReadDTO,
  PaneSnapshotDTO,
} from "../../store/protocol";
import type { PaneAnswer, QuestionInputs } from "../questions/contract";
import { isStaleFreshness } from "../questions/freshness";

// **The next-race question** (#119), answered over #245's pane shell and
// #266's `race-schedule/v1` lane.
//
// The lane is the poller's (`server/race-poll`): one `context_snapshots` row
// per adapted series, replaced wholesale every six hours, plus the race-start
// alert at the 90-minute lead under the same source string. Everything here
// is the *read*: which series to ask about, which weekend is next, how far
// away it is, and how loudly to say so — computed fresh at read time from the
// snapshot and never stored (ADR-0002).
//
// Three inherited decisions are worth reading before changing anything:
//
//   * **The body is a contract with a Rust producer and no compiler between
//     them.** `server/race-poll/tests/fixtures/golden-body.json` is the
//     committed artifact; `race.test.ts` parses that exact file and
//     `server/race-poll/tests/contract.rs` asserts these key spellings
//     against this file's own text.
//   * **`starts_at_ms` on the event is the race start, and `sessions` never
//     contains the race.** That asymmetry is deliberate (an IndyCar event
//     has a race start and no ladder), so nothing here may look for a
//     `kind: "race"` session.
//   * **No session end time exists anywhere in this lane, and one must
//     never be invented**
//     — see [`nextRaceAt`] for what this pane does instead of an "under way"
//     verdict.

/** Both the snapshot's source and every race-start alert's — ADR-0009's
 * join constraint is that these are one string, and ADR-0015's envelope
 * `schema` is the same one again.
 *
 * `race-schedule/v1` is in the frozen source registry
 * (`server/domain/src/sources.rs`, #266) as `Writes::Both`. Nothing on this
 * side checks that registry — ADR-0015 forbids resolving a snapshot's
 * `schema` against it — so this constant is this file's own and the two
 * agree by review, not by import. */
export const SOURCE = "race-schedule/v1";

/** The binding that has to be set before this question can be asked at all
 * — the same key `server/race-poll/src/binding.rs` reads, unversioned so a
 * `/v1 → /v2` source bump cannot orphan it. */
export const BINDING_KEY = "race-series";

/** How old an answer may be before it is worth saying so — beside the band
 * function, where ADR-0015 puts every threshold.
 *
 * **Twelve hours: `2 ×` the schedule poller's declared six-hour cadence.**
 * Unlike `waste.ts`'s 26h — which had to depart from `2 × cadence` because a
 * 47-hour-old daily answer can name the wrong night — a race schedule moves
 * rarely and a missed poll costs nothing until it does, so the plain rule
 * stands. It must stay in step with `race_poll::body::POLLED_EVERY_MS`, and
 * `server/race-poll/tests/contract.rs` asserts exactly that against this
 * file's text. */
export const STALE_AFTER_MS = 12 * 60 * 60 * 1000;

/** One supporting session — practice, qualifying, the sprint and its own
 * qualifying. **Never the race**; see the module note. */
export interface RaceSession {
  /** The poller's stable machine name (`practice`, `qualifying`, `sprint`,
   * `sprint_qualifying`). Deliberately an open string, not a closed union:
   * a feed that grows a session kind must not fail this whole parse, and
   * nothing here branches on the value — `label` is what goes on screen. */
  kind: string;
  label: string;
  startsAtMs: number;
}

/** One race weekend: the race start, plus its supporting ladder. */
export interface RaceEvent {
  name: string;
  locality: string;
  /** **The race start**, never the first session's. */
  startsAtMs: number;
  sessions: RaceSession[];
}

/** The `race-schedule/v1` body — the whole season, in feed order. This
 * parser is what pins the shape, since the body inside ADR-0015's envelope
 * is opaque to everything else. */
export interface RaceBody {
  events: RaceEvent[];
}

/** A body that could be read, or the reason it could not — the "gap, not
 * absence" split in one type. `reason` is words a pane can render. */
export type RaceParse = { kind: "ok"; body: RaceBody } | { kind: "gap"; reason: string };

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function parseSession(raw: unknown): RaceSession | null {
  if (typeof raw !== "object" || raw === null) {
    return null;
  }
  const session = raw as { kind?: unknown; label?: unknown; starts_at_ms?: unknown };
  if (typeof session.kind !== "string" || typeof session.label !== "string") {
    return null;
  }
  if (!isFiniteNumber(session.starts_at_ms)) {
    return null;
  }
  return { kind: session.kind, label: session.label, startsAtMs: session.starts_at_ms };
}

function parseEvent(raw: unknown): RaceEvent | null {
  if (typeof raw !== "object" || raw === null) {
    return null;
  }
  const event = raw as {
    name?: unknown;
    locality?: unknown;
    starts_at_ms?: unknown;
    sessions?: unknown;
  };
  if (typeof event.name !== "string" || typeof event.locality !== "string") {
    return null;
  }
  if (!isFiniteNumber(event.starts_at_ms) || !Array.isArray(event.sessions)) {
    return null;
  }
  const sessions: RaceSession[] = [];
  for (const entry of event.sessions) {
    const session = parseSession(entry);
    if (session === null) {
      return null;
    }
    sessions.push(session);
  }
  return {
    name: event.name,
    locality: event.locality,
    startsAtMs: event.starts_at_ms,
    sessions,
  };
}

/** Reads one snapshot row into a season, or says why it could not.
 *
 * Every arm names what was wrong, and an **unrecognised `schema`** gets its
 * own wording — "this device is behind" — because it is not a broken payload
 * at all: a newer build wrote a shape this one has never heard of, which is
 * fixed by updating the app, not by looking at the feed. */
export function parseRaceBody(snapshot: PaneSnapshotDTO | undefined): RaceParse {
  if (snapshot === undefined) {
    return { kind: "gap", reason: "No schedule has been fetched for this series yet." };
  }
  if (snapshot.envelope.kind === "malformed") {
    return { kind: "gap", reason: `The schedule payload couldn't be read: ${snapshot.envelope.reason}` };
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
    return { kind: "gap", reason: "The schedule payload isn't JSON." };
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { kind: "gap", reason: "The schedule payload isn't an object." };
  }
  const body = parsed as { events?: unknown };
  if (!Array.isArray(body.events)) {
    return { kind: "gap", reason: "The schedule payload carries no season." };
  }
  const events: RaceEvent[] = [];
  for (const entry of body.events) {
    const event = parseEvent(entry);
    if (event === null) {
      return { kind: "gap", reason: "The schedule payload lists an event this app can't read." };
    }
    events.push(event);
  }
  // An **empty season is a legitimate value, not a failure** — the poller
  // says so from the other side (`body.rs`). Off-season, the pane answers
  // "no races scheduled"; reading it as a gap would blame the feed for the
  // calendar.
  return { kind: "ok", body: { events } };
}

/** The followed series, read out of the `race-series` binding's text.
 *
 * **The same reading `server/race-poll/src/binding.rs` gives that row**, and
 * that agreement is the point: trimmed, lowercased, blanks dropped, repeats
 * dropped, order kept. The two sides parse the same text with no type
 * between them, so a pane that read `F1` as its own series key would render
 * a pane nothing ever writes a snapshot for.
 *
 * The value is a comma-separated string rather than a JSON array because
 * `settings` stores canonical JSON text and the binding editor's own
 * vocabulary is `Unset | Text | Other` — an array lands as `Other`, which no
 * shipped editor can write. */
export function seriesFromBinding(text: string): string[] {
  const series: string[] = [];
  for (const entry of text.split(",")) {
    const key = entry.trim().toLowerCase();
    if (key === "" || series.includes(key)) {
      continue;
    }
    series.push(key);
  }
  return series;
}

/** Whether the question has been asked at all — **four answers, not a
 * boolean**, exactly as `waste.ts`'s `wasteSetup` is and for the same
 * reasons: an unread table is not an unset one, and a row this build cannot
 * use is a gap the reader can act on rather than an absence. */
export type RaceSetup =
  | { kind: "bound"; series: string[] }
  | { kind: "unread" }
  | { kind: "unusable" }
  | { kind: "unset" };

export function raceSetup(inputs: QuestionInputs): RaceSetup {
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
  const series = seriesFromBinding(binding.value.text);
  // A row blanked to whitespace (or to nothing but separators) is the
  // nearest thing `settings` has to a DELETE, and reads as never set.
  return series.length === 0 ? { kind: "unset" } : { kind: "bound", series };
}

// -- the read-time answer ---------------------------------------------------
//
// Everything below is computed fresh from the snapshot on every render and
// stored nowhere (ADR-0002). The clock arrives as `nowMs`; no function here
// reads one.

const MINUTE_MS = 60 * 1000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/** The next race still ahead — the soonest event whose **start instant** has
 * not passed, scanned by instant rather than trusted to feed order.
 *
 * **The start instant is this pane's horizon, and that is the settled answer
 * to the "under way" question** (#266's grilling note on #119). The prototype
 * flipped its headline to "Monaco Practice 3 under way" mid-weekend; saying
 * that needs a session *end* time, Jolpica publishes none, and #266 refused
 * to add a session end field — a fabricated per-kind duration inside a stored
 * row is the same "a judgement duplicated into a payload is a fact that can
 * disagree with itself" rule `city-waste`'s absent `deviation` field already
 * enforces. Deciding it at read time instead only moves the invention: "under
 * way" from a start alone is an assumed duration wherever it is computed.
 *
 * So the under-way headline is **dropped**, with its verdict, rather than
 * silently lost — and the lane already reads the same way, since
 * `race-schedule/v1` is registered with `Expiry::Always("the race's start
 * time")`: the race-start alert expires at the start too. The cost, recorded
 * rather than hidden: for the couple of hours a race is actually running,
 * this pane names the *following* race. */
export function nextRaceAt(events: readonly RaceEvent[], nowMs: number): RaceEvent | null {
  let next: RaceEvent | null = null;
  for (const event of events) {
    if (event.startsAtMs <= nowMs) {
      continue;
    }
    if (next === null || event.startsAtMs < next.startsAtMs) {
      next = event;
    }
  }
  return next;
}

/** The next thing on track for one weekend — the soonest upcoming start
 * among the supporting ladder and the race itself.
 *
 * Separate from the race because the two differ for most of a race weekend
 * (Friday practice is two days before Sunday's race), and the headline
 * answers the *question* ("when is the next race") while this answers what
 * actually happens first. Never `null` for an event this pane is showing —
 * the race start is itself a candidate and is upcoming by construction. */
export function nextStartOf(
  event: RaceEvent,
  nowMs: number,
): { label: string; startsAtMs: number } {
  let next = { label: "Race", startsAtMs: event.startsAtMs };
  for (const session of event.sessions) {
    if (session.startsAtMs > nowMs && session.startsAtMs < next.startsAtMs) {
      next = { label: session.label, startsAtMs: session.startsAtMs };
    }
  }
  return next;
}

/** The headline number and its unit, kept apart so the rendering can set
 * them in different type: `90 min`, `4 hr`, `12 days`.
 *
 * **The minutes boundary sits above the alert's own 90-minute lead**
 * (`race_poll::next::LEAD_MS`) deliberately: rounding that instant to "2 hr"
 * would have this pane contradict the notification the reader is holding,
 * about the same race, from the same source string. `min` and `hr` do not
 * inflect — an abbreviation is already a machine value, and "1 mins" is the
 * only way to get one wrong. */
export function countdown(deltaMs: number): { value: string; unit: string } {
  const minutes = Math.round(deltaMs / MINUTE_MS);
  if (minutes < 120) {
    return { value: String(Math.max(minutes, 0)), unit: "min" };
  }
  const hours = Math.round(deltaMs / HOUR_MS);
  if (hours <= 36) {
    return { value: String(hours), unit: "hr" };
  }
  const days = Math.round(deltaMs / DAY_MS);
  return { value: String(days), unit: days === 1 ? "day" : "days" };
}

/** "Monaco Grand Prix" reads as "Monaco GP". A name with no Grand Prix in it
 * ("Iowa 275", once an IndyCar adapter exists) passes through untouched. */
export function abbreviate(eventName: string): string {
  return eventName.replace(/\s+Grand Prix$/i, " GP");
}

/** Human names for the series keys this build knows about. A series the
 * binding names and this map does not is rendered from its own key rather
 * than hidden — the same reading `Core::bindings` gives an unrecognised
 * binding key, and the reason adding a series takes no code change here. */
const SERIES_LABELS: Record<string, string> = { f1: "F1", indycar: "IndyCar" };

export function seriesLabel(series: string): string {
  return SERIES_LABELS[series] ?? series.toUpperCase();
}

/** The one sentinel subject an unbound (or not-yet-read) question emits, so
 * the setup prompt exists to be found. The shell contract's own rule: a
 * question that vanished until it was configured could never be configured.
 * It is never a real series key — no feed calls a series "setup". */
export const SETUP_SUBJECT = "setup";

/** This question's subjects: one per followed series, in binding order.
 *
 * **0..N from a `settings` row, which is the acceptance criterion**: the
 * series list comes from the binding, so following another series is an edit
 * in Settings rather than a code change here. A series with no adapter
 * upstream (today `indycar`, #266) still gets its pane — as a gap, which is
 * what makes the deferral visible instead of silent. */
export function raceSubjects(inputs: QuestionInputs): string[] {
  const setup = raceSetup(inputs);
  return setup.kind === "bound" ? setup.series : [SETUP_SUBJECT];
}

/** Everything one answered pane needs, computed once and read by both the
 * answer and the expanded rendering. */
export interface RaceView {
  series: string;
  label: string;
  snapshot: PaneSnapshotDTO;
  /** The next race weekend, or `null` **off-season** — a season whose races
   * have all run is an answer ("no races scheduled"), not a gap. */
  event: RaceEvent | null;
  /** The next thing on track for that weekend: Friday practice for most of
   * the year, the race itself once the ladder is done. `null` off-season. */
  nextStart: { label: string; startsAtMs: number } | null;
  /** The race-start alert this series currently has live, joined on
   * `(source, subjectKey)` ↔ `(source, key)`. */
  liveAlert: PaneAlertDTO | null;
  stale: boolean;
  freshness: FreshnessDTO;
}

function snapshotFor(read: PaneReadDTO | undefined, series: string): PaneSnapshotDTO | undefined {
  return read?.snapshots.find((snapshot) => snapshot.key === series);
}

/** The alert join, and the only place it is spelled.
 *
 * ADR-0015 added `alerts.subject_key` naming this pane as its forcing case:
 * ONE source (`race-schedule/v1`) carries a row per series, so joining on
 * `source` alone would put every series' race-start alert on every series'
 * pane. `source_key` stays occurrence identity and is never parsed for this.
 * The join is **additive** — an alert matching no pane (including one naming
 * no subject at all) is not dropped, it simply lives in `AlertsScreen`. */
function liveAlertFor(read: PaneReadDTO | undefined, series: string): PaneAlertDTO | null {
  return read?.liveAlerts.find((alert) => alert.subjectKey === series) ?? null;
}

type RaceResolve = { kind: "view"; view: RaceView } | { kind: "gap"; reason: string };

function resolveRace(series: string, inputs: QuestionInputs): RaceResolve {
  const read = inputs.paneReads[SOURCE];
  const snapshot = snapshotFor(read, series);
  const parsed = parseRaceBody(snapshot);
  if (parsed.kind !== "ok" || snapshot === undefined) {
    return {
      kind: "gap",
      reason: parsed.kind === "gap" ? parsed.reason : NO_SEASON,
    };
  }
  const event = nextRaceAt(parsed.body.events, inputs.nowMs);
  return {
    kind: "view",
    view: {
      series,
      label: seriesLabel(series),
      snapshot,
      event,
      nextStart: event === null ? null : nextStartOf(event, inputs.nowMs),
      liveAlert: liveAlertFor(read, series),
      stale: isStaleFreshness(snapshot.freshness, STALE_AFTER_MS),
      freshness: snapshot.freshness,
    },
  };
}

const NO_SEASON = "This series' schedule couldn't be read.";

/** The whole answered view, or `null` when there is nothing to answer with
 * (no snapshot for this series, or a body this build cannot read). The
 * caller turns `null` into the right gap — [`raceGapReason`] has its
 * words. */
export function raceView(series: string, inputs: QuestionInputs): RaceView | null {
  const resolved = resolveRace(series, inputs);
  return resolved.kind === "view" ? resolved.view : null;
}

/** Why this pane has no answer, in words — read only when [`raceView`]
 * returned `null`. */
export function raceGapReason(series: string, inputs: QuestionInputs): string {
  const resolved = resolveRace(series, inputs);
  return resolved.kind === "gap" ? resolved.reason : NO_SEASON;
}

/** The expanded pane's headline, in parts — counting to **race day** and
 * naming the **race**: `Monaco GP` · `in` · `12` · `days`.
 *
 * Race-first, count second, with the joining words between them, which is the
 * vacation countdown's shape (`vacation-pane/VacationPaneExpanded.tsx`) and is
 * why this returns parts rather than the joined string it used to
 * ("12 days before Monaco GP"): the two things the line says — which race, and
 * how long — are set at display size and the grammar between them at body
 * size, so neither answer reads as a caption on the other. A joined string
 * cannot carry that, and the two countdown panes in one aside must not answer
 * the same shape of question in two different shapes.
 *
 * It applies at every unit, minutes included: the split is about which words
 * are the answer, and that does not change when the answer gets close.
 *
 * The session that actually happens first is a fact worth having and is not
 * lost: it goes on the line underneath (`RacePaneExpanded`), where it cannot
 * be mistaken for the answer to "when is the next race". */
export type RaceHeadlineParts =
  | { kind: "countdown"; name: string; value: string; unit: string }
  | { kind: "fallback"; text: string };

export function raceHeadlineParts(view: RaceView, nowMs: number): RaceHeadlineParts {
  if (view.event === null) {
    return { kind: "fallback", text: "No races scheduled" };
  }
  const { value, unit } = countdown(view.event.startsAtMs - nowMs);
  return { kind: "countdown", name: abbreviate(view.event.name), value, unit };
}

/** The same answer in one line, for the shell's collapsed row — which is
 * also how a dormant race pane renders, since there is no separate quiet
 * card. The series is named here and not in `QuestionDef.label`, because
 * with two series followed the shell draws the same question twice and only
 * this line can say which is which. */
export function raceCollapsedHeadline(view: RaceView, nowMs: number): string {
  if (view.event === null) {
    return `${view.label} · No races scheduled`;
  }
  const { value, unit } = countdown(view.event.startsAtMs - nowMs);
  return `${view.label} · ${abbreviate(view.event.name)} in ${value} ${unit}`;
}

/** The race is close enough that the day is about it. */
const IMMINENT_MS = 24 * HOUR_MS;
/** The weekend is running, or begins soon enough to plan around. */
const NEAR_MS = 72 * HOUR_MS;

/** This question's answer for the shell (#245).
 *
 * The bands, and why each threshold is where it is:
 *
 *   * `live` — **the lane's own race-start alert is live for this series**,
 *     joined on `subjectKey`. Deliberately not a second time threshold: the
 *     alert is raised at `race_poll::next::LEAD_MS` (90 minutes) and expires
 *     at the start, so reading the alert rather than re-deriving its window
 *     keeps the pane and the notification from ever disagreeing about the
 *     same race.
 *   * `imminent` — the race starts within a day.
 *   * `near` — the next thing on track (usually Friday practice) is within
 *     three days, i.e. the race weekend is here.
 *   * `distant` — a race is scheduled, further out than that.
 *   * `dormant` — off-season: nothing is scheduled at all.
 *
 * `withinBand` is the **instant** of the next thing on track, per the shell
 * contract — an absolute moment, never a duration, so the sort reads no
 * clock and the value cannot age between renders. `null` only off-season,
 * where there is genuinely nothing to order by. */
export function raceAnswer(subjectKey: string, inputs: QuestionInputs): PaneAnswer {
  const setup = raceSetup(inputs);
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
    // The table has not been read yet, or holds something this pane cannot
    // use. Both are gaps: "Not set up" would state a fact about the reader's
    // configuration this device has not established.
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

  const view = raceView(subjectKey, inputs);
  if (view === null) {
    // A followed series nothing has written a snapshot for — today every
    // series but `f1`, since #266 ships one adapter. Named rather than
    // dropped: a series added to the binding that produced no pane at all
    // would look like the edit did nothing.
    return {
      answerState: "bound-but-unacquired",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: `${seriesLabel(subjectKey)} · Never polled`,
      icon: [{ kind: "icon", name: "cloud-fog", label: "never polled" }],
    };
  }
  if (view.event === null || view.nextStart === null) {
    return {
      answerState: "answered",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: raceCollapsedHeadline(view, inputs.nowMs),
      icon: [{ kind: "icon", name: "flag", label: "no races scheduled" }],
    };
  }

  const toRaceMs = view.event.startsAtMs - inputs.nowMs;
  const toNextMs = view.nextStart.startsAtMs - inputs.nowMs;
  const band =
    view.liveAlert !== null
      ? "live"
      : toRaceMs <= IMMINENT_MS
        ? "imminent"
        : toNextMs <= NEAR_MS
          ? "near"
          : "distant";

  return {
    answerState: "answered",
    band,
    withinBand: view.nextStart.startsAtMs,
    collapsedHeadline: raceCollapsedHeadline(view, inputs.nowMs),
    icon: [
      view.liveAlert !== null
        ? { kind: "icon", name: "siren", label: "starting soon" }
        : { kind: "icon", name: "flag", label: "next race" },
    ],
  };
}

// -- device-local words for an instant --------------------------------------
//
// **ADR-0015 is device-local everywhere, with no zone label.** The prototype
// pinned `America/Los_Angeles` and suffixed every time with "PT"; both are
// gone. The cost is taken deliberately and is written down in the ADR: a race
// appears to move when you travel. What replaces the suffix is nothing —
// there is no zone to name when the zone is the one the reader is standing
// in.

/** Which local calendar day an instant falls on, as a comparable integer.
 * Compared as days rather than as elapsed milliseconds because a 23:00 race
 * and a 01:00 "now" are two hours and two days apart. */
function localDayIndex(atMs: number): number {
  const at = new Date(atMs);
  return Math.floor(Date.UTC(at.getFullYear(), at.getMonth(), at.getDate()) / DAY_MS);
}

/** "Today" · "Tomorrow" · "Thursday" · "Sep 20" — the device's own day. */
export function dayLabel(atMs: number, nowMs: number): string {
  const days = localDayIndex(atMs) - localDayIndex(nowMs);
  if (days === 0) {
    return "Today";
  }
  if (days === 1) {
    return "Tomorrow";
  }
  if (days > 1 && days < 7) {
    return new Intl.DateTimeFormat("en-US", { weekday: "long" }).format(new Date(atMs));
  }
  return new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric" }).format(new Date(atMs));
}

/** The wall-clock time on this device — "4:00 PM", and **no zone suffix**. */
export function clock(atMs: number): string {
  return new Intl.DateTimeFormat("en-US", { hour: "numeric", minute: "2-digit" }).format(
    new Date(atMs),
  );
}
