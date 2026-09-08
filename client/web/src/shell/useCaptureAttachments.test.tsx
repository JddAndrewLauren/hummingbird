// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { renderHook } from "../test/component";
import type { TaskCaptureResult, TaskFileLinkResult, TaskTriageResult } from "../store/store";
import type { FileLinksWiring } from "./useFileLinksWiring";
import {
  BOTH_ATTACH_FAILURE,
  FILE_ATTACH_FAILURE,
  NOTE_ATTACH_FAILURE,
  hasAttachments,
  useCaptureAttachments,
} from "./useCaptureAttachments";

/** The three broadcast slots the hook reads, as one render's worth of props —
 * every one of them is shared by every connected view, which is the whole
 * reason the hook recognises its own writes by seed. */
interface Slots {
  lastCapture: TaskCaptureResult | null;
  lastTriage: TaskTriageResult | null;
  lastFileLinkWrite: TaskFileLinkResult | null;
}

const NO_SLOTS: Slots = { lastCapture: null, lastTriage: null, lastFileLinkWrite: null };

function ok(seed: string, id: string | null = "item-9"): TaskCaptureResult {
  return { seed, kind: "ok", id, error: null };
}

function mount() {
  const triage = vi.fn(() => "triage-seed");
  const createFileLink = vi.fn(() => "file-seed");
  const fileLinks: FileLinksWiring = {
    localRoot: null,
    createFileLink,
    removeFileLink: vi.fn(() => "remove-seed"),
  };
  const view = renderHook(
    (slots: Slots) =>
      useCaptureAttachments(
        triage,
        fileLinks,
        slots.lastCapture,
        slots.lastTriage,
        slots.lastFileLinkWrite,
      ),
    { initialProps: NO_SLOTS },
  );
  return { triage, createFileLink, ...view };
}

describe("hasAttachments", () => {
  it("is false only when a capture asked for neither", () => {
    expect(hasAttachments({ vaultPath: null, filePath: null })).toBe(false);
    expect(hasAttachments({ vaultPath: "Hummingbird/x.md", filePath: null })).toBe(true);
    expect(hasAttachments({ vaultPath: null, filePath: "a/b.pdf" })).toBe(true);
  });
});

describe("useCaptureAttachments", () => {
  it("writes both attachments against the id the capture minted", () => {
    const { triage, createFileLink, result, rerender } = mount();

    result.current.remember("cap-1", { vaultPath: "Hummingbird/Knee.md", filePath: "Finance/x.pdf" });
    rerender({ ...NO_SLOTS, lastCapture: ok("cap-1") });

    // `destination: null` (#122): attaching a note moves the item through no
    // stage, exactly as the item panel's own note save does.
    expect(triage).toHaveBeenCalledWith("item-9", null, { vaultPath: "Hummingbird/Knee.md" });
    expect(createFileLink).toHaveBeenCalledWith("item-9", "Finance/x.pdf");
  });

  it("writes only the half that was asked for", () => {
    const { triage, createFileLink, result, rerender } = mount();

    result.current.remember("cap-1", { vaultPath: null, filePath: "Finance/x.pdf" });
    rerender({ ...NO_SLOTS, lastCapture: ok("cap-1") });

    expect(triage).not.toHaveBeenCalled();
    expect(createFileLink).toHaveBeenCalledTimes(1);
  });

  /** `lastCapture` is a broadcast rather than a reply, so the same result can
   * arrive again on a later render for reasons that have nothing to do with
   * this hook. The entry is dropped as it fires, which is what makes the
   * second arrival inert. */
  it("attaches once, however often the same result is re-broadcast", () => {
    const { triage, result, rerender } = mount();

    result.current.remember("cap-1", { vaultPath: "Hummingbird/Knee.md", filePath: null });
    const landed = ok("cap-1");
    rerender({ ...NO_SLOTS, lastCapture: landed });
    rerender({ ...NO_SLOTS, lastCapture: { ...landed } });

    expect(triage).toHaveBeenCalledTimes(1);
  });

  it("ignores a capture some other view sent", () => {
    const { triage, createFileLink, result, rerender } = mount();

    result.current.remember("cap-1", { vaultPath: "Hummingbird/Knee.md", filePath: "a/b.pdf" });
    rerender({ ...NO_SLOTS, lastCapture: ok("someone-else") });

    expect(triage).not.toHaveBeenCalled();
    expect(createFileLink).not.toHaveBeenCalled();
  });

  /** There is no item to attach to, so both are dropped with it. The capture
   * box has kept what was typed for the retry (#222). */
  it.each(["failed", "busy"] as const)("drops the attachments when the capture came back %s", (kind) => {
    const { triage, createFileLink, result, rerender } = mount();

    result.current.remember("cap-1", { vaultPath: "Hummingbird/Knee.md", filePath: "a/b.pdf" });
    rerender({ ...NO_SLOTS, lastCapture: { seed: "cap-1", kind, id: null, error: null } });

    expect(triage).not.toHaveBeenCalled();
    expect(createFileLink).not.toHaveBeenCalled();
  });

  /** An `"ok"` with no id names no item, so there is nothing to write
   * against — and writing against a guess is worse than not writing. */
  it("attaches nothing to an ok that names no id", () => {
    const { triage, result, rerender } = mount();

    result.current.remember("cap-1", { vaultPath: "Hummingbird/Knee.md", filePath: null });
    rerender({ ...NO_SLOTS, lastCapture: ok("cap-1", null) });

    expect(triage).not.toHaveBeenCalled();
  });

  it("reports a follow-up note write that did not land", () => {
    const { result, rerender } = mount();

    result.current.remember("cap-1", { vaultPath: "Hummingbird/Knee.md", filePath: null });
    rerender({ ...NO_SLOTS, lastCapture: ok("cap-1") });
    expect(result.current.failure).toBeNull();

    // `lastCapture` still holds this capture when the follow-up result lands:
    // it is a slot holding the most recent capture, not an event that clears.
    rerender({
      ...NO_SLOTS,
      lastCapture: ok("cap-1"),
      lastTriage: { seed: "triage-seed", itemId: "item-9", kind: "failed", error: "nope" },
    });

    // The capture itself succeeded, so the sentence says so — a reader who
    // took this for a failed capture would go looking for an item that is
    // already there.
    expect(result.current.failure).toBe(NOTE_ATTACH_FAILURE);
  });

  it("reports a follow-up file write that did not land", () => {
    const { result, rerender } = mount();

    result.current.remember("cap-1", { vaultPath: null, filePath: "Finance/x.pdf" });
    rerender({ ...NO_SLOTS, lastCapture: ok("cap-1") });
    rerender({
      ...NO_SLOTS,
      lastCapture: ok("cap-1"),
      lastFileLinkWrite: { seed: "file-seed", itemId: "item-9", kind: "failed", error: "nope" },
    });

    expect(result.current.failure).toBe(FILE_ATTACH_FAILURE);
  });

  it("says nothing about a write that landed, or about another view's", () => {
    const { result, rerender } = mount();

    result.current.remember("cap-1", { vaultPath: "Hummingbird/Knee.md", filePath: null });
    rerender({ ...NO_SLOTS, lastCapture: ok("cap-1") });
    rerender({
      ...NO_SLOTS,
      lastCapture: ok("cap-1"),
      lastTriage: { seed: "triage-seed", itemId: "item-9", kind: "ok", error: null },
    });
    expect(result.current.failure).toBeNull();

    rerender({
      ...NO_SLOTS,
      lastCapture: ok("cap-1"),
      lastTriage: { seed: "some-panel-edit", itemId: "item-3", kind: "failed", error: "nope" },
    });
    expect(result.current.failure).toBeNull();
  });

  /** Both halves failing must not leave the operator told about one of them.
   * The second result to arrive widens the sentence rather than replacing
   * it. */
  it("names both halves when both follow-up writes fail", () => {
    const { result, rerender } = mount();

    result.current.remember("cap-1", { vaultPath: "Hummingbird/Knee.md", filePath: "a/b.pdf" });
    rerender({ ...NO_SLOTS, lastCapture: ok("cap-1") });
    rerender({
      ...NO_SLOTS,
      lastCapture: ok("cap-1"),
      lastTriage: { seed: "triage-seed", itemId: "item-9", kind: "failed", error: "nope" },
    });
    expect(result.current.failure).toBe(NOTE_ATTACH_FAILURE);

    rerender({
      ...NO_SLOTS,
      lastCapture: ok("cap-1"),
      lastFileLinkWrite: { seed: "file-seed", itemId: "item-9", kind: "failed", error: "nope" },
    });

    expect(result.current.failure).toBe(BOTH_ATTACH_FAILURE);
  });

  /** The sentence must not outlive the item it is about. A later capture —
   * one that asked for no attachment at all, so nothing calls `remember` —
   * retires it by arriving, rather than standing under a freshly emptied box
   * describing an item two captures ago. */
  it("retires a failure once a later capture lands, attachment or not", () => {
    const { result, rerender } = mount();

    result.current.remember("cap-1", { vaultPath: "Hummingbird/Knee.md", filePath: null });
    rerender({ ...NO_SLOTS, lastCapture: ok("cap-1") });
    rerender({
      ...NO_SLOTS,
      lastCapture: ok("cap-1"),
      lastTriage: { seed: "triage-seed", itemId: "item-9", kind: "failed", error: "nope" },
    });
    expect(result.current.failure).toBe(NOTE_ATTACH_FAILURE);

    // A plain capture: `App.tsx` never calls `remember` for one of these.
    rerender({ ...NO_SLOTS, lastCapture: ok("cap-2", "item-10") });

    expect(result.current.failure).toBeNull();
  });

  /** A failure belongs to the capture it happened to. The next capture that
   * asks for an attachment is the moment it stops being the most recent
   * thing that happened. */
  it("clears a standing failure when the next attachment is remembered", () => {
    const { result, rerender } = mount();

    result.current.remember("cap-1", { vaultPath: "Hummingbird/Knee.md", filePath: null });
    rerender({ ...NO_SLOTS, lastCapture: ok("cap-1") });
    rerender({
      ...NO_SLOTS,
      lastCapture: ok("cap-1"),
      lastTriage: { seed: "triage-seed", itemId: "item-9", kind: "failed", error: "nope" },
    });
    expect(result.current.failure).toBe(NOTE_ATTACH_FAILURE);

    result.current.remember("cap-2", { vaultPath: "Hummingbird/Other.md", filePath: null });
    rerender({ ...NO_SLOTS, lastCapture: ok("cap-1") });

    expect(result.current.failure).toBeNull();
  });
});
