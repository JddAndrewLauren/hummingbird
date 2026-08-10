// PROTOTYPE — throwaway. Delete with the rest of this directory (#122).
//
// Scenario data for the weekend pane. Two halves, matching the two lanes
// #122 merges:
//
//   - `calendar` — the calendar mirror (#46), which does not exist yet. The
//     event shape is a GUESS, chosen as the union of what a weekend pane
//     could want (title, start/end, all-day, which calendar it came from),
//     the same way `prototype-race-pane/fixture.ts` guessed its payload.
//     `null` means a device that never opted in — a state the pane has to
//     render as *partial*, not empty.
//   - `items` — mirror rows, cut down to `WeekendItem`. `deadline` and
//     `scheduledDate` are naive-local strings exactly as ADR-0009/0013
//     defines them.
//
// Everything is anchored to fixed LOCAL wall-clock instants (not `Date.UTC`)
// because the window itself is local: "Saturday" is a local day, and a
// fixture built in UTC would put a 9am event on the wrong side of midnight
// for half the world.

import type { WeekendEvent, WeekendItem } from "./weekend";

export interface WeekendScenario {
  key: string;
  label: string;
  /** What this scenario is here to expose. */
  note: string;
  nowMs: number;
  /** `null` on a device that never connected a calendar. */
  calendar: { asOfMs: number; events: WeekendEvent[] } | null;
  items: WeekendItem[];
}

const MIN = 60_000;
const HOUR = 60 * MIN;

// Monday 2026-08-10, 09:00 local. The coming weekend is Sat 15 / Sun 16.
const MONDAY_9AM = new Date(2026, 7, 10, 9, 0).getTime();

function at(day: number, hour: number, minute = 0): number {
  return new Date(2026, 7, day, hour, minute).getTime();
}

function event(
  id: string,
  title: string,
  day: number,
  fromHour: number,
  toHour: number,
  calendarName = "Personal",
): WeekendEvent {
  return {
    id,
    title,
    startMs: at(day, fromHour),
    endMs: at(day, toHour),
    allDay: false,
    calendarName,
  };
}

function allDay(id: string, title: string, fromDay: number, toDay: number, calendarName = "Personal"): WeekendEvent {
  return {
    id,
    title,
    startMs: at(fromDay, 0),
    endMs: at(toDay, 23, 59),
    allDay: true,
    calendarName,
  };
}

function item(
  id: string,
  title: string,
  deadline: string | null,
  scheduledDate: string | null,
  projectName: string | null = null,
  size: WeekendItem["size"] = null,
): WeekendItem {
  return { id, title, deadline, scheduledDate, projectName, size };
}

const SAT = "2026-08-15";
const SUN = "2026-08-16";

// The everyday case, and the one that carries the dedupe proof:
// `ION-141` is both due Saturday and scheduled Saturday, and must appear
// exactly once, as due (#122's fourth criterion).
const TYPICAL_ITEMS: WeekendItem[] = [
  item("ION-141", "Renew the car insurance", SAT, SAT, "Admin", "quick"),
  item("ION-142", "File the Q3 expenses", `${SUN}T17:00`, null, "Admin", "short"),
  item("ION-143", "Repot the olive tree", null, SAT, "Garden", "short"),
  item("ION-144", "Draft the trip itinerary", "2026-08-19", SUN, "Portugal", "deep"),
  item("ION-145", "Replace the bathroom bulb", null, SUN, null, "quick"),
  // Neither due nor scheduled in the window — must not appear at all.
  item("ION-146", "Rewrite the onboarding doc", "2026-09-02", "2026-08-24", "Work", "deep"),
];

const TYPICAL_EVENTS: WeekendEvent[] = [
  event("ev-1", "Parkrun", 15, 9, 10),
  event("ev-2", "Brunch with Sam", 15, 11, 13),
  event("ev-3", "Nia's birthday party", 15, 15, 17, "Family"),
  event("ev-4", "Call Mum", 16, 18, 19, "Family"),
];

export const SCENARIOS: WeekendScenario[] = [
  {
    key: "typical",
    label: "Typical week",
    note: "Asked on Monday. ION-141 is due AND scheduled Saturday — it must appear once, as due.",
    nowMs: MONDAY_9AM,
    calendar: { asOfMs: MONDAY_9AM - 4 * MIN, events: TYPICAL_EVENTS },
    items: TYPICAL_ITEMS,
  },
  {
    key: "packed",
    label: "Packed weekend",
    note: "Eleven events, five deadlines, an all-day trip across both days. Density and the all-day span.",
    nowMs: MONDAY_9AM,
    calendar: {
      asOfMs: MONDAY_9AM - 2 * MIN,
      events: [
        allDay("ev-10", "Cousins visiting", 15, 16, "Family"),
        event("ev-11", "Parkrun", 15, 9, 10),
        event("ev-12", "Farmers market", 15, 10, 11),
        event("ev-13", "Brunch with Sam", 15, 11, 13),
        event("ev-14", "Nia's birthday party", 15, 15, 17, "Family"),
        event("ev-15", "Dinner, the Reids", 15, 19, 22, "Family"),
        event("ev-16", "Swim lesson", 16, 8, 9, "Family"),
        event("ev-17", "Church", 16, 10, 11),
        event("ev-18", "Lunch, Gran", 16, 12, 14, "Family"),
        event("ev-19", "Football", 16, 15, 17),
        event("ev-20", "Call Mum", 16, 18, 19, "Family"),
      ],
    },
    items: [
      ...TYPICAL_ITEMS,
      item("ION-150", "Pay the council tax", `${SAT}T12:00`, null, "Admin", "quick"),
      item("ION-151", "Return the router", SAT, null, "Admin", "quick"),
      item("ION-152", "Sign the school forms", SUN, SAT, "Family", "quick"),
      item("ION-153", "Book the MOT", SUN, null, "Admin", "quick"),
    ],
  },
  {
    key: "quiet",
    label: "Quiet weekend",
    note: "Nothing booked, nothing due, three do-dates. An empty calendar is good news, not an apology.",
    nowMs: MONDAY_9AM,
    calendar: { asOfMs: MONDAY_9AM - 6 * MIN, events: [] },
    items: [
      item("ION-143", "Repot the olive tree", null, SAT, "Garden", "short"),
      item("ION-145", "Replace the bathroom bulb", null, SUN, null, "quick"),
      item("ION-147", "Sort the loft", null, SUN, "House", "deep"),
    ],
  },
  {
    key: "nothing",
    label: "Nothing at all",
    note: "No events, no deadlines, no do-dates. The pane still has to say something true.",
    nowMs: MONDAY_9AM,
    calendar: { asOfMs: MONDAY_9AM - 3 * MIN, events: [] },
    items: [item("ION-146", "Rewrite the onboarding doc", "2026-09-02", "2026-08-24", "Work", "deep")],
  },
  {
    key: "no-calendar",
    label: "No calendar",
    note: "Device never opted in. Half the answer is missing and the pane must SAY so, not render as quiet.",
    nowMs: MONDAY_9AM,
    calendar: null,
    items: TYPICAL_ITEMS,
  },
  {
    key: "stale",
    label: "Stale mirror",
    note: "Calendar mirror last polled 9h ago. Keep showing it, say its age (the ContextTile habit).",
    nowMs: MONDAY_9AM,
    calendar: { asOfMs: MONDAY_9AM - 9 * HOUR, events: TYPICAL_EVENTS },
    items: TYPICAL_ITEMS,
  },
  {
    key: "friday",
    label: "Friday evening",
    note: "Asked at 5pm Friday — when the question is most likely asked. Friday dinner is OUTSIDE the window.",
    nowMs: at(14, 17, 0),
    calendar: {
      asOfMs: at(14, 16, 50),
      events: [event("ev-30", "Dinner, the Reids", 14, 19, 22, "Family"), ...TYPICAL_EVENTS],
    },
    items: TYPICAL_ITEMS,
  },
  {
    key: "mid",
    label: "Saturday 2pm",
    note: "The weekend is under way. Window drops Saturday morning; Nia's party is 1h out.",
    nowMs: at(15, 14, 0),
    calendar: { asOfMs: at(15, 13, 55), events: TYPICAL_EVENTS },
    items: TYPICAL_ITEMS,
  },
  {
    key: "sunday-night",
    label: "Sunday 9pm",
    note: "The degenerate window: two hours left. Honest, nearly useless — does it roll to next weekend?",
    nowMs: at(16, 21, 0),
    calendar: { asOfMs: at(16, 20, 55), events: TYPICAL_EVENTS },
    items: TYPICAL_ITEMS,
  },
];

export function scenarioByKey(key: string): WeekendScenario {
  return SCENARIOS.find((entry) => entry.key === key) ?? SCENARIOS[0];
}
