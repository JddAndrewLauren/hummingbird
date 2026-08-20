// @vitest-environment jsdom

// #457: `AlertsScreen` lost `App.tsx`'s prop threading and gained its own
// dev-gated `demoData()` read (`fixtures/demo-data.ts`). No test in this
// file's history ever mounted the component — `docs/SURFACES.md` named that
// gap after #455 retired the kit-world capture pass — so this covers the
// same three states `demo-data.test.ts` covers for the accessor itself, but
// through the component that actually calls it.

import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "../test/component";
import { AlertsScreen } from "./AlertsScreen";

function setSearch(search: string): void {
  window.history.pushState({}, "", `/${search}`);
}

afterEach(() => {
  vi.unstubAllEnvs();
  setSearch("");
});

describe("AlertsScreen", () => {
  it("renders the real-device empty copy with no ?demo=kit, the same state the board world gets", () => {
    vi.stubEnv("DEV", true);
    setSearch("");
    render(<AlertsScreen />);
    expect(
      screen.getByText("No rules are wired to the notification lane yet. What no rule matches stays silent."),
    ).toBeDefined();
    // No "Alert rules" aside without a fixture to fill it.
    expect(screen.queryByText("Urgent tier only on the watch")).toBeNull();
  });

  it("renders the kit fixture's alerts and rules aside under ?demo=kit in development", () => {
    vi.stubEnv("DEV", true);
    setSearch("?demo=kit");
    render(<AlertsScreen />);
    expect(screen.getByText("Google Tasks adapter returned 503 twice.")).toBeDefined();
    expect(screen.getByText("Two consecutive adapter failures from one source.")).toBeDefined();
  });

  it("stands down for ?demo=kit in a production build, so the fixture cannot reach a real device", () => {
    vi.stubEnv("DEV", false);
    setSearch("?demo=kit");
    render(<AlertsScreen />);
    expect(
      screen.getByText("No rules are wired to the notification lane yet. What no rule matches stays silent."),
    ).toBeDefined();
  });
});
