import { afterEach, describe, expect, it, vi } from "vitest";
import { demoCalendar, demoTaskState } from "./demo";

// `demoTaskState`/`demoCalendar` read `window.location.search`, and the test
// environment is node (vitest.config.ts) — there is no DOM to read it from.
// Stubbing the global is what lets the gate be tested as the pure module it
// is. `demoData()`'s own tests moved to `demo-data.test.ts` alongside it in
// #457.
function withSearch(search: string): void {
  vi.stubGlobal("window", { location: { search } });
}

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

describe("demoTaskState", () => {
  it("seeds the real render path for bare ?demo, the default since #455", () => {
    vi.stubEnv("DEV", true);
    withSearch("?demo");
    const state = demoTaskState();
    // Built per call rather than a shared const — see `demo-task-state.ts`'s
    // header for why that is a bundling requirement — so this asserts the
    // shape it returns, never object identity.
    expect(state).not.toBeNull();
    expect(state?.frontier).toHaveLength(14);
    expect(state?.triageInbox).toHaveLength(17);
    expect(state?.lastTriage?.kind).toBe("failed");
  });

  it("also seeds for ?demo=board, the sibling spelling", () => {
    vi.stubEnv("DEV", true);
    withSearch("?demo=board");
    expect(demoTaskState()).not.toBeNull();
  });

  it("returns nothing in a production build, so no fixture item can reach a real device", () => {
    vi.stubEnv("DEV", false);
    withSearch("?demo");
    expect(demoTaskState()).toBeNull();
  });

  it("stands down for the kit world and for no query string at all", () => {
    vi.stubEnv("DEV", true);
    withSearch("?demo=kit");
    expect(demoTaskState()).toBeNull();
    withSearch("");
    expect(demoTaskState()).toBeNull();
  });
});

describe("demoCalendar", () => {
  it("seeds Settings' calendar card for bare ?demo, the default since #455", () => {
    vi.stubEnv("DEV", true);
    withSearch("?demo");
    const calendar = demoCalendar();
    expect(calendar).not.toBeNull();
    expect(calendar?.connected).toBe(true);
    expect(calendar?.availableCalendars.length).toBeGreaterThan(0);
  });

  it("returns nothing in a production build, so no fixture calendar can reach a real device", () => {
    vi.stubEnv("DEV", false);
    withSearch("?demo");
    expect(demoCalendar()).toBeNull();
  });

  it("stands down for the kit world and for no query string at all", () => {
    vi.stubEnv("DEV", true);
    withSearch("?demo=kit");
    expect(demoCalendar()).toBeNull();
    withSearch("");
    expect(demoCalendar()).toBeNull();
  });
});
