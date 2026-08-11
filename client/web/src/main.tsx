import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { registerSW } from "virtual:pwa-register";
import { App } from "./App";
import { appUpdateSignal } from "./shell/app-update";
import { coreStore } from "./store/store";
import { watchForReadyTimeout } from "./store/ready-timeout";
import { attachWorkerClient } from "./store/worker-client";
import { applyInitialTheme } from "./theme/useTheme";
import "./styles.css";

// Before the first render, so nothing paints in the wrong theme.
applyInitialTheme();

// The ONLY file that touches `virtual:pwa-register` — the module
// vite-plugin-pwa synthesises at build time, which vitest (running without
// the plugin) could not resolve at all. `main.tsx` already plays exactly
// this role for the `SharedWorker` above: the browser-only wiring lives
// here and hands the rest of `src/` a plain module to read.
//
// `registerType: "prompt"` leaves a new worker waiting rather than
// skip-waiting into a page still rendering the old precached shell, so
// `onNeedRefresh` is where the reader gets told. `updateSW(true)` is what
// swaps to the waiting worker and reloads.
const updateSW = registerSW({
  onNeedRefresh() {
    appUpdateSignal.markReady(() => void updateSW(true));
  },
});

// ADR-0010 (#126): one core per origin, in a `SharedWorker` — every tab and
// the installed PWA window is a view that connects a `MessagePort` to it,
// rather than each owning a dedicated `Worker`.
const sharedWorker = new SharedWorker(new URL("./worker/core.worker.ts", import.meta.url), {
  type: "module",
});
const worker = sharedWorker.port;
attachWorkerClient(worker, coreStore);

// `sharedWorker.onerror` is NOT the CSP/wasm-failure catch-all a dedicated
// Worker's `onerror` was. Per spec, a SharedWorker's `error` event covers
// only its script's initial *fetch* failing; an uncaught error during the
// script's own evaluation (a CSP rejecting WebAssembly compilation is
// exactly this) is reported to the worker's own global scope, never here
// (PR #167 round-1 review, blocker 2). The real fallback for that case is
// worker-side: `core.worker.ts` wraps its wasm init in a try/catch and, on
// failure, `PortRegistry.activateError` posts a real `{type: "error"}`
// message to this port, which `attachWorkerClient` already routes to the
// store above. `watchForReadyTimeout` below is the last-resort backstop for
// anything that reaches neither path. This handler is kept for what
// `onerror` genuinely still catches — the initial script fetch failing.
sharedWorker.onerror = (event) => {
  coreStore.setState({
    status: "error",
    error: event.message || "worker failed to load",
  });
};

// The last-resort backstop for blocker 2 above: if nothing — neither a
// `ready`, an `{type: "error"}` message, nor `sharedWorker.onerror` — is
// ever heard from, this moves the UI off "Loading core…" instead of
// leaving it stuck forever.
watchForReadyTimeout(coreStore, 10_000);

const root = document.getElementById("root");
if (!root) {
  throw new Error("missing #root element");
}

createRoot(root).render(
  <StrictMode>
    <App worker={worker} />
  </StrictMode>,
);
