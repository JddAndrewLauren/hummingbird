// The message protocol between the main thread and the core Web Worker
// (#69). Shared by worker-client.ts (main thread) and worker/core.worker.ts
// (the worker itself) so both sides stay in sync on the wire shape.

export type WorkerRequest = { type: "init" };

export type WorkerResponse =
  | { type: "ready"; apiVersion: number }
  | { type: "error"; message: string };
