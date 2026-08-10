// `?raw` source-text pins, the same technique — and for the same reason — as
// `sync-timer-ownership.test.ts`: `core.worker.ts` is a
// `SharedWorkerGlobalScope` script (it reads `self.location.origin` and
// assigns `self.onconnect`) that loads the wasm core through a dynamic
// `import()`, so no vitest (node) test can import it and read
// `TASK_BASE_URL` as a value. What it CAN do is prove the expression that
// produces it has not been rewritten into one of the two shapes that break
// the deployed app silently.
import coreWorkerSource from "./core.worker.ts?raw";
import viteConfigSource from "../../vite.config.ts?raw";
import wranglerSource from "../../wrangler.toml?raw";
import authorityWranglerSource from "../../../../server/worker/wrangler.toml?raw";
import { describe, expect, it } from "vitest";

// ADR-0006/0008: the API is same-origin with the shell — `connect-src
// 'self'`, and the authority sends no `Access-Control-*` header at all, so
// cross-origin is not "slower", it is "does not work". Three separate files
// have to agree for that to hold, and none of them is exercised by any other
// test in this suite: the client's base URL, the production route, and the
// dev proxy standing in for that route. A change to any one alone produces a
// build that typechecks, passes every other test, and cannot reach the
// authority.
describe("the API is same-origin, in every configuration", () => {
  it("core.worker.ts derives the authority's base URL from the worker's own origin", () => {
    expect(coreWorkerSource).toContain("self.location.origin");
  });

  it("core.worker.ts never falls back to an empty base URL", () => {
    // `{base}/api/...` with an empty base is a RELATIVE url, which `reqwest`
    // rejects before opening a socket (see the assertion on that behaviour
    // in `client/ffi-web/src/task_host.rs`) — so this regression does not
    // surface as a network error to debug, it surfaces as every sync cycle
    // in the deployed app reporting `pull_failed` forever.
    expect(coreWorkerSource).not.toMatch(/VITE_API_BASE_URL\s*\?\?\s*""/);
  });

  it("VITE_API_BASE_URL is an override, not the source of the origin", () => {
    // It may stay as an escape hatch (a staging DO), but it must not be the
    // thing production depends on: nothing sets it in any checked-in
    // configuration, so a bundle that NEEDS it is a bundle that ships broken.
    expect(coreWorkerSource).toContain("VITE_API_BASE_URL ?? self.location.origin");
  });

  it("vite dev proxies /api so dev exercises the same same-origin path as production", () => {
    expect(viteConfigSource).toContain('"/api"');
    expect(viteConfigSource).toContain("127.0.0.1:8787");
  });

  it("the shell claims the hostname without claiming /api/*", () => {
    // A Custom Domain is hostname-exact and takes no path, which is what
    // leaves `/api/*` available to the narrower route below. Were this ever
    // widened to a `hb.twinion.net/*` route, it would collide with that
    // route instead of yielding to it.
    expect(wranglerSource).toContain("custom_domain = true");
    expect(wranglerSource).not.toContain('pattern = "hb.twinion.net/*"');
  });

  it("the authority worker holds the narrower hb.twinion.net/api/* route", () => {
    // Without this the shell's `not_found_handling = "single-page-application"`
    // answers `/api/...` with a 200 `text/html` app shell — a JSON parse
    // error at the far end of the sync core, not a 404.
    expect(authorityWranglerSource).toContain('pattern = "hb.twinion.net/api/*"');
    expect(authorityWranglerSource).toContain('zone_name = "twinion.net"');
  });
});
