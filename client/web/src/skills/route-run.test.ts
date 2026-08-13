import { describe, expect, it } from "vitest";
import type { BackendEntry } from "./backend-registry";
import { NO_TOKEN } from "./decline";
import { EMPTY_MEMO, isFreshDead, markDead, type ReachabilityMemo } from "./reachability-memo";
import { runRouted, type ReachabilityMemoStore } from "./route-run";
import type { SkillEvent } from "./run-state";

const CLOUD: BackendEntry = { id: "cloud", label: "Cloud runner", model: null, endpoint: "/a", connectTimeoutMs: 50 };
const HOME: BackendEntry = { id: "home", label: "Home runner", model: "llama3", endpoint: "/b", connectTimeoutMs: 50 };

function ndjson(status: number, ...lines: string[]): Response {
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      const encoder = new TextEncoder();
      for (const line of lines) controller.enqueue(encoder.encode(`${line}\n`));
      controller.close();
    },
  });
  return new Response(body, { status });
}

function memoStore(initial: ReachabilityMemo = EMPTY_MEMO): ReachabilityMemoStore & { current: ReachabilityMemo } {
  const store = {
    current: initial,
    get() {
      return store.current;
    },
    set(next: ReachabilityMemo) {
      store.current = next;
    },
  };
  return store;
}

async function collect(events: AsyncGenerator<SkillEvent>): Promise<SkillEvent[]> {
  const out: SkillEvent[] = [];
  for await (const event of events) out.push(event);
  return out;
}

describe("runRouted", () => {
  it("Auto attempts the first registered entry and forwards its stamp verbatim", async () => {
    const memo = memoStore();
    const events = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD, HOME],
        memoStore: memo,
        now: () => 0,
        readToken: async () => "hb_token",
        fetch: async (_input) => ndjson(200, '{"ok":true,"result":null,"backend":"anthropic","model":"opus"}'),
      }),
    );
    expect(events).toEqual([
      { kind: "started" },
      { kind: "ok", result: null, backend: "anthropic", model: "opus" },
    ]);
    expect(isFreshDead(memo.current, "cloud", 0)).toBe(false);
  });

  it("Auto falls through to the next tier when the first is unreachable, narrating the skip", async () => {
    const memo = memoStore();
    const events = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD, HOME],
        memoStore: memo,
        now: () => 0,
        readToken: async () => "hb_token",
        fetch: async (input) =>
          input === "/a"
            ? Promise.reject(new Error("connection refused"))
            : ndjson(200, '{"ok":true,"result":null,"backend":"home-runner","model":"llama3"}'),
      }),
    );
    expect(events).toEqual([
      { kind: "started" },
      { kind: "progress", message: "Cloud runner unreachable, trying the next backend." },
      // The stamp names whoever actually answered — home, not the first tier.
      { kind: "ok", result: null, backend: "home-runner", model: "llama3" },
    ]);
    expect(isFreshDead(memo.current, "cloud", 0)).toBe(true);
  });

  it("Auto declines, naming the last tier tried, when every tier fails", async () => {
    const memo = memoStore();
    const events = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD, HOME],
        memoStore: memo,
        now: () => 0,
        readToken: async () => "hb_token",
        fetch: async () => Promise.reject(new Error("connection refused")),
      }),
    );
    expect(events[0]).toEqual({ kind: "started" });
    expect(events[1]).toMatchObject({ kind: "progress" });
    const last = events.at(-1);
    expect(last).toMatchObject({ kind: "failed" });
    expect((last as { error: string }).error).toContain("connection refused");
  });

  it("Auto skips a tier the memo already knows is dead, without calling fetch for it", async () => {
    const memo = memoStore(markDead(EMPTY_MEMO, "cloud", 0, 30_000));
    const calledEndpoints: string[] = [];
    const events = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD, HOME],
        memoStore: memo,
        now: () => 1_000,
        readToken: async () => "hb_token",
        fetch: async (input) => {
          calledEndpoints.push(String(input));
          return ndjson(200, '{"ok":true,"result":null,"backend":"home-runner","model":"llama3"}');
        },
      }),
    );
    expect(calledEndpoints).toEqual(["/b"]);
    expect(events).toEqual([
      { kind: "started" },
      { kind: "progress", message: "Skipping Cloud runner: recently unreachable." },
      { kind: "ok", result: null, backend: "home-runner", model: "llama3" },
    ]);
  });

  it("a pin succeeds by attempting only itself, even with other entries registered", async () => {
    const memo = memoStore();
    const calledEndpoints: string[] = [];
    const events = await collect(
      runRouted("cloud", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD, HOME],
        memoStore: memo,
        now: () => 0,
        readToken: async () => "hb_token",
        fetch: async (input) => {
          calledEndpoints.push(String(input));
          return ndjson(200, '{"ok":true,"result":null,"backend":"anthropic","model":"opus"}');
        },
      }),
    );
    expect(calledEndpoints).toEqual(["/a"]);
    expect(events.at(-1)).toMatchObject({ kind: "ok" });
  });

  it("a pin that fails to answer declines loudly, naming it, and never tries another entry", async () => {
    const memo = memoStore();
    const calledEndpoints: string[] = [];
    const events = await collect(
      runRouted("cloud", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD, HOME],
        memoStore: memo,
        now: () => 0,
        readToken: async () => "hb_token",
        fetch: async (input) => {
          calledEndpoints.push(String(input));
          return Promise.reject(new Error("connection refused"));
        },
      }),
    );
    expect(calledEndpoints).toEqual(["/a"]);
    const last = events.at(-1);
    expect(last).toMatchObject({ kind: "failed" });
    expect((last as { error: string }).error).toContain("Cloud runner did not answer");
    expect((last as { error: string }).error).toContain("connection refused");
    expect(isFreshDead(memo.current, "cloud", 0)).toBe(true);
  });

  it("a pin memoized dead declines immediately, without calling fetch, naming the fallback", async () => {
    const memo = memoStore(markDead(EMPTY_MEMO, "cloud", 0, 30_000));
    let called = false;
    const events = await collect(
      runRouted("cloud", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD, HOME],
        memoStore: memo,
        now: () => 1_000,
        readToken: async () => "hb_token",
        fetch: async () => {
          called = true;
          return ndjson(200, '{"ok":true,"result":null,"backend":"a","model":null}');
        },
      }),
    );
    expect(called).toBe(false);
    expect(events).toEqual([
      { kind: "started" },
      { kind: "failed", error: "Cloud runner is not answering right now. Try Home runner instead.", backend: null, model: null },
    ]);
  });

  it("a pin memoized dead with no other registered entry declines with no fallback named", async () => {
    const memo = memoStore(markDead(EMPTY_MEMO, "cloud", 0, 30_000));
    const events = await collect(
      runRouted("cloud", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD],
        memoStore: memo,
        now: () => 1_000,
        readToken: async () => "hb_token",
        fetch: async () => ndjson(200, "{}"),
      }),
    );
    expect(events).toEqual([
      { kind: "started" },
      { kind: "failed", error: "Cloud runner is not answering right now.", backend: null, model: null },
    ]);
  });

  it("calls buildBody once per attempted entry, with that entry", async () => {
    const seen: string[] = [];
    const memo = memoStore();
    await collect(
      runRouted(
        "auto",
        (entry) => {
          seen.push(entry.id);
          return { skill: "microtask", args: { model: entry.model } };
        },
        {
          registry: [CLOUD, HOME],
          memoStore: memo,
          now: () => 0,
          readToken: async () => "hb_token",
          fetch: async (input) =>
            input === "/a"
              ? Promise.reject(new Error("down"))
              : ndjson(200, '{"ok":true,"result":null,"backend":"b","model":"llama3"}'),
        },
      ),
    );
    expect(seen).toEqual(["cloud", "home"]);
  });

  it("with no token stored it declines without any fetch, same as the unrouted seam", async () => {
    const memo = memoStore();
    let called = false;
    const events = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD],
        memoStore: memo,
        now: () => 0,
        readToken: async () => null,
        fetch: async () => {
          called = true;
          return ndjson(200, "{}");
        },
      }),
    );
    expect(called).toBe(false);
    expect(events).toEqual([{ kind: "started" }, { kind: "failed", error: NO_TOKEN, backend: null, model: null }]);
  });
});
