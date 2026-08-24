// @vitest-environment jsdom

// The board world's calendar card (#452, piece 4) has no reader unless it
// survives `SettingsScreen`'s live-only preconditions — until #585 that was
// `VITE_GOOGLE_CLIENT_ID` unset (the vitest default this file deliberately
// did not stub away); now it is `taskTokenState === "unset"` (#585: the
// gates key off whether this device holds a device token, not off a
// build-time env var, which nothing in `client/web` reads any more).
// `SettingsScreen.test.tsx` defaults `taskTokenState` to `"resting"` for
// every one of its cases, which is right for that file's own concerns but
// would hide a regression here: this pins that `calendarIsDemo` bypasses the
// "no device token" and "core not ready" gates, so the fixture calendar
// still renders under `?demo=board` on a device with no device token at
// all — piece 4's actual reader.

import { describe, expect, it, vi } from "vitest";

import { SettingsScreen } from "./SettingsScreen";
import { fireEvent, render, screen, taskState } from "../test/component";
import type { CalendarState } from "../store/store";
import type { TaskTokenUiState } from "../task/token-ui";

const demoCalendar: CalendarState = {
  connected: true,
  needsReconnect: false,
  selectedCalendarIds: [],
  availableCalendars: [
    { id: "demo-personal", summary: "Fictional (personal)" },
    { id: "demo-family", summary: "Fictional (family)" },
  ],
  lastPollOutcome: "succeeded",
  connectPending: false,
  connectError: null,
  silentRemintBlocked: false,
  eventReads: {},
};

function renderBoardSettings(
  status: "loading" | "ready" = "loading",
  taskTokenState: TaskTokenUiState = "unset",
) {
  const onSelectionChange = vi.fn();
  render(
    <SettingsScreen
      status={status}
      apiVersion={null}
      coreId={null}
      viewOrdinal={null}
      error={null}
      calendar={demoCalendar}
      calendarIsDemo
      themePreference="system"
      onThemePreference={vi.fn()}
      backendSelection="auto"
      onBackendSelection={vi.fn()}
      onConnect={vi.fn()}
      onSelectionChange={onSelectionChange}
      onRefresh={vi.fn()}
      taskTokenState={taskTokenState}
      taskTokenEnteredAtMs={null}
      onSubmitTaskToken={vi.fn()}
      onForgetTaskToken={vi.fn()}
      task={taskState()}
      online
      syncNowMs={10_000}
      onDownloadMirror={vi.fn()}
      onDownloadDiagnostics={vi.fn()}
      onClearDiagnostics={vi.fn()}
    />,
  );
  return { onSelectionChange };
}

describe("SettingsScreen — the board world's calendar card", () => {
  it("renders the fixture calendars with no device token on this device", () => {
    // `calendarIsDemo` bypasses the device-token gate entirely — the board
    // fixture is not a live connection and needs no token to show.
    renderBoardSettings("loading", "unset");
    expect(screen.queryByText(/no device token/i)).toBeNull();
    expect(screen.getByText("Fictional (personal)")).toBeDefined();
    expect(screen.getByText("Fictional (family)")).toBeDefined();
  });

  it("renders the fixture calendars even while the core is not ready", () => {
    renderBoardSettings("loading", "unset");
    expect(screen.queryByText(/calendar context is unavailable/i)).toBeNull();
    expect(screen.getByText("Fictional (personal)")).toBeDefined();
  });

  // The round-2 hazard (#452's own words): the fixture card's toggle must
  // never reach `onSelectionChange`, whose live handler persists the ids to
  // the shared localStorage key and polls Google for calendars that do not
  // exist. `SettingsScreen` has no `demo` prop (#456), so the toggle must
  // branch on `calendarIsDemo`.
  it("toggles locally and never calls onSelectionChange", () => {
    const { onSelectionChange } = renderBoardSettings("loading", "unset");
    const family = screen.getByRole("checkbox", { name: /Fictional \(family\)/ });
    fireEvent.click(family);
    expect(onSelectionChange).not.toHaveBeenCalled();
    expect((family as HTMLInputElement).checked).toBe(true);
  });

  // The board world has no bindings table to read (`demo-calendar.ts`), so no
  // row may render locked off the live `task.bindings`— every fixture row
  // stays toggleable.
  it("locks no row off the live bindings", () => {
    renderBoardSettings("loading", "unset");
    for (const name of [/Fictional \(personal\)/, /Fictional \(family\)/]) {
      expect((screen.getByRole("checkbox", { name }) as HTMLInputElement).disabled).toBe(false);
    }
  });

  it("still gates a live (non-demo) render on the device token, unchanged", () => {
    render(
      <SettingsScreen
        status="ready"
        apiVersion={1}
        coreId={null}
        viewOrdinal={null}
        error={null}
        calendar={{ ...demoCalendar, availableCalendars: [] }}
        calendarIsDemo={false}
        themePreference="system"
        onThemePreference={vi.fn()}
        backendSelection="auto"
        onBackendSelection={vi.fn()}
        onConnect={vi.fn()}
        onSelectionChange={vi.fn()}
        onRefresh={vi.fn()}
        taskTokenState="unset"
        taskTokenEnteredAtMs={null}
        onSubmitTaskToken={vi.fn()}
        onForgetTaskToken={vi.fn()}
        task={taskState()}
        online
        syncNowMs={10_000}
        onDownloadMirror={vi.fn()}
        onDownloadDiagnostics={vi.fn()}
        onClearDiagnostics={vi.fn()}
      />,
    );
    // `status: "ready"` here also renders the device-token card's own
    // "No device token yet" sentence, so this pins the *calendar* section's
    // wording specifically rather than any "no device token" text on screen.
    expect(
      screen.getByText(/calendar context is unavailable: this device has no device token/i),
    ).toBeDefined();
  });
});
