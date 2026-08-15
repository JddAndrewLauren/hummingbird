import { describe, expect, it } from "vitest";
import { isRecallHotkey } from "./recall-hotkey";

function keyEvent(overrides: Partial<Parameters<typeof isRecallHotkey>[0]> = {}) {
  return {
    key: "/",
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    targetIsEditable: false,
    isComposing: false,
    ...overrides,
  };
}

describe("isRecallHotkey", () => {
  it("matches a bare '/' with no modifier and no editable target focused", () => {
    expect(isRecallHotkey(keyEvent())).toBe(true);
  });

  it("never fires while typing in an editable field — '/' has to stay a character there", () => {
    expect(isRecallHotkey(keyEvent({ targetIsEditable: true }))).toBe(false);
  });

  it("never fires with ctrl/cmd/alt held — those are the browser's own shortcuts", () => {
    expect(isRecallHotkey(keyEvent({ ctrlKey: true }))).toBe(false);
    expect(isRecallHotkey(keyEvent({ metaKey: true }))).toBe(false);
    expect(isRecallHotkey(keyEvent({ altKey: true }))).toBe(false);
  });

  it("does not match any other key", () => {
    expect(isRecallHotkey(keyEvent({ key: "c" }))).toBe(false);
    expect(isRecallHotkey(keyEvent({ key: "Enter" }))).toBe(false);
    expect(isRecallHotkey(keyEvent({ key: "?" }))).toBe(false);
  });

  it("never fires mid IME composition — a composing '/' keydown is not a real '/'", () => {
    expect(isRecallHotkey(keyEvent({ isComposing: true }))).toBe(false);
  });
});
