// PROTOTYPE — throwaway. Delete with the rest of this directory (#122).
//
// The read-time merge #122 asks for: the coming weekend's window, and one
// list per day made of calendar-mirror events + items *scheduled* in the
// window + items *due* in the window. Nothing here is stored, and nothing
// here writes back onto an item — same contract `screens/urgency.ts` already
// holds for urgency (CONTEXT.md: "computed by consumers at read time").
//
// This module is the part of the prototype that is NOT throwaway thinking:
// the three variants disagree about how to render the merge, but they all
// call this, so the merge rules get decided once and judged three times.

import { computeUrgency, deadlineSortKey, type Urgency } from "../urgency";

/** A `TaskItemDTO` cut down to what the merge reads. Deliberately not the
 * real DTO: the pane needs four fields, and a fixture that had to fill in
 * twenty would hide which four actually decide anything. */
export interface WeekendItem {
  id: string;
  title: string;
  /** `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`, naive local (ADR-0009/0013). */
  deadline: string | null;
  /** The do-date the human chose. Day-only by construction — a preference
   * about which day, never a time, and never an input to urgency. */
  scheduledDate: string | null;
  projectName: string | null;
  size: "quick" | "short" | "deep" | null;
}

/** One calendar-mirror event (#46). The mirror is not built yet, so this
 * shape is a guess — the union of what a weekend pane could want — chosen
 * the same way `prototype-race-pane/fixture.ts` chose its payload. */
export interface WeekendEvent {
  id: string;
  title: string;
  startMs: number;
  endMs: number;
  allDay: boolean;
  calendarName: string;
}

export type EntryKind = "event" | "due" | "scheduled";

export interface WindowEntry {
  id: string;
  kind: EntryKind;
  title: string;
  /** Where this sorts inside its day. */
  atMs: number;
  /** `"time"` — the entry names a moment (an event, a minute-precision
   * deadline). `"day"` — it only names a day (a scheduled item, a day-only
   * deadline). A variant that renders a clock face has to know which. */
  anchor: "time" | "day";
  dayKey: string;
  event?: WeekendEvent;
  item?: WeekendItem;
  /** Due entries only, and the whole point of the dedupe rule: this item is
   * ALSO scheduled inside the window, and is rendered once, here, as due —
   * but the do-date it carries is a real fact the reader chose, so it is
   * kept rather than swallowed. */
  alsoScheduledOn?: string;
  /** Scheduled entries only: the item has a deadline, and it is OUTSIDE the
   * window. A do-date this weekend for something due next Wednesday is the
   * ordinary case, and a pane that dropped the deadline would read as if
   * the work had none. */
  deadlineOutsideWindow?: string;
}

export interface WeekendDay {
  key: string;
  /** "Saturday". */
  label: string;
  /** "Aug 15". */
  dateLabel: string;
  startMs: number;
  endMs: number;
  entries: WindowEntry[];
}

export interface WeekendWindow {
  startMs: number;
  endMs: number;
  days: WeekendDay[];
  /** True when `now` is already inside the weekend — the window is the one
   * under way, not the next one. See OPEN QUESTION below. */
  inProgress: boolean;
}

function startOfLocalDay(ms: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

function addLocalDays(ms: number, days: number): number {
  const d = new Date(ms);
  // Date-part arithmetic, not `+ n * 86400000` — a DST boundary inside the
  // window would slide a midnight by an hour and put a 00:30 event on the
  // wrong day.
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + days).getTime();
}

export function dayKeyOf(ms: number): string {
  const d = new Date(ms);
  const month = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${month}-${day}`;
}

/** Parses a naive-local `YYYY-MM-DD[THH:MM]` to an instant, resolving a
 * day-only value through `deadlineSortKey` (end of that day) exactly as
 * urgency does — the pane and the urgency dot must agree about what "due
 * Saturday" means, or a row can read as calm next to a deadline the same
 * screen calls overdue. */
export function deadlineToMs(deadline: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(deadlineSortKey(deadline));
  if (!match) return null;
  const [, y, m, d, h, min] = match;
  const at = new Date(Number(y), Number(m) - 1, Number(d), Number(h), Number(min));
  return Number.isNaN(at.getTime()) ? null : at.getTime();
}

/** A scheduled date is day-only by construction, so it anchors to the START
 * of its day — a do-date says "this day", and sorting it to 00:00 puts the
 * plan above the day's booked time rather than buried inside it. */
function scheduledToMs(scheduled: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(scheduled);
  if (!match) return null;
  const [, y, m, d] = match;
  const at = new Date(Number(y), Number(m) - 1, Number(d));
  return Number.isNaN(at.getTime()) ? null : at.getTime();
}

/**
 * The coming weekend, in local wall-clock time: Saturday 00:00 through
 * Sunday 23:59:59.999.
 *
 * OPEN QUESTION (deliberately left visible rather than decided here):
 *
 * 1. **Friday.** For most people the weekend starts Friday evening. This
 *    window does not include it, so a Friday-night dinner is invisible on
 *    Friday afternoon — the moment someone is most likely to ask the
 *    question. The `friday-evening` scenario exists to make that gap
 *    visible on screen.
 * 2. **Mid-weekend.** Asked on Saturday afternoon, "this coming weekend"
 *    is the weekend you are IN, not the next one — so the window clamps to
 *    the current weekend and drops the days already spent. Asked on Sunday
 *    night, that leaves a one-day window with two hours in it, which is
 *    honest but nearly useless; the alternative (roll to next weekend at
 *    some hour on Sunday) needs a threshold nobody has picked.
 */
export function weekendWindow(nowMs: number): WeekendWindow {
  const today = startOfLocalDay(nowMs);
  const dow = new Date(nowMs).getDay(); // 0 Sun … 6 Sat
  const inProgress = dow === 0 || dow === 6;

  const saturday = dow === 0 ? addLocalDays(today, -1) : addLocalDays(today, (6 - dow + 7) % 7);
  const sunday = addLocalDays(saturday, 1);

  // A weekend already under way shows only the days that are left. Saturday
  // morning's plans are not an answer to "what are my plans" asked on
  // Sunday.
  const dayStarts = [saturday, sunday].filter((start) => start >= today);

  const days = dayStarts.map((start) => {
    const at = new Date(start);
    return {
      key: dayKeyOf(start),
      label: at.toLocaleDateString([], { weekday: "long" }),
      dateLabel: at.toLocaleDateString([], { month: "short", day: "numeric" }),
      startMs: start,
      endMs: addLocalDays(start, 1) - 1,
      entries: [],
    };
  });

  return {
    startMs: days[0]?.startMs ?? saturday,
    endMs: days[days.length - 1]?.endMs ?? addLocalDays(sunday, 1) - 1,
    days,
    inProgress,
  };
}

function inWindow(ms: number, window: WeekendWindow): boolean {
  return ms >= window.startMs && ms <= window.endMs;
}

/**
 * The merge, run fresh on every render: events + due items + scheduled
 * items, grouped by day, chronological within a day.
 *
 * The dedupe rule is #122's own acceptance criterion — an item both
 * scheduled and due inside the window appears **once, as due** — and the
 * reason is the domain's, not a display convenience: a deadline is a
 * consequence and a do-date is a preference, so when both apply, the one
 * with consequences is what the day owes. The do-date survives on the entry
 * (`alsoScheduledOn`) because the human chose it; it is a note, not a
 * second row.
 *
 * Note what this does NOT do: an item due Saturday but scheduled for
 * Thursday still lands on Saturday. The window is the question ("what are
 * my plans this weekend"), and the deadline is what makes it a weekend
 * problem regardless of when someone meant to do it.
 */
export function mergeWindow(
  window: WeekendWindow,
  events: WeekendEvent[],
  items: WeekendItem[],
): WeekendWindow {
  const days = window.days.map((day) => ({ ...day, entries: [] as WindowEntry[] }));
  const byKey = new Map(days.map((day) => [day.key, day]));

  for (const event of events) {
    // An all-day event spanning both days belongs to both — it is a fact
    // about each day, and showing it once on Saturday would leave Sunday
    // reading as free.
    for (const day of days) {
      const overlaps = event.startMs <= day.endMs && event.endMs >= day.startMs;
      if (!overlaps) continue;
      day.entries.push({
        id: `${event.id}@${day.key}`,
        kind: "event",
        title: event.title,
        atMs: event.allDay ? day.startMs : Math.max(event.startMs, day.startMs),
        anchor: event.allDay ? "day" : "time",
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
    day.entries.sort((a, b) => a.atMs - b.atMs || a.kind.localeCompare(b.kind) || a.id.localeCompare(b.id));
  }

  return { ...window, days };
}

/** Urgency is read from the deadline and NOTHING else — the one line in
 * this prototype that proves #122's third criterion. Setting or clearing a
 * scheduled date re-runs the whole merge and can never move this dot. */
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
  if (!event || event.allDay) return "all day";
  const from = new Date(event.startMs).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  const to = new Date(event.endMs).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return `${from} – ${to}`;
}

/** Short "Sat" / "Sun" for a day key, for the plan chips. */
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

/** Items with a deadline in the window that the reader has NOT planned a
 * day for. Variant B and C both surface this as the pane's one call to
 * action — it is the only thing on screen a person can actually do
 * something about, and it is what the `scheduled_date` affordance is FOR. */
export function unplanned(window: WeekendWindow): WeekendItem[] {
  const out: WeekendItem[] = [];
  for (const day of window.days) {
    for (const entry of day.entries) {
      if (entry.kind === "due" && entry.item && !entry.alsoScheduledOn) {
        out.push(entry.item);
      }
    }
  }
  return out;
}
