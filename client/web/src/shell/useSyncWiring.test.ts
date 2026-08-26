// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import { renderHook } from "../test/component";
import { useSyncWiring } from "./useSyncWiring";
import { createCoreStore } from "../store/store";
import { attachWorkerClient, type WorkerLike } from "../store/worker-client";
import * as diagnosticsDownload from "./diagnostics-download";
import * as mirrorDownload from "./mirror-download";

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

function types(worker: ReturnType<typeof fakeWorker>): string[] {
  return worker.postMessage.mock.calls.map(([message]) => (message as { type: string }).type);
}

// #191/#194/#707's remaining, previously-untested wiring: the ready-gated
// sync-status reads, the visibility/focus reporters, the manual-sync
// trigger, and the mirror-download pending-ref round trip
// (`handleDownloadDiagnostics`'s own sibling, per the hook's doc).
describe("useSyncWiring — the ready-gated reads and the view-level reporters", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("asks nothing while the core is still loading", () => {
    const worker = fakeWorker();
    renderHook(() => useSyncWiring(worker, "loading"));
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("requests the queue depth and dead letters, and reports this view's own visibility, once ready", () => {
    const worker = fakeWorker();
    renderHook(() => useSyncWiring(worker, "ready"));
    expect(types(worker)).toEqual(
      expect.arrayContaining(["getQueueDepth", "getDeadLetters", "setViewVisibility"]),
    );
  });

  it("does not re-request the queue depth or dead letters on a later ready render — that refresh moved into the worker (#191)", () => {
    const worker = fakeWorker();
    const { rerender } = renderHook(({ status }) => useSyncWiring(worker, status), {
      initialProps: { status: "ready" as const },
    });
    worker.postMessage.mockClear();
    rerender({ status: "ready" });
    expect(types(worker)).toEqual([]);
  });

  it("re-reports visibility on a visibilitychange event", () => {
    const worker = fakeWorker();
    renderHook(() => useSyncWiring(worker, "ready"));
    worker.postMessage.mockClear();

    document.dispatchEvent(new Event("visibilitychange"));

    expect(worker.postMessage).toHaveBeenCalledWith({ type: "setViewVisibility", hidden: document.hidden });
  });

  it("triggers a focus-driven sync cycle on a window focus event", () => {
    const worker = fakeWorker();
    renderHook(() => useSyncWiring(worker, "ready"));
    worker.postMessage.mockClear();

    window.dispatchEvent(new Event("focus"));

    expect(worker.postMessage).toHaveBeenCalledWith({ type: "syncFocusTrigger" });
  });

  it("handleManualSync posts the manual-sync trigger", () => {
    const worker = fakeWorker();
    const { result } = renderHook(() => useSyncWiring(worker, "ready"));
    worker.postMessage.mockClear();

    result.current.handleManualSync();

    expect(worker.postMessage).toHaveBeenCalledWith({ type: "manualSyncTrigger" });
  });

  it("handleDownloadMirror requests a snapshot and writes it to disk once it arrives", () => {
    const mirrorSpy = vi.spyOn(mirrorDownload, "downloadMirrorSnapshot").mockImplementation(() => {});
    const worker = fakeWorker();
    const { result } = renderHook(() => useSyncWiring(worker, "ready"));

    result.current.handleDownloadMirror();
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getMirrorSnapshot" });

    const store = createCoreStore();
    attachWorkerClient(worker, store);
    worker.onmessage?.({ data: { type: "mirrorSnapshot", mirror: { items: [] } } } as MessageEvent);

    expect(mirrorSpy).toHaveBeenCalledTimes(1);
    expect(mirrorSpy).toHaveBeenCalledWith({ items: [] }, expect.any(Number));
  });

  it("does not write a mirror to disk that arrives without having been requested", () => {
    const mirrorSpy = vi.spyOn(mirrorDownload, "downloadMirrorSnapshot").mockImplementation(() => {});
    const worker = fakeWorker();
    renderHook(() => useSyncWiring(worker, "ready"));

    const store = createCoreStore();
    attachWorkerClient(worker, store);
    // No `handleDownloadMirror()` call — the same "an unrequested broadcast
    // must not trigger a spontaneous download" shape the diagnostics-export
    // round trip above proves.
    worker.onmessage?.({ data: { type: "mirrorSnapshot", mirror: { items: [] } } } as MessageEvent);

    expect(mirrorSpy).not.toHaveBeenCalled();
  });
});
