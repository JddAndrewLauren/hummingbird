// The breakpoint is spelled twice — `PHONE_MAX_WIDTH_PX` here in TypeScript,
// and as a literal inside every `@media` in `responsive.css` — because CSS
// cannot read a custom property from a media query and this project carries no
// PostCSS plugin that could inline one. A drift between the two is invisible:
// the layout would flip at one width and `useIsPhone` would swap the nav at
// another, giving a band of widths with a bottom bar and a desktop header, or
// a rail and a stacked aside.
//
// So it is pinned by reading the stylesheet's own source text — the same
// technique `worker/sync-timer-ownership.test.ts` and `skills/no-queue.test.ts`
// use for invariants that are structural rather than executable. `?raw` is
// Vite/vitest's own import (typed by `vite/client`, see `vite-env.d.ts`)
// rather than `node:fs`: the browser tsconfig has no `@types/node`.
import responsiveSource from "./responsive.css?raw";
import { describe, expect, it } from "vitest";
import { PHONE_MAX_WIDTH_PX, PHONE_QUERY } from "./breakpoints";

const MEDIA_QUERY = /@media\s*\(([^)]*)\)/g;

describe("the phone breakpoint is defined once", () => {
  it("every @media in responsive.css uses PHONE_MAX_WIDTH_PX", () => {
    const conditions = [...responsiveSource.matchAll(MEDIA_QUERY)].map((match) => match[1].trim());
    // Not merely "they all agree": an empty match set would agree vacuously,
    // and this file's whole job is asserting a stylesheet that has phone rules.
    expect(conditions.length).toBeGreaterThan(0);
    for (const condition of conditions) {
      expect(condition).toBe(`max-width: ${PHONE_MAX_WIDTH_PX}px`);
    }
  });

  it("PHONE_QUERY is the same query the stylesheet spells out", () => {
    expect(PHONE_QUERY).toBe(`(max-width: ${PHONE_MAX_WIDTH_PX}px)`);
    expect(responsiveSource).toContain(`(max-width: ${PHONE_MAX_WIDTH_PX}px)`);
  });

  it("640, not 768 — the three desktop visual projects stay on the desktop side", () => {
    // `playwright.config.ts`'s pre-existing widths. If someone raises the
    // breakpoint to 768, the narrowest of them silently becomes a phone and
    // `docs/SURFACES.md`'s matrix row for it stops describing what is
    // photographed.
    for (const desktopWidth of [1440, 1024, 768]) {
      expect(desktopWidth).toBeGreaterThan(PHONE_MAX_WIDTH_PX);
    }
    // And the phone project must be on the phone side of it.
    expect(390).toBeLessThanOrEqual(PHONE_MAX_WIDTH_PX);
  });
});
