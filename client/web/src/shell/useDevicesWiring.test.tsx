// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { createCoreStore, type CoreStatus } from "../store/store";
import { attachWorkerClient, type WorkerLike } from "../store/worker-client";
import { act, renderHook } from "../test/component";
import { useDevicesWiring } from "./useDevicesWiring";

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  const worker = { onmessage: null, postMessage: vi.fn() } as WorkerLike & {
    postMessage: ReturnType<typeof vi.fn>;
  };
  attachWorkerClient(worker, createCoreStore());
  return worker;
}

function types(worker: ReturnType<typeof fakeWorker>): string[] {
  return worker.postMessage.mock.calls.map(([message]) => (message as { type: string }).type);
}

function mount(
  worker: WorkerLike,
  props: { status: CoreStatus; showing: boolean; hasToken: boolean },
  onSignedOut: () => Promise<void> = vi.fn().mockResolvedValue(undefined),
) {
  return renderHook(
    ({ status, showing, hasToken }) => useDevicesWiring(worker, status, showing, hasToken, onSignedOut, () => 3_000),
    { initialProps: props },
  );
}

async function revokeAnswering(
  worker: ReturnType<typeof fakeWorker>,
  revoke: (id: string) => Promise<string | null>,
  id: string,
  kind: string,
): Promise<string | null> {
  let result: string | null = "unsettled";
  await act(async () => {
    const pending = revoke(id);
    const sent = worker.postMessage.mock.calls.at(-1)![0] as { seed: string };
    worker.onmessage?.({ data: { type: "revokeDeviceResult", seed: sent.seed, id, kind, error: null } } as MessageEvent);
    result = await pending;
  });
  return result;
}

describe("useDevicesWiring", () => {
  it("reads the list only while Settings is showing a ready core", () => {
    const worker = fakeWorker();
    const { rerender } = mount(worker, { status: "loading", showing: true, hasToken: true });
    expect(types(worker)).toEqual([]);
    rerender({ status: "ready", showing: false, hasToken: true });
    expect(types(worker)).toEqual([]);
    rerender({ status: "ready", showing: true, hasToken: true });
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getDevices", nowMs: 3_000 });
  });

  it("reads it again when this device gains or loses its token", () => {
    const worker = fakeWorker();
    const { rerender } = mount(worker, { status: "ready", showing: true, hasToken: false });
    rerender({ status: "ready", showing: true, hasToken: true });
    expect(types(worker)).toEqual(["getDevices", "getDevices"]);
  });

  it("signing this device out deletes the stored token", async () => {
    const worker = fakeWorker();
    const onSignedOut = vi.fn().mockResolvedValue(undefined);
    const { result } = mount(worker, { status: "ready", showing: true, hasToken: true }, onSignedOut);
    expect(await revokeAnswering(worker, result.current.handleRevokeDevice, "device-web", "signed_out")).toBeNull();
    expect(onSignedOut).toHaveBeenCalledTimes(1);
  });

  it("signing another device out touches this one's token not at all", async () => {
    const worker = fakeWorker();
    const onSignedOut = vi.fn();
    const { result } = mount(worker, { status: "ready", showing: true, hasToken: true }, onSignedOut);
    expect(await revokeAnswering(worker, result.current.handleRevokeDevice, "device-android", "revoked")).toBeNull();
    expect(onSignedOut).not.toHaveBeenCalled();
  });

  it("words a refusal, and re-reads a list that named a device already gone", async () => {
    const worker = fakeWorker();
    const { result } = mount(worker, { status: "ready", showing: true, hasToken: true });
    expect(await revokeAnswering(worker, result.current.handleRevokeDevice, "runner", "not_revocable")).toBe(
      "This device cannot be signed out from here.",
    );
    worker.postMessage.mockClear();
    expect(await revokeAnswering(worker, result.current.handleRevokeDevice, "device-x", "not_found")).toBe(
      "That device is no longer signed in.",
    );
    expect(types(worker)).toEqual(["revokeDevice", "getDevices"]);
  });
});
