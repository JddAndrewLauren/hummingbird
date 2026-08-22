import type { FreshnessDTO } from "../../store/protocol";
import type { PaneAnswer, QuestionInputs } from "../questions/contract";
import { deviceCivilToday } from "../waste-pane/zoned-day";
import { resolveZoneFacts } from "../questions/zone-bridge";
import {
  scpsAnswerFromCore,
  scpsViewFromCore,
  scpsZoneQueriesFromCore,
  type PaneInputsSource,
  type ScpsEventCore,
  type ScpsKindCore,
  type ScpsQuestFactCore,
} from "../../decisions/seam";

// **The SCPS pane** (#693, ADR-0032) — "when is the next SCPS event, and
// what is this month's Photo Quest", the tenth standing question, over the
// calendar arm plus one binding.
//
// Every decision here is `hummingbird_core::decisions::panes::scps`: the
// title parser, the band, the event queue, the month-token parser and the
// quest classification. This file renders — composes sentences off already
// -decided facts, the same split `vacation.ts`'s own header states.
//
// **`sources: []`, no per-question calendar binding.** Unlike `vacation.ts`
// (a designated Trips calendar) this pane reads every non-cancelled event
// on every calendar the device already polls, filtered only by the
// `SCPS ` title prefix — so there is no "which calendar" setup step, and
// (`scps.rs`'s own rule) `AnswerState.unbound` never crosses here.

export const SUBJECT_KEY = "next-scps-event";
export const CALENDAR_REQUEST_KEY = "scps";
export const QUEST_BINDING_KEY = "scps-quest";

const HOUR_MS = 60 * 60 * 1000;
const DAY_MS = 24 * HOUR_MS;

export const HORIZON_BEFORE_MS = 6 * HOUR_MS;
export const HORIZON_AFTER_DAYS = 90;
export const STALE_AFTER_MS = 24 * HOUR_MS;

export type ScpsKind = ScpsKindCore;
export type ScpsEvent = ScpsEventCore;
export type ScpsQuestFact = ScpsQuestFactCore;

const KIND_LABEL: Record<ScpsKind, string> = {
  meeting: "Meeting",
  activity: "Activity",
  happy_hour: "Happy Hour",
  // The contract's own rule: an unrecognised `SCPS ` title reads as the
  // literal lowercase word "event", never a capitalised label.
  event: "event",
};

const WEEKDAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

const MONTH_NAMES = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/** `"January"` for `"2026-09"`'s `09` — the two-digit month token every
 * quest value and every current-month comparison carries. */
export function monthName(monthToken: string): string {
  const month = Number(monthToken.slice(5, 7));
  return MONTH_NAMES[month - 1] ?? monthToken;
}

/** `"9:00am"` — local time at the device's own zone, which is what every
 * `ScpsEvent.startMs` already resolved against (`scps.rs`'s `DEVICE_ZONE`,
 * never a payload-carried zone). */
export function scpsTimeLabel(ms: number): string {
  const date = new Date(ms);
  const hour24 = date.getHours();
  const minutes = date.getMinutes().toString().padStart(2, "0");
  const hour12 = hour24 % 12 || 12;
  const ampm = hour24 >= 12 ? "pm" : "am";
  return `${hour12}:${minutes}${ampm}`;
}

/** "today" · "tomorrow" · "Sat" · "9 Mar" — the day half of the headline. */
export function scpsDayLabel(event: ScpsEvent): string {
  if (event.inProgress || event.daysUntil <= 0) {
    return "today";
  }
  if (event.daysUntil === 1) {
    return "tomorrow";
  }
  if (event.daysUntil < 7) {
    const weekday = new Date(`${event.startDate}T00:00:00Z`).getUTCDay();
    return WEEKDAY_NAMES[weekday];
  }
  const [, month, day] = event.startDate.split("-");
  const abbrev = MONTH_NAMES[Number(month) - 1]?.slice(0, 3) ?? month;
  return `${Number(day)} ${abbrev}`;
}

/** The whole answer in one line — ADR-0032 part 5's headline table:
 * `"SCPS Activity Sat 9:00am — Super Girl Surf Festival"`,
 * `"SCPS Meeting today 2:00pm — Impressions of Venice"`,
 * `"SCPS Happy Hour in progress"`, or, dormant, `"No SCPS event
 * scheduled"`. */
export function scpsHeadline(next: ScpsEvent | null): string {
  if (next === null) {
    return "No SCPS event scheduled";
  }
  if (next.kind === "happy_hour" && next.inProgress) {
    return "SCPS Happy Hour in progress";
  }
  const topic = next.topic !== null && next.topic.trim().length > 0 ? ` — ${next.topic}` : "";
  return `SCPS ${KIND_LABEL[next.kind]} ${scpsDayLabel(next)} ${scpsTimeLabel(next.startMs)}${topic}`;
}

/** The collapsed row's meta: `"Quest: <phrase>"` appended when the quest's
 * month is the current civil month, on the repo's own middot-join
 * convention (`race.ts`'s/`github.ts`'s collapsed headlines) — nothing
 * otherwise (ADR-0032 part 5's own rule: the quest is never shown for a
 * month it was not posted for). */
export function scpsCollapsedHeadline(next: ScpsEvent | null, quest: ScpsQuestFact): string {
  const headline = scpsHeadline(next);
  return quest.kind === "current" ? `${headline} · Quest: ${quest.phrase}` : headline;
}

/** The quest line the expanded pane draws: the current month's phrase, the
 * last-posted one named against its own month, or the plain "unset" line —
 * never a phrase this pane cannot attribute to a month. */
export function scpsQuestLine(quest: ScpsQuestFact, nowMs: number): string {
  if (quest.kind === "current") {
    return `Photo Quest — ${quest.phrase}`;
  }
  if (quest.kind === "other") {
    const today = deviceCivilToday(nowMs);
    const currentMonth = today !== null ? monthName(today) : "this month";
    return `No quest posted for ${currentMonth}; last: ${quest.phrase} (${monthName(quest.month)})`;
  }
  return "No quest set";
}

function paneInputs(inputs: QuestionInputs): PaneInputsSource {
  return {
    nowMs: inputs.nowMs,
    bindings: inputs.bindings,
    paneReads: inputs.paneReads,
    calendarReads: inputs.calendarReads,
    calendarConnected: inputs.calendarConnected,
    items: inputs.items,
  };
}

export interface ScpsView {
  next: ScpsEvent | null;
  later: ScpsEvent[];
  quest: ScpsQuestFact;
  freshness: FreshnessDTO;
  stale: boolean;
}

/** The answered view an expanded pane draws — `null` while unacquired
 * (never a calendar-id "unbound" setup step, `scps.rs`'s own header). */
export function scpsView(inputs: QuestionInputs): ScpsView | null {
  const source = paneInputs(inputs);
  const facts = resolveZoneFacts(scpsZoneQueriesFromCore(source));
  const resolved = scpsViewFromCore(source, facts);
  if (resolved === null || resolved.kind !== "facts") {
    return null;
  }
  return {
    next: resolved.next,
    later: resolved.later,
    quest: resolved.quest,
    freshness: resolved.freshness,
    stale: resolved.stale,
  };
}

/** This question's answer for the shell. */
export function scpsAnswer(inputs: QuestionInputs): PaneAnswer {
  const source = paneInputs(inputs);
  const facts = resolveZoneFacts(scpsZoneQueriesFromCore(source));
  const answer = scpsAnswerFromCore(source, facts);

  if (answer.answerState === "bound-but-unacquired") {
    return { ...answer, collapsedHeadline: "Waiting for the first calendar sync" };
  }

  const view = scpsView(inputs);
  const next = view?.next ?? null;
  const quest: ScpsQuestFact = view?.quest ?? { kind: "none" };
  return { ...answer, collapsedHeadline: scpsCollapsedHeadline(next, quest) };
}

/** The pane's own calendar-arm request (#267): the standard horizon,
 * [`HORIZON_BEFORE_MS`] behind now and [`HORIZON_AFTER_DAYS`] ahead — the
 * same window `calendar/google/adapter.rs`'s `WINDOW_AFTER_DAYS` polls, not
 * the long horizon `vacation.ts` asks for. */
export function scpsCalendarInterval(nowMs: number): {
  startMs: number;
  endMs: number;
  startDate: string;
  endDate: string;
} {
  const startMs = nowMs - HORIZON_BEFORE_MS;
  const endMs = nowMs + HORIZON_AFTER_DAYS * DAY_MS;
  const startDate = deviceCivilToday(startMs) ?? new Date(startMs).toISOString().slice(0, 10);
  const endDay = deviceCivilToday(endMs) ?? new Date(endMs).toISOString().slice(0, 10);
  return { startMs, endMs, startDate, endDate: addOneDay(endDay) };
}

function addOneDay(date: string): string {
  const [year, month, day] = date.split("-").map(Number);
  const next = new Date(Date.UTC(year, month - 1, day + 1));
  return next.toISOString().slice(0, 10);
}
