import { describe, expect, it } from "vitest";
import type { BackendEntry } from "./backend-registry";
import { markDead } from "./reachability-memo";
import { planRoute } from "./route-plan";

const CLOUD: BackendEntry = { id: "cloud", label: "Cloud runner", model: null, endpoint: "/a", connectTimeoutMs: 1 };
const HOME: BackendEntry = { id: "home", label: "Home runner", model: null, endpoint: "/b", connectTimeoutMs: 1 };
const REGISTRY = [CLOUD, HOME];

describe("planRoute", () => {
  it("Auto with nothing memoized tries every entry in registry order", () => {
    const plan = planRoute("auto", REGISTRY, {}, 0);
    expect(plan).toEqual({
      kind: "sequence",
      steps: [
        { kind: "attempt", entry: CLOUD },
        { kind: "attempt", entry: HOME },
      ],
    });
  });

  it("Auto skips a tier whose memo says it just failed", () => {
    const memo = markDead({}, "cloud", 0, 30_000);
    const plan = planRoute("auto", REGISTRY, memo, 1_000);
    expect(plan).toEqual({
      kind: "sequence",
      steps: [
        { kind: "skip", entry: CLOUD },
        { kind: "attempt", entry: HOME },
      ],
    });
  });

  it("Auto retries a tier once its memo has expired", () => {
    const memo = markDead({}, "cloud", 0, 30_000);
    const plan = planRoute("auto", REGISTRY, memo, 31_000);
    expect(plan.kind).toBe("sequence");
    expect((plan as { kind: "sequence"; steps: unknown[] }).steps[0]).toEqual({ kind: "attempt", entry: CLOUD });
  });

  it("a pin with nothing memoized against it is a one-step sequence naming only itself", () => {
    const plan = planRoute("home", REGISTRY, {}, 0);
    expect(plan).toEqual({ kind: "sequence", steps: [{ kind: "attempt", entry: HOME }] });
  });

  it("a pin never falls through to another entry on its own, even mid-sequence", () => {
    const plan = planRoute("cloud", REGISTRY, {}, 0);
    expect(plan).toEqual({ kind: "sequence", steps: [{ kind: "attempt", entry: CLOUD }] });
  });

  it("a pin memoized dead declines outright, naming the entry and the fallback", () => {
    const memo = markDead({}, "cloud", 0, 30_000);
    const plan = planRoute("cloud", REGISTRY, memo, 1_000);
    expect(plan).toEqual({ kind: "declined", entry: CLOUD, fallback: HOME });
  });

  it("a pin memoized dead with no other registered entry declines with no fallback", () => {
    const memo = markDead({}, "cloud", 0, 30_000);
    const plan = planRoute("cloud", [CLOUD], memo, 1_000);
    expect(plan).toEqual({ kind: "declined", entry: CLOUD, fallback: null });
  });

  it("a stale selection naming a retired backend degrades to Auto", () => {
    const plan = planRoute("retired", REGISTRY, {}, 0);
    expect(plan).toEqual({
      kind: "sequence",
      steps: [
        { kind: "attempt", entry: CLOUD },
        { kind: "attempt", entry: HOME },
      ],
    });
  });
});
