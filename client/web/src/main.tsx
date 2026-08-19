import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { registerSW } from "virtual:pwa-register";
import { App } from "./App";
import { initDecisions } from "./decisions/seam";
import { appUpdateSignal } from "./shell/app-update";
import { watchForActivation } from "./shell/reload-on-activate";
import { SeamFailure } from "./shell/SeamFailure";
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
// `onNeedRefresh` is where the reader gets told. `updateSW()` is what posts
// `SKIP_WAITING` to that waiting worker; the reload is `watchForActivation`
// below, in EVERY open view, once the new worker takes control.
//
// `false` states that intent, and nothing more: in vite-plugin-pwa 0.21.2
// the argument is named `_reloadPage` and never read (the plugin attaches
// its own `controlling` -> reload listener when it shows the prompt, not
// when this flag is set). So passing `true` would behave identically today —
// it would just describe an ownership that is not ours to claim. Verified
// against `node_modules/vite-plugin-pwa/dist/client/build/register.js`; if a
// version bump makes the argument live again, `false` is still what we want.
const updateSW = registerSW({
  onNeedRefresh() {
    appUpdateSignal.markReady(() => void updateSW(false));
  },
});

// Reload when a new worker takes control — including when ANOTHER view is
// the one that applied the update. Wired here, beside the registration,
// because `navigator.serviceWorker` is browser-only wiring and `main.tsx` is
// where the rest of `src/` is spared knowing that; `reload-on-activate.ts`
// holds the decision and is provable without a browser.
//
// Not disposed: this lives as long as the document does, and the only exit
// from it is the reload itself.
if ("serviceWorker" in navigator) {
  watchForActivation(
    {
      hasController: () => navigator.serviceWorker.controller !== null,
      onControllerChange(listener) {
        navigator.serviceWorker.addEventListener("controllerchange", listener);
        return () => navigator.serviceWorker.removeEventListener("controllerchange", listener);
      },
    },
    () => window.location.reload(),
  );
}

// ADR-0010 (#126): one core per origin, in a `SharedWorker` — every tab and
// the installed PWA window is a view that connects a `MessagePort` to it,
// rather than each owning a dedicated `Worker`.
const sharedWorker = new SharedWorker(new URL("./worker/core.worker.ts", import.meta.url), {
  type: "module",
});
const worker = sharedWorker.port;
attachWorkerClient(worker, coreStore, localStorage);

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

// ADR-0025 (#141/M1-1): the SECOND wasm instantiation, on the main thread,
// behind `src/decisions/seam.ts`. It is awaited before the first render
// rather than joined to a spinner because there is no render-time gate to
// join: `App` mounts every screen while the worker is still connecting (the
// core's status is a label, not a gate), so a component could call a
// synchronous decision wrapper in its very first render. Awaiting here IS
// the gate, and it is the strongest form of it — no component ever renders
// against a not-ready seam, so the wrapper's not-ready branch can stay a
// throw instead of a stale TS fallback.
//
// A failure keeps the gate CLOSED and renders `SeamFailure` instead of
// `App` — never both, and never `App` alone. Rendering the app anyway would
// undo the paragraph above: the wrappers throw when the seam is not ready,
// there is no error boundary here, and the throw happens inside a render
// (`CaptureBox`'s `canSubmitCapture`), so the first capture the reader opens
// would unmount the whole tree into a blank page. Reporting it through
// `coreStore` is not enough either — the SharedWorker's `ready` message sets
// `error: null` (`store/worker-client.ts`), so a worker that connects after
// this rejection would erase the report and leave a healthy-looking shell
// over a seam that still throws.
initDecisions().then(
  () => {
    createRoot(root).render(
      <StrictMode>
        <App worker={worker} />
      </StrictMode>,
    );
  },
  (cause: unknown) => {
    createRoot(root).render(
      <StrictMode>
        <SeamFailure
          detail={cause instanceof Error ? cause.message : "decision seam failed to load"}
        />
      </StrictMode>,
    );
  },
);
