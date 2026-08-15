// The board world's demo calendar (#452, piece 4) — a seeded `CalendarState`
// for Settings' calendar card, and NOWHERE else.
//
// **Read this before wiring it up anywhere new.** `calendar` lives on
// `CoreState`, not `TaskState`, and `App.tsx`'s slice of it feeds
// `useCalendarWiring`, which runs REAL effects on whatever it is given: a
// selection push, a 15-minute poll timer, and token rotation. A seeded
// `connected: true` handed to that hook would start polling against a
// worker holding no token, and the hook's own `setCalendarState` writes
// would be silently swallowed by the override — producing a state neither
// world actually has. `TaskState` has no such hook (`demoTaskState()` is
// exactly as safe as a plain value gets); `CalendarState` does, which is why
// this fixture is injected at the PROP boundary — `SettingsScreen`'s
// `calendar` prop only — and never substituted at the store read the way
// `demoTask` overrides `liveTask`. `useCalendarWiring` keeps reading the
// live store slice, unconditionally, in every mode.
//
// Dev-only, gated twice, same as the kit world and the board `TaskState` —
// see `demo.ts`. Built inside a function for the same bundling reason
// `demo-task-state.ts`'s header documents: no top-level clock read, no const
// built at import.

import type { CalendarState } from "../store/store";

export function buildDemoCalendarState(): CalendarState {
  return {
    connected: true,
    needsReconnect: false,
    // Nothing designates a Trips calendar in this world — `SettingsScreen`
    // reads that through `task.bindings`, which the board `TaskState` seeds
    // separately (`demo-pane-reads.ts`'s `boundRaceBinding`/`boundWasteBinding`
    // plus its own trips binding), so this stays empty rather than guessing.
    selectedCalendarIds: [],
    availableCalendars: [
      { id: "demo-personal", summary: "Fictional (personal)" },
      { id: "demo-family", summary: "Fictional (family)" },
      { id: "demo-work", summary: "Fictional (work)" },
    ],
    lastPollOutcome: "succeeded",
    connectPending: false,
    connectError: null,
    silentRemintBlocked: false,
    eventReads: {},
  };
}
