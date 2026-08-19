import { describe, expect, it, vi } from "vitest";
import { CALENDAR_TOKEN_ENDPOINT, createAuthorityTokenClient } from "./authority-token-client";
import { isTokenResult } from "./token-client";

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status });
}

describe("createAuthorityTokenClient", () => {
  it("never calls fetch when no device token is stored, and returns no_device_token", async () => {
    const fetch = vi.fn();
    const client = createAuthorityTokenClient({ fetch, readToken: async () => null });

    const result = await client.requestToken("none");

    expect(result).toEqual({ error: "no_device_token" });
    expect(fetch).not.toHaveBeenCalled();
  });

  it("never calls fetch for an empty stored token either", async () => {
    const fetch = vi.fn();
    const client = createAuthorityTokenClient({ fetch, readToken: async () => "" });

    expect(await client.requestToken("consent")).toEqual({ error: "no_device_token" });
    expect(fetch).not.toHaveBeenCalled();
  });

  /** The token store is IndexedDB, which rejects outright when it is blocked
   * or corrupt. This client is documented never to throw, so that has to read
   * as "no token" rather than escaping as an unhandled rejection. */
  it("never calls fetch when the token store itself rejects, and returns no_device_token", async () => {
    const fetch = vi.fn();
    const client = createAuthorityTokenClient({
      fetch,
      readToken: async () => {
        throw new Error("InvalidStateError: the database is closing");
      },
    });

    expect(await client.requestToken("none")).toEqual({ error: "no_device_token" });
    expect(fetch).not.toHaveBeenCalled();
  });

  it("posts same-origin, authenticated, ignoring the prompt argument", async () => {
    const fetch = vi.fn(async (..._args: Parameters<typeof globalThis.fetch>) =>
      jsonResponse(200, { access_token: "ya29.x", expires_at_ms: 123 }),
    );
    const client = createAuthorityTokenClient({ fetch, readToken: async () => "hb_device_token" });

    await client.requestToken("consent");

    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe(CALENDAR_TOKEN_ENDPOINT);
    expect(init?.method).toBe("POST");
    expect((init?.headers as Record<string, string>).authorization).toBe("Bearer hb_device_token");
  });

  it("resolves to a TokenResult on 200 with a usable body", async () => {
    const client = createAuthorityTokenClient({
      fetch: async () => jsonResponse(200, { access_token: "ya29.x", expires_at_ms: 123_456 }),
      readToken: async () => "tok",
    });

    const result = await client.requestToken("none");

    expect(isTokenResult(result)).toBe(true);
    expect(result).toEqual({ accessToken: "ya29.x", expiresAtMs: 123_456 });
  });

  it("maps a rejected fetch to authority_unreachable, without throwing", async () => {
    const client = createAuthorityTokenClient({
      fetch: async () => {
        throw new Error("offline");
      },
      readToken: async () => "tok",
    });

    await expect(client.requestToken("none")).resolves.toEqual({ error: "authority_unreachable" });
  });

  it.each([401, 403])("maps a %d to authority_rejected_device_token", async (status) => {
    const client = createAuthorityTokenClient({
      fetch: async () => new Response(null, { status }),
      readToken: async () => "tok",
    });

    expect(await client.requestToken("none")).toEqual({ error: "authority_rejected_device_token" });
  });

  it("maps a 503 to authority_unconfigured", async () => {
    const client = createAuthorityTokenClient({
      fetch: async () => new Response(null, { status: 503 }),
      readToken: async () => "tok",
    });

    expect(await client.requestToken("none")).toEqual({ error: "authority_unconfigured" });
  });

  it("maps a 502 to authority_upstream", async () => {
    const client = createAuthorityTokenClient({
      fetch: async () => new Response(null, { status: 502 }),
      readToken: async () => "tok",
    });

    expect(await client.requestToken("none")).toEqual({ error: "authority_upstream" });
  });

  it("maps an unrecognised non-2xx status to bad_token_response", async () => {
    const client = createAuthorityTokenClient({
      fetch: async () => new Response(null, { status: 500 }),
      readToken: async () => "tok",
    });

    expect(await client.requestToken("none")).toEqual({ error: "bad_token_response" });
  });

  it("maps a 200 body that is not readable JSON to bad_token_response, without throwing", async () => {
    const client = createAuthorityTokenClient({
      fetch: async () => new Response("not json", { status: 200 }),
      readToken: async () => "tok",
    });

    await expect(client.requestToken("none")).resolves.toEqual({ error: "bad_token_response" });
  });

  it("maps a 200 body missing access_token to no_access_token", async () => {
    const client = createAuthorityTokenClient({
      fetch: async () => jsonResponse(200, { expires_at_ms: 123 }),
      readToken: async () => "tok",
    });

    expect(await client.requestToken("none")).toEqual({ error: "no_access_token" });
  });

  it("maps a 200 body missing expires_at_ms to no_access_token", async () => {
    const client = createAuthorityTokenClient({
      fetch: async () => jsonResponse(200, { access_token: "ya29.x" }),
      readToken: async () => "tok",
    });

    expect(await client.requestToken("none")).toEqual({ error: "no_access_token" });
  });

  /** `null` and `"ya29.x"` are both valid JSON, so `response.json()` resolves
   * and the guard — not the parse `try` — is what has to survive them. Reading
   * a field off either would throw out of a client documented never to. */
  it.each([null, "ya29.x", 7])("maps a 200 body that is not an object (%p) to no_access_token", async (body) => {
    const client = createAuthorityTokenClient({
      fetch: async () => jsonResponse(200, body),
      readToken: async () => "tok",
    });

    await expect(client.requestToken("none")).resolves.toEqual({ error: "no_access_token" });
  });

  it("never puts the device token in any returned string", async () => {
    const client = createAuthorityTokenClient({
      fetch: async () => new Response(null, { status: 401 }),
      readToken: async () => "hb_super_secret_device_token",
    });

    const result = await client.requestToken("none");

    expect(JSON.stringify(result)).not.toContain("hb_super_secret_device_token");
  });
});
