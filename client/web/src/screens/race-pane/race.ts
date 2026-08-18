import {
  nextRaceAtFromCore,
  parseRaceBodyFromCore,
  raceAnswerFromCore,
  raceFactsFromCore,
  raceSeriesFromBindingFromCore,
  raceSetupFromCore,
  raceSubjectsFromCore,
  type PaneInputsSource,
  type RaceEventCore,
  type RaceFacts as RaceFactsCore,
  type RaceGap,
} from "../../decisions/seam";
import type { FreshnessDTO, PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { PaneAnswer, QuestionInputs } from "../questions/contract";
import { isStaleFreshness } from "../questions/freshness";

// **The next-race question** (#119), answered over #245's pane shell — and
// since #534, **the web's rendering half of it only**.
//
// Every rule this file used to hold is now
// `hummingbird_core::decisions::panes::race`: the payload parser, the setup
// arm, `nextRaceAt`/`nextStartOf`, the live-alert join, the band and its two
// thresholds, and the gap kinds. Read that module for the reasoning behind
// any of them — the three inherited decisions its own module doc names
// (the golden-body contract, `starts_at_ms` vs. `sessions`, no session end
// time) are stated there once.
//
// What stayed here is what ADR-0025 leaves per-client: `countdown`'s
// numeric split, `abbreviate`, `seriesLabel`, `raceHeadlineParts`,
// `raceCollapsedHeadline`, and `dayLabel`/`clock` — the last two explicitly
// device-local wall-clock words (ADR-0015).

export const SOURCE = "race-schedule/v1";
export const BINDING_KEY = "race-series";
export const STALE_AFTER_MS = 12 * 60 * 60 * 1000;
export const SETUP_SUBJECT = "setup";

export interface RaceSession {
  kind: string;
  label: string;
  startsAtMs: number;
}

export interface RaceEvent {
  name: string;
  locality: string;
  startsAtMs: number;
  sessions: RaceSession[];
}

export interface RaceBody {
  events: RaceEvent[];
}

export type RaceParse = { kind: "ok"; body: RaceBody } | { kind: "gap"; reason: string };

/** Reads one snapshot row into a season, or says why it could not —
 * `race.rs`'s `parse_race_body` with this client's wording put back on. */
export function parseRaceBody(snapshot: PaneSnapshotDTO | undefined): RaceParse {
  const parsed = parseRaceBodyFromCore(snapshot);
  return parsed.kind === "ok" ? { kind: "ok", body: parsed.body } : { kind: "gap", reason: gapReason(parsed.gap) };
}

/** `race.rs`'s `series_from_binding`. */
export function seriesFromBinding(text: string): string[] {
  return raceSeriesFromBindingFromCore(text);
}

export type RaceSetup =
  | { kind: "bound"; series: string[] }
  | { kind: "unread" }
  | { kind: "unusable" }
  | { kind: "unset" };

function paneInputs(inputs: QuestionInputs): PaneInputsSource {
  return { nowMs: inputs.nowMs, bindings: inputs.bindings, paneReads: inputs.paneReads };
}

/** `race.rs`'s `race_setup`. */
export function raceSetup(inputs: QuestionInputs): RaceSetup {
  return raceSetupFromCore(paneInputs(inputs));
}

const MINUTE_MS = 60 * 1000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/** The next race still ahead, by start instant — `race.rs`'s
 * `next_race_at`. See that module's own doc for why "under way" is
 * dropped rather than fabricated from a session start alone. */
export function nextRaceAt(events: readonly RaceEvent[], nowMs: number): RaceEvent | null {
  return nextRaceAtFromCore(events as RaceEventCore[], nowMs);
}

/** The headline number and its unit, kept apart so the rendering can set
 * them in different type: `90 min`, `4 hr`, `12 days`. Pure formatting, so
 * it stays here. */
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

/** "Monaco Grand Prix" reads as "Monaco GP". */
export function abbreviate(eventName: string): string {
  return eventName.replace(/\s+Grand Prix$/i, " GP");
}

const SERIES_LABELS: Record<string, string> = { f1: "F1", indycar: "IndyCar" };

export function seriesLabel(series: string): string {
  return SERIES_LABELS[series] ?? series.toUpperCase();
}

/** This question's subjects — `race.rs`'s `race_subjects`. */
export function raceSubjects(inputs: QuestionInputs): string[] {
  return raceSubjectsFromCore(paneInputs(inputs));
}

export interface RaceView {
  series: string;
  label: string;
  event: RaceEvent | null;
  nextStart: { label: string; startsAtMs: number } | null;
  liveAlert: PaneReadDTO["liveAlerts"][number] | null;
  stale: boolean;
  freshness: FreshnessDTO;
}

/** The alert join, done here rather than in the core: the core's
 * `hasLiveAlert` boolean is what the *band* reads, but a live alert's own
 * `title`/`body` are display data the client already holds on
 * `inputs.paneReads` — re-deriving it here costs nothing and keeps the
 * core from having to cross a whole `PaneAlertDTO` it never renders. */
function liveAlertFor(inputs: QuestionInputs, series: string): PaneReadDTO["liveAlerts"][number] | null {
  return inputs.paneReads[SOURCE]?.liveAlerts.find((alert) => alert.subjectKey === series) ?? null;
}

function toView(series: string, inputs: QuestionInputs, facts: RaceFactsCore): RaceView {
  return {
    series,
    label: seriesLabel(series),
    event: facts.event,
    nextStart: facts.nextStart === null ? null : { label: facts.nextStart[0], startsAtMs: facts.nextStart[1] },
    liveAlert: liveAlertFor(inputs, series),
    stale: facts.stale,
    freshness: facts.freshness,
  };
}

const NO_SEASON = "This series' schedule couldn't be read.";

/** The whole answered view, or `null` when there is nothing to answer with
 * (no snapshot for this series, or a body this build cannot read). */
export function raceView(series: string, inputs: QuestionInputs): RaceView | null {
  const resolved = raceFactsFromCore(series, paneInputs(inputs));
  return resolved.kind === "facts" ? toView(series, inputs, resolved) : null;
}

/** Why this pane has no answer, in words — read only when [`raceView`]
 * returned `null`. */
export function raceGapReason(series: string, inputs: QuestionInputs): string {
  const resolved = raceFactsFromCore(series, paneInputs(inputs));
  return resolved.kind === "gap" ? gapReason(resolved.gap) : NO_SEASON;
}

function gapReason(gap: RaceGap): string {
  switch (gap.gap) {
    case "notFetched":
      return "No schedule has been fetched for this series yet.";
    case "malformed":
      return `The schedule payload couldn't be read: ${gap.reason}`;
    case "unknownSchema":
      return `This device doesn't know how to read ${gap.schema} yet. Update the app.`;
    case "notJson":
      return "The schedule payload isn't JSON.";
    case "notAnObject":
      return "The schedule payload isn't an object.";
    case "noSeason":
      return "The schedule payload carries no season.";
    case "badEvent":
      return "The schedule payload lists an event this app can't read.";
    default:
      return NO_SEASON;
  }
}

/** The expanded pane's headline, in parts. */
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

/** The same answer in one line, for the shell's collapsed row. */
export function raceCollapsedHeadline(view: RaceView, nowMs: number): string {
  if (view.event === null) {
    return `${view.label} · No races scheduled`;
  }
  const { value, unit } = countdown(view.event.startsAtMs - nowMs);
  return `${view.label} · ${abbreviate(view.event.name)} in ${value} ${unit}`;
}

export { isStaleFreshness };

/** This question's answer for the shell (#245/#119). */
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
    return {
      answerState: "bound-but-unacquired",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: `${seriesLabel(subjectKey)} · Never polled`,
      icon: [{ kind: "icon", name: "cloud-fog", label: "never polled" }],
    };
  }

  const source = paneInputs(inputs);
  const answer = raceAnswerFromCore(subjectKey, source);
  if (view.event === null || view.nextStart === null) {
    return {
      ...answer,
      collapsedHeadline: raceCollapsedHeadline(view, inputs.nowMs),
      icon: [{ kind: "icon", name: "flag", label: "no races scheduled" }],
    };
  }

  return {
    ...answer,
    collapsedHeadline: raceCollapsedHeadline(view, inputs.nowMs),
    icon: [
      view.liveAlert !== null
        ? { kind: "icon", name: "siren", label: "starting soon" }
        : { kind: "icon", name: "flag", label: "next race" },
    ],
  };
}

// -- device-local words for an instant --------------------------------------

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
