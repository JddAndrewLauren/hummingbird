import { afterEach, describe, expect, it, vi } from "vitest";
import { DEMO_DATA, demoData } from "./demo-data";

// `demoData()` reads `window.location.search`, and the test environment is
// node (vitest.config.ts) — there is no DOM to read it from. Stubbing the
// global is what lets the gate be tested as the pure module it is. Moved
// here from `demo.test.ts` in #457, alongside the accessor itself.
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
    expect(demoData()).toBeNull();
  });
});
