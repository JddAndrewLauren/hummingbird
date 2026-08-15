import { defineConfig } from "vitest/config";

// Unit tests cover the pure logic (store, worker protocol, CSP worker) that
// #69's TDD slices are built against. The wasm-in-worker path itself is
// exercised by `pnpm build` + `wrangler dev` (no headless-wasm test runner
// available in this repo's stdlib-only test posture).
//
// `environment: "node"` stays the DEFAULT, deliberately. The `worker/*`
// tests assert against a `SharedWorker`-shaped world that has no `document`
// (see `visibility-tracker.ts`), and handing them a jsdom global would let a
// test pass on a `document` the real runtime does not have. Component tests
// (`*.test.tsx`) opt into jsdom per file with a
// `// @vitest-environment jsdom` docblock — one line at the top of the file,
// read by vitest before the module is loaded. That is the per-file switch
// rather than `environmentMatchGlobs`, which is deprecated upstream.
//
// Why component tests exist at all: three of the four PRs in the S10-S13
// batch shipped UI state with no reader — code that compiled, typechecked,
// was unit-tested and never executed. Typecheck cannot see that a prop, a
// hook or a pure module has no caller; mounting the component can. See
// `src/test/component.tsx`.
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx", "csp-worker/**/*.test.ts"],
    // The main-thread decision seam (ADR-0025, #141) is a wasm module, and
    // the wrappers over it are synchronous — a component calls one during
    // render and cannot await anything. This setup file instantiates it
    // once per test file, before that file's imports run, in whichever
    // environment the file asked for; `src/test/wasm-setup.ts` explains why
    // it hand-instantiates rather than importing the bundler entry point.
    // A `setupFiles` entry, not a per-file import, precisely so no test
    // file has to know: `CapturePopover.test.tsx` passes unchanged.
    setupFiles: ["src/test/wasm-setup.ts"],
    // Vitest's default (`css: false`) replaces the contents of every CSS
    // module with an empty string, and it decides that by file extension — so
    // it empties a `?raw` import too, silently. `responsive-breakpoint.test.ts`
    // pins the phone breakpoint by reading `responsive.css`'s source text, and
    // under the default it read `""` and asserted nothing at all. No test
    // imports CSS for its styling, so turning this on only makes `?raw` honest.
    css: true,
  },
});
