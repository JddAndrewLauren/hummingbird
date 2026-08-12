import { describe, expect, it, vi } from "vitest";
import { NO_TERMINAL_LINE, NO_TOKEN } from "./decline";
import { runSkill, type RunSkillDeps, type SkillBackend } from "./run-skill";
import type { SkillEvent } from "./run-state";

const BACKEND: SkillBackend = { id: "cloud", endpoint: "/api/skills/run" };

/** A `Response` whose body streams the given chunks — deliberately at
 * awkward boundaries, since a chunk has no relationship to a line. */
function ndjsonResponse(status: number, chunks: string[]): Response {
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      const encoder = new TextEncoder();
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      controller.close();
    },
  });
  return new Response(body, { status, headers: { "content-type": "application/x-ndjson" } });
}

async function collect(
  deps: Partial<RunSkillDeps> & Pick<RunSkillDeps, "fetch">,
): Promise<SkillEvent[]> {
  const events: SkillEvent[] = [];
  for await (const event of runSkill(BACKEND, { skill: "microtask", args: { ref: "i" } }, {
    readToken: async () => "hb_device_token",
    ...deps,
  })) {
    events.push(event);
  }
  return events;
}

describe("runSkill", () => {
  it("yields started, then the progress lines, then exactly one terminal", async () => {
    const events = await collect({
      fetch: async () =>
        ndjsonResponse(200, [
          '{"type":"progress","mess',
          'age":"reading"}\n{"type":"progress","message":"writing"}\n',
          '{"ok":true,"skill":"microtask","result":{"steps":["a"],"note":"n"},"backend":"anthropic","model":"opus"}\n',
        ]),
    });
    expect(events).toEqual([
      { kind: "started" },
      { kind: "progress", message: "reading" },
      { kind: "progress", message: "writing" },
      { kind: "ok", result: { steps: ["a"], note: "n" }, backend: "anthropic", model: "opus" },
    ]);
  });

  it("reads a last line that arrives with no trailing newline", async () => {
    const events = await collect({
      fetch: async () => ndjsonResponse(200, ['{"ok":true,"result":null,"backend":"a","model":null}']),
    });
    expect(events.at(-1)).toMatchObject({ kind: "ok" });
  });

  /** A multi-byte character split across two chunks would become two U+FFFD
   * under a per-chunk `decode()`. */
  it("survives a multi-byte character split across chunks", async () => {
    const encoded = new TextEncoder().encode('{"type":"progress","message":"café"}\n');
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(encoded.slice(0, 34));
        controller.enqueue(encoded.slice(34));
        controller.close();
      },
    });
    const events = await collect({
      fetch: async () => new Response(body, { status: 200 }),
    });
    expect(events).toContainEqual({ kind: "progress", message: "café" });
  });

  it("discards everything after the first terminal line", async () => {
    const events = await collect({
      fetch: async () =>
        ndjsonResponse(200, [
          '{"ok":true,"result":null,"backend":"a","model":null}\n',
          '{"ok":false,"error":"second","backend":"a","model":null}\n',
          '{"type":"progress","message":"late"}\n',
        ]),
    });
    expect(events.filter((e) => e.kind === "ok" || e.kind === "failed")).toHaveLength(1);
    expect(events.at(-1)).toMatchObject({ kind: "ok" });
  });

  it("drops an unreadable line without ending the run", async () => {
    const events = await collect({
      fetch: async () =>
        ndjsonResponse(200, ["not json at all\n", '{"ok":true,"result":null,"backend":"a","model":null}\n']),
    });
    expect(events).toHaveLength(2);
    expect(events.at(-1)).toMatchObject({ kind: "ok" });
  });

  /** A 400 or 413 is forwarded verbatim by the proxy and IS a valid
   * terminal line, so status alone must not decide. */
  it("a 400 carrying a terminal line uses that line, not the status", async () => {
    const events = await collect({
      fetch: async () => ndjsonResponse(400, ['{"ok":false,"skill":"microtask","error":"bad ref"}\n']),
    });
    expect(events.at(-1)).toEqual({ kind: "failed", error: "bad ref", backend: null, model: null });
  });

  it("a 503 carrying the proxy's own line uses that line", async () => {
    const events = await collect({
      fetch: async () =>
        ndjsonResponse(503, [
          '{"ok":false,"skill":null,"error":"The cloud runner is not configured on this server."}\n',
        ]),
    });
    expect(events.at(-1)).toMatchObject({
      kind: "failed",
      error: "The cloud runner is not configured on this server.",
    });
  });

  it("an empty-bodied 401 declines by naming the credential", async () => {
    const events = await collect({ fetch: async () => new Response(null, { status: 401 }) });
    expect(events.at(-1)).toMatchObject({ kind: "failed" });
    expect((events.at(-1) as { error: string }).error).toMatch(/device token/i);
  });

  it("a 200 that ends with nothing terminal says so", async () => {
    const events = await collect({
      fetch: async () => ndjsonResponse(200, ['{"type":"progress","message":"reading"}\n']),
    });
    expect(events.at(-1)).toEqual({ kind: "failed", error: NO_TERMINAL_LINE, backend: null, model: null });
  });

  it("a rejecting fetch becomes a terminal event, never a throw", async () => {
    const events = await collect({
      fetch: async () => {
        throw new Error("Failed to fetch");
      },
    });
    expect(events.at(-1)).toMatchObject({ kind: "failed" });
    expect((events.at(-1) as { error: string }).error).toContain("Failed to fetch");
  });

  /** Both spellings of "no credential": `null` from a store that holds
   * nothing, and `""` from one holding an empty record. Issuing the request
   * anyway would spend a round trip to be told what is already known here. */
  it("with no token stored it declines and never calls fetch", async () => {
    for (const token of [null, ""]) {
      const fetchSpy = vi.fn();
      const events = await collect({ fetch: fetchSpy as never, readToken: async () => token });
      expect(fetchSpy, String(token)).not.toHaveBeenCalled();
      expect(events).toEqual([
        { kind: "started" },
        { kind: "failed", error: NO_TOKEN, backend: null, model: null },
      ]);
    }
  });

  it("sends the token as a bearer, and never puts it in a message", async () => {
    const fetchSpy = vi.fn(
      async (_input: unknown, _init?: RequestInit) =>
        ndjsonResponse(200, ['{"ok":true,"result":null,"backend":"a","model":null}\n']),
    );
    const events = await collect({ fetch: fetchSpy as never });
    const headers = fetchSpy.mock.calls[0]?.[1]?.headers as Record<string, string>;
    expect(headers.authorization).toBe("Bearer hb_device_token");
    expect(JSON.stringify(events)).not.toContain("hb_device_token");
  });

  it("aborting mid-stream stops without throwing", async () => {
    const controller = new AbortController();
    const body = new ReadableStream<Uint8Array>({
      start(streamController) {
        const encoder = new TextEncoder();
        streamController.enqueue(encoder.encode('{"type":"progress","message":"reading"}\n'));
        controller.abort();
        streamController.enqueue(encoder.encode('{"ok":true,"result":null,"backend":"a","model":null}\n'));
        streamController.close();
      },
    });
    const events = await collect({
      fetch: async () => new Response(body, { status: 200 }),
      signal: controller.signal,
    });
    // The run stops being narrated. The terminal event it synthesizes is the
    // honest one: nothing said the run finished.
    expect(events.at(-1)).toMatchObject({ kind: "failed" });
  });
});
