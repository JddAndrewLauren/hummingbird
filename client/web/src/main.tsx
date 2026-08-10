import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { coreStore } from "./store/store";
import { attachWorkerClient } from "./store/worker-client";
import { applyInitialTheme } from "./theme/useTheme";
import "./styles.css";

// Before the first render, so nothing paints in the wrong theme.
applyInitialTheme();

// ADR-0010 (#126): one core per origin, in a `SharedWorker` — every tab and
// the installed PWA window is a view that connects a `MessagePort` to it,
// rather than each owning a dedicated `Worker`.
const sharedWorker = new SharedWorker(new URL("./worker/core.worker.ts", import.meta.url), {
  type: "module",
});
const worker = sharedWorker.port;
attachWorkerClient(worker, coreStore);

// The worker's wasm import is top-level, so a module-eval failure (e.g. the
// CSP rejecting WebAssembly compilation) never reaches attachWorkerClient's
// message handler -- it fires here instead. Without this, the UI would sit
// on "Loading core…" forever instead of showing the error branch App.tsx
// already implements. `onerror` lives on the `SharedWorker` object itself,
// not the port.
sharedWorker.onerror = (event) => {
  coreStore.setState({
    status: "error",
    error: event.message || "worker failed to load",
  });
};

const root = document.getElementById("root");
if (!root) {
  throw new Error("missing #root element");
}

createRoot(root).render(
  <StrictMode>
    <App worker={worker} />
  </StrictMode>,
);
