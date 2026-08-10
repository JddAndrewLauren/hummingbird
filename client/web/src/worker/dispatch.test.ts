import { describe, expect, it, vi } from "vitest";
import type { TaskWorkerRequest } from "../store/protocol";
import { createDispatch } from "./dispatch";

// `core.worker.ts`'s routing used to live inline in that file, where no test
// can import it — the one place a `setViewVisibility` could be mis-routed
// into a wasm queue, and where the "sweep on app open" trigger fired at the
// wrong moment for a whole batch without anything noticing (post-batch
// review of PR #185). Extracted, it is ordinary decision logic.

function harness(options: { taskEnqueue?: (request: TaskWorkerRequest) => Promise<void> } = {}) {
  const cadence = { onOpen: vi.fn(), onFocus: vi.fn() };
  const visibility = { setHidden: vi.fn() };
  const taskEnqueue = vi.fn(options.taskEnqueue ?? (() => Promise.resolve()));
  const calendarEnqueue = vi.fn(() => Promise.resolve());
  const dispatch = createDispatch<string>({
    cadence,
    visibility,
    taskEnqueueReady: Promise.resolve(taskEnqueue),
    calendarEnqueue,
  });
  return { cadence, visibility, taskEnqueue, calendarEnqueue, dispatch };
}

const PUSH_KEY: TaskWorkerRequest = { type: "pushTaskApiKey", apiKey: "device-token" };

describe("createDispatch routing", () => {
  it("intercepts setViewVisibility for the reporting port, reaching neither wasm queue", async () => {
    const h = harness();

    await h.dispatch({ type: "setViewVisibility", hidden: true }, "tab-1");

    expect(h.visibility.setHidden).toHaveBeenCalledWith("tab-1", true);
    expect(h.taskEnqueue).not.toHaveBeenCalled();
    expect(h.calendarEnqueue).not.toHaveBeenCalled();
  });

  it("intercepts syncFocusTrigger as a cadence focus, reaching neither wasm queue", async () => {
    const h = harness();

    await h.dispatch({ type: "syncFocusTrigger" }, "tab-1");

    expect(h.cadence.onFocus).toHaveBeenCalledTimes(1);
    expect(h.taskEnqueue).not.toHaveBeenCalled();
    expect(h.calendarEnqueue).not.toHaveBeenCalled();
  });

  it("routes a task request to the task queue only", async () => {
    const h = harness();

    await h.dispatch({ type: "getFrontier" }, "tab-1");

    expect(h.taskEnqueue).toHaveBeenCalledWith({ type: "getFrontier" });
    expect(h.calendarEnqueue).not.toHaveBeenCalled();
  });

  it("routes everything else to the calendar queue", async () => {
    const h = harness();

    await h.dispatch({ type: "pollTimer", nowMs: 1 }, "tab-1");

    expect(h.calendarEnqueue).toHaveBeenCalledWith({ type: "pollTimer", nowMs: 1 });
    expect(h.taskEnqueue).not.toHaveBeenCalled();
  });

  it("preserves task-request order across the deferred queue's resolution", async () => {
    const seen: string[] = [];
    const h = harness({
      taskEnqueue: (request) => {
        seen.push(request.type);
        return Promise.resolve();
      },
    });

    await Promise.all([
      h.dispatch(PUSH_KEY, "tab-1"),
      h.dispatch({ type: "getFrontier" }, "tab-1"),
      h.dispatch({ type: "getTriageInbox" }, "tab-1"),
    ]);

    expect(seen).toEqual(["pushTaskApiKey", "getFrontier", "getTriageInbox"]);
  });
});

// ADR-0007's "sweep on app open / core start", and `sync-cadence.ts`'s own
// contract for it: "call once the core is ready AND a task credential is
// known." PR #181 called it at core activation, before any view could have
// pushed one, so every session's first cycle returned `no_credential` and
// Settings read "Held — device token needed" on a device with a good stored
// token until the next 60s tick.
describe("the app-open sweep waits for a credential", () => {
  it("does not fire on activation, nor on any request that is not a credential push", async () => {
    const h = harness();

    await h.dispatch({ type: "getFrontier" }, "tab-1");
    await h.dispatch({ type: "syncFocusTrigger" }, "tab-1");
    await h.dispatch({ type: "pollTimer", nowMs: 1 }, "tab-1");

    expect(h.cadence.onOpen).not.toHaveBeenCalled();
  });

  it("fires once the first device token reaches the core", async () => {
    const h = harness();

    await h.dispatch(PUSH_KEY, "tab-1");

    expect(h.cadence.onOpen).toHaveBeenCalledTimes(1);
  });

  it("fires only AFTER the key is in the core, never before it", async () => {
    const order: string[] = [];
    const h = harness({
      taskEnqueue: async () => {
        order.push("key pushed");
      },
    });
    h.cadence.onOpen.mockImplementation(() => order.push("open sweep"));

    await h.dispatch(PUSH_KEY, "tab-1");

    expect(order).toEqual(["key pushed", "open sweep"]);
  });

  it("fires exactly once per core, however many views push a key or rotate one", async () => {
    const h = harness();

    await h.dispatch(PUSH_KEY, "tab-1");
    await h.dispatch(PUSH_KEY, "tab-2");
    await h.dispatch({ type: "pushTaskApiKey", apiKey: "rotated" }, "tab-1");

    expect(h.taskEnqueue).toHaveBeenCalledTimes(3);
    expect(h.cadence.onOpen).toHaveBeenCalledTimes(1);
  });
});
