import { defineConfig } from "@playwright/test";

// The visual gate (G2). Local-only on purpose: it is NOT wired into
// `.github/workflows/client.yml`.
//
// Why not CI. `pnpm typecheck` already rebuilds the wasm core and is the slow
// step in that workflow; a browser matrix on top of it compounds that cost,
// and screenshot jobs buy flake before they buy signal on a single-operator
// project with no baseline history. The gate's real value is at authoring
// time — `/wrapup`'s visual verification phase reads `docs/SURFACES.md` and
// runs this. Promote it to CI the first time a visual regression actually
// lands on main; until then this stays a command someone runs.
//
// Why Playwright rather than `chrome --headless --screenshot`: the viewport
// renders wrong under the latter, which is how a UI can look fine in a
// capture and be broken in a browser. `page.setViewportSize` (here, via the
// per-project `viewport`) is the only viewport this repo trusts.
//
// Requires a one-time `pnpm exec playwright install chromium` per machine —
// deliberately not run by `pnpm dev`/`pnpm test`, so nobody pays for a
// browser download they did not ask for.
//
// The three desktop widths come from the design system's fixed layout
// constants (a 236px nav rail, a 320px context panel, a 380px minimum
// content column) and from `screens/layout.tsx`'s `TwoColumn`, which wraps
// rather than using a media query. The fourth, `phone`, is the one width
// where that stops being the whole story: below `PHONE_MAX_WIDTH_PX`
// (`src/shell/breakpoints.ts`) the app does use a media query, and the rail
// is a bottom bar. See `docs/SURFACES.md` for the surface-by-surface matrix.
// The dev server's port. `HB_VISUAL_PORT` exists because `reuseExistingServer`
// below will happily photograph whatever is already listening: with several
// worktrees of this repo on one machine, a gate run on the default port can
// capture ANOTHER checkout's build and report it as this one's. Set it to a
// port nobody else uses and the gate starts, and photographs, its own server.
const port = Number(process.env.HB_VISUAL_PORT ?? 5173);

export default defineConfig({
  testDir: "./visual",
  outputDir: "./visual/.output",
  snapshotDir: "./visual/.snapshots",
  fullyParallel: true,
  reporter: [["list"]],
  use: {
    baseURL: `http://localhost:${port}`,
    // A capture of a half-finished paint is a false finding, and this app's
    // fonts are self-hosted (the production CSP allows no Google Fonts), so
    // waiting on the network alone is not enough.
    screenshot: "off",
  },
  projects: [
    // 1440: everything side by side — rail, content column, context panel.
    { name: "wide", use: { viewport: { width: 1440, height: 900 } } },
    // 1024: the wrap boundary. 236 rail + 380 min column + 320 panel plus
    // gaps and page padding lands within a few pixels of this, so it is
    // where `TwoColumn` decides — exactly the width a layout change breaks
    // first, and the reason this is not just "desktop and mobile".
    { name: "boundary", use: { viewport: { width: 1024, height: 900 } } },
    // 768: the panel has wrapped below the column.
    { name: "narrow", use: { viewport: { width: 768, height: 1000 } } },
    // 390x844: an iPhone in portrait, the surface this app is installed on.
    // The media query fires on the viewport alone — `matchMedia("(max-width:
    // 640px)")` reads width and nothing else, so the class-driven phone form
    // and `useIsPhone` would both flip at 390px without any emulation.
    // `isMobile`/`hasTouch` earn their place elsewhere: they drive the touch
    // affordances and the pointer/hover media features, which is the half of
    // the phone form a bare viewport would not exercise. They also change what
    // the capture sees — the `fullPage` height artifact documented in
    // `docs/SURFACES.md` is theirs, and is why captures here are
    // viewport-sized. Chromium-only, matching the sole browser configured
    // above.
    {
      name: "phone",
      use: {
        viewport: { width: 390, height: 844 },
        deviceScaleFactor: 3,
        isMobile: true,
        hasTouch: true,
      },
    },
  ],
  webServer: {
    // `pnpm dev` builds the wasm core first, which the SharedWorker needs.
    command: `pnpm dev --port ${port} --strictPort`,
    url: `http://localhost:${port}`,
    reuseExistingServer: true,
    timeout: 180_000,
  },
});
