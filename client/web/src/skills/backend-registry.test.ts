import { describe, expect, it } from "vitest";
import { AUTO_SELECTION, BACKEND_REGISTRY, fallbackEntry, findBackendEntry } from "./backend-registry";

describe("backend-registry", () => {
  it("registers exactly the cloud runner in this slice", () => {
    expect(BACKEND_REGISTRY).toHaveLength(1);
    expect(BACKEND_REGISTRY[0]?.id).toBe("cloud");
  });

  it("AUTO_SELECTION is never a registered entry's id", () => {
    expect(BACKEND_REGISTRY.some((entry) => entry.id === AUTO_SELECTION)).toBe(false);
  });

  it("findBackendEntry finds a registered entry by id", () => {
    expect(findBackendEntry(BACKEND_REGISTRY, "cloud")?.label).toBe("Cloud runner");
  });

  it("findBackendEntry returns null for an unknown id", () => {
    expect(findBackendEntry(BACKEND_REGISTRY, "nope")).toBeNull();
  });

  it("fallbackEntry finds the next entry that is not the dead one", () => {
    const registry = [
      { id: "a", label: "A", model: null, endpoint: "/a", connectTimeoutMs: 1 },
      { id: "b", label: "B", model: null, endpoint: "/b", connectTimeoutMs: 1 },
    ];
    expect(fallbackEntry(registry, "a")?.id).toBe("b");
  });

  it("fallbackEntry is null when the dead entry is the only one", () => {
    expect(fallbackEntry(BACKEND_REGISTRY, "cloud")).toBeNull();
  });
});
