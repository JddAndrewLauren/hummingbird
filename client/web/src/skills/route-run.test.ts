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

/** A response that resolves (headers) immediately but trickles its body in
 * slowly, and — like a real `fetch` — actually tears the read down when
 * `signal` aborts mid-wait. `perLineDelayMs` is real elapsed time, not a
 * mocked clock: this is exercising `AbortSignal` timing, which fake timers
 * do not drive. */
function slowNdjson(
  status: number,
  perLineDelayMs: number,
  signal: AbortSignal | undefined,
  ...lines: string[]
): Response {
  const body = new ReadableStream<Uint8Array>({
    async start(controller) {
      const encoder = new TextEncoder();
      for (const line of lines) {
        await new Promise<void>((resolve, reject) => {
          if (signal?.aborted) {
            reject(new DOMException("Aborted", "AbortError"));
            return;
          }
          const timer = setTimeout(resolve, perLineDelayMs);
          signal?.addEventListener(
            "abort",
            () => {
              clearTimeout(timer);
              reject(new DOMException("Aborted", "AbortError"));
            },
            { once: true },
          );
        });
        controller.enqueue(encoder.encode(`${line}\n`));
      }
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

  it("connectTimeoutMs bounds only the connect phase, not the whole streamed run", async () => {
    // `fetch` itself resolves instantly (a prompt first response), but the
    // body then takes longer than connectTimeoutMs to finish streaming: a
    // progress line at 40ms, the terminal line at 80ms, against a 50ms
    // connect timeout. A connect timeout that stays armed across the whole
    // run aborts the read mid-stream (same signal used for both `fetch` and
    // the body reader); one scoped to the connect phase alone must let this
    // finish.
    const entry: BackendEntry = { ...CLOUD, connectTimeoutMs: 50 };
    const memo = memoStore();
    const events = await collect(
      runRouted("cloud", () => ({ skill: "microtask", args: {} }), {
        registry: [entry],
        memoStore: memo,
        now: () => 0,
        readToken: async () => "hb_token",
        fetch: async (_input, init) =>
          slowNdjson(
            200,
            40,
            init?.signal ?? undefined,
            '{"type":"progress","message":"still running"}',
            '{"ok":true,"result":null,"backend":"anthropic","model":"opus"}',
          ),
      }),
    );
    expect(events.at(-1)).toMatchObject({ kind: "ok" });
  });

  it("an all-skipped plan still emits exactly one terminal event, never leaving the run open", async () => {
    // Every registered entry memoized dead: `planRoute` returns a sequence
    // of nothing but `skip` steps, and no attempt is ever made. The
    // `started`/terminal contract (module header) has to hold even here —
    // otherwise a caller's reducer is stuck at `phase: "running"` forever,
    // because a `progress` line never closes a run.
    const memo = memoStore(markDead(EMPTY_MEMO, "cloud", 0, 30_000));
    const events = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD],
        memoStore: memo,
        now: () => 1_000,
        readToken: async () => "hb_token",
        fetch: async () => {
          throw new Error("must not be called: every tier is a skip");
        },
      }),
    );
    expect(events[0]).toEqual({ kind: "started" });
    const terminals = events.filter((event) => event.kind === "ok" || event.kind === "failed");
    expect(terminals).toHaveLength(1);
    expect(terminals[0]).toMatchObject({ kind: "failed" });
  });

  it("a NO_TOKEN decline does not mark the backend dead", async () => {
    const memo = memoStore();
    const events = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD],
        memoStore: memo,
        now: () => 0,
        readToken: async () => null,
        fetch: async () => {
          throw new Error("must not be called: NO_TOKEN never reaches fetch");
        },
      }),
    );
    expect(events).toEqual([{ kind: "started" }, { kind: "failed", error: NO_TOKEN, backend: null, model: null }]);
    expect(isFreshDead(memo.current, "cloud", 0)).toBe(false);
  });

  it("double-tapping with no device token declines both times, each with exactly one terminal event", async () => {
    // The reported repro: no device token stored, tap the affordance twice
    // within the memo's TTL. If the first tap wrongly memoized "cloud" as
    // dead, the second tap's plan would be all-skip and (absent the fix
    // above) emit zero terminal events — the button-disabled-forever bug.
    const memo = memoStore();
    const deps = {
      registry: [CLOUD],
      memoStore: memo,
      readToken: async () => null,
      fetch: async () => {
        throw new Error("must not be called: NO_TOKEN never reaches fetch");
      },
    };

    const first = await collect(runRouted("auto", () => ({ skill: "microtask", args: {} }), { ...deps, now: () => 0 }));
    const second = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), { ...deps, now: () => 1_000 }),
    );

    for (const events of [first, second]) {
      expect(events[0]).toEqual({ kind: "started" });
      const terminals = events.filter((event) => event.kind === "ok" || event.kind === "failed");
      expect(terminals).toHaveLength(1);
      expect(terminals[0]).toMatchObject({ kind: "failed", error: NO_TOKEN });
    }
  });

  // A tier that ANSWERED with an error is not an unreachable tier. #307
  // made the seam's decline a first-class outcome carrying a stamp, and a
  // 400/413 is forwarded verbatim by the proxy — routing must treat both as
  // this run's answer, not as a reason to memoize, reword or reroute.

  const SEAM_DECLINE = '{"ok":false,"error":"That item already has live steps.","backend":"anthropic","model":"opus"}';

  it("a seam decline is this run's terminal, verbatim and stamped, and never memoizes the tier dead", async () => {
    const memo = memoStore();
    const events = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD],
        memoStore: memo,
        now: () => 0,
        readToken: async () => "hb_token",
        fetch: async () => ndjson(200, SEAM_DECLINE),
      }),
    );
    // The stamp survives: the wire said anthropic/opus, so the event does
    // too. `failure()` would have flattened both to null and taken the
    // badge off every declined run.
    expect(events).toEqual([
      { kind: "started" },
      { kind: "failed", error: "That item already has live steps.", backend: "anthropic", model: "opus" },
    ]);
    expect(isFreshDead(memo.current, "cloud", 0)).toBe(false);
  });

  it("a seam decline under Auto is the answer, never a fallthrough to the next tier", async () => {
    const memo = memoStore();
    const calledEndpoints: string[] = [];
    const events = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD, HOME],
        memoStore: memo,
        now: () => 0,
        readToken: async () => "hb_token",
        fetch: async (input) => {
          calledEndpoints.push(String(input));
          return ndjson(200, SEAM_DECLINE);
        },
      }),
    );
    // Home is never tried: the seam's answer is the run's answer, and
    // re-asking a second backend would re-run work #307 just declined.
    expect(calledEndpoints).toEqual(["/a"]);
    expect(events.at(-1)).toMatchObject({ kind: "failed", error: "That item already has live steps." });
    expect(isFreshDead(memo.current, "home", 0)).toBe(false);
  });

  it("a pinned seam decline reaches the caller byte-identically, unprefixed", async () => {
    const memo = memoStore();
    const events = await collect(
      runRouted("cloud", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD, HOME],
        memoStore: memo,
        now: () => 0,
        readToken: async () => "hb_token",
        fetch: async () => ndjson(200, SEAM_DECLINE),
      }),
    );
    // NOT "Cloud runner did not answer: …" — it answered. `decline.ts`'s
    // verbatim rule is the point, and the panel pins the same string.
    expect(events.at(-1)).toEqual({
      kind: "failed",
      error: "That item already has live steps.",
      backend: "anthropic",
      model: "opus",
    });
    expect(isFreshDead(memo.current, "cloud", 0)).toBe(false);
  });

  it("a 400 forwarded verbatim by the proxy does not memoize the tier dead", async () => {
    const memo = memoStore();
    const events = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), {
        registry: [CLOUD],
        memoStore: memo,
        now: () => 0,
        readToken: async () => "hb_token",
        fetch: async () => ndjson(400, '{"ok":false,"error":"grain must be a number","backend":"anthropic","model":null}'),
      }),
    );
    expect(events.at(-1)).toEqual({
      kind: "failed",
      error: "grain must be a number",
      backend: "anthropic",
      model: null,
    });
    expect(isFreshDead(memo.current, "cloud", 0)).toBe(false);
  });

  it("a second tap inside the memo's TTL still reaches a backend that declined the first", async () => {
    // The reviewer's repro: single entry, default Auto, 30s TTL. The first
    // tap is declined by the seam; the user fixes the thing and taps again
    // eight seconds later. Memoizing that decline as death would answer the
    // second tap "No backend is currently reachable." with no request sent.
    const memo = memoStore();
    const calledEndpoints: string[] = [];
    const deps = {
      registry: [CLOUD],
      memoStore: memo,
      readToken: async () => "hb_token",
    };
    const first = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), {
        ...deps,
        now: () => 0,
        fetch: async (input: RequestInfo | URL) => {
          calledEndpoints.push(String(input));
          return ndjson(200, SEAM_DECLINE);
        },
      }),
    );
    const second = await collect(
      runRouted("auto", () => ({ skill: "microtask", args: {} }), {
        ...deps,
        now: () => 8_000,
        fetch: async (input: RequestInfo | URL) => {
          calledEndpoints.push(String(input));
          return ndjson(200, '{"ok":true,"result":null,"backend":"anthropic","model":"opus"}');
        },
      }),
    );
    expect(calledEndpoints).toEqual(["/a", "/a"]);
    expect(first.at(-1)).toMatchObject({ kind: "failed" });
    expect(second.at(-1)).toMatchObject({ kind: "ok" });
  });

  it("a connect timeout that fires declines as unreachable and does memoize the tier dead", async () => {
    // The positive direction of the connect-timeout fix: a sleeping host
    // whose `fetch` settles only when aborted. Nothing else in the suite
    // notices if the arming listener is dropped altogether, and this is
    // also the case that must STILL mark dead — the classification above
    // must not over-correct into never memoizing anything.
    const memo = memoStore();
    const events = await collect(
      runRouted("cloud", () => ({ skill: "microtask", args: {} }), {
        registry: [{ ...CLOUD, connectTimeoutMs: 20 }],
        memoStore: memo,
        now: () => 0,
        readToken: async () => "hb_token",
        fetch: (_input, init) =>
          new Promise<Response>((_resolve, reject) => {
            init?.signal?.addEventListener(
              "abort",
              () => reject(new DOMException("The operation was aborted.", "AbortError")),
              { once: true },
            );
          }),
      }),
    );
    const last = events.at(-1);
    expect(last).toMatchObject({ kind: "failed" });
    expect((last as { error: string }).error).toContain("Cloud runner did not answer");
    expect(isFreshDead(memo.current, "cloud", 0)).toBe(true);
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
