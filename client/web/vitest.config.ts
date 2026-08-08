import { defineConfig } from "vitest/config";

// Unit tests cover the pure logic (store, worker protocol, CSP worker) that
// #69's TDD slices are built against. The wasm-in-worker path itself is
// exercised by `pnpm build` + `wrangler dev` (no headless-wasm test runner
// available in this repo's stdlib-only test posture).
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts", "csp-worker/**/*.test.ts"],
  },
});
