import { describe, expect, it } from "vitest";
import worker from "./worker";
import { CONTENT_SECURITY_POLICY } from "./csp";

function fakeEnv(assetResponse: Response) {
  return {
    ASSETS: {
      fetch: async () => assetResponse,
    },
  };
}

describe("csp-worker", () => {
  it("adds the strict CSP header to every served response", async () => {
    const env = fakeEnv(new Response("<html></html>", { status: 200 }));

    const response = await worker.fetch(
      new Request("https://hb.twinion.net/"),
      env,
    );

    expect(response.headers.get("Content-Security-Policy")).toBe(
      CONTENT_SECURITY_POLICY,
    );
  });

  it("grants same-origin microphone and on-device speech recognition via Permissions-Policy", async () => {
    const env = fakeEnv(new Response("<html></html>", { status: 200 }));

    const response = await worker.fetch(
      new Request("https://hb.twinion.net/"),
      env,
    );

    expect(response.headers.get("Permissions-Policy")).toBe(
      "microphone=(self), on-device-speech-recognition=(self)",
    );
  });

  it("preserves the underlying asset response's status and body", async () => {
    const env = fakeEnv(new Response("not found", { status: 404 }));

    const response = await worker.fetch(
      new Request("https://hb.twinion.net/missing"),
      env,
    );

    expect(response.status).toBe(404);
    expect(await response.text()).toBe("not found");
  });

  it("has no unsafe-inline and scopes connect-src to self and www.googleapis.com only", () => {
    // The task API is same-origin (ADR-0008), so 'self' covers it;
    // api.linear.app was retired with the authority move. #586: the
    // authority mints the calendar token server-side (ADR-0028), so
    // accounts.google.com is gone from connect-src too.
    expect(CONTENT_SECURITY_POLICY).not.toMatch(/unsafe-inline/);
    expect(CONTENT_SECURITY_POLICY).not.toMatch(/linear/);
    expect(CONTENT_SECURITY_POLICY).toMatch(
      /connect-src 'self' https:\/\/www\.googleapis\.com;/,
    );
  });

  it("keeps www.googleapis.com in connect-src for the wasm core's direct calendar poll (ADR-0005/ADR-0028)", () => {
    // The device still polls Google Calendar directly from
    // `core::calendar::google::reqwest_transport` -- only the *source* of
    // the token moved to the authority (ADR-0028). Removing this allowance
    // would break calendar silently, with no console error an operator
    // would connect to a CSP change.
    const connectSrc = CONTENT_SECURITY_POLICY.split("; ").find((directive) =>
      directive.startsWith("connect-src "),
    );
    expect(connectSrc).toBe("connect-src 'self' https://www.googleapis.com");
  });

  it("allows wasm compilation via 'wasm-unsafe-eval' without the broader 'unsafe-eval', and grants no Google script-src origin", () => {
    // Chrome (and other browsers following the WebAssembly/CSP integration
    // spec) refuse WebAssembly.compile/instantiate under `script-src 'self'`
    // alone -- 'wasm-unsafe-eval' is the narrow source expression that
    // permits wasm compilation without also permitting arbitrary eval.
    // #586: GIS's script tag is gone, so script-src carries no Google origin.
    expect(CONTENT_SECURITY_POLICY).toMatch(
      /script-src 'self' 'wasm-unsafe-eval';/,
    );
    expect(CONTENT_SECURITY_POLICY).not.toMatch(/script-src[^;]*'unsafe-eval'/);
  });

  it("does not mention accounts.google.com in any directive", () => {
    // #577/#586: nothing in the browser mints a Google credential any more
    // (ADR-0028) -- the GIS script, its own XHRs, and its hidden re-mint
    // iframe are all gone, so this origin has no directive left to live in.
    expect(CONTENT_SECURITY_POLICY).not.toMatch(/accounts\.google\.com/);
  });

  it("sets frame-src to an explicit 'none', not left unset", () => {
    // Removing the directive entirely would fall back to `default-src
    // 'self'` for frames, which permits the app to frame itself -- an
    // explicit 'none' is required, not merely the absence of a grant.
    expect(CONTENT_SECURITY_POLICY).toMatch(/frame-src 'none';/);
  });

  it("adds no speech-related origin to connect-src for on-device dictation (#379/#382)", () => {
    // On-device recognition issues no fetch from the page, so the policy
    // needs no new connect-src grant for it -- but that is true only while
    // local processing is REQUIRED rather than preferred (ADR-0022 Decision
    // 1), and the cloud fallback a browser might otherwise take is entirely
    // browser-internal, so CSP itself would never catch a relaxation of that
    // guarantee on its own. This pins the one thing CSP CAN catch: nobody
    // has added a speech-recognition origin (Google's cloud endpoint or any
    // other) to connect-src as a "just in case" grant.
    const connectSrc = CONTENT_SECURITY_POLICY.split("; ").find((directive) =>
      directive.startsWith("connect-src "),
    );
    expect(connectSrc).not.toMatch(/speech/i);
    expect(connectSrc).toBe("connect-src 'self' https://www.googleapis.com");
  });

  it("does not admit any Google origin beyond the calendar fetch's own host", () => {
    // Pins the policy to exactly the one Google host this feature needs --
    // no wildcards, no broader googleapis.com/google.com grant, and no
    // accounts.google.com now that nothing mints Google credentials in the
    // browser.
    const googleMatches = CONTENT_SECURITY_POLICY.match(
      /https:\/\/[a-z0-9.-]*google[a-z0-9.-]*/g,
    );
    expect(new Set(googleMatches)).toEqual(
      new Set(["https://www.googleapis.com"]),
    );
  });
});
