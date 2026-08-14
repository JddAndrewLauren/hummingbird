import { describe, expect, it } from "vitest";
import type { StepDTO } from "../store/protocol";
import { planReplacementLabel, wouldStrandPlan } from "./grill-review";

function step(overrides: Partial<StepDTO> = {}): StepDTO {
  return {
    id: "step-1",
    itemId: "item-1",
    body: "pack",
    done: false,
    position: 0,
    deletedAt: null,
    version: 1,
    ...overrides,
  };
}

describe("wouldStrandPlan", () => {
  it("is true for fog_remains with at least one live undone step", () => {
    expect(wouldStrandPlan("fog_remains", [step()])).toBe(true);
  });

  it("is false for fog_remains with no steps at all", () => {
    expect(wouldStrandPlan("fog_remains", [])).toBe(false);
  });

  it("is false for fog_remains when every step is done or deleted", () => {
    expect(
      wouldStrandPlan("fog_remains", [step({ done: true }), step({ id: "step-2", deletedAt: 5_000 })]),
    ).toBe(false);
  });

  it("is false for resolved, whatever the steps — resolved never demotes a live stage", () => {
    expect(wouldStrandPlan("resolved", [step()])).toBe(false);
  });
});

describe("planReplacementLabel", () => {
  it("names the live undone count, singular", () => {
    expect(planReplacementLabel([step()])).toBe("Also delete 1 unfinished step");
  });

  it("names the live undone count, plural", () => {
    expect(planReplacementLabel([step(), step({ id: "step-2" })])).toBe("Also delete 2 unfinished steps");
  });

  it("excludes done and deleted steps from the count", () => {
    expect(
      planReplacementLabel([step({ done: true }), step({ id: "step-2", deletedAt: 5_000 }), step({ id: "step-3" })]),
    ).toBe("Also delete 1 unfinished step");
  });
});
