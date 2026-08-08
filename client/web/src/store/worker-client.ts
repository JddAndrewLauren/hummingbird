import type { createCoreStore } from "./store";
import type { WorkerRequest, WorkerResponse } from "./protocol";

// The narrow slice of the DOM Worker interface the client needs — narrow
// enough that tests can pass a plain object instead of a real Worker.
export interface WorkerLike {
  postMessage(message: WorkerRequest): void;
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null;
}

type Store = Pick<ReturnType<typeof createCoreStore>, "setState">;

// Wires a worker's response messages into the store, and kicks off the
// worker's wasm-core init. This is the only place that translates the
// worker protocol into store writes.
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

  worker.postMessage({ type: "init" });
}
