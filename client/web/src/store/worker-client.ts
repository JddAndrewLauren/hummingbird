import type { createCoreStore } from "./store";
import type { WorkerResponse } from "./protocol";

// The narrow slice of the DOM Worker interface the client needs — narrow
// enough that tests can pass a plain object instead of a real Worker.
// Listen-only by design: the worker announces readiness itself (see
// protocol.ts), so the client never posts and there is no init race to lose.
export interface WorkerLike {
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null;
}

type Store = Pick<ReturnType<typeof createCoreStore>, "setState">;

// Wires a worker's response messages into the store. This is the only place
// that translates the worker protocol into store writes. Must be called in
// the same synchronous task that constructs the Worker, so the listener is
// attached before any worker message can be dispatched.
export function attachWorkerClient(worker: WorkerLike, store: Store): void {
  worker.onmessage = (event) => {
    const message = event.data;
    if (message.type === "ready") {
      store.setState({
        status: "ready",
        apiVersion: message.apiVersion,
        error: null,
      });
    } else if (message.type === "error") {
      store.setState({ status: "error", error: message.message });
    }
  };
}
