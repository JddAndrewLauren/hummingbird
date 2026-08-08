import { describe, expect, it, vi } from "vitest";
import type { TokenClient, TokenPrompt } from "../google/gis";
import {
  connect,
  type ConnectionDeps,
  handleCredentialNeeded,
  initConnection,
  msUntilRotation,
} from "./connection";

function fakeTokenClient(
  respond: (prompt: TokenPrompt) => Awaited<ReturnType<TokenClient["requestToken"]>>,
): TokenClient {
  return { requestToken: vi.fn(async (prompt) => respond(prompt)) };
}

function deps(tokenClient: TokenClient): ConnectionDeps & { pushToken: ReturnType<typeof vi.fn> } {
  return { tokenClient, pushToken: vi.fn() };
}

describe("initConnection", () => {
  it("does nothing and stays disconnected on a never-opted-in device", async () => {
    const tokenClient = fakeTokenClient(() => ({ error: "should not be called" }));
    const d = deps(tokenClient);

    const result = await initConnection(d, false);

    expect(result).toEqual({ connected: false, needsReconnect: false, expiresAtMs: null });
    expect(d.pushToken).not.toHaveBeenCalled();
    expect(tokenClient.requestToken).not.toHaveBeenCalled();
  });

  it("silently re-mints and pushes the token for a previously-connected device", async () => {
    const tokenClient = fakeTokenClient((prompt) => {
      expect(prompt).toBe("none");
      return { accessToken: "tok-1", expiresAtMs: 10_000 };
    });
    const d = deps(tokenClient);

    const result = await initConnection(d, true);

    expect(result).toEqual({ connected: true, needsReconnect: false, expiresAtMs: 10_000 });
    expect(d.pushToken).toHaveBeenCalledWith("tok-1");
  });

  it("flags needsReconnect when the silent re-mint fails", async () => {
    const tokenClient = fakeTokenClient(() => ({ error: "interaction_required" }));
    const d = deps(tokenClient);

    const result = await initConnection(d, true);

    expect(result).toEqual({ connected: true, needsReconnect: true, expiresAtMs: null });
    expect(d.pushToken).not.toHaveBeenCalled();
  });
});

describe("connect", () => {
  it("requests interactive consent and pushes the resulting token", async () => {
    const tokenClient = fakeTokenClient((prompt) => {
      expect(prompt).toBe("consent");
      return { accessToken: "tok-2", expiresAtMs: 20_000 };
    });
    const d = deps(tokenClient);

    const result = await connect(d);

    expect(result).toEqual({ connected: true, needsReconnect: false, expiresAtMs: 20_000 });
    expect(d.pushToken).toHaveBeenCalledWith("tok-2");
  });

  it("stays disconnected (not stuck needing reconnect) if the user declines consent", async () => {
    const tokenClient = fakeTokenClient(() => ({ error: "access_denied" }));
    const d = deps(tokenClient);

    const result = await connect(d);

    expect(result).toEqual({ connected: false, needsReconnect: false, expiresAtMs: null });
    expect(d.pushToken).not.toHaveBeenCalled();
  });
});

describe("handleCredentialNeeded", () => {
  it("silently re-mints and pushes a fresh token, resolving the hold", async () => {
    const tokenClient = fakeTokenClient((prompt) => {
      expect(prompt).toBe("none");
      return { accessToken: "tok-3", expiresAtMs: 30_000 };
    });
    const d = deps(tokenClient);

    const result = await handleCredentialNeeded(d);

    expect(result).toEqual({ connected: true, needsReconnect: false, expiresAtMs: 30_000 });
    expect(d.pushToken).toHaveBeenCalledWith("tok-3");
  });

  it("falls back to a re-connect affordance when the silent re-mint fails", async () => {
    const tokenClient = fakeTokenClient(() => ({ error: "interaction_required" }));
    const d = deps(tokenClient);

    const result = await handleCredentialNeeded(d);

    expect(result).toEqual({ connected: true, needsReconnect: true, expiresAtMs: null });
    expect(d.pushToken).not.toHaveBeenCalled();
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
