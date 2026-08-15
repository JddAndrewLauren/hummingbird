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
import { DEMO_DATA } from "../fixtures/demo-data";
import { fireEvent, render, screen, taskState } from "../test/component";
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
  const onSelectionChange = vi.fn();
  render(
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
      onSelectionChange={onSelectionChange}
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
  return { onSelectionChange };
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

  // The round-2 hazard (#452's own words): the fixture card's toggle must
  // never reach `onSelectionChange`, whose live handler persists the ids to
  // the shared localStorage key and polls Google for calendars that do not
  // exist. `demo` is null in this world, so the toggle must branch on
  // `calendarIsDemo`, not `demo`.
  it("toggles locally and never calls onSelectionChange", () => {
    const { onSelectionChange } = renderBoardSettings("loading");
    const family = screen.getByRole("checkbox", { name: /Fictional \(family\)/ });
    fireEvent.click(family);
    expect(onSelectionChange).not.toHaveBeenCalled();
    expect((family as HTMLInputElement).checked).toBe(true);
  });

  // The board world has no bindings table to read (`demo-calendar.ts`), so no
  // row may render locked off the live `task.bindings` — every fixture row
  // stays toggleable.
  it("locks no row off the live bindings", () => {
    renderBoardSettings("loading");
    for (const name of [/Fictional \(personal\)/, /Fictional \(family\)/]) {
      expect((screen.getByRole("checkbox", { name }) as HTMLInputElement).disabled).toBe(false);
    }
  });

  // Round-2's kit-world pin: bare `?demo` in a tree with no client id (this
  // repo ships only `.env.example`, and the visual gate's dev server reads no
  // `.env.local`) shows the "no Google client id" Note, exactly as it did
  // before #452 — `SettingsScreen.test.tsx` cannot see this because it mocks
  // the client id truthy.
  it("keeps the kit world's no-client-id Note unchanged", () => {
    render(
      <SettingsScreen
        demo={DEMO_DATA}
        status="loading"
        apiVersion={null}
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
    expect(screen.queryByText("Andrew (personal)")).toBeNull();
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
