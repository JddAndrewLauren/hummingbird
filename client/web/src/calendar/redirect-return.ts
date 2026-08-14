// What a return from the OAuth redirect is allowed to do to a device that was
// already connected.
//
// **Why this is not just `shouldKeepExistingConnection`.** The popup path and
// the redirect path share the question — "may a failed interactive attempt
// un-connect this device?", answered `no` by `connection.ts` — but they cannot
// share the remedy, because they run at different moments in the page's life.
//
//   - The popup runs inside a live page. The store already holds the device's
//     real state (connected, its selection, a live `expiresAtMs`), so the
//     remedy is to touch nothing: `handleConnectClick` returns before writing.
//   - The redirect's answer arrives on the NEXT page load. Nothing has been
//     written yet — the store is at its initial, disconnected default and the
//     only surviving truth is the persisted opt-in flag. Touching nothing there
//     would leave a connected device rendering as a never-connected one, which
//     is the same wipe by another route.
//
// So the redirect's remedy is to write back, positively, the state a
// previously-connected device has when it holds no live token: connected, and
// needing a reconnect. That is byte-identical to what a failed silent re-mint
// produces (`connection.ts`'s `stillNeedsReconnect`), which is the point — the
// start effect's downstream is already written for exactly that shape, the
// last-good snapshot and the Reconnect affordance both stand, and the
// credential-needed effect picks the recovery up from there.
//
// The failure this closes is not only the sad path of a real attempt. Any
// fragment of the form `#error=…&state=…` on the origin root parses (via
// `google/redirect-flow.ts`) to `{kind:"error", error:"state_mismatch"}` — an
// attacker-supplied link needs no interaction beyond being opened — and before
// this it flowed straight into `writeConnected(localStorage, false)`. A link
// that silently drops someone's calendar opt-in is a state change nobody asked
// for.

import { type ConnectionResult, shouldKeepExistingConnection } from "./connection";

/** The `ConnectionResult` the start effect should actually apply for a return
 * from the redirect flow, given whether this device was connected before it
 * left (`persistence.ts`'s flag, read at the top of that effect).
 *
 * `error` is carried through untouched in both branches: keeping the
 * connection is about `connected`/`needsReconnect` only, and a failed
 * reconnect is precisely when the reader needs telling — the same division
 * `shouldKeepExistingConnection`'s own doc insists on. */
export function resolveRedirectReturn(
  wasConnected: boolean,
  result: ConnectionResult,
): ConnectionResult {
  if (!shouldKeepExistingConnection(wasConnected, result)) {
    return result;
  }
  return {
    connected: true,
    needsReconnect: true,
    // No token was pushed, so there is no expiry to rotate against. `null`
    // parks the rotation timer, which is right: `needsReconnect` is what drives
    // recovery from here, and a rotation scheduled off a stale expiry would
    // race it.
    expiresAtMs: null,
    error: result.error,
  };
}
