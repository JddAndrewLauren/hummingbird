import type { CalendarEventDTO, CalendarReadDTO, TaskItemDTO } from "../../store/protocol";
import type { Band, PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";
import { computeUrgency, deadlineSortKey, type Urgency } from "../urgency";

// The weekend-plans pane (#122), rewritten from
// `screens/prototype-weekend-pane/` (deleted with this slice — variant A
// won on 2026-08-10; see that directory's NOTES.md for the settled UI
// verdicts this module does not re-litigate).
//
// The merge, at read time, nothing stored: calendar events overlapping the
// window, items scheduled in the window and items due in the window,
// interleaved chronologically within each day. `scheduled_date` never feeds
// this pane's own urgency reading — `entryUrgency` below reads only
// `item.deadline`, the same read-time-only discipline `urgency.ts` documents
// for the frontier.

/** The one subject this question ever has — always present, even unbound,
 * so the setup prompt is discoverable (`waste.ts`'s own reasoning). */
export const SUBJECT_KEY = "coming-weekend";

/** The registry's `requiredCalendarRequests()` key, and the
 * `QuestionInputs.calendarReads` lookup key — this pane's own choice, since
 * the calendar mirror has no source vocabulary of its own to key on. */
export const CALENDAR_REQUEST_KEY = "weekend";

const HOUR_MS = 60 * 60 * 1000;

/** The window opens this many hours before it starts to read as `imminent`
 * rather than `near` — the same "read-time threshold beside the band
 * function" discipline `waste.ts`'s `STALE_AFTER_MS` documents. */
const IMMINENT_WITHIN_MS = 48 * HOUR_MS;

/** Beyond `IMMINENT_WITHIN_MS` but inside this reads as `near`; beyond it,
 * `dormant`. Four days: long enough that the pane goes quiet for most of a
 * normal week and wakes as the weekend actually approaches. */
const NEAR_WITHIN_MS = 96 * HOUR_MS;

function startOfLocalDay(ms: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

function addLocalDays(ms: number, days: number): number {
  const d = new Date(ms);
  // Date-part arithmetic, not `+ n * DAY_MS` — a DST boundary inside the
  // window would slide a midnight by an hour and put a late-evening entry
  // on the wrong day.
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
  /** "Friday". */
  label: string;
  /** "Aug 14". */
  dateLabel: string;
  /** Local midnight. */
  startMs: number;
  /** Local midnight of the next day, minus one millisecond. */
  endMs: number;
  entries: WindowEntry[];
}

export interface WeekendWindow {
  /** Friday 17:00 local. */
  startMs: number;
  /** Sunday 23:59:59.999 local. */
  endMs: number;
  /** Always exactly three days — Friday, Saturday, Sunday — even once some
   * are in the past: "what are my plans", not "what is left" (#122's own
   * acceptance criterion; contrast the prototype, which dropped days
   * already spent). */
  days: WeekendDay[];
  /** Whether `nowMs` falls inside `[startMs, endMs]` — the weekend answered
   * is the one under way, not the next one. */
  underWay: boolean;
}

/**
 * Friday 17:00 local through Sunday 23:59:59.999 local — #122's pinned
 * window (triaged 2026-08-10; do not re-litigate). Local means the
 * device's own zone, resolved as civil wall-clock time via plain `Date`
 * arithmetic — never a UTC-offset shortcut.
 *
 * Rolls forward to the *next* weekend from Sunday 20:00 local: the
 * threshold is computed once, as the current candidate window's own Sunday
 * 20:00, and if `nowMs` has already passed it the whole window shifts
 * forward by exactly one week. This is what makes a weekday's "coming
 * weekend" the upcoming Friday (the naive "last Friday on or before today"
 * candidate is always in the past by the time a weekday asks) and what
 * keeps a weekend already under way answering as itself, right up to
 * Sunday 19:59.
 */
export function weekendWindow(nowMs: number): WeekendWindow {
  const today = startOfLocalDay(nowMs);
  const dow = new Date(nowMs).getDay(); // 0 Sun … 6 Sat
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
  /** Where this sorts inside its day. */
  atMs: number;
  /** `"time"` — the entry names a moment. `"day"` — it only names a day (a
   * scheduled item, a day-only deadline, an all-day event). An all-day
   * event is always `"day"`, structurally: its `when` carries no instant
   * to anchor a time to. */
  anchor: "time" | "day";
  dayKey: string;
  event?: CalendarEventDTO;
  item?: TaskItemDTO;
  /** Due entries only — the dedupe rule's residue: this item is ALSO
   * scheduled inside the window, and is rendered once, here, as due, but
   * the do-date it carries is a real fact the reader chose and is kept
   * rather than swallowed. */
  alsoScheduledOn?: string;
  /** Scheduled entries only: the item has a deadline outside the window —
   * a do-date this weekend for something due later must still show that
   * it is due, or the work reads as having no deadline at all. */
  deadlineOutsideWindow?: string;
}

/** Day-membership test for an item's due/do-date, deliberately NOT
 * `[window.startMs, window.endMs]` — #122 review fix. `window.startMs` is
 * Friday **17:00** (the band's own "has the weekend started" instant), but
 * `window.days[0]` (Friday) spans the whole day from local midnight, and a
 * scheduled or due date anchors to the *start* of its day
 * (`scheduledToMs`/`deadlineToMs`'s day-only case). Testing against
 * `window.startMs` made every Friday do-date or day-only Friday deadline
 * fail `inWindow` even though `PlanChips` offers a `FRI` chip for it: the
 * write landed, the row either disappeared from the merge entirely or the
 * chip never filled, and there was no way to tell from the pane that
 * anything had happened. The lower bound here is the first day's own
 * `startMs` (Friday local midnight); the upper bound is unchanged, since
 * `window.days[2].endMs === window.endMs` already. */
function inWindow(ms: number, window: WeekendWindow): boolean {
  const lowerMs = window.days[0]?.startMs ?? window.startMs;
  return ms >= lowerMs && ms <= window.endMs;
}

/** Parses a naive-local `YYYY-MM-DD[THH:MM]` deadline to an instant,
 * resolving a day-only value through `deadlineSortKey` (end of that day)
 * exactly as `urgency.ts` does — the pane and the urgency dot must agree
 * about what "due Saturday" means. */
function deadlineToMs(deadline: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(deadlineSortKey(deadline));
  if (!match) return null;
  const [, y, m, d, h, min] = match;
  const at = new Date(Number(y), Number(m) - 1, Number(d), Number(h), Number(min));
  return Number.isNaN(at.getTime()) ? null : at.getTime();
}

/** A scheduled date is day-only by construction (ADR-0009/0013), so it
 * anchors to the START of its day — a do-date says "this day", never a
 * time, and sorts above that day's booked time rather than being buried
 * inside it. */
function scheduledToMs(scheduled: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(scheduled);
  if (!match) return null;
  const [, y, m, d] = match;
  const at = new Date(Number(y), Number(m) - 1, Number(d));
  return Number.isNaN(at.getTime()) ? null : at.getTime();
}

/**
 * The merge, run fresh on every render: events + due items + scheduled
 * items, grouped by day, chronological within a day. Nothing here is
 * stored, and nothing here writes back onto an item.
 *
 * The dedupe rule is #122's own acceptance criterion — an item both
 * scheduled and due inside the window appears **once, as due** (a deadline
 * is a consequence, a do-date is a preference, and when both apply, the one
 * with consequences is what the day owes) — and the inverse: an item
 * scheduled in the window but due *outside* it still shows its deadline.
 */
export function mergeWindow(
  window: WeekendWindow,
  events: readonly CalendarEventDTO[],
  items: readonly TaskItemDTO[],
): WeekendWindow {
  const days = window.days.map((day) => ({ ...day, entries: [] as WindowEntry[] }));
  const byKey = new Map(days.map((day) => [day.key, day]));

  for (const event of events) {
    if (event.status === "cancelled") continue; // defence in depth — the core already filters these.

    // The two arms are asked in their own terms and never converted into
    // one another (ADR-0015's 2026-08-10 amendment). An all-day event's
    // day membership is a pure string compare against the day's own
    // `YYYY-MM-DD` key — no zone, no instant, and so no way for a device
    // east or west of the calendar to land it on the wrong day. A timed
    // event overlaps in milliseconds, rendered with plain `Date` in the
    // reader's device zone, which is the only zone anywhere in this pane.
    //
    // Either way, an event spanning more than one day of the window
    // belongs to each of them — a fact about each day, so showing it once
    // would leave the other day reading as free.
    if (event.when.kind === "allDay") {
      const { startDate, endDate } = event.when;
      for (const day of days) {
        if (!(startDate <= day.key && day.key < endDate)) continue; // exclusive end date.
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
      continue; // the dedupe: never also emitted as scheduled.
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

/** Urgency is read from the item's deadline and NOTHING else — #122's own
 * acceptance criterion: setting or clearing a do-date re-runs the whole
 * merge and can never move this dot. */
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

/** Short "Fri" / "Sat" / "Sun" for a day key, for the plan chips. */
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

/** This request's read — never a second calendar read (#267 owns the
 * seam; this pane only ever reads through `QuestionInputs.calendarReads`). */
export function weekendCalendarRead(inputs: QuestionInputs): CalendarReadDTO | undefined {
  return inputs.calendarReads[CALENDAR_REQUEST_KEY];
}

/** How soon the window matters — never `distant`: the window is at most a
 * handful of days away by construction (`weekendWindow`'s own rollover), so
 * the fifth band never applies (ADR-0015's own tripwire on that word). */
export function weekendBand(window: WeekendWindow, nowMs: number): Band {
  if (window.underWay) return "live";
  const untilStartMs = window.startMs - nowMs;
  if (untilStartMs <= IMMINENT_WITHIN_MS) return "imminent";
  if (untilStartMs <= NEAR_WITHIN_MS) return "near";
  return "dormant";
}

/** Epoch ms of the window's next relevant moment — its start while that is
 * still ahead, its end once the weekend is under way (so a live weekend
 * still under way sorts by how much longer it has left, not by when it
 * began). */
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

/** This question's answer for the shell (#245/#122).
 *
 * Answer state reads `calendarConnected` first — #122 review fix. The
 * criterion the brief draws is "no calendar → `unbound`" vs. "no snapshot →
 * `bound-but-unacquired`", and those are two different facts:
 * `CalendarState.connected` (`store/store.ts`) is "has this device ever
 * connected a calendar", while the calendar arm's `"not_read"` state is the
 * core's "no snapshot at all" — which is ALSO true of a connected device
 * that has not polled yet, is offline, or is sitting on `needsReconnect`.
 * Collapsing `"not_read"` straight to `unbound` told an already-configured
 * reader to go set the pane up. So: `!calendarConnected` is the only path to
 * `unbound`; a missing calendar-arm entry (never requested / a busy core the
 * worker dropped) or a connected-but-unacquired `"not_read"` read are both
 * `bound-but-unacquired` — the same "the table hasn't answered yet" reading
 * `waste.ts`'s `"unread"` pane read gives — and a real `"read"` answer is
 * always `answered`, including a genuinely empty weekend: a question that
 * answered nothing has not failed, and must not sort with the failures. */
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

  const read = weekendCalendarRead(inputs);
  if (read === undefined || read.state === "not_read") {
    return {
      answerState: "bound-but-unacquired",
      band: "dormant",
      withinBand: null,
      collapsedHeadline: "Checking calendar",
      icon: [{ kind: "icon", name: "cloud-fog", label: "checking calendar" }],
    };
  }

  const window = mergeWindow(weekendWindow(inputs.nowMs), read.events, inputs.items);

  return {
    answerState: "answered",
    band: weekendBand(window, inputs.nowMs),
    withinBand: weekendWithinBand(window),
    collapsedHeadline: weekendCollapsedHeadline(window),
    icon: weekendGlyphs(window),
  };
}

/** The merged window an answered pane's expanded rendering draws — `null`
 * for every gap state (including `!calendarConnected`, #122 review fix),
 * mirroring `waste.ts`'s `wasteView`. */
export function weekendView(inputs: QuestionInputs): WeekendWindow | null {
  if (!inputs.calendarConnected) return null;
  const read = weekendCalendarRead(inputs);
  if (read === undefined || read.state === "not_read") {
    return null;
  }
  return mergeWindow(weekendWindow(inputs.nowMs), read.events, inputs.items);
}
