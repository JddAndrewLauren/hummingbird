import { useEffect } from "react";
import { revokeFailureMessage } from "../screens/devices";
import type { CoreStatus } from "../store/store";
import { requestDevices, revokeDevice, type WorkerLike } from "../store/worker-client";

// Signed-in devices on the web (#840, ADR-0040 decisions 6-7): the list is
// read whenever Settings is showing — and again whenever this device gains
// or loses its token, since that is what decides whether there is a list at
// all — and a sign-out goes through `Core::revoke_device`. Signing THIS
// device out leaves the core with no credential and the queue intact; the
// token wiring's `handleSignedOut` then deletes the stored copy.

export interface DevicesWiring {
  /** Signs one device out. Resolves to `null` once it is done, or the
   * sentence saying why it is not. */
  handleRevokeDevice: (id: string) => Promise<string | null>;
}

export function useDevicesWiring(
  worker: WorkerLike,
  status: CoreStatus,
  showing: boolean,
  hasToken: boolean,
  onSignedOut: () => Promise<void>,
  now: () => number = Date.now,
): DevicesWiring {
  useEffect(() => {
    if (status === "ready" && showing) {
      requestDevices(worker, now());
    }
    // `now` is a clock, not an input.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [worker, status, showing, hasToken]);

  async function handleRevokeDevice(id: string): Promise<string | null> {
    const answer = await revokeDevice(worker, id, now());
    switch (answer.kind) {
      case "revoked":
        return null;
      case "signed_out":
        await onSignedOut();
        return null;
      case "not_found":
        // Someone else already signed it out: the list shown is stale.
        requestDevices(worker, now());
        return revokeFailureMessage(answer.kind, answer.error);
      default:
        return revokeFailureMessage(answer.kind, answer.error);
    }
  }

  return { handleRevokeDevice };
}
