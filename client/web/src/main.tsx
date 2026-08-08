import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { coreStore } from "./store/store";
import { attachWorkerClient } from "./store/worker-client";
import "./styles.css";

const worker = new Worker(new URL("./worker/core.worker.ts", import.meta.url), {
  type: "module",
});
attachWorkerClient(worker, coreStore);

const root = document.getElementById("root");
if (!root) {
  throw new Error("missing #root element");
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
