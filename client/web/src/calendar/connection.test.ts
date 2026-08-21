import { describe, expect, it, vi } from "vitest";
import type { TokenClient } from "./token-client";
import {
  connect,
  type ConnectionDeps,
  handleCredentialNeeded,
  initConnection,
  msUntilRotation,
  shouldKeepExistingConnection,
} from "./connection";

// `prompt` is accepted but ignored by every real fake's caller here: the
// current `TokenClient` (`authority-token-client.ts`) ignores it too, so
// this module's own behaviour never depends on which value was passed —
// only that `requestToken` was (or was not) called.
function fakeTokenClient(
  respond: () => Awaited<ReturnType<TokenClient["requestToken"]>>,
): TokenClient {
  return { requestToken: vi.fn(async () => respond()) };
}

function deps(tokenClient: TokenClient): ConnectionDeps & { pushToken: ReturnType<typeof vi.fn> } {
  return { tokenClient, pushToken: vi.fn() };
}

describe("initConnection", () => {
  // `error: null` here is the assertion, not boilerplate: a device that was
  // never opted in has not FAILED, and Settings' error message is gated on
  // this field being non-null. Reporting a code here would put a Google
  // error on screen for someone who never pressed Connect.
  it("does nothing and stays disconnected on a never-opted-in device", async () => {
    const tokenClient = fakeTokenClient(() => ({ error: "should not be called" }));
    const d = deps(tokenClient);

    const result = await initConnection(d, false);

    expect(result).toEqual({ connected: false, needsReconnect: false, expiresAtMs: null, error: null });
    expect(d.pushToken).not.toHaveBeenCalled();
    expect(tokenClient.requestToken).not.toHaveBeenCalled();
  });

  it("silently re-mints and pushes the token for a previously-connected device", async () => {
    const tokenClient = fakeTokenClient(() => ({ accessToken: "tok-1", expiresAtMs: 10_000 }));
    const d = deps(tokenClient);

    const result = await initConnection(d, true);

    expect(result).toEqual({ connected: true, needsReconnect: false, expiresAtMs: 10_000, error: null });
    expect(d.pushToken).toHaveBeenCalledWith("tok-1");
  });

  it("flags needsReconnect when the silent re-mint fails", async () => {
    const tokenClient = fakeTokenClient(() => ({ error: "authority_upstream" }));
    const d = deps(tokenClient);

    const result = await initConnection(d, true);

    expect(result).toEqual({ connected: true, needsReconnect: true, expiresAtMs: null, error: "authority_upstream" });
    expect(d.pushToken).not.toHaveBeenCalled();
  });
});

describe("connect", () => {
  it("requests interactive consent and pushes the resulting token", async () => {
    const tokenClient = fakeTokenClient(() => ({ accessToken: "tok-2", expiresAtMs: 20_000 }));
    const d = deps(tokenClient);

    const result = await connect(d);

    expect(result).toEqual({ connected: true, needsReconnect: false, expiresAtMs: 20_000, error: null });
    expect(d.pushToken).toHaveBeenCalledWith("tok-2");
  });

  it("stays disconnected (not stuck needing reconnect) if the attempt fails", async () => {
    const tokenClient = fakeTokenClient(() => ({ error: "authority_rejected_device_token" }));
    const d = deps(tokenClient);

    const result = await connect(d);

    expect(result).toEqual({
      connected: false,
      needsReconnect: false,
      expiresAtMs: null,
      error: "authority_rejected_device_token",
    });
    expect(d.pushToken).not.toHaveBeenCalled();
  });
});

describe("handleCredentialNeeded", () => {
  it("silently re-mints and pushes a fresh token, resolving the hold", async () => {
    const tokenClient = fakeTokenClient(() => ({ accessToken: "tok-3", expiresAtMs: 30_000 }));
    const d = deps(tokenClient);

    const result = await handleCredentialNeeded(d);

    expect(result).toEqual({ connected: true, needsReconnect: false, expiresAtMs: 30_000, error: null });
    expect(d.pushToken).toHaveBeenCalledWith("tok-3");
  });

  it("falls back to a re-connect affordance when the silent re-mint fails", async () => {
    const tokenClient = fakeTokenClient(() => ({ error: "authority_upstream" }));
    const d = deps(tokenClient);

    const result = await handleCredentialNeeded(d);

    expect(result).toEqual({ connected: true, needsReconnect: true, expiresAtMs: null, error: "authority_upstream" });
    expect(d.pushToken).not.toHaveBeenCalled();
  });
});

describe("shouldKeepExistingConnection", () => {
  const failed = { connected: false, needsReconnect: false, expiresAtMs: null, error: null };
  const succeeded = { connected: true, needsReconnect: false, expiresAtMs: 10_000, error: null };

  it("keeps the opt-in when a Reconnect fails", () => {
    // The device stays connected-but-needing-reconnect: its last-good tile
    // and the Reconnect button both survive a mint the authority refused.
    expect(shouldKeepExistingConnection(true, failed)).toBe(true);
  });

  it("lets a first-time Connect that fails end disconnected", () => {
    expect(shouldKeepExistingConnection(false, failed)).toBe(false);
  });

  it("never discards a successful result", () => {
    expect(shouldKeepExistingConnection(true, succeeded)).toBe(false);
    expect(shouldKeepExistingConnection(false, succeeded)).toBe(false);
  });
});

describe("msUntilRotation", () => {
  it("schedules rotation the margin before expiry", () => {
    expect(msUntilRotation(100_000, 0, 10_000)).toBe(90_000);
  });

  it("clamps to zero once inside the margin (or already expired)", () => {
    expect(msUntilRotation(100_000, 95_000, 10_000)).toBe(0);
    expect(msUntilRotation(100_000, 200_000, 10_000)).toBe(0);
  });

  it("defaults to a five-minute safety margin", () => {
    expect(msUntilRotation(10 * 60 * 1000, 0)).toBe(5 * 60 * 1000);
  });
});
