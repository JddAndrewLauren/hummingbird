/// <reference lib="webworker" />

// The core Web Worker (#69): loads the wasm-bindgen core (hummingbird-ffi-web,
// #67) off the main thread. `vite-plugin-wasm` + `vite-plugin-top-level-await`
// (vite.config.ts) let this import the wasm-pack `--target bundler` output
// directly as an ES module. Talks to the main thread only over the
// WorkerRequest/WorkerResponse protocol in ../store/protocol.ts.

import { core_api_version } from "../wasm/pkg/hummingbird_ffi_web";
import type { WorkerRequest, WorkerResponse } from "../store/protocol";

function reply(response: WorkerResponse): void {
  (self as unknown as Worker).postMessage(response);
}

self.onmessage = (event: MessageEvent<WorkerRequest>) => {
  if (event.data.type !== "init") {
    return;
  }
  try {
    const apiVersion = core_api_version();
    reply({ type: "ready", apiVersion });
  } catch (err) {
    reply({ type: "error", message: err instanceof Error ? err.message : String(err) });
  }
};
