// `?raw` imports (Vite/vitest's own, typed by `vite/client`'s ambient
// declarations — see `vite-env.d.ts`) rather than `node:fs`: this project's
// browser tsconfig carries no `@types/node`, and this only needs the
// source text, not filesystem access.
import coreWorkerSource from "./core.worker.ts?raw";
import useSyncWiringSource from "../shell/useSyncWiring.ts?raw";
import { describe, expect, it } from "vitest";

// ADR-0010's amendment to ADR-0007: "a second tab is a view, not a second
// cycle." Round-1 review of #107/PR#181 caught a `setInterval` inside
// `useSyncWiring.ts` — a per-view hook — driving ADR-0007's 60-second sync
// cadence: N open tabs meant N timers meant N cycles/minute against a
// single shared core, well past the ADR's explicit ~60 req/hr budget.
//
// The fix moved timer (and open/reconnect-trigger) ownership into
// `core.worker.ts`, which is constructed exactly once per origin
// (`SharedWorker`, ADR-0010) regardless of how many views connect to it —
// so "N views produce exactly one cycle" is a structural fact about WHERE
// the timer lives, not a runtime race to reproduce in a DOM-less test
// environment. This mechanically enforces that fact by inspecting the
// source text directly, the same technique `client/core/src/lib.rs`'s own
// `cargo_toml_has_no_binding_macro_dependencies` test uses to pin an
// architectural invariant that isn't otherwise executable in a unit test.

describe("the ADR-0007 sync timer lives in exactly one place", () => {
  it("core.worker.ts constructs the shared cadence and owns exactly one SYNC_TIMER_MS-driven interval", () => {
    expect(coreWorkerSource).toContain("createSyncCadence");
    expect(coreWorkerSource).toContain("SYNC_TIMER_MS");
    const intervalCalls = coreWorkerSource.match(/\bsetInterval\(/g) ?? [];
    expect(intervalCalls).toHaveLength(1);
  });

  it("core.worker.ts's cadence.onOpen()/onReconnect() are each called exactly once, not per connecting port", () => {
    expect(coreWorkerSource.match(/cadence\.onOpen\(\)/g) ?? []).toHaveLength(1);
    expect(coreWorkerSource.match(/cadence\.onReconnect\(\)/g) ?? []).toHaveLength(1);
    // Neither call is inside `self.onconnect` — which runs once PER
    // connecting view, exactly the multiplying shape this fix removes.
    const start = coreWorkerSource.indexOf("self.onconnect");
    const onconnectBody = coreWorkerSource.slice(start, start + 200);
    expect(onconnectBody).not.toContain("cadence.");
  });

  it("useSyncWiring.ts no longer owns a per-view sync timer or cadence controller", () => {
    expect(useSyncWiringSource).not.toContain("SYNC_TIMER_MS");
    expect(useSyncWiringSource).not.toContain("createSyncCadence");
    expect(useSyncWiringSource).not.toContain("runTaskSync");
  });
});
