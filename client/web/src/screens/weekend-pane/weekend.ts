import {
  weekendAnswerFromCore,
  weekendZoneQueriesFromCore,
  type PaneInputsSource,
} from "../../decisions/seam";
import { resolveZoneFacts } from "../questions/zone-bridge";
import type { CalendarEventDTO, TaskItemDTO } from "../../store/protocol";
import type { Band, PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";
import { computeUrgency, deadlineSortKey, type Urgency } from "../urgency";

// The weekend-plans pane (#122), answered over #245's pane shell — and
// since #534, **the web's rendering half, plus one pinned-not-called
// exception**.
//
// **What sank to `hummingbird_core::decisions::panes::weekend`, and is
// actually called from here**: `weekendAnswer`'s three decided fields
// (answerState/band/withinBand — `weekend_answer`), computed from the
// window, resolved through the zone bridge's `DEVICE_ZONE` rather than
// raw `Date` math, plus the gap kinds.
//
// **`weekendWindow` itself stays local TS, pinned rather than called.**
// `weekend.test.ts` calls `weekendWindow` at `describe`-body top level
// (`const window = weekendWindow(...)`, before any `it()` runs), which
// executes during vitest's synchronous test *collection* — before
// `wasm-setup.ts`'s `beforeAll` has resolved `initDecisions()`. Routing
// the window computation itself through the seam would throw the "used
// before ready" guard on every collection pass, the same
// module-evaluation-order trap `field-vocabulary.ts`'s vocab arrays hit
// at #500 — except here the constraint is describe-collection order
// rather than module-evaluation order. The resolution is the same:
// `weekendWindow` (and the trivial `weekendBand`/`weekendWithinBand`
// arithmetic built from it) stay literal TS, and `weekend.rs`'s own
// `weekend_window`/`weekend_band`/`weekend_within_band` are the pinned
// canonical definitions — cross-checked by
// `weekend-window.shared.test.ts` rather than called at runtime, on
// `field-vocabulary.test.ts`'s own precedent.
//
// **The full per-entry merge (`mergeWindow`) also stays here**, with its
// titles, ids and anchors — the decision only ever needs the *counts*
// (`weekend.rs`'s own module header), and every title/id crossing the
// seam with no decision reading it would be exactly the "do not re-cross
// whole DTOs" violation `inputs.rs`'s own discipline forbids.
// `entryUrgency` is also unsunk — it reads `computeUrgency`, which is
// already `hummingbird_core::decisions::urgency` (M1-2, #500) under a
// different name; there is nothing second to sink.

export const SUBJECT_KEY = "coming-weekend";
export const CALENDAR_REQUEST_KEY = "weekend";

const HOUR_MS = 60 * 60 * 1000;

function startOfLocalDay(ms: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

function addLocalDays(ms: number, days: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + days).getTime();
}

export function dayKeyOf(ms: number): string {
  const d = new Date(ms);
  const month = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${month}-${day}`;
}

export interface WeekendDay {
  key: string;
  label: string;
  dateLabel: string;
  startMs: number;
  endMs: number;
  entries: WindowEntry[];
}

export interface WeekendWindow {
  startMs: number;
  endMs: number;
  days: WeekendDay[];
  underWay: boolean;
}

/**
 * Friday 17:00 local through Sunday 23:59:59.999 local — #122's pinned
 * window, rolling forward from Sunday 20:00 local. Pinned against
 * `weekend.rs`'s `weekend_window` by `weekend-window.shared.test.ts`
 * rather than called through the seam — see the module header for why. */
export function weekendWindow(nowMs: number): WeekendWindow {
  const today = startOfLocalDay(nowMs);
  const dow = new Date(nowMs).getDay();
  const daysSinceLastFriday = (dow - 5 + 7) % 7;

  let fridayMidnight = addLocalDays(today, -daysSinceLastFriday);
  let sundayMidnight = addLocalDays(fridayMidnight, 2);
  const rollAtMs = sundayMidnight + 20 * HOUR_MS;

  if (nowMs >= rollAtMs) {
    fridayMidnight = addLocalDays(fridayMidnight, 7);
    sundayMidnight = addLocalDays(fridayMidnight, 2);
  }

  const startMs = fridayMidnight + 17 * HOUR_MS;
  const endMs = addLocalDays(sundayMidnight, 1) - 1;

  const days: WeekendDay[] = [0, 1, 2].map((offset) => {
    const dayStart = addLocalDays(fridayMidnight, offset);
    const at = new Date(dayStart);
    return {
      key: dayKeyOf(dayStart),
      label: at.toLocaleDateString([], { weekday: "long" }),
      dateLabel: at.toLocaleDateString([], { month: "short", day: "numeric" }),
      startMs: dayStart,
      endMs: addLocalDays(dayStart, 1) - 1,
      entries: [],
    };
  });

  return { startMs, endMs, days, underWay: nowMs >= startMs && nowMs <= endMs };
}

export type EntryKind = "event" | "due" | "scheduled";

export interface WindowEntry {
  id: string;
  kind: EntryKind;
  title: string;
  atMs: number;
  anchor: "time" | "day";
  dayKey: string;
  event?: CalendarEventDTO;
  item?: TaskItemDTO;
  alsoScheduledOn?: string;
  deadlineOutsideWindow?: string;
}

function inWindow(ms: number, window: WeekendWindow): boolean {
  const lowerMs = window.days[0]?.startMs ?? window.startMs;
  return ms >= lowerMs && ms <= window.endMs;
}

function deadlineToMs(deadline: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(deadlineSortKey(deadline));
  if (!match) return null;
  const [, y, m, d, h, min] = match;
  const at = new Date(Number(y), Number(m) - 1, Number(d), Number(h), Number(min));
  return Number.isNaN(at.getTime()) ? null : at.getTime();
}

function scheduledToMs(scheduled: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(scheduled);
  if (!match) return null;
  const [, y, m, d] = match;
  const at = new Date(Number(y), Number(m) - 1, Number(d));
  return Number.isNaN(at.getTime()) ? null : at.getTime();
}

/**
 * The merge, run fresh on every render, for the expanded rendering alone —
 * see the module header for why this is not the decision's own path.
 */
export function mergeWindow(
  window: WeekendWindow,
  events: readonly CalendarEventDTO[],
  items: readonly TaskItemDTO[],
): WeekendWindow {
  const days = window.days.map((day) => ({ ...day, entries: [] as WindowEntry[] }));
  const byKey = new Map(days.map((day) => [day.key, day]));

  for (const event of events) {
    if (event.status === "cancelled") continue;

    if (event.when.kind === "allDay") {
      const { startDate, endDate } = event.when;
      for (const day of days) {
        if (!(startDate <= day.key && day.key < endDate)) continue;
        day.entries.push({
          id: `${event.providerEventId}@${day.key}`,
          kind: "event",
          title: event.title,
          atMs: day.startMs,
          anchor: "day",
          dayKey: day.key,
          event,
        });
      }
      continue;
    }

    const overlapStart = Math.max(event.when.startMs, window.startMs);
    const overlapEnd = Math.min(event.when.endMs, window.endMs);
    if (overlapStart > overlapEnd) continue;

    for (const day of days) {
      const dayOverlapStart = Math.max(overlapStart, day.startMs);
      const dayOverlapEnd = Math.min(overlapEnd, day.endMs);
      if (dayOverlapStart > dayOverlapEnd) continue;
      day.entries.push({
        id: `${event.providerEventId}@${day.key}`,
        kind: "event",
        title: event.title,
        atMs: dayOverlapStart,
        anchor: "time",
        dayKey: day.key,
        event,
      });
    }
  }

  for (const item of items) {
    const dueMs = item.deadline ? deadlineToMs(item.deadline) : null;
    const scheduledMs = item.scheduledDate ? scheduledToMs(item.scheduledDate) : null;
    const dueHere = dueMs !== null && inWindow(dueMs, window);
    const scheduledHere = scheduledMs !== null && inWindow(scheduledMs, window);

    if (dueHere && dueMs !== null) {
      const day = byKey.get(dayKeyOf(dueMs));
      if (day) {
        day.entries.push({
          id: item.id,
          kind: "due",
          title: item.title,
          atMs: dueMs,
          anchor: item.deadline && item.deadline.length === 10 ? "day" : "time",
          dayKey: day.key,
          item,
          ...(scheduledHere && item.scheduledDate ? { alsoScheduledOn: item.scheduledDate } : {}),
        });
      }
      continue;
    }

    if (scheduledHere && scheduledMs !== null) {
      const day = byKey.get(dayKeyOf(scheduledMs));
      if (day) {
        day.entries.push({
          id: item.id,
          kind: "scheduled",
          title: item.title,
          atMs: scheduledMs,
          anchor: "day",
          dayKey: day.key,
          item,
          ...(item.deadline ? { deadlineOutsideWindow: item.deadline } : {}),
        });
      }
    }
  }

  for (const day of days) {
    day.entries.sort(
      (a, b) => a.atMs - b.atMs || a.kind.localeCompare(b.kind) || a.id.localeCompare(b.id),
    );
  }

  return { ...window, days };
}

export function entryUrgency(entry: WindowEntry, nowMs: number): Urgency {
  return computeUrgency(entry.item?.deadline ?? null, nowMs);
}

export function timeLabel(entry: WindowEntry): string {
  if (entry.kind === "scheduled") return "anytime";
  if (entry.kind === "due") {
    return entry.anchor === "day"
      ? "by end of day"
      : `by ${new Date(entry.atMs).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`;
  }
  const event = entry.event;
  if (!event || event.when.kind === "allDay") return "all day";
  const from = new Date(event.when.startMs).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
  });
  const to = new Date(event.when.endMs).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
  });
  return `${from} – ${to}`;
}

export function shortDayLabel(dayKey: string): string {
  const ms = scheduledToMs(dayKey);
  return ms === null ? dayKey : new Date(ms).toLocaleDateString([], { weekday: "short" });
}

export interface WindowCounts {
  events: number;
  due: number;
  scheduled: number;
}

export function countKinds(window: WeekendWindow): WindowCounts {
  const counts: WindowCounts = { events: 0, due: 0, scheduled: 0 };
  for (const day of window.days) {
    for (const entry of day.entries) {
      if (entry.kind === "event") counts.events += 1;
      else if (entry.kind === "due") counts.due += 1;
      else counts.scheduled += 1;
    }
  }
  return counts;
}

// -- the shell's answer (#245) ---------------------------------------------

export function weekendCalendarRead(inputs: QuestionInputs) {
  return inputs.calendarReads[CALENDAR_REQUEST_KEY];
}

/** `weekend.rs`'s `weekend_band`, exposed for rendering callers that
 * already hold a merged window — used only by tests today; the shell's
 * own answer goes through [`weekendAnswer`]. */
export function weekendBand(window: WeekendWindow, nowMs: number): Band {
  if (window.underWay) return "live";
  const untilStartMs = window.startMs - nowMs;
  const IMMINENT_WITHIN_MS = 48 * HOUR_MS;
  const NEAR_WITHIN_MS = 96 * HOUR_MS;
  if (untilStartMs <= IMMINENT_WITHIN_MS) return "imminent";
  if (untilStartMs <= NEAR_WITHIN_MS) return "near";
  return "dormant";
}

export function weekendWithinBand(window: WeekendWindow): number {
  return window.underWay ? window.endMs : window.startMs;
}

const NOTHING_HEADLINE_UNDER_WAY = "Clear so far";
const NOTHING_HEADLINE_AHEAD = "Nothing planned";

export function weekendCollapsedHeadline(window: WeekendWindow): string {
  const counts = countKinds(window);
  const total = counts.events + counts.due + counts.scheduled;
  if (total === 0) {
    return window.underWay ? NOTHING_HEADLINE_UNDER_WAY : NOTHING_HEADLINE_AHEAD;
  }
  const parts: string[] = [];
  if (counts.due > 0) parts.push(`${counts.due} due`);
  if (counts.events > 0) parts.push(`${counts.events} on the calendar`);
  if (counts.scheduled > 0) parts.push(`${counts.scheduled} planned`);
  return parts.join(" · ");
}

export function weekendGlyphs(window: WeekendWindow): PaneGlyph[] {
  const counts = countKinds(window);
  const glyphs: PaneGlyph[] = [];
  if (counts.due > 0) {
    glyphs.push({ kind: "icon", name: "flag", label: `${counts.due} due this weekend` });
  }
  if (counts.events > 0) {
    glyphs.push({ kind: "icon", name: "calendar-clock", label: `${counts.events} on the calendar` });
  }
  if (counts.scheduled > 0) {
    glyphs.push({ kind: "icon", name: "calendar", label: `${counts.scheduled} planned` });
  }
  return glyphs;
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

/** This question's answer for the shell (#245/#122). The three decided
 * fields come from `weekend.rs`'s `weekend_answer`; the headline and the
 * glyphs are composed here from a locally-merged window, exactly the cut
 * ADR-0025 draws through `PaneAnswer`. */
export function weekendAnswer(inputs: QuestionInputs): PaneAnswer {
  if (!inputs.calendarConnected) {
    return {
      answerState: "unbound",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: "Not set up",
      icon: [{ kind: "icon", name: "help-circle", label: "not set up" }],
    };
  }

  const queries = weekendZoneQueriesFromCore(inputs.nowMs);
  const facts = resolveZoneFacts(queries);
  const answer = weekendAnswerFromCore(paneInputs(inputs), facts);

  const read = weekendCalendarRead(inputs);
  if (read === undefined || read.state === "not_read") {
    return {
      ...answer,
      collapsedHeadline: "Checking calendar",
      icon: [{ kind: "icon", name: "cloud-fog", label: "checking calendar" }],
    };
  }

  const window = mergeWindow(weekendWindow(inputs.nowMs), read.events, inputs.items);
  return {
    ...answer,
    collapsedHeadline: weekendCollapsedHeadline(window),
    icon: weekendGlyphs(window),
  };
}

/** The merged window an answered pane's expanded rendering draws — `null`
 * for every gap state (including `!calendarConnected`), mirroring
 * `waste.ts`'s `wasteView`. */
export function weekendView(inputs: QuestionInputs): WeekendWindow | null {
  if (!inputs.calendarConnected) return null;
  const read = weekendCalendarRead(inputs);
  if (read === undefined || read.state === "not_read") {
    return null;
  }
  return mergeWindow(weekendWindow(inputs.nowMs), read.events, inputs.items);
}
