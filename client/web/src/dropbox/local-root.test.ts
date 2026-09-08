import { describe, expect, it } from "vitest";
import { readDropboxLocalRoot, writeDropboxLocalRoot, type StorageLike } from "./local-root";

function memory(): StorageLike & { map: Map<string, string> } {
  const map = new Map<string, string>();
  return {
    map,
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, value) => void map.set(key, value),
    removeItem: (key) => void map.delete(key),
  };
}

describe("the device-local Dropbox root", () => {
  it("reads null with no storage, nothing stored, or a blank value", () => {
    expect(readDropboxLocalRoot(undefined)).toBe(null);
    const storage = memory();
    expect(readDropboxLocalRoot(storage)).toBe(null);
    storage.setItem("hb.dropbox-local-root", "   ");
    expect(readDropboxLocalRoot(storage)).toBe(null);
  });

  it("round-trips a trimmed value and clears on blank", () => {
    const storage = memory();
    writeDropboxLocalRoot(storage, "  C:\\Dropbox  ");
    expect(readDropboxLocalRoot(storage)).toBe("C:\\Dropbox");
    writeDropboxLocalRoot(storage, "");
    expect(storage.map.size).toBe(0);
  });
});
