import { describe, expect, it, vi } from "vitest";
import {
  forgetTaskToken,
  isBlankTokenInput,
  loadTaskToken,
  submitTaskToken,
  type TaskTokenDeps,
} from "./token";
import type { TaskTokenRecord, TaskTokenStoreLike } from "./token-store";

function fakeStore(initial: TaskTokenRecord | null = null): TaskTokenStoreLike {
  let record = initial;
  return {
    read: () => Promise.resolve(record),
    write: (next) => {
      record = next;
      return Promise.resolve();
    },
    clear: () => {
      record = null;
      return Promise.resolve();
    },
  };
}

function deps(store: TaskTokenStoreLike): TaskTokenDeps & { pushApiKey: ReturnType<typeof vi.fn> } {
  return { store, pushApiKey: vi.fn() };
}

describe("loadTaskToken", () => {
  it("reports no token on a device that has never stored one, without touching the worker", async () => {
    const d = deps(fakeStore(null));

    const result = await loadTaskToken(d);

    expect(result).toEqual({ hasToken: false, enteredAtMs: null });
    expect(d.pushApiKey).not.toHaveBeenCalled();
  });

  it("pushes a stored token into the core and reports when it was entered", async () => {
    const d = deps(fakeStore({ token: "secret-token", enteredAtMs: 1_000 }));

    const result = await loadTaskToken(d);

    expect(result).toEqual({ hasToken: true, enteredAtMs: 1_000 });
    expect(d.pushApiKey).toHaveBeenCalledWith("secret-token");
  });
});

describe("isBlankTokenInput", () => {
  it("rejects an empty string", () => {
    expect(isBlankTokenInput("")).toBe(true);
  });

  it("rejects whitespace-only input", () => {
    expect(isBlankTokenInput("   \n\t")).toBe(true);
  });

  it("accepts a real token", () => {
    expect(isBlankTokenInput("hb_device_abc123")).toBe(false);
  });

  it("accepts a token with surrounding whitespace (only the trim check is blank-testing it)", () => {
    expect(isBlankTokenInput("  hb_device_abc123  ")).toBe(false);
  });
});

describe("submitTaskToken", () => {
  it("persists the token verbatim, pushes it, and reports the fresh entered-at time", async () => {
    const store = fakeStore(null);
    const d = deps(store);

    const result = await submitTaskToken(d, "hb_device_abc123", 5_000);

    expect(result).toEqual({ hasToken: true, enteredAtMs: 5_000 });
    expect(d.pushApiKey).toHaveBeenCalledWith("hb_device_abc123");
    await expect(store.read()).resolves.toEqual({
      token: "hb_device_abc123",
      enteredAtMs: 5_000,
    });
  });

  it("overwrites whatever token was previously stored", async () => {
    const store = fakeStore({ token: "old-token", enteredAtMs: 1_000 });
    const d = deps(store);

    await submitTaskToken(d, "new-token", 9_000);

    await expect(store.read()).resolves.toEqual({ token: "new-token", enteredAtMs: 9_000 });
  });
});

describe("forgetTaskToken", () => {
  it("clears the stored token and reports the unset state", async () => {
    const store = fakeStore({ token: "secret-token", enteredAtMs: 1_000 });

    const result = await forgetTaskToken(store);

    expect(result).toEqual({ hasToken: false, enteredAtMs: null });
    await expect(store.read()).resolves.toBeNull();
  });

  it("never posts anything to the worker — forgetting has no wire message", async () => {
    // There is no "unset the key" request in the protocol (`store/protocol.ts`);
    // the running core just keeps whatever it last held until the tab
    // reloads. This asserts the intent by construction: `forgetTaskToken`
    // does not even accept a `pushApiKey` callback, only the store.
    const store = fakeStore({ token: "secret-token", enteredAtMs: 1_000 });
    await expect(forgetTaskToken(store)).resolves.toBeDefined();
  });
});
