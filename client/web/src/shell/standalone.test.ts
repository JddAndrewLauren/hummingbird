// @vitest-environment jsdom

// `isStandalone` decides which OAuth flow the app takes — the popup that
// cannot come back in an installed iOS web app, or the redirect that can — and
// which advice `connect-error.ts` gives. Both branches are one boolean away
// from being wrong in a way no type checks and no screen shows.

import { afterEach, describe, expect, it, vi } from "vitest";
import { isStandalone } from "./standalone";

afterEach(() => {
  vi.unstubAllGlobals();
});

/** jsdom implements neither `matchMedia` nor `navigator.standalone`, which is
 * the environment the guard exists for — so each test states exactly the
 * platform it is describing. */
function platform(options: { displayMode?: boolean; iosStandalone?: boolean }) {
  if (options.displayMode !== undefined) {
    vi.stubGlobal(
      "matchMedia",
      (query: string) => ({ matches: query.includes("standalone") && options.displayMode }),
    );
  }
  if (options.iosStandalone !== undefined) {
    Object.defineProperty(navigator, "standalone", {
      value: options.iosStandalone,
      configurable: true,
    });
  }
}

describe("isStandalone", () => {
  it("is false in an ordinary browser tab", () => {
    platform({ displayMode: false, iosStandalone: false });
    expect(isStandalone()).toBe(false);
  });

  it("is true for the standard display-mode signal", () => {
    platform({ displayMode: true });
    expect(isStandalone()).toBe(true);
  });

  // The one that actually matters on the target device: iOS Safari does not
  // report `display-mode: standalone` for a home-screen app, it sets its own
  // non-standard `navigator.standalone`. Reading only the standard signal
  // would hand the installed iOS app the popup flow — the exact flow that
  // escapes to Safari and never comes back, which is the bug the redirect
  // exists to fix.
  it("is true for iOS's own non-standard signal, with display-mode saying nothing", () => {
    platform({ displayMode: false, iosStandalone: true });
    expect(isStandalone()).toBe(true);
  });

  // The guard. This repo's jsdom has no `matchMedia` at all, and an unguarded
  // call throws inside render — it took down 21 component tests once.
  it("does not throw where matchMedia does not exist", () => {
    Object.defineProperty(navigator, "standalone", { value: undefined, configurable: true });
    expect(() => isStandalone()).not.toThrow();
    expect(isStandalone()).toBe(false);
  });
});
