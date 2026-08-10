import { describe, expect, it } from "vitest";
import { isCaptureHotkey } from "./capture-hotkey";

function keyEvent(overrides: Partial<Parameters<typeof isCaptureHotkey>[0]> = {}) {
  return {
    key: "c",
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    targetIsEditable: false,
    ...overrides,
  };
}

describe("isCaptureHotkey", () => {
  it("matches a bare 'c' with no modifier and no editable target focused", () => {
    expect(isCaptureHotkey(keyEvent())).toBe(true);
  });

  it("matches uppercase 'C' the same as lowercase (a person's shift state should not matter)", () => {
    expect(isCaptureHotkey(keyEvent({ key: "C" }))).toBe(true);
  });

  it("never fires while typing in an editable field", () => {
    expect(isCaptureHotkey(keyEvent({ targetIsEditable: true }))).toBe(false);
  });

  it("never fires with ctrl/cmd/alt held — those are the browser's own shortcuts", () => {
    expect(isCaptureHotkey(keyEvent({ ctrlKey: true }))).toBe(false);
    expect(isCaptureHotkey(keyEvent({ metaKey: true }))).toBe(false);
    expect(isCaptureHotkey(keyEvent({ altKey: true }))).toBe(false);
  });

  it("does not match any other key", () => {
    expect(isCaptureHotkey(keyEvent({ key: "v" }))).toBe(false);
    expect(isCaptureHotkey(keyEvent({ key: "Enter" }))).toBe(false);
  });
});
