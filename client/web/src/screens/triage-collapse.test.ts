import { describe, expect, it } from "vitest";
import {
  readTriageCollapsed,
  writeTriageCollapsed,
  type StorageLike,
} from "./triage-collapse";

function fakeStorage(seed: Record<string, string> = {}): StorageLike & {
  entries: Record<string, string>;
} {
  const entries = { ...seed };
  return {
    entries,
    getItem: (key) => entries[key] ?? null,
    setItem: (key, value) => {
      entries[key] = value;
    },
    removeItem: (key) => {
      delete entries[key];
    },
  };
}

const KEY = "hb.now.triage-collapsed";

describe("triage-collapse", () => {
  it("defaults to expanded when nothing is stored", () => {
    expect(readTriageCollapsed(fakeStorage())).toBe(false);
  });

  it("round-trips a collapse", () => {
    const storage = fakeStorage();
    writeTriageCollapsed(storage, true);
    expect(storage.entries[KEY]).toBe("1");
    expect(readTriageCollapsed(storage)).toBe(true);
  });

  it("removes the key rather than storing the default, so it cannot rot", () => {
    const storage = fakeStorage({ [KEY]: "1" });
    writeTriageCollapsed(storage, false);
    expect(KEY in storage.entries).toBe(false);
    expect(readTriageCollapsed(storage)).toBe(false);
  });

  it("reads any other stored value as expanded", () => {
    // A newer build's shape, or a hand-edited key: the default rule is always
    // a correct answer, and a view preference is never worth an error.
    expect(readTriageCollapsed(fakeStorage({ [KEY]: "yes" }))).toBe(false);
  });

  it("survives an absent storage and a throwing one", () => {
    const throwing: StorageLike = {
      getItem: () => {
        throw new Error("nope");
      },
      setItem: () => {
        throw new Error("nope");
      },
      removeItem: () => {
        throw new Error("nope");
      },
    };
    expect(readTriageCollapsed(undefined)).toBe(false);
    expect(readTriageCollapsed(throwing)).toBe(false);
    expect(() => writeTriageCollapsed(undefined, true)).not.toThrow();
    expect(() => writeTriageCollapsed(throwing, true)).not.toThrow();
  });
});
