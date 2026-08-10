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

function deps(
  store: TaskTokenStoreLike,
): TaskTokenDeps & { pushApiKey: ReturnType<typeof vi.fn>; clearApiKey: ReturnType<typeof vi.fn> } {
  return { store, pushApiKey: vi.fn(), clearApiKey: vi.fn() };
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

  it("reports the unset state (not an unhandled rejection) when the store read fails", async () => {
    const store: TaskTokenStoreLike = {
      read: () => Promise.reject(new Error("indexeddb blocked")),
      write: () => Promise.resolve(),
      clear: () => Promise.resolve(),
    };
    const d = deps(store);

    const result = await loadTaskToken(d);

    expect(result).toEqual({ hasToken: false, enteredAtMs: null });
    expect(d.pushApiKey).not.toHaveBeenCalled();
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
  it("persists the trimmed token, pushes it, and reports ok", async () => {
    const store = fakeStore(null);
    const d = deps(store);

    const outcome = await submitTaskToken(d, "hb_device_abc123", 5_000);

    expect(outcome).toBe("ok");
    expect(d.pushApiKey).toHaveBeenCalledWith("hb_device_abc123");
    await expect(store.read()).resolves.toEqual({
      token: "hb_device_abc123",
      enteredAtMs: 5_000,
    });
  });

  it("trims a pasted trailing newline or surrounding whitespace before storing or pushing", async () => {
    const store = fakeStore(null);
    const d = deps(store);

    await submitTaskToken(d, "  hb_device_abc123\n", 5_000);

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

  it("rejects a blank submission before touching storage or the worker", async () => {
    const store = fakeStore(null);
    const d = deps(store);

    const outcome = await submitTaskToken(d, "   ", 5_000);

    expect(outcome).toBe("blank");
    expect(d.pushApiKey).not.toHaveBeenCalled();
    await expect(store.read()).resolves.toBeNull();
  });

  it("reports storeError and never pushes the key when the store write fails", async () => {
    const store: TaskTokenStoreLike = {
      read: () => Promise.resolve(null),
      write: () => Promise.reject(new Error("indexeddb blocked")),
      clear: () => Promise.resolve(),
    };
    const d = deps(store);

    const outcome = await submitTaskToken(d, "hb_device_abc123", 5_000);

    expect(outcome).toBe("storeError");
    expect(d.pushApiKey).not.toHaveBeenCalled();
  });
});

describe("forgetTaskToken", () => {
  it("clears the stored token and reports the unset state", async () => {
    const store = fakeStore({ token: "secret-token", enteredAtMs: 1_000 });
    const d = deps(store);

    const result = await forgetTaskToken(d);

    expect(result).toEqual({ hasToken: false, enteredAtMs: null });
    await expect(store.read()).resolves.toBeNull();
  });

  it("also clears the live key the core is holding — not just local storage", async () => {
    // Round-1 review finding: #126's SharedWorker outlives any one tab, so
    // clearing IndexedDB alone would leave the running core still holding
    // (and using) the "forgotten" key. `forgetTaskToken` must call
    // `clearApiKey` every time.
    const store = fakeStore({ token: "secret-token", enteredAtMs: 1_000 });
    const d = deps(store);

    await forgetTaskToken(d);

    expect(d.clearApiKey).toHaveBeenCalledTimes(1);
    expect(d.pushApiKey).not.toHaveBeenCalled();
  });
});
