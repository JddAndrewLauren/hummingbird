import { afterEach, describe, expect, it, vi } from "vitest";
import { DEMO_DATA } from "./demo-data";
import { DEMO_TASK_STATE } from "./demo-task-state";
import { demoData, demoTaskState } from "./demo";

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
    withSearch("?demo");
    expect(demoData()).toBeNull();
  });

  it("returns nothing in a production build with no query string either", () => {
    vi.stubEnv("DEV", false);
    withSearch("");
    expect(demoData()).toBeNull();
  });

  it("serves the fixtures for ?demo in development, which is the whole point of the seam", () => {
    vi.stubEnv("DEV", true);
    withSearch("?demo");
    expect(demoData()).toBe(DEMO_DATA);
  });

  it("stays off in development without ?demo, so `pnpm dev` shows the honest empty states", () => {
    vi.stubEnv("DEV", true);
    withSearch("");
    expect(demoData()).toBeNull();
  });

  it("serves the KIT world for ?demo=board's sibling spellings, which must not change meaning", () => {
    vi.stubEnv("DEV", true);
    withSearch("?demo=1");
    expect(demoData()).toBe(DEMO_DATA);
  });

  it("stands down for ?demo=board — the two worlds are mutually exclusive", () => {
    vi.stubEnv("DEV", true);
    withSearch("?demo=board");
    // The null `demo` prop is exactly what makes `NowScreen` take its
    // `RealFrontier` branch, which is the point of the board world.
    expect(demoData()).toBeNull();
  });
});

describe("demoTaskState", () => {
  it("seeds the real render path for ?demo=board", () => {
    vi.stubEnv("DEV", true);
    withSearch("?demo=board");
    expect(demoTaskState()).toBe(DEMO_TASK_STATE);
  });

  it("returns nothing in a production build, so no fixture item can reach a real device", () => {
    vi.stubEnv("DEV", false);
    withSearch("?demo=board");
    expect(demoTaskState()).toBeNull();
  });

  it("stands down for the kit world and for no query string at all", () => {
    vi.stubEnv("DEV", true);
    withSearch("?demo");
    expect(demoTaskState()).toBeNull();
    withSearch("");
    expect(demoTaskState()).toBeNull();
  });
});
