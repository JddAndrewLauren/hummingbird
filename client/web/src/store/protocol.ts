// The message protocol between the main thread and the core Web Worker
// (#69). Shared by worker-client.ts (main thread) and worker/core.worker.ts
// (the worker itself) so both sides stay in sync on the wire shape.
//
// Push-only, worker -> main. The worker announces its own readiness once its
// wasm module is up; the main thread never sends a request. This is what
// makes the handshake immune to bundler transforms (PR #79 round-2 blocker):
// vite-plugin-top-level-await wraps the worker module in an async IIFE, so
// any handler the worker registers "at the top" can still land after a
// message posted from the main thread at construction time — that message is
// silently dropped. In the worker->main direction there is no such race: the
// main thread attaches its listener synchronously in the same task that
// constructs the Worker, before any worker message can be dispatched.

export type WorkerResponse =
  | { type: "ready"; apiVersion: number }
  | { type: "error"; message: string };
