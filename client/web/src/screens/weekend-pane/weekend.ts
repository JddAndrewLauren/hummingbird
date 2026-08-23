import {
  weekendAnswerFromCore,
  weekendFactsFromCore,
  weekendZoneQueriesFromCore,
  type PaneInputsSource,
  type WeekendEntryCore,
} from "../../decisions/seam";
import { resolveZoneFacts } from "../questions/zone-bridge";
import type { CalendarEventDTO, TaskItemDTO } from "../../store/protocol";
import type { Band, PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";
import { computeUrgency, type Urgency } from "../urgency";

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
// **The per-entry merge sank at #564**, and `mergeWindow` below is now a
// call through the seam plus the re-attachment this side alone can do.
// It stayed local at #534 on the grounds that no decision read an entry's
// `id`/`title`/`atMs`/`anchor` — true, and the right call with one client.
// Android's expanded weekend rendering is the second caller ADR-0025's own
// tie-breaker asks for, and the dedupe rule (due beats scheduled) is
// exactly the sort of rule two hand-written merges would each have to get
// right. What still does not cross is the DTOs: the core hands back a
// `sourceId`, and the events and items are re-attached here, where they
// already are.
//
// **The window shrinks as the weekend is spent.** `days` used to be
// Friday/Saturday/Sunday for the whole life of the window; a day now
// leaves it at its own local midnight, so on Sunday the pane draws Sunday
// alone and an unfinished item that was due or scheduled on a dropped day
// goes with it. The filter lives in `weekendWindow` below, not in
// `WeekendPaneExpanded.tsx` — everything downstream (the merge, the
// counts, the headline, the glyphs, the plan chips) reads `days`, so one
// filter moves all of them, and `weekend.rs`'s `weekend_window` carries
// the identical `retain` on the other side of the pin.
//
// `entryUrgency` is still unsunk — it reads `computeUrgency`, which is
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
  label: string;
  dateLabel: string;
  startMs: number;
  endMs: number;
  entries: WindowEntry[];
}

export interface WeekendWindow {
  startMs: number;
  endMs: number;
  /** The window's days that have not yet ended at the device, in window
   * order — Friday/Saturday/Sunday while the weekend is still ahead, then
   * shrinking by one as each day's own local midnight passes. Never empty
   * (the rollover precedes Sunday's end). `weekend.rs`'s
   * `WeekendWindow::days`, mirrored — that field's doc is canonical. */
  days: WeekendDay[];
  underWay: boolean;
}

/**
 * Friday 17:00 local through Sunday 23:59:59.999 local — #122's pinned
 * window, rolling forward from Sunday 20:00 local. Pinned against
 * `weekend.rs`'s `weekend_window` by `weekend-window.shared.test.ts`
 * rather than called through the seam — see the module header for why.
 *
 * `startMs`/`endMs`/`underWay` are facts about the whole weekend and never
 * shrink; `days` does, dropping each day as it ends (its own doc above). */
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

  const days: WeekendDay[] = [0, 1, 2]
    .map((offset) => {
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
    })
    // The shrink, mirroring `weekend_window`'s own `retain`: a day leaves
    // the window at its own end. Filtering here rather than in the
    // renderer is the whole point — `mergeWindow` below maps over these
    // days, so the columns, the plan chips, the counts and the headline
    // all follow from one place, on both hosts.
    .filter((day) => day.endMs >= nowMs);

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

function scheduledToMs(scheduled: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(scheduled);
  if (!match) return null;
  const [, y, m, d] = match;
  const at = new Date(Number(y), Number(m) - 1, Number(d));
  return Number.isNaN(at.getTime()) ? null : at.getTime();
}

/** One core entry, re-attached to whichever DTO it came from.
 *
 * `event`/`item` are the whole reason this mapping exists: `timeLabel`
 * reads the event's own `when`, and `entryUrgency` reads the item's
 * deadline. Neither crosses the seam (`weekend.rs`'s module header — a
 * DTO does not go out any more than it comes in), so `sourceId` is what
 * does, and the lookup happens here, where both maps already are. */
function toWindowEntry(
  entry: WeekendEntryCore,
  eventsById: ReadonlyMap<string, CalendarEventDTO>,
  itemsById: ReadonlyMap<string, TaskItemDTO>,
): WindowEntry {
  const attached =
    entry.kind === "event"
      ? { event: eventsById.get(entry.sourceId) }
      : { item: itemsById.get(entry.sourceId) };
  return {
    id: entry.id,
    kind: entry.kind,
    title: entry.title,
    atMs: entry.atMs,
    anchor: entry.anchor,
    dayKey: entry.dayKey,
    ...attached,
    ...(entry.alsoScheduledOn !== null ? { alsoScheduledOn: entry.alsoScheduledOn } : {}),
    ...(entry.deadlineOutsideWindow !== null
      ? { deadlineOutsideWindow: entry.deadlineOutsideWindow }
      : {}),
  };
}

/**
 * The merge: events + due items + scheduled items, grouped by day and in
 * display order within a day — `weekend.rs`'s `merge_window` since #564,
 * with the DTOs re-attached here.
 *
 * The dedupe rule is #122's own acceptance criterion — an item both
 * scheduled and due inside the window appears **once, as due** — and its
 * inverse (scheduled inside, due outside, deadline still shown) rides the
 * surviving entry. Both now live in the core alone.
 *
 * **`nowMs` is the first still-ahead day's own midnight**, not a sampled
 * clock: the core computes its own window from the instant it is given,
 * and that instant re-derives the identical one — same weekend (it is
 * inside it, or is its Friday midnight before it opens) and the same days,
 * because every day the caller's window still holds ends at or after it.
 * Sampling `Date.now()` here instead would let a merge run against next
 * weekend's window on a Sunday evening render — the exact rollover this
 * function's caller has already decided. `window.startMs` (Friday 17:00),
 * which this used before the days began to shrink, would now re-derive a
 * *wider* window than the caller's on a Saturday or Sunday.
 */
export function mergeWindow(
  window: WeekendWindow,
  events: readonly CalendarEventDTO[],
  items: readonly TaskItemDTO[],
): WeekendWindow {
  const nowMs = window.days[0]?.startMs ?? window.startMs;
  const facts = resolveZoneFacts(weekendZoneQueriesFromCore(nowMs));
  const resolved = weekendFactsFromCore(
    {
      nowMs,
      bindings: null,
      paneReads: {},
      calendarReads: {
        [CALENDAR_REQUEST_KEY]: {
          state: "read",
          events: [...events],
          freshness: { kind: "unknown" },
        },
      },
      // The gap states are the *caller's* to decide — `weekendAnswer` and
      // `weekendView` both check them before ever reaching here — so this
      // asks the core the merge question alone.
      calendarConnected: true,
      items: [...items],
    },
    facts,
  );

  const emptyDays = window.days.map((day) => ({ ...day, entries: [] as WindowEntry[] }));
  if (resolved.kind !== "facts") {
    // Only reachable when the device's own zone will not resolve, which
    // is the same nothing-to-show the callers already render.
    return { ...window, days: emptyDays };
  }

  const eventsById = new Map(events.map((event) => [event.providerEventId, event]));
  const itemsById = new Map(items.map((item) => [item.id, item]));
  const days = window.days.map((day) => ({
    ...day,
    entries: (resolved.days.find((core) => core.date === day.key)?.entries ?? []).map((entry) =>
      toWindowEntry(entry, eventsById, itemsById),
    ),
  }));
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

/** The window opens this many hours before it starts to read as `imminent`
 * rather than `near`. Kept literal TS on `weekendWindow`'s own
 * describe-collection-order reasoning (the module header above) — pinned
 * against `weekend_constants_json()`'s `imminentWithinMs` by
 * `seam.test.ts`, not read through the seam at runtime. */
export const IMMINENT_WITHIN_MS = 48 * HOUR_MS;

/** Beyond `IMMINENT_WITHIN_MS` but inside this reads as `near`; beyond it,
 * `dormant`. Pinned against `weekend_constants_json()`'s `nearWithinMs`,
 * same reason. */
export const NEAR_WITHIN_MS = 96 * HOUR_MS;

/** `weekend.rs`'s `weekend_band` — kept literal TS for the same
 * describe-collection-order reason `weekendWindow` is (this module's own
 * header), pinned against the core directly by
 * `weekend-window.shared.test.ts` rather than called through the seam.
 *
 * **No production caller.** `weekendAnswer` gets its band from
 * `weekendAnswerFromCore` (the real decision), not from this — this export
 * exists so a pin test can hold the local arithmetic against the core's
 * own `weekend_band` directly, on `wasteSetup`-adjacent test-only exports
 * elsewhere in this family. Kept exported (rather than folded into the pin
 * test itself) because a caller-side desync between this and
 * `weekendAnswerFromCore` is exactly the class of bug ADR-0025 exists to
 * catch, and a private copy inside a test file could drift unnoticed. */
export function weekendBand(window: WeekendWindow, nowMs: number): Band {
  if (window.underWay) return "live";
  const untilStartMs = window.startMs - nowMs;
  if (untilStartMs <= IMMINENT_WITHIN_MS) return "imminent";
  if (untilStartMs <= NEAR_WITHIN_MS) return "near";
  return "dormant";
}

/** `weekend.rs`'s `weekend_within_band` — same reason, same pin, and the
 * same "no production caller" note as [`weekendBand`]. */
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
