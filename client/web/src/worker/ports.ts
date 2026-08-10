import type { CalendarWorkerRequest, WorkerResponse } from "../store/protocol";
import { announceReady } from "./announce";

// The narrow slice of `MessagePort` the registry needs — narrow enough that
// tests can pass a plain object instead of a real port (same discipline as
// `store/worker-client.ts`'s `WorkerLike`).
export interface PortLike {
  postMessage(response: WorkerResponse): void;
  onmessage: ((event: MessageEvent<CalendarWorkerRequest>) => void) | null;
  start(): void;
}

/** ADR-0010: one core in a `SharedWorker`, N connecting views. Every tab and
 * the installed PWA window connects a `MessagePort`; the registry is what
 * turns "one core" into "every view sees the same thing" — it keeps the
 * port list `core.worker.ts`'s `onconnect` grows, broadcasts every published
 * event (poll outcomes, credential events, the tile, the picker list) to
 * every connected port, and announces the "ready"/"error" handshake to only
 * the port that just connected. That last part is the fix for the sharpest
 * edge in the migration: a dedicated worker's `announceReady` posted once,
 * unprompted, at module evaluation — correct when there is exactly one
 * view, but a second view connecting after the core is already running
 * would never see a handshake that already happened. Posting it per
 * connecting port keeps the push-only, unprompted shape (see protocol.ts)
 * while covering every view, not just the first. */
export class PortRegistry {
  private readonly ports = new Set<PortLike>();

  constructor(
    private readonly enqueue: (request: CalendarWorkerRequest) => Promise<void>,
    private readonly coreApiVersion: () => number,
  ) {}

  /** Wires a newly connecting port: routes its incoming requests through the
   * shared one-at-a-time queue, starts it, adds it to the broadcast set, and
   * announces readiness to it alone. */
  connect(port: PortLike): void {
    port.onmessage = (event) => {
      void this.enqueue(event.data);
    };
    port.start();
    this.ports.add(port);
    announceReady((response) => port.postMessage(response), this.coreApiVersion);
  }

  /** Posts one published event to every connected view. */
  broadcast(response: WorkerResponse): void {
    for (const port of this.ports) {
      port.postMessage(response);
    }
  }
}
