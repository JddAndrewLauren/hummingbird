// @vitest-environment jsdom

// #457: `RoutesScreen` lost `App.tsx`'s prop threading and gained its own
// dev-gated `demoData()` read (`fixtures/demo-data.ts`). No test in this
// file's history ever mounted the component — `docs/SURFACES.md` named that
// gap after #455 retired the kit-world capture pass — so this covers the
// same three states `demo-data.test.ts` covers for the accessor itself, but
// through the component that actually calls it.

import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "../test/component";
import { RoutesScreen } from "./RoutesScreen";

function setSearch(search: string): void {
  window.history.pushState({}, "", `/${search}`);
}

afterEach(() => {
  vi.unstubAllEnvs();
  setSearch("");
});

describe("RoutesScreen", () => {
  it("renders the honest empty state with no ?demo=kit, the same state the board world gets", () => {
    vi.stubEnv("DEV", true);
    setSearch("");
    render(<RoutesScreen />);
    expect(screen.getByRole("heading", { level: 2, name: "No routes yet" })).toBeDefined();
  });

  it("renders the kit fixture's route under ?demo=kit in development", () => {
    vi.stubEnv("DEV", true);
    setSearch("?demo=kit");
    render(<RoutesScreen />);
    expect(
      screen.getByText(
        "The greenhouse holds temperature overnight through winter, without me checking it.",
      ),
    ).toBeDefined();
    expect(screen.getByText("Regenerate the Gmail fixture set")).toBeDefined();
  });

  it("stands down for ?demo=kit in a production build, so the fixture cannot reach a real device", () => {
    vi.stubEnv("DEV", false);
    setSearch("?demo=kit");
    render(<RoutesScreen />);
    expect(screen.getByRole("heading", { level: 2, name: "No routes yet" })).toBeDefined();
  });
});
