import { describe, expect, it } from "vitest";
import { closesItemDetail, isCaptureHotkey } from "./capture-hotkey";

function keyEvent(overrides: Partial<Parameters<typeof isCaptureHotkey>[0]> = {}) {
  return {
    key: "c",
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    targetIsEditable: false,
    isComposing: false,
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

  it("never fires mid IME composition — a composing 'c' keydown is not a real 'c'", () => {
    expect(isCaptureHotkey(keyEvent({ isComposing: true }))).toBe(false);
  });
});

describe("closesItemDetail", () => {
  const escape = {
    key: "Escape",
    isComposing: false,
    captureOpen: false,
    itemDetailOpen: true,
  };

  it("closes the open item detail", () => {
    expect(closesItemDetail(escape)).toBe(true);
  });

  it("does nothing when nothing is open to close", () => {
    expect(closesItemDetail({ ...escape, itemDetailOpen: false })).toBe(false);
  });

  it("yields to the capture popover, which sits over the panel", () => {
    // One Escape closes the shallowest open thing. Without this, dismissing
    // the popover would also take away the panel underneath it — something
    // the reader never asked to close.
    expect(closesItemDetail({ ...escape, captureOpen: true })).toBe(false);
  });

  it("leaves an IME composition's Escape to the composition", () => {
    expect(closesItemDetail({ ...escape, isComposing: true })).toBe(false);
  });

  it("ignores every other key", () => {
    for (const key of ["Enter", "c", "Esc", "escape", " "]) {
      expect(closesItemDetail({ ...escape, key })).toBe(false);
    }
  });
});
