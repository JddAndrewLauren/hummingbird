import { describe, expect, it } from "vitest";
import { contextAddProblem, contextEditError, removalCopy, sameContext } from "./contexts";

const entries = [
  { name: "@home", itemCount: 0 },
  { name: "@errands", itemCount: 3 },
];

describe("contexts — the add check", () => {
  it("refuses a blank draft, the reserved label, and a duplicate under the core's rule", () => {
    expect(contextAddProblem("   ", entries)).toBe("Type a context to add.");
    expect(contextAddProblem("No Context", entries)).toMatch(/reserved/);
    expect(contextAddProblem("@Errands", entries)).toMatch(/already in the list/);
    expect(contextAddProblem("errands ", entries)).toMatch(/already in the list/);
  });

  it("accepts a new name, with or without the @", () => {
    expect(contextAddProblem(" @calls ", entries)).toBeNull();
    expect(contextAddProblem("calls", entries)).toBeNull();
  });

  it("matches the way the ranker does", () => {
    expect(sameContext("@errands", "Errands")).toBe(true);
    expect(sameContext("@errands", "@errand")).toBe(false);
  });
});

describe("contexts — the removal copy", () => {
  it("names the count it will clear, singular and plural, or nothing at all", () => {
    expect(removalCopy({ name: "@home", itemCount: 0 })).toBe("Remove @home");
    expect(removalCopy({ name: "@home", itemCount: 1 })).toBe("Remove @home — clears it from 1 item");
    expect(removalCopy({ name: "@errands", itemCount: 3 })).toBe(
      "Remove @errands — clears it from 3 items",
    );
  });
});

describe("contexts — the edit error", () => {
  const failed = {
    seed: "s",
    name: "@calls",
    edit: "add" as const,
    kind: "failed" as const,
    error: "disk full",
    cleared: null,
  };

  it("words a failure on its own name only", () => {
    expect(contextEditError(failed, "@calls")).toBe("disk full");
    expect(contextEditError(failed, "@home")).toBeNull();
    expect(contextEditError({ ...failed, kind: "ok" }, "@calls")).toBeNull();
    expect(contextEditError(null, "@calls")).toBeNull();
  });

  it("has a sentence for each refusal", () => {
    expect(contextEditError({ ...failed, kind: "busy" }, "@calls")).toMatch(/busy/);
    expect(contextEditError({ ...failed, kind: "unknown" }, "@calls")).toMatch(/isn't in the list/);
    expect(contextEditError({ ...failed, kind: "invalid", error: null }, "@calls")).toMatch(
      /can't be added/,
    );
  });
});
