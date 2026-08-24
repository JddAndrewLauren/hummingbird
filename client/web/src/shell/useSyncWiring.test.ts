// @vitest-environment jsdom

import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useSyncWiring } from "./useSyncWiring";
import { createCoreStore } from "../store/store";
import { attachWorkerClient, type WorkerLike } from "../store/worker-client";
import * as diagnosticsDownload from "./diagnostics-download";

// Review round 1 of PR #736: "useSyncWiring's pending-ref round trip [is]
// untested." The download itself only fires once a REQUESTED export
// actually arrives — `handleDownloadMirror`'s own doc calls this out as
// the whole reason the ref exists (a broadcast the view never asked for
// must not trigger a spontaneous download). This proves both halves: a
// requested export downloads, and an unrequested one (any other view's
// export, or a stray late arrival) does not.

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return {
    onmessage: null,
    postMessage: vi.fn(),
  };
}

/** Delivers a `diagnosticsExport` broadcast to whatever handler is
 * currently registered — `attachWorkerClient`'s own `onmessage` wiring is
 * the real production path to that registration
 * (`worker-client.ts`'s single `diagnosticsExportHandler` slot), so this
 * reaches the identical handler `useSyncWiring`'s effect installed rather
 * than calling anything internal directly. */
function deliverDiagnosticsExport(worker: WorkerLike, events: unknown[], droppedCount: number) {
  const store = createCoreStore();
  attachWorkerClient(worker, store);
  worker.onmessage?.({ data: { type: "diagnosticsExport", events, droppedCount } } as MessageEvent);
}

describe("useSyncWiring — #707's diagnostics download pending-ref round trip", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("downloads the export once requested and it arrives", () => {
    const downloadSpy = vi.spyOn(diagnosticsDownload, "downloadDiagnosticsExport").mockImplementation(() => {});
    const worker = fakeWorker();
    const { result } = renderHook(() => useSyncWiring(worker, "ready"));

    result.current.handleDownloadDiagnostics();
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getDiagnostics" });

    deliverDiagnosticsExport(worker, [{ seq: 1 }], 2);

    expect(downloadSpy).toHaveBeenCalledTimes(1);
    expect(downloadSpy).toHaveBeenCalledWith([{ seq: 1 }], 2, expect.any(Number));
  });

  it("does not download an export that arrives without having been requested", () => {
    const downloadSpy = vi.spyOn(diagnosticsDownload, "downloadDiagnosticsExport").mockImplementation(() => {});
    const worker = fakeWorker();
    renderHook(() => useSyncWiring(worker, "ready"));

    // No `handleDownloadDiagnostics()` call — this simulates another view's
    // own requested export, or `PortRegistry`'s broadcast reaching a view
    // that never asked, landing here anyway.
    deliverDiagnosticsExport(worker, [{ seq: 1 }], 0);

    expect(downloadSpy).not.toHaveBeenCalled();
  });

  it("only downloads once per request — a second, unrequested arrival after a fulfilled request does not re-download", () => {
    const downloadSpy = vi.spyOn(diagnosticsDownload, "downloadDiagnosticsExport").mockImplementation(() => {});
    const worker = fakeWorker();
    const { result } = renderHook(() => useSyncWiring(worker, "ready"));

    result.current.handleDownloadDiagnostics();
    deliverDiagnosticsExport(worker, [{ seq: 1 }], 0);
    expect(downloadSpy).toHaveBeenCalledTimes(1);

    // A second broadcast, with no second request in between.
    deliverDiagnosticsExport(worker, [{ seq: 2 }], 0);
    expect(downloadSpy).toHaveBeenCalledTimes(1);
  });

  it("handleClearDiagnostics posts clearDiagnostics and triggers no download", () => {
    const downloadSpy = vi.spyOn(diagnosticsDownload, "downloadDiagnosticsExport").mockImplementation(() => {});
    const worker = fakeWorker();
    const { result } = renderHook(() => useSyncWiring(worker, "ready"));

    result.current.handleClearDiagnostics();

    expect(worker.postMessage).toHaveBeenCalledWith({ type: "clearDiagnostics" });
    expect(downloadSpy).not.toHaveBeenCalled();
  });
});
