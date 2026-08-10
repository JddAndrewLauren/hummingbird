import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskWorkerResponse } from "../store/protocol";
import { createTaskRequestQueue, handleTaskRequest, type TaskHostLike } from "./task-worker";

function fakeHost(overrides: Partial<TaskHostLike> = {}): TaskHostLike {
  return {
    pushApiKey: vi.fn(),
    clearApiKey: vi.fn(),
    capture: vi.fn().mockResolvedValue('{"kind":"ok","id":"item-1","error":null}'),
    frontier: vi.fn().mockReturnValue('{"kind":"ok","items":[]}'),
    triageInbox: vi.fn().mockReturnValue('{"kind":"ok","items":[]}'),
    isPending: vi.fn().mockReturnValue('{"kind":"ok","pending":false}'),
    takeEvents: vi.fn().mockReturnValue("[]"),
    runSync: vi.fn().mockResolvedValue(
      '{"kind":"no_credential","retry_after_ms":null,"active_item_count":null,"was_full_sweep":null,"dead_lettered":null}',
    ),
    ...overrides,
  };
}

async function run(
  request: Parameters<typeof handleTaskRequest>[0],
  host: TaskHostLike,
): Promise<TaskWorkerResponse[]> {
  const posted: TaskWorkerResponse[] = [];
  await handleTaskRequest(request, host, (response) => posted.push(response));
  return posted;
}

const rawItem = {
  id: "item-1",
  seq: 1,
  title: "buy milk",
  description: null,
  stage: "ready",
  size: null,
  energy: null,
  context: null,
  priority: 0,
  project_id: null,
  project_pos: null,
  due_date: null,
  scheduled_date: null,
  source: null,
  source_key: null,
  source_url: null,
  archived_at: null,
  created_at: 1_000,
  updated_at: 1_000,
  version: 0,
};

const dtoItem = {
  id: "item-1",
  seq: 1,
  title: "buy milk",
  description: null,
  stage: "ready",
  size: null,
  energy: null,
  context: null,
  priority: 0,
  projectId: null,
  projectPos: null,
  dueDate: null,
  scheduledDate: null,
  source: null,
  sourceKey: null,
  sourceUrl: null,
  archivedAt: null,
  createdAt: 1_000,
  updatedAt: 1_000,
  version: 0,
};

describe("handleTaskRequest", () => {
  it("pushTaskApiKey forwards the key to the host and posts nothing — never echoed back", async () => {
    const host = fakeHost();
    const posted = await run({ type: "pushTaskApiKey", apiKey: "device-token-1" }, host);

    expect(host.pushApiKey).toHaveBeenCalledWith("device-token-1");
    expect(posted).toEqual([]);
    // No message this handler ever posts carries the string it was pushed —
    // check every posted response's serialized form as a defensive-in-depth
    // net, not just the empty-array assertion above.
    expect(JSON.stringify(posted)).not.toContain("device-token-1");
  });

  it("clearTaskApiKey forwards to the host and posts nothing back", async () => {
    const host = fakeHost();
    const posted = await run({ type: "clearTaskApiKey" }, host);

    expect(host.clearApiKey).toHaveBeenCalledTimes(1);
    expect(posted).toEqual([]);
  });

  it("capture posts the minted id keyed by the seed the caller chose", async () => {
    const host = fakeHost();
    const posted = await run(
      { type: "capture", seed: "seed-1", title: "buy milk", stage: "ready", nowMs: 1_000 },
      host,
    );

    expect(host.capture).toHaveBeenCalledWith("seed-1", "buy milk", "ready", 1_000);
    expect(posted).toEqual([
      { type: "captureResult", seed: "seed-1", kind: "ok", id: "item-1", error: null },
    ]);
  });

  it("capture posts a failed result with the error message, no id", async () => {
    const host = fakeHost({
      capture: vi.fn().mockResolvedValue('{"kind":"failed","id":null,"error":"boom"}'),
    });
    const posted = await run(
      { type: "capture", seed: "seed-1", title: "x", stage: "triage", nowMs: 1_000 },
      host,
    );

    expect(posted).toEqual([
      { type: "captureResult", seed: "seed-1", kind: "failed", id: null, error: "boom" },
    ]);
  });

  it("capture also drains and posts any credential event the cycle recorded", async () => {
    const host = fakeHost({
      takeEvents: vi.fn().mockReturnValue('[{"kind":"credential_needed","at_ms":5000}]'),
    });
    const posted = await run(
      { type: "capture", seed: "seed-1", title: "x", stage: "triage", nowMs: 1_000 },
      host,
    );

    expect(posted).toEqual([
      { type: "captureResult", seed: "seed-1", kind: "ok", id: "item-1", error: null },
      { type: "taskEvents", events: [{ kind: "credential_needed", atMs: 5000 }] },
    ]);
  });

  it("getFrontier maps every raw item to its camelCase DTO", async () => {
    const host = fakeHost({
      frontier: vi.fn().mockReturnValue(JSON.stringify({ kind: "ok", items: [rawItem] })),
    });
    const posted = await run({ type: "getFrontier" }, host);

    expect(posted).toEqual([{ type: "frontier", items: [dtoItem] }]);
  });

  it('getFrontier posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      frontier: vi.fn().mockReturnValue('{"kind":"busy","items":[]}'),
    });
    expect(await run({ type: "getFrontier" }, host)).toEqual([]);
  });

  it("getTriageInbox maps every raw item to its camelCase DTO", async () => {
    const host = fakeHost({
      triageInbox: vi.fn().mockReturnValue(JSON.stringify({ kind: "ok", items: [rawItem] })),
    });
    const posted = await run({ type: "getTriageInbox" }, host);

    expect(posted).toEqual([{ type: "triageInbox", items: [dtoItem] }]);
  });

  it('getTriageInbox posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      triageInbox: vi.fn().mockReturnValue('{"kind":"busy","items":[]}'),
    });
    expect(await run({ type: "getTriageInbox" }, host)).toEqual([]);
  });

  it("isPending posts the item id alongside the answer", async () => {
    const host = fakeHost({
      isPending: vi.fn().mockReturnValue('{"kind":"ok","pending":true}'),
    });
    const posted = await run({ type: "isPending", itemId: "item-1" }, host);

    expect(host.isPending).toHaveBeenCalledWith("item-1");
    expect(posted).toEqual([{ type: "isPendingResult", itemId: "item-1", pending: true }]);
  });

  it('isPending posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      isPending: vi.fn().mockReturnValue('{"kind":"busy","pending":false}'),
    });
    expect(await run({ type: "isPending", itemId: "item-1" }, host)).toEqual([]);
  });

  it("runSync posts the mapped outcome", async () => {
    const host = fakeHost({
      runSync: vi.fn().mockResolvedValue(
        JSON.stringify({
          kind: "completed",
          retry_after_ms: null,
          active_item_count: 3,
          was_full_sweep: false,
          dead_lettered: 0,
        }),
      ),
    });

    const posted = await run(
      { type: "runSync", nowMs: 1_000, trigger: "user", forceFullSweep: true, jitterUnit: 0 },
      host,
    );

    expect(host.runSync).toHaveBeenCalledWith(1_000, "user", true, 0);
    expect(posted).toEqual([
      {
        type: "syncOutcome",
        kind: "completed",
        retryAfterMs: null,
        activeItemCount: 3,
        wasFullSweep: false,
        deadLettered: 0,
      },
    ]);
  });

  it("runSync also drains and posts a credential-needed event when the cycle held", async () => {
    const host = fakeHost({
      runSync: vi.fn().mockResolvedValue(
        '{"kind":"credential_needed","retry_after_ms":null,"active_item_count":null,"was_full_sweep":null,"dead_lettered":0}',
      ),
      takeEvents: vi.fn().mockReturnValue('[{"kind":"credential_needed","at_ms":9000}]'),
    });

    const posted = await run(
      { type: "runSync", nowMs: 9_000, trigger: "timer", forceFullSweep: false, jitterUnit: 1 },
      host,
    );

    expect(posted).toEqual([
      {
        type: "syncOutcome",
        kind: "credential_needed",
        retryAfterMs: null,
        activeItemCount: null,
        wasFullSweep: null,
        deadLettered: 0,
      },
      { type: "taskEvents", events: [{ kind: "credential_needed", atMs: 9000 }] },
    ]);
  });
});

describe("createTaskRequestQueue", () => {
  function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((r) => {
      resolve = r;
    });
    return { promise, resolve };
  }

  it("does not let a second request reach the host while the first is in flight", async () => {
    const inFlight = deferred<string>();
    const host = fakeHost({
      runSync: vi.fn().mockReturnValue(inFlight.promise),
    });
    const posted: TaskWorkerResponse[] = [];
    const enqueue = createTaskRequestQueue(host, (response) => posted.push(response));

    void enqueue({ type: "runSync", nowMs: 1_000, trigger: "user", forceFullSweep: true, jitterUnit: 0 });
    const second = enqueue({ type: "getFrontier" });
    await Promise.resolve();

    expect(host.frontier).not.toHaveBeenCalled();

    inFlight.resolve(
      '{"kind":"completed","retry_after_ms":null,"active_item_count":0,"was_full_sweep":true,"dead_lettered":0}',
    );
    await second;

    expect(host.frontier).toHaveBeenCalled();
  });

  it("keeps draining after a request fails", async () => {
    const host = fakeHost({
      runSync: vi.fn().mockRejectedValue(new Error("boom")),
    });
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const posted: TaskWorkerResponse[] = [];
    const enqueue = createTaskRequestQueue(host, (response) => posted.push(response));

    await enqueue({ type: "runSync", nowMs: 1_000, trigger: "user", forceFullSweep: true, jitterUnit: 0 });
    await enqueue({ type: "getFrontier" });

    expect(consoleError).toHaveBeenCalled();
    expect(posted).toEqual([{ type: "frontier", items: [] }]);
    consoleError.mockRestore();
  });

  describe("a request that never settles", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it("is abandoned so the queue does not wedge behind it", async () => {
      const host = fakeHost({
        runSync: vi.fn().mockReturnValue(new Promise<string>(() => {})),
      });
      const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
      const posted: TaskWorkerResponse[] = [];
      const enqueue = createTaskRequestQueue(host, (response) => posted.push(response));

      const first = enqueue({
        type: "runSync",
        nowMs: 1_000,
        trigger: "user",
        forceFullSweep: true,
        jitterUnit: 0,
      });
      const second = enqueue({ type: "getFrontier" });

      await vi.advanceTimersByTimeAsync(30_100);
      await first;
      await second;

      expect(host.frontier).toHaveBeenCalled();
      expect(posted).toContainEqual({ type: "frontier", items: [] });
      consoleError.mockRestore();
    });
  });
});
