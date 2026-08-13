// `?raw` source-text pins, the same technique — and for the same reason —
// as `worker/same-origin-api.test.ts`.
//
// #273's acceptance says an unreachable or failed run puts **nothing** in
// the sync queue, the pending-mutation overlay or the dead-letter journal,
// and introduces **no new timer**. An assertion over an empty queue proves
// nothing about a lane that must not exist: the queue would be empty
// whether or not this code could reach it. What can be proved is that the
// lane is *physically unable* to reach any of them — an import-graph fact,
// and this repo already accepts source pins for that class of invariant.
//
// A sync mutation is a fact the user already decided; a skill request is a
// question, and questions go stale (#269). That is the whole reason the two
// lanes must not touch.

import { describe, expect, it } from "vitest";
import backendRegistrySource from "./backend-registry.ts?raw";
import backendSelectionSource from "./backend-selection.ts?raw";
import declineSource from "./decline.ts?raw";
import envelopeSource from "./envelope.ts?raw";
import microtaskAffordanceSource from "./microtask-affordance.ts?raw";
import microtaskArgsSource from "./microtask-args.ts?raw";
import ndjsonSource from "./ndjson.ts?raw";
import reachabilityMemoSource from "./reachability-memo.ts?raw";
import routePlanSource from "./route-plan.ts?raw";
import routeRunSource from "./route-run.ts?raw";
import runSkillSource from "./run-skill.ts?raw";
import runStateSource from "./run-state.ts?raw";
import wiringSource from "../shell/useMicrotaskWiring.ts?raw";
import panelSource from "../components/domain/ItemDetailPanel.tsx?raw";

const SKILL_MODULES: Array<[string, string]> = [
  ["backend-registry.ts", backendRegistrySource],
  ["backend-selection.ts", backendSelectionSource],
  ["decline.ts", declineSource],
  ["envelope.ts", envelopeSource],
  ["microtask-affordance.ts", microtaskAffordanceSource],
  ["microtask-args.ts", microtaskArgsSource],
  ["ndjson.ts", ndjsonSource],
  ["reachability-memo.ts", reachabilityMemoSource],
  ["route-plan.ts", routePlanSource],
  ["route-run.ts", routeRunSource],
  ["run-skill.ts", runSkillSource],
  ["run-state.ts", runStateSource],
];

/** Comments discuss all of this at length; only code counts. */
function code(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .split("\n")
    .filter((line) => !line.trimStart().startsWith("//"))
    .join("\n");
}

describe("the skill lane cannot reach the sync engine", () => {
  it("no module in src/skills/ imports the store or the worker layer", () => {
    for (const [name, source] of SKILL_MODULES) {
      const imports = [...code(source).matchAll(/from\s+"([^"]+)"/g)].map((match) => match[1]);
      for (const specifier of imports) {
        // The one permitted crossing is a TYPE: `StepDTO` from the
        // protocol, which carries no runtime code at all.
        const allowed = specifier.startsWith("./") || specifier === "../store/protocol";
        expect(allowed, `${name} imports ${specifier}`).toBe(true);
      }
    }
  });

  it("the wiring hook reaches worker-client for exactly one thing", () => {
    // `triggerSyncManual` asks the SHARED CADENCE for one cycle — it is not
    // an enqueue, and it is the only worker-client symbol this lane may
    // name. Anything that enqueues a mutation (`captureTask`, `actOnTask`,
    // `triageItem`, `setBinding`, …) reaching this file would put a
    // question into the queue that holds decisions.
    const match = /import\s*\{([^}]*)\}\s*from\s*"\.\.\/store\/worker-client"/.exec(code(wiringSource));
    expect(match, "the hook must import from worker-client explicitly").not.toBeNull();
    const named = (match?.[1] ?? "")
      .split(",")
      .map((entry) => entry.replace(/^\s*type\s+/, "").trim())
      .filter((entry) => entry.length > 0);
    expect(named.sort()).toEqual(["WorkerLike", "triggerSyncManual"]);
  });

  it("nothing in the lane starts a timer", () => {
    // ADR-0007's single 60s interval stays the only clock. An
    // `AbortController` is not a clock, which is why aborting on unmount is
    // allowed here and a `setTimeout` is not.
    for (const [name, source] of [...SKILL_MODULES, ["useMicrotaskWiring.ts", wiringSource]] as Array<
      [string, string]
    >) {
      const body = code(source);
      for (const banned of ["setInterval", "setTimeout", "requestAnimationFrame"]) {
        expect(body.includes(banned), `${name} calls ${banned}`).toBe(false);
      }
    }
  });

  it("nothing in the lane touches the dead-letter journal or the pending overlay", () => {
    for (const [name, source] of [...SKILL_MODULES, ["useMicrotaskWiring.ts", wiringSource]] as Array<
      [string, string]
    >) {
      const body = code(source);
      for (const banned of ["deadLetter", "requestQueueDepth", "coreStore"]) {
        expect(body.includes(banned), `${name} references ${banned}`).toBe(false);
      }
    }
  });
});

describe("the stamp is not hardcoded at the render site", () => {
  /** #273's acceptance names this exactly. The panel renders
   * `stampLabel(run)`, whose only source is the envelope — so no provider
   * or model name may appear in the panel's own source at all. */
  it("ItemDetailPanel.tsx contains no backend or model name", () => {
    const body = code(panelSource);
    for (const name of ["anthropic", "claude-", "opus", "sonnet", "haiku", "moonshot"]) {
      expect(body.toLowerCase().includes(name), `the panel names ${name}`).toBe(false);
    }
  });

  /** #274 deleted `skills/models.ts` and the panel's own model select along
   * with it — which backend/model answers is an app-level preference now
   * (`backend-registry.ts`, chosen in Settings), never a per-item control. */
  it("the panel offers no model select of its own", () => {
    expect(code(panelSource)).not.toContain("CLOUD_RUNNER_MODELS");
    expect(code(panelSource)).not.toMatch(/label="Model"/);
  });
});
