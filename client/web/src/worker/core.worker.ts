/// <reference lib="webworker" />

// The core Web Worker (#69): loads the wasm-bindgen core (hummingbird-ffi-web,
// #67) off the main thread. `vite-plugin-wasm` + `vite-plugin-top-level-await`
// (vite.config.ts) let this import the wasm-pack `--target bundler` output
// directly as an ES module.
//
// The worker ANNOUNCES readiness itself at module evaluation — it never waits
// for a request from the main thread. The top-level-await plugin wraps this
// whole module in an async IIFE, so nothing here is guaranteed to run before
// the main thread's messages arrive; a request/response handshake therefore
// drops the request (PR #79 round-2 blocker). Pushing worker -> main cannot
// race: the main thread attaches its onmessage synchronously at construction
// (main.tsx), before any worker message can be dispatched. If the wasm import
// itself fails (e.g. CSP), module evaluation throws and the main thread's
// worker.onerror surfaces it instead.

import { core_api_version } from "../wasm/pkg/hummingbird_ffi_web";
import { announceReady } from "./announce";

announceReady(
  (response) => (self as unknown as Worker).postMessage(response),
  core_api_version,
);
