import { afterEach, describe, expect, it, vi } from "vitest";

// `environment: "node"` (vitest.config.ts) has no real `window`/`document`;
// stub just enough of the script-tag-injection surface that
// `createGisTokenClient`'s `loadGisScript` touches.
//
// Each test imports `./gis` through `vi.resetModules()` + `await import`
// rather than at the top of the file: the module memoises its script-load
// promise, and that memo is exactly what these tests are about, so it has to
// start empty for each one.

interface FakeScript {
  onload: (() => void) | null;
  onerror: (() => void) | null;
  remove: () => void;
}

interface FakeWindow {
  google?: {
    accounts: {
      oauth2: {
        initTokenClient(config: {
          callback: (response: { access_token?: string; expires_in?: number }) => void;
        }): { requestAccessToken(overrides?: { prompt?: string }): void };
      };
    };
  };
}

/** Makes `window.google` present and its token client NEVER call back — the
 * installed-iOS-PWA failure this timeout exists for, where the popup escapes
 * to Safari, loses its opener, and neither `callback` nor `error_callback`
 * ever fires. Returns the prompts it was asked for. */
function installSilentGis(window: FakeWindow): string[] {
  const prompts: string[] = [];
  window.google = {
    accounts: {
      oauth2: {
        initTokenClient: () => ({
          requestAccessToken: (overrides) => {
            prompts.push(overrides?.prompt ?? "");
          },
        }),
      },
    },
  };
  return prompts;
}

function stubDocumentAndWindow(onScriptTag: (script: FakeScript) => void) {
  const appended: FakeScript[] = [];
  vi.stubGlobal("document", {
    createElement: vi.fn(
      (): FakeScript => ({ onload: null, onerror: null, remove: vi.fn() }),
    ),
    head: {
      appendChild: vi.fn((script: FakeScript) => {
        appended.push(script);
        onScriptTag(script);
      }),
    },
  });
  const window: FakeWindow = {};
  vi.stubGlobal("window", window);
  return { appended, window };
}

/** Makes `window.google` present and its token client hand back `token`. */
function installLoadedGis(window: FakeWindow, token: string) {
  window.google = {
    accounts: {
      oauth2: {
        initTokenClient: (config) => ({
          requestAccessToken: () => {
            config.callback({ access_token: token, expires_in: 3600 });
          },
        }),
      },
    },
  };
}

async function importGis() {
  vi.resetModules();
  return import("./gis");
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
    const { createGisTokenClient } = await importGis();
    const client = createGisTokenClient("test-client-id");

    await expect(client.requestToken("none")).resolves.toEqual({
      error: "gis_script_load_failed",
    });
  });

  it("retries the script load after a failure instead of replaying the cached rejection", async () => {
    // Regression test: the load promise was memoised unconditionally, so
    // one offline failure poisoned every later attempt -- Connect and
    // Reconnect kept returning that same stale rejection until a page
    // reload, even once connectivity was back.
    let failNext = true;
    const { appended, window } = stubDocumentAndWindow((script) => {
      if (failNext) {
        script.onerror?.();
        return;
      }
      installLoadedGis(window, "tok-after-retry");
      script.onload?.();
    });
    const { createGisTokenClient, isTokenResult } = await importGis();
    const client = createGisTokenClient("test-client-id", () => 1_000);

    expect(await client.requestToken("none")).toEqual({
      error: "gis_script_load_failed",
    });

    failNext = false;
    const second = await client.requestToken("none");

    expect(isTokenResult(second) && second.accessToken).toBe("tok-after-retry");
    // A real second attempt: a fresh tag, and the spent one cleaned up.
    expect(appended).toHaveLength(2);
    expect(appended[0].remove).toHaveBeenCalled();
  });

  it("loads the GIS script exactly once across repeated successful requests", async () => {
    const { appended, window } = stubDocumentAndWindow((script) => {
      installLoadedGis(window, "tok-1");
      script.onload?.();
    });
    const { createGisTokenClient } = await importGis();
    const client = createGisTokenClient("test-client-id", () => 1_000);

    await client.requestToken("none");
    await client.requestToken("");

    expect(appended).toHaveLength(1);
  });
});

describe("the token request timeout", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  /** `requestToken` awaits `loadGisScript()` before it arms anything, so the
   * timer is started a microtask after the call, not synchronously. */
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

  it("resolves to TOKEN_TIMEOUT_ERROR when GIS never calls back at all", async () => {
    const { window } = stubDocumentAndWindow(() => {});
    installSilentGis(window);
    const { createGisTokenClient, TOKEN_TIMEOUT_ERROR } = await importGis();

    let fire: (() => void) | null = null;
    const client = createGisTokenClient("id", () => 1_000, (run) => {
      fire = run;
      return () => {};
    });

    const pending = client.requestToken("consent");
    await flush();
    // Nothing has settled it — this is the state the button was stuck in.
    expect(fire).not.toBeNull();
    fire!();

    expect(await pending).toEqual({ error: TOKEN_TIMEOUT_ERROR });
  });

  it("uses the short ceiling for a silent re-mint and the long one for an interactive request", async () => {
    const { window } = stubDocumentAndWindow(() => {});
    installSilentGis(window);
    const {
      createGisTokenClient,
      SILENT_TOKEN_TIMEOUT_MS,
      INTERACTIVE_TOKEN_TIMEOUT_MS,
    } = await importGis();

    const delays: number[] = [];
    const client = createGisTokenClient("id", () => 1_000, (_run, delayMs) => {
      delays.push(delayMs);
      return () => {};
    });

    void client.requestToken("none");
    void client.requestToken("consent");
    void client.requestToken("");
    await flush();

    expect(delays).toEqual([
      SILENT_TOKEN_TIMEOUT_MS,
      INTERACTIVE_TOKEN_TIMEOUT_MS,
      INTERACTIVE_TOKEN_TIMEOUT_MS,
    ]);
    // A person reading a consent screen is not a hang; an unattended iframe
    // round-trip taking two minutes is.
    expect(SILENT_TOKEN_TIMEOUT_MS).toBeLessThan(INTERACTIVE_TOKEN_TIMEOUT_MS);
  });

  it("cancels the timer once GIS answers, and a late callback cannot re-settle", async () => {
    const { window } = stubDocumentAndWindow(() => {});
    installLoadedGis(window, "tok");
    const { createGisTokenClient, isTokenResult } = await importGis();

    const cancel = vi.fn();
    let fire: (() => void) | null = null;
    const client = createGisTokenClient("id", () => 1_000, (run) => {
      fire = run;
      return cancel;
    });

    const result = await client.requestToken("consent");
    expect(isTokenResult(result)).toBe(true);
    expect(cancel).toHaveBeenCalledTimes(1);

    // Firing the timer anyway must not change the already-resolved value —
    // the `settled` guard, not the cancel, is what makes that true.
    fire!();
    expect(isTokenResult(await Promise.resolve(result))).toBe(true);
  });

  it("routes a synchronous throw from requestAccessToken into the error union", async () => {
    const { window } = stubDocumentAndWindow(() => {});
    window.google = {
      accounts: {
        oauth2: {
          initTokenClient: () => ({
            requestAccessToken: () => {
              throw new Error("popup blocked before any callback existed");
            },
          }),
        },
      },
    };
    const { createGisTokenClient } = await importGis();
    const client = createGisTokenClient("id", () => 1_000, () => () => {});

    // Before the try/catch this escaped the promise executor as an unhandled
    // rejection and the caller waited forever.
    expect(await client.requestToken("consent")).toEqual({ error: "gis_request_failed" });
  });
});
