import type { WorkerResponse } from "../store/protocol";

// The worker's half of the handshake, kept free of the wasm import so vitest
// (node) can exercise it. Posts ready with the core's api version — or error
// if probing the core throws — unprompted, to whichever `post` callback the
// caller supplies. Under ADR-0010 (#126) that caller is `PortRegistry.connect`
// (ports.ts), which calls this once per connecting `MessagePort`, so a view
// opened after the core is already running still gets its own handshake.
// See protocol.ts for why this is push-only.
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
