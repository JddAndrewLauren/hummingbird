import { describe, expect, it } from "vitest";
import type { BackendEntry } from "./backend-registry";
import { readBackendSelection, writeBackendSelection } from "./backend-selection";

const CLOUD: BackendEntry = { id: "cloud", label: "Cloud runner", model: null, endpoint: "/a", connectTimeoutMs: 1 };
const REGISTRY = [CLOUD];

function fakeStorage(initial: Record<string, string> = {}) {
  const store = { ...initial };
  return {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, value: string) => {
      store[key] = value;
    },
    removeItem: (key: string) => {
      delete store[key];
    },
    dump: () => store,
  };
}

describe("backend-selection", () => {
  it("defaults to Auto when nothing is stored", () => {
    expect(readBackendSelection(fakeStorage(), REGISTRY)).toBe("auto");
  });

  it("round-trips a pinned selection", () => {
    const storage = fakeStorage();
    writeBackendSelection(storage, "cloud");
    expect(readBackendSelection(storage, REGISTRY)).toBe("cloud");
  });

  it("round-trips Auto explicitly written", () => {
    const storage = fakeStorage();
    writeBackendSelection(storage, "auto");
    expect(readBackendSelection(storage, REGISTRY)).toBe("auto");
  });

  it("a stored selection naming a retired backend degrades to Auto", () => {
    const storage = fakeStorage({ "hb.backend-selection": "retired-backend" });
    expect(readBackendSelection(storage, REGISTRY)).toBe("auto");
  });

  it("never writes through anything but the injected storage", () => {
    const storage = fakeStorage();
    writeBackendSelection(storage, "cloud");
    expect(storage.dump()).toEqual({ "hb.backend-selection": "cloud" });
  });
});
