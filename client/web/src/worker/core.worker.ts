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
//
// Issue #73's calendar wiring is the one case that goes the other direction
// (main -> worker): `self.onmessage` is attached below BEFORE `announceReady`
// posts "ready", and the main thread only ever sends a `CalendarWorkerRequest`
// after observing "ready" (see protocol.ts) — so by the time any such
// request could arrive, the listener has already been attached, in the same
// synchronous continuation of this module's async evaluation.

import { CalendarHost, core_api_version } from "../wasm/pkg/hummingbird_ffi_web";
import type { CalendarWorkerRequest, WorkerResponse } from "../store/protocol";
import { announceReady } from "./announce";
import { handleCalendarRequest } from "./calendar-worker";

// The IndexedDB database name (ADR-0003: the host contributes exactly one
// thing at init — a storage path/namespace). No calendars are selected
// until the picker (main thread) calls `setCalendarIds`.
const CALENDAR_NAMESPACE = "hummingbird-calendar";

const post = (response: WorkerResponse) =>
  (self as unknown as Worker).postMessage(response);

const calendarHost = new CalendarHost(CALENDAR_NAMESPACE, []);

self.onmessage = (event: MessageEvent<CalendarWorkerRequest>) => {
  void handleCalendarRequest(event.data, calendarHost, post);
};

announceReady(post, core_api_version);
