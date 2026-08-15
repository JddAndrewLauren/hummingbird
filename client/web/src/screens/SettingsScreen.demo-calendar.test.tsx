// @vitest-environment jsdom

// The board world's calendar card (#452, piece 4) has no reader in the
// environment #454 photographs unless it survives `useCalendarWiring`'s
// live-only preconditions with no `VITE_GOOGLE_CLIENT_ID` set — exactly the
// vitest default this file, deliberately, does NOT stub away.
// `SettingsScreen.test.tsx` mocks `GOOGLE_CLIENT_ID` to a truthy value for
// every one of its cases, which is right for that file's own concerns but
// would hide a regression here: this pins that `calendarIsDemo` bypasses the
// "no Google client id" and "core not ready" gates, so the fixture calendar
// still renders under `?demo=board` in a build with no client id configured
// at all — piece 4's actual reader.

import { describe, expect, it, vi } from "vitest";

import { SettingsScreen } from "./SettingsScreen";
import { render, screen, taskState } from "../test/component";
import type { CalendarState } from "../store/store";

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

function renderBoardSettings(status: "loading" | "ready" = "loading") {
  return render(
    <SettingsScreen
      demo={null}
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
      onSelectionChange={vi.fn()}
      onRefresh={vi.fn()}
      taskTokenState="resting"
      taskTokenEnteredAtMs={null}
      onSubmitTaskToken={vi.fn()}
      onForgetTaskToken={vi.fn()}
      task={taskState()}
      online
      syncNowMs={10_000}
      onDownloadMirror={vi.fn()}
    />,
  );
}

describe("SettingsScreen — the board world's calendar card", () => {
  it("renders the fixture calendars with no Google client id configured", () => {
    renderBoardSettings("loading");
    expect(screen.queryByText(/no Google client id/i)).toBeNull();
    expect(screen.getByText("Fictional (personal)")).toBeDefined();
    expect(screen.getByText("Fictional (family)")).toBeDefined();
  });

  it("renders the fixture calendars even while the core is not ready", () => {
    renderBoardSettings("loading");
    expect(screen.queryByText(/calendar context is unavailable/i)).toBeNull();
    expect(screen.getByText("Fictional (personal)")).toBeDefined();
  });

  it("still gates a live (non-demo) render on the client id, unchanged", () => {
    render(
      <SettingsScreen
        demo={null}
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
        taskTokenState="resting"
        taskTokenEnteredAtMs={null}
        onSubmitTaskToken={vi.fn()}
        onForgetTaskToken={vi.fn()}
        task={taskState()}
        online
        syncNowMs={10_000}
        onDownloadMirror={vi.fn()}
      />,
    );
    expect(screen.getByText(/no Google client id/i)).toBeDefined();
  });
});
