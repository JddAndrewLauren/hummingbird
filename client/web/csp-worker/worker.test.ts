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

  it("preserves the underlying asset response's status and body", async () => {
    const env = fakeEnv(new Response("not found", { status: 404 }));

    const response = await worker.fetch(
      new Request("https://hb.twinion.net/missing"),
      env,
    );

    expect(response.status).toBe(404);
    expect(await response.text()).toBe("not found");
  });

  it("has no unsafe-inline and scopes connect-src to self and api.linear.app", () => {
    expect(CONTENT_SECURITY_POLICY).not.toMatch(/unsafe-inline/);
    expect(CONTENT_SECURITY_POLICY).toMatch(
      /connect-src 'self' https:\/\/api\.linear\.app/,
    );
  });
});
