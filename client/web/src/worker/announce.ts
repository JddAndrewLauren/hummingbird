import type { WorkerResponse } from "../store/protocol";

// The worker's half of the handshake, kept free of the wasm import so vitest
// (node) can exercise it. Posts ready with the core's api version — or error
// if probing the core throws — unprompted, to whichever `post` callback the
// caller supplies. Under ADR-0010 (#126) that caller is `PortRegistry.connect`
// (ports.ts), which calls this once per connecting `MessagePort`, so a view
// opened after the core is already running still gets its own handshake.
// See protocol.ts for why this is push-only.
//
// `identity` is issue #172's ADR-0010 probe, and the handshake is its
// carrier precisely because `PortRegistry` posts one per connecting port: a
// PWA standalone window joining an already-running core still gets its own,
// with no new request direction and no `LATEST_STATE_TYPES` entry. Two views
// reporting the same `coreId` prove they share one core; two different ones
// refute ADR-0010's central assumption.
export interface CoreIdentity {
  /** The core instance this handshake came from — minted once per
   * `SharedWorker` global scope. */
  coreId: string;
  /** Which connect this was, counted by the registry: 1 for the view that
   * started the core, 2 for the next, and so on. */
  viewOrdinal: number;
}

export function announceReady(
  post: (response: WorkerResponse) => void,
  coreApiVersion: () => number,
  identity: CoreIdentity,
): void {
  try {
    post({
      type: "ready",
      apiVersion: coreApiVersion(),
      coreId: identity.coreId,
      viewOrdinal: identity.viewOrdinal,
    });
  } catch (err) {
    post({
      type: "error",
      message: err instanceof Error ? err.message : String(err),
    });
  }
}
