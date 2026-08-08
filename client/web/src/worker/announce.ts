import type { WorkerResponse } from "../store/protocol";

// The worker's half of the handshake, kept free of the wasm import so vitest
// (node) can exercise it. Posts ready with the core's api version — or error
// if probing the core throws — exactly once, unprompted. See protocol.ts for
// why this is push-only.
export function announceReady(
  post: (response: WorkerResponse) => void,
  coreApiVersion: () => number,
): void {
  try {
    post({ type: "ready", apiVersion: coreApiVersion() });
  } catch (err) {
    post({
      type: "error",
      message: err instanceof Error ? err.message : String(err),
    });
  }
}
