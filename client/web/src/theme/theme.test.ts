import { describe, expect, it } from "vitest";
import type { StorageLike } from "../calendar/persistence";
import {
  readThemePreference,
  resolveTheme,
  toggledPreference,
  writeThemePreference,
} from "./theme";

function fakeStorage(seed: Record<string, string> = {}): StorageLike & {
  entries: Record<string, string>;
} {
  const entries: Record<string, string> = { ...seed };
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

describe("resolveTheme", () => {
  it("follows the system setting under the system preference", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });

  it("ignores the system setting under an explicit preference", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });
});

describe("toggledPreference", () => {
  it("always lands on an explicit preference", () => {
    expect(toggledPreference("light")).toBe("dark");
    expect(toggledPreference("dark")).toBe("light");
  });
});

describe("readThemePreference", () => {
  it("defaults to system when nothing is stored", () => {
    expect(readThemePreference(fakeStorage())).toBe("system");
  });

  it("reads a stored explicit preference", () => {
    expect(readThemePreference(fakeStorage({ "hb.theme": "dark" }))).toBe("dark");
  });

  it("falls back to system on a garbage value", () => {
    expect(readThemePreference(fakeStorage({ "hb.theme": "chartreuse" }))).toBe("system");
  });
});

describe("writeThemePreference", () => {
  it("persists an explicit preference", () => {
    const storage = fakeStorage();
    writeThemePreference(storage, "dark");
    expect(storage.entries["hb.theme"]).toBe("dark");
  });

  it("clears the key for system, so a later OS change is honoured", () => {
    const storage = fakeStorage({ "hb.theme": "light" });
    writeThemePreference(storage, "system");
    expect(storage.entries["hb.theme"]).toBeUndefined();
  });
});
