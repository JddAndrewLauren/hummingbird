import { describe, expect, it } from "vitest";
import type { ConnectionResult } from "./connection";
import { resolveRedirectReturn } from "./redirect-return";

const SUCCESS: ConnectionResult = {
  connected: true,
  needsReconnect: false,
  expiresAtMs: 1_700_000_000_000,
  error: null,
};

function failure(error: string): ConnectionResult {
  return { connected: false, needsReconnect: false, expiresAtMs: null, error };
}

describe("resolveRedirectReturn", () => {
  it("keeps a connected device connected when the return failed", () => {
    // The reported bug, in its first form: a connected phone user taps
    // Reconnect, cancels at Google, and comes back to
    // `#error=access_denied&state=…`. Applying that answer as it stands writes
    // `connected: false`, which drops the persisted opt-in, the last-good
    // snapshot and the Reconnect affordance itself — one cancelled consent
    // costing the reader their offline context.
    expect(resolveRedirectReturn(true, failure("access_denied"))).toEqual({
      connected: true,
      needsReconnect: true,
      expiresAtMs: null,
      error: "access_denied",
    });
  });

  it("survives a fragment nobody asked for", () => {
    // The second and worse form, because it needs no gesture from the reader
    // at all: any link to `https://hb.twinion.net/#error=x&state=y` parses (in
    // `google/redirect-flow.ts`) to a `state_mismatch` error, which is a
    // failed interactive attempt as far as everything downstream can tell. A
    // link that silently un-connects someone's calendar is a state change with
    // no interaction beyond opening it.
    expect(resolveRedirectReturn(true, failure("state_mismatch")).connected).toBe(true);
  });

  it("reports the failure even while keeping the connection", () => {
    // Keeping the connection is about `connected`/`needsReconnect` ONLY. A
    // reconnect that failed is exactly when the reader needs telling, and an
    // earlier version of this rule in the popup path swallowed the error along
    // with the state change — a press that produced nothing on screen.
    expect(resolveRedirectReturn(true, failure("access_denied")).error).toBe("access_denied");
  });

  it("parks the rotation timer rather than rotating against a stale expiry", () => {
    // `expiresAtMs: null` is what stops the proactive rotation effect racing
    // the credential-needed recovery that `needsReconnect` has just armed.
    expect(resolveRedirectReturn(true, failure("access_denied")).expiresAtMs).toBeNull();
  });

  it("lets a first-time opt-in fail honestly", () => {
    // A device that was never connected has nothing to protect, and inventing
    // `connected: true` for it would put a Reconnect button and a promise of
    // context in front of someone who has neither.
    expect(resolveRedirectReturn(false, failure("access_denied"))).toEqual(
      failure("access_denied"),
    );
  });

  it("passes a success straight through, connected before or not", () => {
    expect(resolveRedirectReturn(false, SUCCESS)).toEqual(SUCCESS);
    expect(resolveRedirectReturn(true, SUCCESS)).toEqual(SUCCESS);
  });
});
