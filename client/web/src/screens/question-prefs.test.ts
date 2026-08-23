import { describe, expect, it } from "vitest";
import {
  readExpandedQuestions,
  toggleExpandedQuestion,
  writeExpandedQuestions,
} from "./question-prefs";
import type { StorageLike } from "./storage";

const KEY = "hb.settings.questions-expanded";

function stub(seed: Record<string, string> = {}) {
  const store = new Map(Object.entries(seed));
  return {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
    read: (key: string) => store.get(key) ?? null,
    has: (key: string) => store.has(key),
  };
}

/** A storage that throws on every call — private-mode Safari, and a worker
 * context with none at all. A preference that cannot persist must still let
 * the screen render. */
const broken: StorageLike = {
  getItem() {
    throw new Error("denied");
  },
  setItem() {
    throw new Error("denied");
  },
  removeItem() {
    throw new Error("denied");
  },
};

describe("readExpandedQuestions", () => {
  it("reads no storage, no key and a broken storage all as 'nothing open'", () => {
    // Collapsed is the default, and the default is key absence — so all
    // three degrade to the same answer rather than to an error.
    expect(readExpandedQuestions(undefined).size).toBe(0);
    expect(readExpandedQuestions(stub()).size).toBe(0);
    expect(readExpandedQuestions(broken).size).toBe(0);
  });

  it("reads back the rows that were open", () => {
    const storage = stub({ [KEY]: JSON.stringify(["race", "waste"]) });
    expect([...readExpandedQuestions(storage)].sort()).toEqual(["race", "waste"]);
  });

  it("degrades anything it cannot read to 'nothing open'", () => {
    for (const stored of ["not json", '{"race":true}', "[1,2]", '["ok",3]', "null"]) {
      expect(readExpandedQuestions(stub({ [KEY]: stored })).size).toBe(0);
    }
  });
});

describe("writeExpandedQuestions", () => {
  it("stores the open set", () => {
    const storage = stub();
    writeExpandedQuestions(storage, new Set(["race"]));
    expect(JSON.parse(storage.read(KEY) ?? "null")).toEqual(["race"]);
  });

  it("removes the key rather than storing an empty list", () => {
    // The default is absence, so shutting the last row must leave no value
    // behind that a changed default would later have to reinterpret.
    const storage = stub({ [KEY]: JSON.stringify(["race"]) });
    writeExpandedQuestions(storage, new Set());
    expect(storage.has(KEY)).toBe(false);
  });

  it("survives a storage that refuses, and an absent one", () => {
    expect(() => writeExpandedQuestions(broken, new Set(["race"]))).not.toThrow();
    expect(() => writeExpandedQuestions(broken, new Set())).not.toThrow();
    expect(() => writeExpandedQuestions(undefined, new Set(["race"]))).not.toThrow();
  });

  it("round-trips through a real read", () => {
    const storage = stub();
    writeExpandedQuestions(storage, new Set(["race", "waste"]));
    expect([...readExpandedQuestions(storage)].sort()).toEqual(["race", "waste"]);
  });
});

describe("toggleExpandedQuestion", () => {
  it("opens a shut row and shuts an open one, leaving the rest alone", () => {
    const opened = toggleExpandedQuestion(new Set(["waste"]), "race");
    expect([...opened].sort()).toEqual(["race", "waste"]);
    expect([...toggleExpandedQuestion(opened, "waste")]).toEqual(["race"]);
  });

  it("returns a new set rather than mutating the one it was given", () => {
    const before = new Set(["waste"]);
    toggleExpandedQuestion(before, "race");
    expect([...before]).toEqual(["waste"]);
  });
});
