import { describe, expect, it, vi } from "vitest";
import { createGrillTranscriptClient, GRILL_ENDPOINT } from "./transcript-client";

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status });
}

const GRILL = {
  id: "grill-1",
  item_id: "item-1",
  transcript: "Q: What does done look like?\nA: The tap stops dripping.",
  summary: "Resolved",
  verdict: "resolved",
  model_proposal: "{}",
  applied_patch: "{}",
  resulting_stage: "ready",
  completed_at: 1,
  version: 1,
};

describe("createGrillTranscriptClient", () => {
  it("never calls fetch when no device token is stored", async () => {
    const fetch = vi.fn();
    const fetchTranscript = createGrillTranscriptClient({ fetch, readToken: async () => null });

    expect(await fetchTranscript("grill-1")).toEqual({ kind: "failed", reason: "no_device_token" });
    expect(fetch).not.toHaveBeenCalled();
  });

  it("reads a rejecting token store as no token, never throwing", async () => {
    const fetch = vi.fn();
    const fetchTranscript = createGrillTranscriptClient({
      fetch,
      readToken: async () => {
        throw new Error("InvalidStateError");
      },
    });

    expect(await fetchTranscript("grill-1")).toEqual({ kind: "failed", reason: "no_device_token" });
    expect(fetch).not.toHaveBeenCalled();
  });

  it("GETs the one grill same-origin, authenticated, and answers its transcript", async () => {
    const fetch = vi.fn(async (..._args: Parameters<typeof globalThis.fetch>) => jsonResponse(200, GRILL));
    const fetchTranscript = createGrillTranscriptClient({ fetch, readToken: async () => "hb_device_token" });

    expect(await fetchTranscript("grill-1")).toEqual({ kind: "ok", transcript: GRILL.transcript });
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe(`${GRILL_ENDPOINT}grill-1`);
    expect(init?.method).toBe("GET");
    expect((init?.headers as Record<string, string>).authorization).toBe("Bearer hb_device_token");
  });

  it.each([
    [401, "rejected"],
    [403, "rejected"],
    [404, "not_found"],
    [500, "bad_response"],
  ] as const)("maps a %i to %s", async (status, reason) => {
    const fetch = vi.fn(async () => jsonResponse(status, { error: "x" }));
    const fetchTranscript = createGrillTranscriptClient({ fetch, readToken: async () => "t" });

    expect(await fetchTranscript("grill-1")).toEqual({ kind: "failed", reason });
  });

  it("reads a rejected fetch as unreachable", async () => {
    const fetch = vi.fn(async () => {
      throw new TypeError("Failed to fetch");
    });
    const fetchTranscript = createGrillTranscriptClient({ fetch, readToken: async () => "t" });

    expect(await fetchTranscript("grill-1")).toEqual({ kind: "failed", reason: "unreachable" });
  });

  it("reads a 200 with no string transcript as a bad response", async () => {
    for (const body of [null, {}, { transcript: 3 }]) {
      const fetch = vi.fn(async () => jsonResponse(200, body));
      const fetchTranscript = createGrillTranscriptClient({ fetch, readToken: async () => "t" });
      expect(await fetchTranscript("grill-1")).toEqual({ kind: "failed", reason: "bad_response" });
    }
  });
});
