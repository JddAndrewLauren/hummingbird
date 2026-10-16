// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { CoreStatus } from "../store/store";
import { attachWorkerClient, type WorkerLike } from "../store/worker-client";
import { createCoreStore } from "../store/store";
import { act, renderHook } from "../test/component";
import type { SignInReturn } from "../task/google-signin";
import { NONCE_KEY } from "../task/google-signin";
import type { TaskTokenSubmitOutcome } from "../task/token";
import { useSignInWiring, type SignInWiringDeps } from "./useSignInWiring";

// Mounted for `useTaskTokenWiring.test.tsx`'s reason: a hook nothing calls
// compiles clean and does nothing. The worker is a fake whose answers are
// delivered through the real `attachWorkerClient`, so the seed matching the
// hook relies on is the real one.

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  const worker = { onmessage: null, postMessage: vi.fn() } as WorkerLike & {
    postMessage: ReturnType<typeof vi.fn>;
  };
  attachWorkerClient(worker, createCoreStore());
  return worker;
}

function answer(worker: WorkerLike, data: Record<string, unknown>): void {
  worker.onmessage?.({ data } as MessageEvent);
}

function memoryStorage() {
  const data: Record<string, string> = {};
  return {
    data,
    getItem: (key: string) => data[key] ?? null,
    setItem: (key: string, value: string) => {
      data[key] = value;
    },
    removeItem: (key: string) => {
      delete data[key];
    },
  };
}

function deps(overrides: Partial<SignInWiringDeps> = {}): SignInWiringDeps {
  return {
    clientId: "client-1.apps.googleusercontent.com",
    storage: memoryStorage(),
    origin: "http://127.0.0.1:5181",
    navigate: vi.fn(),
    userAgent:
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
    now: () => 4_000,
    ...overrides,
  };
}

function mount(
  worker: WorkerLike,
  status: CoreStatus,
  signInReturn: SignInReturn | null,
  submitToken: (input: string) => Promise<TaskTokenSubmitOutcome>,
  wiringDeps: SignInWiringDeps = deps(),
) {
  return renderHook(
    ({ coreStatus }) => useSignInWiring(worker, coreStatus, signInReturn, submitToken, wiringDeps),
    { initialProps: { coreStatus: status } },
  );
}

describe("useSignInWiring", () => {
  it("rests idle with no return to read", () => {
    const { result } = mount(fakeWorker(), "ready", null, vi.fn());
    expect(result.current.signIn).toEqual({ kind: "idle" });
  });

  it("begins a sign-in by navigating to Google with a kept nonce", () => {
    const wiringDeps = deps();
    const { result } = mount(fakeWorker(), "ready", null, vi.fn(), wiringDeps);
    act(() => result.current.beginSignIn?.());
    const url = new URL((wiringDeps.navigate as ReturnType<typeof vi.fn>).mock.calls[0][0] as string);
    expect(url.hostname).toBe("accounts.google.com");
    expect(url.searchParams.get("redirect_uri")).toBe("http://127.0.0.1:5181/signin");
    expect(url.searchParams.get("nonce")).toBe(wiringDeps.storage.getItem(NONCE_KEY));
  });

  it("offers no sign-in at all without a configured client", () => {
    const { result } = mount(fakeWorker(), "ready", null, vi.fn(), deps({ clientId: null }));
    expect(result.current.beginSignIn).toBeNull();
  });

  it("shows a refused return's message without asking the core anything", () => {
    const worker = fakeWorker();
    const { result } = mount(worker, "ready", { kind: "error", message: "Sign-in was cancelled." }, vi.fn());
    expect(result.current.signIn).toEqual({ kind: "error", message: "Sign-in was cancelled." });
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("waits for the core, then exchanges the token once and stores what comes back", async () => {
    const worker = fakeWorker();
    const submitToken = vi.fn().mockResolvedValue("ok");
    const { result, rerender } = mount(worker, "loading", { kind: "token", idToken: "id.tok.en" }, submitToken);
    expect(result.current.signIn).toEqual({ kind: "pending" });
    expect(worker.postMessage).not.toHaveBeenCalled();

    rerender({ coreStatus: "ready" });
    rerender({ coreStatus: "ready" });
    const sent = worker.postMessage.mock.calls.map(([message]) => message);
    expect(sent).toEqual([
      expect.objectContaining({
        type: "signInWithGoogle",
        idToken: "id.tok.en",
        name: "Chrome on macOS",
        nowMs: 4_000,
      }),
    ]);

    await act(async () => {
      answer(worker, { type: "signInResult", seed: sent[0].seed, kind: "signed_in", token: "hb_minted", error: null });
    });
    expect(submitToken).toHaveBeenCalledWith("hb_minted");
    expect(result.current.signIn).toEqual({ kind: "idle" });
  });

  it("words a refused exchange and stores nothing", async () => {
    const worker = fakeWorker();
    const submitToken = vi.fn();
    const { result } = mount(worker, "ready", { kind: "token", idToken: "id.tok.en" }, submitToken);
    const seed = (worker.postMessage.mock.calls[0][0] as { seed: string }).seed;
    await act(async () => {
      answer(worker, { type: "signInResult", seed, kind: "not_allowed", token: null, error: null });
    });
    expect(submitToken).not.toHaveBeenCalled();
    expect(result.current.signIn).toEqual({
      kind: "error",
      message: "That Google account isn't allowed to sign in here. Nothing on this device changed.",
    });
  });

  it("leaves the card signed out with a readable message when the exchange is abandoned (#889)", async () => {
    const worker = fakeWorker();
    const submitToken = vi.fn();
    const { result } = mount(worker, "ready", { kind: "token", idToken: "id.tok.en" }, submitToken);
    expect(result.current.signIn).toEqual({ kind: "pending" });
    const seed = (worker.postMessage.mock.calls[0][0] as { seed: string }).seed;
    await act(async () => {
      answer(worker, { type: "signInResult", seed, kind: "abandoned", token: null, error: null });
    });
    expect(submitToken).not.toHaveBeenCalled();
    expect(result.current.signIn).toEqual({
      kind: "error",
      message: "Sign-in didn't finish. Try again, or paste a token instead.",
    });
  });

  it("says so when the minted token could not be saved", async () => {
    const worker = fakeWorker();
    const submitToken = vi.fn().mockResolvedValue("storeError");
    const { result } = mount(worker, "ready", { kind: "token", idToken: "id.tok.en" }, submitToken);
    const seed = (worker.postMessage.mock.calls[0][0] as { seed: string }).seed;
    await act(async () => {
      answer(worker, { type: "signInResult", seed, kind: "signed_in", token: "hb_minted", error: null });
    });
    expect(result.current.signIn.kind).toBe("error");
  });
});
