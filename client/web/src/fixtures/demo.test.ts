import { afterEach, describe, expect, it, vi } from "vitest";
import { DEMO_DATA } from "./demo-data";
import { demoCalendar, demoData, demoTaskState } from "./demo";

// `demoData()` reads `window.location.search`, and the test environment is
// node (vitest.config.ts) — there is no DOM to read it from. Stubbing the
// global is what lets the gate be tested as the pure module it is.
function withSearch(search: string): void {
  vi.stubGlobal("window", { location: { search } });
}

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

describe("demoData", () => {
  it("returns nothing in a production build even when the URL asks for it, so fixtures cannot reach a real user", () => {
    vi.stubEnv("DEV", false);
    withSearch("?demo=kit");
    expect(demoData()).toBeNull();
  });

  it("returns nothing in a production build with no query string either", () => {
    vi.stubEnv("DEV", false);
    withSearch("");
    expect(demoData()).toBeNull();
  });

  it("serves the fixtures for ?demo=kit in development, which is the whole point of the seam", () => {
    vi.stubEnv("DEV", true);
    withSearch("?demo=kit");
    expect(demoData()).toBe(DEMO_DATA);
  });

  it("stays off in development without ?demo, so `pnpm dev` shows the honest empty states", () => {
    vi.stubEnv("DEV", true);
    withSearch("");
    expect(demoData()).toBeNull();
  });

  // #455: the flip. Bare `?demo` and every sibling spelling but the kit's
  // one exact spelling now resolve to the board world, which serves no
  // `DemoData` at all — the two worlds stay mutually exclusive, just with
  // the default swapped.
  it("stands down for bare ?demo and its sibling spellings — those are the board world now", () => {
    vi.stubEnv("DEV", true);
    withSearch("?demo");
    expect(demoData()).toBeNull();
    withSearch("?demo=1");
    expect(demoData()).toBeNull();
    withSearch("?demo=board");
    // The null `demo` prop is exactly what makes `NowScreen` take its
    // `RealFrontier` branch, which is the point of the board world.
    expect(demoData()).toBeNull();
  });
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
    expect(state?.frontier).toHaveLength(12);
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
