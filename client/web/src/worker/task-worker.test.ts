import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskWorkerResponse } from "../store/protocol";
import { createTaskRequestQueue, handleTaskRequest, type TaskHostLike } from "./task-worker";

function fakeHost(overrides: Partial<TaskHostLike> = {}): TaskHostLike {
  return {
    pushApiKey: vi.fn(),
    clearApiKey: vi.fn(),
    capture: vi.fn().mockResolvedValue('{"kind":"ok","id":"item-1","error":null}'),
    act: vi.fn().mockResolvedValue('{"kind":"ok","error":null}'),
    frontier: vi.fn().mockReturnValue('{"kind":"ok","items":[]}'),
    triageInbox: vi.fn().mockReturnValue('{"kind":"ok","items":[]}'),
    blocked: vi.fn().mockReturnValue('{"kind":"ok","entries":[]}'),
    steps: vi.fn().mockReturnValue('{"kind":"ok","steps":[]}'),
    projects: vi.fn().mockReturnValue('{"kind":"ok","projects":[]}'),
    isPending: vi.fn().mockReturnValue('{"kind":"ok","pending":false}'),
    takeEvents: vi.fn().mockReturnValue("[]"),
    runSync: vi.fn().mockResolvedValue(
      '{"kind":"no_credential","retry_after_ms":null,"active_item_count":null,"was_full_sweep":null,"dead_lettered":null}',
    ),
    queueDepth: vi.fn().mockReturnValue('{"kind":"ok","depth":0}'),
    deadLetters: vi.fn().mockReturnValue('{"kind":"ok","entries":[]}'),
    mirrorSnapshot: vi.fn().mockReturnValue('{"kind":"ok","mirror":{"version":0}}'),
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
  deadline: null,
  scheduled_date: null,
  source: null,
  source_key: null,
  source_url: null,
  archived_at: null,
  created_at: 1_000,
  updated_at: 1_000,
  version: 0,
  pending: false,
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
  deadline: null,
  scheduledDate: null,
  source: null,
  sourceKey: null,
  sourceUrl: null,
  archivedAt: null,
  createdAt: 1_000,
  updatedAt: 1_000,
  version: 0,
  pending: false,
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

  it("act posts an ok result keyed by seed, item and action", async () => {
    const host = fakeHost();
    const posted = await run(
      { type: "act", seed: "seed-act-1", itemId: "item-1", action: "complete", nowMs: 2_000 },
      host,
    );

    expect(host.act).toHaveBeenCalledWith("seed-act-1", "item-1", "complete", 2_000);
    expect(posted).toEqual([
      {
        type: "actResult",
        seed: "seed-act-1",
        itemId: "item-1",
        action: "complete",
        kind: "ok",
        error: null,
      },
    ]);
  });

  it("act posts a not_found result with its error message", async () => {
    const host = fakeHost({
      act: vi.fn().mockResolvedValue('{"kind":"not_found","error":"item not found"}'),
    });
    const posted = await run(
      { type: "act", seed: "seed-act-1", itemId: "no-such-item", action: "start", nowMs: 2_000 },
      host,
    );

    expect(posted).toEqual([
      {
        type: "actResult",
        seed: "seed-act-1",
        itemId: "no-such-item",
        action: "start",
        kind: "not_found",
        error: "item not found",
      },
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

  it("getFrontier maps a pending raw item to a pending DTO — issue #108 review", async () => {
    const host = fakeHost({
      frontier: vi.fn().mockReturnValue(
        JSON.stringify({ kind: "ok", items: [{ ...rawItem, pending: true }] }),
      ),
    });
    const posted = await run({ type: "getFrontier" }, host);

    expect(posted).toEqual([{ type: "frontier", items: [{ ...dtoItem, pending: true }] }]);
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

  it("getBlocked maps every raw entry to its camelCase DTO", async () => {
    const host = fakeHost({
      blocked: vi.fn().mockReturnValue(
        JSON.stringify({ kind: "ok", entries: [{ item: rawItem, blocked_by: [rawItem] }] }),
      ),
    });
    const posted = await run({ type: "getBlocked" }, host);

    expect(posted).toEqual([
      { type: "blocked", entries: [{ item: dtoItem, blockedBy: [dtoItem] }] },
    ]);
  });

  it('getBlocked posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      blocked: vi.fn().mockReturnValue('{"kind":"busy","entries":[]}'),
    });
    expect(await run({ type: "getBlocked" }, host)).toEqual([]);
  });

  it("getSteps maps every raw step to its camelCase DTO, alongside the requested item id", async () => {
    const rawStep = {
      id: "step-1",
      item_id: "item-1",
      body: "do the thing",
      done: false,
      position: 1,
      deleted_at: null,
      version: 0,
    };
    const host = fakeHost({
      steps: vi.fn().mockReturnValue(JSON.stringify({ kind: "ok", steps: [rawStep] })),
    });
    const posted = await run({ type: "getSteps", itemId: "item-1" }, host);

    expect(posted).toEqual([
      {
        type: "steps",
        itemId: "item-1",
        steps: [
          {
            id: "step-1",
            itemId: "item-1",
            body: "do the thing",
            done: false,
            position: 1,
            deletedAt: null,
            version: 0,
          },
        ],
      },
    ]);
    expect(host.steps).toHaveBeenCalledWith("item-1");
  });

  it('getSteps posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      steps: vi.fn().mockReturnValue('{"kind":"busy","steps":[]}'),
    });
    expect(await run({ type: "getSteps", itemId: "item-1" }, host)).toEqual([]);
  });

  it("getProjects maps every raw project to its camelCase DTO", async () => {
    const rawProject = {
      id: "p-1",
      name: "Ship it",
      archived_at: null,
      created_at: 1,
      updated_at: 1,
      version: 1,
    };
    const host = fakeHost({
      projects: vi.fn().mockReturnValue(JSON.stringify({ kind: "ok", projects: [rawProject] })),
    });
    const posted = await run({ type: "getProjects" }, host);

    expect(posted).toEqual([
      {
        type: "projects",
        projects: [
          { id: "p-1", name: "Ship it", archivedAt: null, createdAt: 1, updatedAt: 1, version: 1 },
        ],
      },
    ]);
  });

  it('getProjects posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      projects: vi.fn().mockReturnValue('{"kind":"busy","projects":[]}'),
    });
    expect(await run({ type: "getProjects" }, host)).toEqual([]);
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

  // ---------------------------------------------- S9 sync-status reads

  it("getQueueDepth posts the depth", async () => {
    const host = fakeHost({
      queueDepth: vi.fn().mockReturnValue('{"kind":"ok","depth":2}'),
    });
    const posted = await run({ type: "getQueueDepth" }, host);

    expect(posted).toEqual([{ type: "queueDepth", depth: 2 }]);
  });

  it('getQueueDepth posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      queueDepth: vi.fn().mockReturnValue('{"kind":"busy","depth":0}'),
    });
    expect(await run({ type: "getQueueDepth" }, host)).toEqual([]);
  });

  it("getDeadLetters maps every raw entry to its camelCase DTO", async () => {
    const host = fakeHost({
      deadLetters: vi.fn().mockReturnValue(
        JSON.stringify({
          kind: "ok",
          entries: [
            {
              id: "item-1",
              reason: "conflict",
              message: null,
              fields: [{ field: "title", local: "buy oat milk", server: "someone else's" }],
              at_ms: 5_000,
            },
          ],
        }),
      ),
    });
    const posted = await run({ type: "getDeadLetters" }, host);

    expect(posted).toEqual([
      {
        type: "deadLetters",
        entries: [
          {
            id: "item-1",
            reason: "conflict",
            message: null,
            fields: [{ field: "title", local: "buy oat milk", server: "someone else's" }],
            atMs: 5_000,
          },
        ],
      },
    ]);
  });

  it('getDeadLetters posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      deadLetters: vi.fn().mockReturnValue('{"kind":"busy","entries":[]}'),
    });
    expect(await run({ type: "getDeadLetters" }, host)).toEqual([]);
  });

  it("getMirrorSnapshot posts the mirror value verbatim", async () => {
    const host = fakeHost({
      mirrorSnapshot: vi.fn().mockReturnValue('{"kind":"ok","mirror":{"version":1}}'),
    });
    const posted = await run({ type: "getMirrorSnapshot" }, host);

    expect(posted).toEqual([{ type: "mirrorSnapshot", mirror: { version: 1 } }]);
  });

  it('getMirrorSnapshot posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      mirrorSnapshot: vi.fn().mockReturnValue('{"kind":"busy","mirror":null}'),
    });
    expect(await run({ type: "getMirrorSnapshot" }, host)).toEqual([]);
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
