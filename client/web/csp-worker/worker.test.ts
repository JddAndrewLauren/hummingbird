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

  it("has no unsafe-inline and scopes connect-src to self, www.googleapis.com, and accounts.google.com", () => {
    // The task API is same-origin (ADR-0008), so 'self' covers it;
    // api.linear.app was retired with the authority move.
    expect(CONTENT_SECURITY_POLICY).not.toMatch(/unsafe-inline/);
    expect(CONTENT_SECURITY_POLICY).not.toMatch(/linear/);
    expect(CONTENT_SECURITY_POLICY).toMatch(
      /connect-src 'self' https:\/\/www\.googleapis\.com https:\/\/accounts\.google\.com/,
    );
  });

  it("lets GIS reach accounts.google.com over connect-src, not only script-src and frame-src", () => {
    // The easy-to-miss half of Google's CSP requirements: minting a token
    // has GIS issue its own requests to accounts.google.com, so a policy
    // that allows the script and the iframe but not the connection blocks
    // consent and silent re-mint anyway -- and only in production, since
    // dev runs without this header.
    const connectSrc = CONTENT_SECURITY_POLICY.split("; ").find((directive) =>
      directive.startsWith("connect-src "),
    );
    expect(connectSrc).toContain("https://accounts.google.com");
  });

  it("allows wasm compilation via 'wasm-unsafe-eval' without the broader 'unsafe-eval'", () => {
    // Chrome (and other browsers following the WebAssembly/CSP integration
    // spec) refuse WebAssembly.compile/instantiate under `script-src 'self'`
    // alone -- 'wasm-unsafe-eval' is the narrow source expression that
    // permits wasm compilation without also permitting arbitrary eval.
    expect(CONTENT_SECURITY_POLICY).toMatch(
      /script-src 'self' 'wasm-unsafe-eval' https:\/\/accounts\.google\.com/,
    );
    expect(CONTENT_SECURITY_POLICY).not.toMatch(/script-src[^;]*'unsafe-eval'/);
  });

  it("allows the GIS script tag to load from accounts.google.com", () => {
    // issue #73: `google/gis.ts` injects
    // `<script src="https://accounts.google.com/gsi/client">` directly into
    // the document -- this must be an explicit script-src grant, not just
    // 'self'.
    expect(CONTENT_SECURITY_POLICY).toMatch(
      /script-src[^;]*\bhttps:\/\/accounts\.google\.com\b/,
    );
  });

  it("scopes frame-src to exactly accounts.google.com for GIS's silent re-mint iframe", () => {
    // GIS's `prompt: "none"` silent re-mint round-trips through a hidden
    // iframe served from accounts.google.com; without an explicit
    // frame-src grant the browser blocks the iframe and the silent re-mint
    // never resolves.
    expect(CONTENT_SECURITY_POLICY).toMatch(
      /frame-src https:\/\/accounts\.google\.com/,
    );
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
    expect(connectSrc).toBe("connect-src 'self' https://www.googleapis.com https://accounts.google.com");
  });

  it("does not admit any Google origin beyond what GIS and the calendar fetch require", () => {
    // Pins the policy to exactly the two Google hosts this feature needs --
    // no wildcards, no broader googleapis.com/google.com grant.
    const googleMatches = CONTENT_SECURITY_POLICY.match(
      /https:\/\/[a-z0-9.-]*google[a-z0-9.-]*/g,
    );
    expect(new Set(googleMatches)).toEqual(
      new Set(["https://accounts.google.com", "https://www.googleapis.com"]),
    );
  });
});
