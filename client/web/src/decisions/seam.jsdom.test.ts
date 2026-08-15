// @vitest-environment jsdom

import { describe, expect, it } from "vitest";

import { canSubmitCapture, decisionsReady } from "./seam";

// The jsdom half of M1-1's "vitest executes the seam in both environments,
// with no per-file hack" measurement. The only difference from
// `seam.test.ts` is the docblock above: the same `setupFiles` entry
// instantiates the module here, in the environment every component test
// runs in. If this file ever needs a loader of its own, the flip condition
// "vitest can't instantiate wasm cleanly" has been hit.

describe("the decision seam under jsdom", () => {
  it("is instantiated by the same shared setup file", () => {
    expect(decisionsReady()).toBe(true);
  });

  it("answers synchronously, as a render-time caller needs", () => {
    expect(canSubmitCapture("   ")).toBe(false);
    expect(canSubmitCapture("buy milk")).toBe(true);
  });
});
