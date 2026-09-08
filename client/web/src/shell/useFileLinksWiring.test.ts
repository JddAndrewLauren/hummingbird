// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import type { WorkerLike } from "../store/worker-client";
import { fileLinkDTO, renderHook } from "../test/component";
import { mintFileLinkRemoveSeed, useFileLinksWiring } from "./useFileLinksWiring";

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("useFileLinksWiring", () => {
  it("createFileLink posts the message and returns the seed it minted", () => {
    vi.useFakeTimers();
    vi.setSystemTime(2_000);
    const worker = fakeWorker();
    const wiring = renderHook(() => useFileLinksWiring(worker, "C:\\Dropbox")).result.current;

    const seed = wiring.createFileLink("item-1", "Finance/2026/receipt.pdf");

    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "createFileLink",
      seed,
      itemId: "item-1",
      path: "Finance/2026/receipt.pdf",
      nowMs: 2_000,
    });
    expect(wiring.localRoot).toBe("C:\\Dropbox");
  });

  it("removeFileLink stamps removedAt, hands the row as base, and returns a deterministic seed", () => {
    vi.useFakeTimers();
    vi.setSystemTime(3_000);
    const worker = fakeWorker();
    const wiring = renderHook(() => useFileLinksWiring(worker, null)).result.current;
    const current = fileLinkDTO({ id: "file-link-1" });

    const seed = wiring.removeFileLink(current);

    expect(seed).toBe(mintFileLinkRemoveSeed("file-link-1", 3_000));
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "removeFileLink",
      seed,
      current,
      removedAt: 3_000,
      nowMs: 3_000,
    });
  });
});
