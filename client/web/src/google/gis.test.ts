import { afterEach, describe, expect, it, vi } from "vitest";
import { createGisTokenClient } from "./gis";

// `environment: "node"` (vitest.config.ts) has no real `window`/`document`;
// stub just enough of the script-tag-injection surface that
// `createGisTokenClient`'s `loadGisScript` touches.
function stubDocumentAndWindow(onScriptTag: (script: {
  onload: (() => void) | null;
  onerror: (() => void) | null;
}) => void) {
  const script: { onload: (() => void) | null; onerror: (() => void) | null } = {
    onload: null,
    onerror: null,
  };
  vi.stubGlobal("document", {
    createElement: vi.fn(() => script),
    head: {
      appendChild: vi.fn(() => {
        onScriptTag(script);
      }),
    },
  });
  vi.stubGlobal("window", {});
}

describe("createGisTokenClient", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("resolves to a TokenError, rather than rejecting, when the GIS script fails to load", async () => {
    // Regression test (issue #73 review): a script-load failure (offline,
    // blocked, CDN down) must surface through the documented
    // `TokenResult | TokenError` union -- callers like
    // `calendar/connection.ts` never wrap `requestToken` in a `catch`, so
    // an unhandled rejection here left the UI stuck on an inert
    // Connect/Reconnect button instead of routing to `needsReconnect`.
    stubDocumentAndWindow((script) => {
      script.onerror?.();
    });
    const client = createGisTokenClient("test-client-id");

    await expect(client.requestToken("none")).resolves.toEqual({
      error: "gis_script_load_failed",
    });
  });
});
