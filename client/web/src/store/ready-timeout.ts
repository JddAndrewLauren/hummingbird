import type { createCoreStore } from "./store";

type Store = Pick<ReturnType<typeof createCoreStore>, "getSnapshot" | "setState">;

/** The view-side backstop for PR #167 round-1 review blocker 2: a
 * SharedWorker's own `onerror` event covers only its script's initial
 * *fetch* failing, not an uncaught error during evaluation (a CSP rejecting
 * WebAssembly compilation is the case this whole path exists for) — see
 * main.tsx. The primary fix is `core.worker.ts`'s `PortRegistry.activateError`,
 * which posts a real `{type: "error"}` message that `attachWorkerClient`
 * already routes to the store; this is what catches anything that reaches
 * neither path (a core that hangs without ever posting at all).
 *
 * If the store is still `"loading"` once `timeoutMs` elapses, moves it to
 * `"error"` so the UI leaves "Loading core…" instead of waiting forever. A
 * `ready` or a more specific `error` that already arrived is left alone —
 * this only ever fires into the loading state it was watching. */
export function watchForReadyTimeout(store: Store, timeoutMs: number): void {
  setTimeout(() => {
    if (store.getSnapshot().status === "loading") {
      store.setState({ status: "error", error: "core did not report ready in time" });
    }
  }, timeoutMs);
}
