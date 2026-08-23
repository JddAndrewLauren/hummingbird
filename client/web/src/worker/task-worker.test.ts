import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskWorkerResponse } from "../store/protocol";
import { PortRegistry, type PortLike } from "./ports";
import { createTaskRequestQueue, handleTaskRequest, type TaskHostLike } from "./task-worker";

function fakeHost(overrides: Partial<TaskHostLike> = {}): TaskHostLike {
  return {
    pushApiKey: vi.fn(),
    rehydrateApiKey: vi.fn(),
    clearApiKey: vi.fn(),
    capture: vi.fn().mockResolvedValue('{"kind":"ok","id":"item-1","error":null}'),
    act: vi.fn().mockResolvedValue('{"kind":"ok","error":null}'),
    triage: vi.fn().mockResolvedValue('{"kind":"ok","error":null}'),
    completeGrill: vi.fn().mockResolvedValue('{"kind":"ok","id":"grill-1","error":null}'),
    saveGrillDraft: vi.fn().mockResolvedValue('{"kind":"ok","error":null}'),
    discardGrillDraft: vi.fn().mockResolvedValue('{"kind":"ok","error":null}'),
    grillDraft: vi.fn().mockReturnValue('{"kind":"ok","exists":false,"turns":null}'),
    grillDraftItemIds: vi.fn().mockReturnValue('{"kind":"ok","item_ids":[]}'),
    setBinding: vi.fn().mockResolvedValue('{"kind":"ok","error":null}'),
    bindings: vi.fn().mockReturnValue('{"kind":"ok","bindings":[]}'),
    setQuestionEnabled: vi.fn().mockResolvedValue('{"kind":"ok","error":null}'),
    questionSwitches: vi.fn().mockReturnValue('{"kind":"ok","switches":[]}'),
    kindRegistry: vi
      .fn()
      .mockReturnValue(
        '{"kind":"ok","kinds":[],"core_fields":[],"alarm_interval_ms":900000,"severities":["low","normal","high","urgent"]}',
      ),
    rules: vi.fn().mockReturnValue('{"kind":"ok","rules":[]}'),
    createRule: vi.fn().mockResolvedValue('{"kind":"ok","id":"rule-1","error":null}'),
    patchRule: vi.fn().mockResolvedValue('{"kind":"ok","error":null}'),
    paneRead: vi.fn().mockReturnValue('{"kind":"ok","snapshots":[],"alerts":[]}'),
    frontier: vi.fn().mockReturnValue('{"kind":"ok","items":[]}'),
    triageInbox: vi.fn().mockReturnValue('{"kind":"ok","items":[]}'),
    grillingItems: vi.fn().mockReturnValue('{"kind":"ok","items":[]}'),
    ledger: vi.fn().mockReturnValue('{"kind":"ok","rows":[]}'),
    search: vi.fn().mockReturnValue('{"kind":"ok","rows":[],"total":0}'),
    done: vi.fn().mockReturnValue('{"kind":"ok","items":[]}'),
    blocked: vi.fn().mockReturnValue('{"kind":"ok","entries":[]}'),
    externallyBlocked: vi.fn().mockReturnValue('{"kind":"ok","items":[]}'),
    steps: vi.fn().mockReturnValue('{"kind":"ok","steps":[]}'),
    createProject: vi.fn().mockResolvedValue('{"kind":"ok","id":"project-1","error":null}'),
    patchProject: vi.fn().mockResolvedValue('{"kind":"ok","error":null}'),
    projects: vi.fn().mockReturnValue('{"kind":"ok","projects":[],"archived":[]}'),
    projectLinks: vi.fn().mockReturnValue('{"kind":"ok","links":[]}'),
    createProjectLink: vi.fn().mockResolvedValue('{"kind":"ok","id":"link-1","error":null}'),
    patchProjectLink: vi.fn().mockResolvedValue('{"kind":"ok","error":null}'),
    route: vi.fn().mockReturnValue('{"kind":"ok","route":null}'),
    patchRoute: vi.fn().mockResolvedValue('{"kind":"ok","error":null}'),
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

/** Every capture field at its resting state, spelled out — the wire type is
 * nullable-required rather than optional precisely so that a new field has to
 * be written here rather than silently defaulting. */
const EMPTY_CAPTURE_FIELDS = {
  size: null,
  energy: null,
  context: null,
  description: null,
  projectId: null,
  priority: null,
  deadline: null,
  scheduledDate: null,
} as const;

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

  it("initTaskApiKey forwards to host.rehydrateApiKey — never host.pushApiKey — and posts nothing back", async () => {
    // Issue #196: this is the rehydration path every view's core-start
    // effect uses, including a SECOND (or later) view connecting while a
    // first view's hold is live. Routing it to `pushApiKey` — the resuming
    // operation — is exactly the regression this test pins against.
    const host = fakeHost();
    const posted = await run({ type: "initTaskApiKey", apiKey: "device-token-1" }, host);

    expect(host.rehydrateApiKey).toHaveBeenCalledWith("device-token-1");
    expect(host.pushApiKey).not.toHaveBeenCalled();
    expect(posted).toEqual([]);
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
      {
        type: "capture",
        seed: "seed-1",
        title: "buy milk",
        stage: "ready",
        fields: EMPTY_CAPTURE_FIELDS,
        nowMs: 1_000,
      },
      host,
    );

    expect(host.capture).toHaveBeenCalledWith(
      "seed-1",
      "buy milk",
      "ready",
      JSON.stringify(EMPTY_CAPTURE_FIELDS),
      1_000,
    );
    expect(posted).toEqual([
      { type: "captureResult", seed: "seed-1", kind: "ok", id: "item-1", error: null },
    ]);
  });

  // #208: the wire's `fields` must reach the host call verbatim — this is
  // the pure-layer half of "the values reach the wire message"; the component
  // test proves the rendered controls produce this request in the first place.
  it("capture forwards every set field to the host verbatim, as one JSON object", async () => {
    const host = fakeHost();
    await run(
      {
        type: "capture",
        seed: "seed-1",
        title: "buy milk",
        stage: "ready",
        fields: {
          ...EMPTY_CAPTURE_FIELDS,
          size: "deep",
          energy: "high",
          context: "@errands",
          deadline: "2026-09-01",
          priority: 3,
        },
        nowMs: 1_000,
      },
      host,
    );

    expect(host.capture).toHaveBeenCalledWith(
      "seed-1",
      "buy milk",
      "ready",
      JSON.stringify({
        ...EMPTY_CAPTURE_FIELDS,
        size: "deep",
        energy: "high",
        context: "@errands",
        deadline: "2026-09-01",
        priority: 3,
      }),
      1_000,
    );
  });

  it("capture posts a failed result with the error message, no id", async () => {
    const host = fakeHost({
      capture: vi.fn().mockResolvedValue('{"kind":"failed","id":null,"error":"boom"}'),
    });
    const posted = await run(
      {
        type: "capture",
        seed: "seed-1",
        title: "x",
        stage: "triage",
        fields: EMPTY_CAPTURE_FIELDS,
        nowMs: 1_000,
      },
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

  it("triage forwards every field to the host and posts an ok result keyed by seed and item", async () => {
    const host = fakeHost();
    const posted = await run(
      {
        type: "triage",
        seed: "seed-triage-1",
        itemId: "item-1",
        destination: "ready",
        edits: {
          title: "buy milk",
          projectId: "project-1",
          size: "quick",
          energy: "low",
          context: "@errands",
          // A clear and an untouched field, so this asserts the whole
          // absent/null/value contract crosses the seam intact.
          deadline: null,
        },
        nowMs: 2_000,
      },
      host,
    );

    expect(host.triage).toHaveBeenCalledWith(
      "seed-triage-1",
      "item-1",
      "ready",
      // One JSON payload: the keys present are the fields touched, `null` is a
      // clear, and everything else is absent — which is what `JSON.stringify`
      // guarantees here.
      '{"title":"buy milk","projectId":"project-1","size":"quick","energy":"low","context":"@errands","deadline":null}',
      2_000,
    );
    expect(posted).toEqual([
      { type: "triageResult", seed: "seed-triage-1", itemId: "item-1", kind: "ok", error: null },
    ]);
  });

  it("triage forwards a scheduledDate set as a null-destination, edits-only call", async () => {
    const host = fakeHost();
    await run(
      {
        type: "triage",
        seed: "seed-triage-2",
        itemId: "item-1",
        destination: null,
        edits: { scheduledDate: "2026-08-15" },
        nowMs: 2_000,
      },
      host,
    );

    expect(host.triage).toHaveBeenCalledWith(
      "seed-triage-2",
      "item-1",
      null,
      '{"scheduledDate":"2026-08-15"}',
      2_000,
    );
  });

  it("triage forwards a scheduledDate clear as an explicit null, never a dropped key", async () => {
    const host = fakeHost();
    await run(
      {
        type: "triage",
        seed: "seed-triage-3",
        itemId: "item-1",
        destination: null,
        edits: { scheduledDate: null },
        nowMs: 2_000,
      },
      host,
    );

    expect(host.triage).toHaveBeenCalledWith(
      "seed-triage-3",
      "item-1",
      null,
      '{"scheduledDate":null}',
      2_000,
    );
  });

  it("triage posts a failed result with its error message", async () => {
    const host = fakeHost({
      triage: vi.fn().mockResolvedValue('{"kind":"failed","error":"unrecognised size \\"giant\\""}'),
    });
    const posted = await run(
      {
        type: "triage",
        seed: "seed-triage-1",
        itemId: "item-1",
        destination: "ready",
        edits: { size: "giant" as never },
        nowMs: 2_000,
      },
      host,
    );

    expect(posted).toEqual([
      {
        type: "triageResult",
        seed: "seed-triage-1",
        itemId: "item-1",
        kind: "failed",
        error: 'unrecognised size "giant"',
      },
    ]);
  });

  it("triage posts a not_found result for an unknown item id", async () => {
    const host = fakeHost({
      triage: vi.fn().mockResolvedValue('{"kind":"not_found","error":"item not found"}'),
    });
    const posted = await run(
      {
        type: "triage",
        seed: "seed-triage-1",
        itemId: "no-such-item",
        destination: "ready",
        edits: {},
        nowMs: 2_000,
      },
      host,
    );

    expect(posted).toEqual([
      {
        type: "triageResult",
        seed: "seed-triage-1",
        itemId: "no-such-item",
        kind: "not_found",
        error: "item not found",
      },
    ]);
  });

  it("completeGrill forwards every field to the host, snake_casing the session steps, and posts an ok result", async () => {
    const host = fakeHost();
    const posted = await run(
      {
        type: "completeGrill",
        seed: "seed-grill-1",
        itemId: "item-1",
        sessionSteps: [
          { id: "step-1", itemId: "item-1", body: "pack", done: false, position: 0, deletedAt: null, version: 1 },
        ],
        transcript: "Q: destination?\nA: Tokyo",
        summary: "Settled on Tokyo",
        verdict: "resolved",
        modelProposal: '{"title":"book flights to Tokyo"}',
        appliedPatch: '{"title":"book flights to Tokyo"}',
        deleteUntickedPlan: false,
        nowMs: 2_000,
      },
      host,
    );

    expect(host.completeGrill).toHaveBeenCalledWith(
      "seed-grill-1",
      "item-1",
      '[{"id":"step-1","item_id":"item-1","body":"pack","done":false,"position":0,"deleted_at":null,"version":1}]',
      "Q: destination?\nA: Tokyo",
      "Settled on Tokyo",
      "resolved",
      '{"title":"book flights to Tokyo"}',
      '{"title":"book flights to Tokyo"}',
      false,
      2_000,
    );
    expect(posted).toEqual([
      {
        type: "completeGrillResult",
        seed: "seed-grill-1",
        itemId: "item-1",
        kind: "ok",
        grillId: "grill-1",
        error: null,
      },
    ]);
  });

  it("completeGrill posts a needs_re_review result verbatim", async () => {
    const host = fakeHost({
      completeGrill: vi
        .fn()
        .mockResolvedValue(
          '{"kind":"needs_re_review","id":null,"error":"unticked steps changed since this review was last shown"}',
        ),
    });
    const posted = await run(
      {
        type: "completeGrill",
        seed: "seed-grill-2",
        itemId: "item-1",
        sessionSteps: [],
        transcript: "t",
        summary: "s",
        verdict: "fog_remains",
        modelProposal: "{}",
        appliedPatch: "{}",
        deleteUntickedPlan: false,
        nowMs: 2_000,
      },
      host,
    );

    expect(posted).toEqual([
      {
        type: "completeGrillResult",
        seed: "seed-grill-2",
        itemId: "item-1",
        kind: "needs_re_review",
        grillId: null,
        error: "unticked steps changed since this review was last shown",
      },
    ]);
  });

  it("saveGrillDraft stringifies turns whole and posts an ok result, then refreshes the bulk item-id list", async () => {
    const host = fakeHost({
      grillDraftItemIds: vi.fn().mockReturnValue('{"kind":"ok","item_ids":["item-1"]}'),
    });
    const posted = await run(
      {
        type: "saveGrillDraft",
        itemId: "item-1",
        turns: [{ question: { prompt: "p", recommendedAnswer: "r", choices: [] }, answer: "a" }],
        nowMs: 1_000,
      },
      host,
    );

    expect(host.saveGrillDraft).toHaveBeenCalledWith(
      "item-1",
      '[{"question":{"prompt":"p","recommendedAnswer":"r","choices":[]},"answer":"a"}]',
      1_000,
    );
    expect(posted).toEqual([
      { type: "saveGrillDraftResult", itemId: "item-1", kind: "ok", error: null },
      { type: "grillDraftItemIds", itemIds: ["item-1"] },
    ]);
  });

  it("saveGrillDraft does not refresh the bulk list on a failed save", async () => {
    const host = fakeHost({
      saveGrillDraft: vi.fn().mockResolvedValue('{"kind":"failed","error":"disk full"}'),
    });
    const posted = await run(
      { type: "saveGrillDraft", itemId: "item-1", turns: [], nowMs: 1_000 },
      host,
    );

    expect(posted).toEqual([
      { type: "saveGrillDraftResult", itemId: "item-1", kind: "failed", error: "disk full" },
    ]);
    expect(host.grillDraftItemIds).not.toHaveBeenCalled();
  });

  it("discardGrillDraft posts an ok result and refreshes the bulk item-id list", async () => {
    const host = fakeHost({
      grillDraftItemIds: vi.fn().mockReturnValue('{"kind":"ok","item_ids":[]}'),
    });
    const posted = await run(
      { type: "discardGrillDraft", itemId: "item-1", nowMs: 2_000 },
      host,
    );

    expect(host.discardGrillDraft).toHaveBeenCalledWith("item-1", 2_000);
    expect(posted).toEqual([
      { type: "discardGrillDraftResult", itemId: "item-1", kind: "ok", error: null },
      { type: "grillDraftItemIds", itemIds: [] },
    ]);
  });

  it("getGrillDraft answers with the item's saved turns", async () => {
    const host = fakeHost({
      grillDraft: vi.fn().mockReturnValue(
        '{"kind":"ok","exists":true,"turns":[{"question":{"prompt":"p","recommendedAnswer":"r","choices":[]},"answer":"a"}]}',
      ),
    });
    const posted = await run({ type: "getGrillDraft", itemId: "item-1" }, host);

    expect(posted).toEqual([
      {
        type: "grillDraft",
        itemId: "item-1",
        exists: true,
        turns: [{ question: { prompt: "p", recommendedAnswer: "r", choices: [] }, answer: "a" }],
      },
    ]);
  });

  it("getGrillDraft posts nothing while the host reports busy — no answer, not an empty one", async () => {
    const host = fakeHost({
      grillDraft: vi.fn().mockReturnValue('{"kind":"busy","exists":false,"turns":null}'),
    });
    const posted = await run({ type: "getGrillDraft", itemId: "item-1" }, host);
    expect(posted).toEqual([]);
  });

  it("getGrillDraftItemIds answers with every item id carrying a draft", async () => {
    const host = fakeHost({
      grillDraftItemIds: vi.fn().mockReturnValue('{"kind":"ok","item_ids":["item-1","item-2"]}'),
    });
    const posted = await run({ type: "getGrillDraftItemIds" }, host);
    expect(posted).toEqual([{ type: "grillDraftItemIds", itemIds: ["item-1", "item-2"] }]);
  });

  it("capture also drains and posts any credential event the cycle recorded", async () => {
    const host = fakeHost({
      takeEvents: vi.fn().mockReturnValue('[{"kind":"credential_needed","at_ms":5000}]'),
    });
    const posted = await run(
      {
        type: "capture",
        seed: "seed-1",
        title: "x",
        stage: "triage",
        fields: EMPTY_CAPTURE_FIELDS,
        nowMs: 1_000,
      },
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

  it("getLedger passes the request's nowMs through and maps rows, item fields flat", async () => {
    const host = fakeHost({
      ledger: vi.fn().mockReturnValue(
        JSON.stringify({
          kind: "ok",
          rows: [
            {
              ...rawItem,
              stage: "done",
              archived_at: 900,
              absent_since_ms: 900,
              dead_lettered: true,
              has_live_alert: true,
            },
          ],
        }),
      ),
    });
    const posted = await run({ type: "getLedger", nowMs: 4_000 }, host);

    expect(host.ledger).toHaveBeenCalledWith(4_000);
    expect(posted).toEqual([
      {
        type: "ledger",
        rows: [
          {
            ...dtoItem,
            stage: "done",
            archivedAt: 900,
            absentSinceMs: 900,
            deadLettered: true,
            hasLiveAlert: true,
          },
        ],
      },
    ]);
  });

  it('getLedger posts nothing when the host answers "busy" — an empty ledger is a claim', async () => {
    const host = fakeHost({
      ledger: vi.fn().mockReturnValue('{"kind":"busy","rows":[]}'),
    });
    expect(await run({ type: "getLedger", nowMs: 4_000 }, host)).toEqual([]);
  });

  it("search passes the request's query and nowMs through and maps rows, item fields flat", async () => {
    const host = fakeHost({
      search: vi.fn().mockReturnValue(
        JSON.stringify({
          kind: "ok",
          rows: [{ ...rawItem, stage: "done", archived_at: 900, group: "archived" }],
          total: 57,
        }),
      ),
    });
    const posted = await run({ type: "search", query: "milk", nowMs: 4_000 }, host);

    expect(host.search).toHaveBeenCalledWith("milk", 4_000);
    expect(posted).toEqual([
      {
        type: "searchResult",
        rows: [{ ...dtoItem, stage: "done", archivedAt: 900, group: "archived" }],
        total: 57,
      },
    ]);
  });

  it('search posts nothing when the host answers "busy" — no answer, not an empty one', async () => {
    const host = fakeHost({
      search: vi.fn().mockReturnValue('{"kind":"busy","rows":[],"total":0}'),
    });
    expect(await run({ type: "search", query: "milk", nowMs: 4_000 }, host)).toEqual([]);
  });

  it("getDone maps every raw item to its camelCase DTO", async () => {
    const host = fakeHost({
      done: vi.fn().mockReturnValue(
        JSON.stringify({ kind: "ok", items: [{ ...rawItem, stage: "done" }] }),
      ),
    });
    const posted = await run({ type: "getDone" }, host);

    expect(posted).toEqual([{ type: "done", items: [{ ...dtoItem, stage: "done" }] }]);
  });

  it('getDone posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      done: vi.fn().mockReturnValue('{"kind":"busy","items":[]}'),
    });
    expect(await run({ type: "getDone" }, host)).toEqual([]);
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

  it("getGrillingItems maps every raw item to its camelCase DTO", async () => {
    const host = fakeHost({
      grillingItems: vi.fn().mockReturnValue(JSON.stringify({ kind: "ok", items: [rawItem] })),
    });
    const posted = await run({ type: "getGrillingItems" }, host);

    expect(posted).toEqual([{ type: "grillingItems", items: [dtoItem] }]);
  });

  it('getGrillingItems posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      grillingItems: vi.fn().mockReturnValue('{"kind":"busy","items":[]}'),
    });
    expect(await run({ type: "getGrillingItems" }, host)).toEqual([]);
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

  it("getExternallyBlocked maps every raw item to its camelCase DTO", async () => {
    const host = fakeHost({
      externallyBlocked: vi.fn().mockReturnValue(JSON.stringify({ kind: "ok", items: [rawItem] })),
    });
    const posted = await run({ type: "getExternallyBlocked" }, host);

    expect(posted).toEqual([{ type: "externallyBlocked", items: [dtoItem] }]);
  });

  it('getExternallyBlocked posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      externallyBlocked: vi.fn().mockReturnValue('{"kind":"busy","items":[]}'),
    });
    expect(await run({ type: "getExternallyBlocked" }, host)).toEqual([]);
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

  // #624: both halves, mapped the same way and kept apart. An archived
  // project is absent in the mirror, so it can only ever arrive on the
  // second list — merging them here would put it in every project picker.
  it("getProjects maps every raw project to its camelCase DTO, live and archived apart", async () => {
    const rawProject = {
      id: "p-1",
      name: "Ship it",
      archived_at: null,
      created_at: 1,
      updated_at: 1,
      version: 1,
    };
    const rawArchived = { ...rawProject, id: "p-9", name: "Old bike", archived_at: 9_000 };
    const host = fakeHost({
      projects: vi
        .fn()
        .mockReturnValue(
          JSON.stringify({ kind: "ok", projects: [rawProject], archived: [rawArchived] }),
        ),
    });
    const posted = await run({ type: "getProjects" }, host);

    expect(posted).toEqual([
      {
        type: "projects",
        projects: [
          { id: "p-1", name: "Ship it", archivedAt: null, createdAt: 1, updatedAt: 1, version: 1 },
        ],
        archivedProjects: [
          {
            id: "p-9",
            name: "Old bike",
            archivedAt: 9_000,
            createdAt: 1,
            updatedAt: 1,
            version: 1,
          },
        ],
      },
    ]);
  });

  it('getProjects posts nothing when the host answers "busy"', async () => {
    const host = fakeHost({
      projects: vi.fn().mockReturnValue('{"kind":"busy","projects":[],"archived":[]}'),
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
    // Issue #191: the tail push — `queueDepth`/`deadLetters` come after the
    // outcome (and any drained task events), unsolicited, using the same
    // fakeHost defaults `getQueueDepth`/`getDeadLetters` themselves use.
    // Issue #195 round-1 review: `atMs` is the cycle's own `nowMs`, not
    // whatever clock a view later reads it with — pinned here as exactly
    // the `nowMs` this `runSync` request carried (1_000), not e.g. a
    // `Date.now()` call inside the handler.
    expect(posted).toEqual([
      {
        type: "syncOutcome",
        kind: "completed",
        retryAfterMs: null,
        activeItemCount: 3,
        wasFullSweep: false,
        deadLettered: 0,
        atMs: 1_000,
      },
      { type: "queueDepth", depth: 0 },
      { type: "deadLetters", entries: [] },
    ]);
  });

  it("runSync also drains and posts a credential-needed event when the cycle held, before the tail push", async () => {
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
        atMs: 9_000,
      },
      { type: "taskEvents", events: [{ kind: "credential_needed", atMs: 9000 }] },
      { type: "queueDepth", depth: 0 },
      { type: "deadLetters", entries: [] },
    ]);
  });

  it('runSync drops a "busy" queue-depth read at the cycle tail instead of posting an empty reading', async () => {
    const host = fakeHost({
      queueDepth: vi.fn().mockReturnValue('{"kind":"busy","depth":0}'),
    });

    const posted = await run(
      { type: "runSync", nowMs: 1_000, trigger: "user", forceFullSweep: false, jitterUnit: 0 },
      host,
    );

    expect(posted.some((response) => response.type === "queueDepth")).toBe(false);
    expect(posted.some((response) => response.type === "deadLetters")).toBe(true);
  });

  it('runSync drops a "busy" dead-letters read at the cycle tail instead of posting an empty reading', async () => {
    const host = fakeHost({
      deadLetters: vi.fn().mockReturnValue('{"kind":"busy","entries":[]}'),
    });

    const posted = await run(
      { type: "runSync", nowMs: 1_000, trigger: "user", forceFullSweep: false, jitterUnit: 0 },
      host,
    );

    expect(posted.some((response) => response.type === "deadLetters")).toBe(false);
    expect(posted.some((response) => response.type === "queueDepth")).toBe(true);
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

  // #163: `map_dead_letter` (`client/ffi-web/src/task_host.rs`) gained a
  // third `reason`, `"contention"` — a second 409 still genuinely disjoint
  // after the one bounded rebase retry. It crosses a JSON boundary, so
  // typecheck is structurally blind to a Rust variant this side does not
  // admit; this pins the string itself, and that the entry carries neither
  // a `message` nor any `fields` (there is no colliding field to name).
  it('getDeadLetters accepts "contention", which carries neither message nor fields', async () => {
    const host = fakeHost({
      deadLetters: vi.fn().mockReturnValue(
        JSON.stringify({
          kind: "ok",
          entries: [
            { id: "item-2", reason: "contention", message: null, fields: [], at_ms: 7_000 },
          ],
        }),
      ),
    });
    const posted = await run({ type: "getDeadLetters" }, host);

    expect(posted).toEqual([
      {
        type: "deadLetters",
        entries: [
          { id: "item-2", reason: "contention", message: null, fields: [], atMs: 7_000 },
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

  // -- #118's bindings -------------------------------------------------

  it("setBinding forwards the key and value to the host and posts a result keyed by seed", async () => {
    const host = fakeHost();
    const posted = await run(
      { type: "setBinding", seed: "seed-b-1", key: "race-series", value: "f1", nowMs: 5_000 },
      host,
    );

    expect(host.setBinding).toHaveBeenCalledWith("seed-b-1", "race-series", "f1", 5_000);
    expect(posted).toEqual([
      {
        type: "setBindingResult",
        seed: "seed-b-1",
        key: "race-series",
        kind: "ok",
        error: null,
      },
    ]);
  });

  it("setBinding surfaces the seam's own rejection of a key outside the vocabulary", async () => {
    const host = fakeHost({
      setBinding: vi
        .fn()
        .mockResolvedValue('{"kind":"unknown_key","error":"unrecognised binding key \\"nope\\""}'),
    });
    const posted = await run(
      { type: "setBinding", seed: "seed-b-2", key: "nope", value: "x", nowMs: 5_000 },
      host,
    );

    expect(posted[0]).toMatchObject({ type: "setBindingResult", kind: "unknown_key" });
  });

  it("getBindings posts every binding, values carried through as their tagged states", async () => {
    const host = fakeHost({
      bindings: vi
        .fn()
        .mockReturnValue(
          '{"kind":"ok","bindings":[{"key":"race-series","known":true,"pending":true,"value":{"state":"text","text":"f1"}},{"key":"trips-calendar","known":true,"pending":false,"value":{"state":"unset"}},{"key":"x","known":false,"pending":false,"value":{"state":"other","raw":"7"}}]}',
        ),
    });

    expect(await run({ type: "getBindings" }, host)).toEqual([
      {
        type: "bindings",
        bindings: [
          { key: "race-series", known: true, pending: true, value: { state: "text", text: "f1" } },
          { key: "trips-calendar", known: true, pending: false, value: { state: "unset" } },
          { key: "x", known: false, pending: false, value: { state: "other", raw: "7" } },
        ],
      },
    ]);
  });

  // -- #715's question off switch ---------------------------------------

  it("setQuestionEnabled forwards the question and the flag, and posts a result keyed by seed", async () => {
    const host = fakeHost();
    const posted = await run(
      {
        type: "setQuestionEnabled",
        seed: "seed-q-1",
        question: "weekend",
        enabled: false,
        nowMs: 5_000,
      },
      host,
    );

    expect(host.setQuestionEnabled).toHaveBeenCalledWith("seed-q-1", "weekend", false, 5_000);
    expect(posted).toEqual([
      {
        type: "setQuestionEnabledResult",
        seed: "seed-q-1",
        question: "weekend",
        kind: "ok",
        error: null,
      },
    ]);
  });

  it("setQuestionEnabled surfaces the seam's own rejection of a question it cannot name", async () => {
    const host = fakeHost({
      setQuestionEnabled: vi
        .fn()
        .mockResolvedValue(
          '{"kind":"unknown_question","error":"unrecognised standing question \\"fantasy\\""}',
        ),
    });
    const posted = await run(
      {
        type: "setQuestionEnabled",
        seed: "seed-q-2",
        question: "fantasy",
        enabled: false,
        nowMs: 5_000,
      },
      host,
    );

    expect(posted[0]).toMatchObject({
      type: "setQuestionEnabledResult",
      kind: "unknown_question",
    });
  });

  it("getQuestionSwitches posts every question's state", async () => {
    const host = fakeHost({
      questionSwitches: vi
        .fn()
        .mockReturnValue(
          '{"kind":"ok","switches":[{"question":"homework","enabled":true,"pending":false},{"question":"weekend","enabled":false,"pending":true}]}',
        ),
    });

    expect(await run({ type: "getQuestionSwitches" }, host)).toEqual([
      {
        type: "questionSwitches",
        switches: [
          { question: "homework", enabled: true, pending: false },
          { question: "weekend", enabled: false, pending: true },
        ],
      },
    ]);
  });

  it('getQuestionSwitches posts nothing when the host answers "busy"', async () => {
    // No answer, not an empty one: an empty switch list would read as
    // "there are no questions", and a roster defaulting that to all-on
    // would state a fact it had not read. `getBindings`' own contract.
    const host = fakeHost({
      questionSwitches: vi.fn().mockReturnValue('{"kind":"busy","switches":[]}'),
    });
    expect(await run({ type: "getQuestionSwitches" }, host)).toEqual([]);
  });

  it("getPaneRead camelCases the whole read, carrying the body through as opaque JSON text", async () => {
    // Pinned against `client/ffi-web/src/task_host.rs`'s own
    // `pane_read_response_serializes_with_the_exact_keys_the_pane_shell_ts_parses`
    // — nothing on this side re-derives the shape from serde's output.
    const host = fakeHost({
      paneRead: vi.fn().mockReturnValue(
        JSON.stringify({
          kind: "ok",
          snapshots: [
            {
              source: "city-waste/v2",
              key: "collection",
              fetched_at: 1_000,
              version: 2,
              freshness: { state: "age", age_ms: 60_000, declared_cadence_ms: 86_400_000 },
              envelope: {
                state: "parsed",
                schema: "city-waste/v2",
                polled_every_ms: 86_400_000,
                body: '{"zone":"Europe/London"}',
              },
            },
            {
              source: "city-waste/v2",
              key: "broken",
              fetched_at: 2_000,
              version: 1,
              freshness: { state: "unknown" },
              envelope: { state: "malformed", reason: "`body` is missing" },
            },
          ],
          alerts: [
            {
              id: "alert-1",
              source: "city-waste/v2",
              source_key: "collection:2026-08-11",
              subject_key: "collection",
              title: "Collection moved",
              body: null,
              url: null,
              severity: null,
              raised_at: 900,
              resolved_at: null,
              dismissed_at: null,
              expires_at: null,
              version: 1,
            },
          ],
        }),
      ),
    });

    const posted = await run(
      { type: "getPaneRead", source: "city-waste/v2", nowMs: 61_000 },
      host,
    );

    expect(host.paneRead).toHaveBeenCalledWith("city-waste/v2", 61_000);
    expect(posted).toEqual([
      {
        type: "paneRead",
        read: {
          source: "city-waste/v2",
          snapshots: [
            {
              key: "collection",
              fetchedAtMs: 1_000,
              envelope: {
                kind: "ok",
                schema: "city-waste/v2",
                polledEveryMs: 86_400_000,
                body: '{"zone":"Europe/London"}',
              },
              freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 86_400_000 },
            },
            {
              key: "broken",
              fetchedAtMs: 2_000,
              envelope: { kind: "malformed", reason: "`body` is missing" },
              freshness: { kind: "unknown" },
            },
          ],
          liveAlerts: [
            {
              id: "alert-1",
              subjectKey: "collection",
              title: "Collection moved",
              body: null,
              raisedAtMs: 900,
              expiresAtMs: null,
            },
          ],
        },
      },
    ]);
  });

  it('getPaneRead posts nothing when the host answers "busy"', async () => {
    // An empty pane read renders as "nothing is due" — a claim, not a
    // blank. A core that has not loaded has no standing to make it.
    const host = fakeHost({
      paneRead: vi.fn().mockReturnValue('{"kind":"busy","snapshots":[],"alerts":[]}'),
    });
    expect(
      await run({ type: "getPaneRead", source: "city-waste/v2", nowMs: 1 }, host),
    ).toEqual([]);
  });

  it('getBindings posts nothing when the host answers "busy"', async () => {
    // An empty list would read as "nothing is bound" — an answer, and the
    // wrong one. Busy says nothing at all.
    const host = fakeHost({
      bindings: vi.fn().mockReturnValue('{"kind":"busy","bindings":[]}'),
    });
    expect(await run({ type: "getBindings" }, host)).toEqual([]);
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

// ---------------------------------------------- issue #191: cycle-tail push

/** A `PortLike` whose `postMessage` calls are counted, wired through a real
 * `PortRegistry` — the acceptance criterion is about `PortRegistry.broadcast`
 * fan-out, not just this module in isolation, so the test exercises the same
 * `post` callable `core.worker.ts` actually wires (`registry.broadcast`). */
function countingPort(): PortLike & { count: () => number } {
  let calls = 0;
  return {
    onmessage: null,
    start: () => {},
    postMessage: () => {
      calls += 1;
    },
    count: () => calls,
  };
}

describe("runSync's cycle-tail push scales with view count in messages, not in wasm calls", () => {
  it.each([1, 3])(
    "reads the queue depth and dead letters exactly once per cycle for N=%i connected views",
    async (n) => {
      const host = fakeHost();
      const registry = new PortRegistry("test-core");
      registry.activate(async () => {}, () => 1);
      const ports = Array.from({ length: n }, () => countingPort());
      for (const port of ports) {
        registry.connect(port);
      }
      // Each `connect` above already posted its own `ready` handshake —
      // record it here so the counts below reflect only the cycle-tail
      // push, not the connection handshake.
      const handshakeCounts = ports.map((port) => port.count());

      const enqueue = createTaskRequestQueue(host, (response) => registry.broadcast(response));

      await enqueue({
        type: "runSync",
        nowMs: 1_000,
        trigger: "user",
        forceFullSweep: false,
        jitterUnit: 0,
      });

      // The wasm host is read exactly once per cycle regardless of N — this
      // is the O(N²) -> O(N) fix: `queueDepth`/`deadLetters` used to be
      // re-requested by every connected view every cycle.
      expect(host.queueDepth).toHaveBeenCalledTimes(1);
      expect(host.deadLetters).toHaveBeenCalledTimes(1);

      // Each of the three broadcasts (syncOutcome, queueDepth, deadLetters)
      // reaches every connected port — that fan-out is still, correctly,
      // O(N), not O(N²): total messages scale linearly with view count.
      const totalMessagesThisCycle = ports.reduce(
        (sum, port, i) => sum + (port.count() - handshakeCounts[i]),
        0,
      );
      expect(totalMessagesThisCycle).toBe(3 * n);
    },
  );
});

/** A `PortLike` that records every posted `WorkerResponse`, so a test can
 * assert on the actual *content* a view ends up holding — not just how many
 * messages arrived. Message count alone cannot catch the frozen-badge
 * regression the per-cycle counter (`TaskState.syncOutcomeSeq`) was
 * originally built to close (round-2 review of PR #181): a view could
 * receive the right number of messages every cycle and still be looking at
 * stale content if the SAME reading were pushed twice by mistake. */
function recordingPort(): PortLike & { messages: () => TaskWorkerResponse[] } {
  const messages: TaskWorkerResponse[] = [];
  return {
    onmessage: null,
    start: () => {},
    postMessage: (response) => {
      messages.push(response as TaskWorkerResponse);
    },
    messages: () => messages,
  };
}

describe("runSync's cycle-tail push keeps every connected view's reading fresh across cycles (issue #191)", () => {
  it("every connected port ends up holding the SECOND cycle's queue depth and dead letters, not the first's", async () => {
    // Two cycles, a genuinely CHANGING host reading between them — this is
    // the freshness assertion the frozen-badge regression (round-2 review of
    // PR #181) demands: a fixed-count assertion alone would pass even if the
    // second cycle re-pushed the first cycle's stale numbers.
    const host = fakeHost({
      queueDepth: vi
        .fn()
        .mockReturnValueOnce('{"kind":"ok","depth":1}')
        .mockReturnValueOnce('{"kind":"ok","depth":7}'),
      deadLetters: vi
        .fn()
        .mockReturnValueOnce('{"kind":"ok","entries":[]}')
        .mockReturnValueOnce(
          JSON.stringify({
            kind: "ok",
            entries: [
              {
                id: "item-9",
                reason: "permanent",
                message: "boom",
                fields: [],
                at_ms: 9_000,
              },
            ],
          }),
        ),
    });
    const registry = new PortRegistry("test-core");
    registry.activate(async () => {}, () => 1);
    // N=3, including one view connecting AFTER the first cycle already
    // ran — the on-ready request (not this push) is what catches such a
    // view up; this test only asserts what the cycle-tail push itself
    // delivers to whoever is connected when it fires.
    const earlyPorts = [recordingPort(), recordingPort()];
    for (const port of earlyPorts) {
      registry.connect(port);
    }

    const enqueue = createTaskRequestQueue(host, (response) => registry.broadcast(response));

    await enqueue({ type: "runSync", nowMs: 1_000, trigger: "user", forceFullSweep: false, jitterUnit: 0 });

    const latePort = recordingPort();
    registry.connect(latePort);

    await enqueue({ type: "runSync", nowMs: 2_000, trigger: "timer", forceFullSweep: false, jitterUnit: 0 });

    for (const port of [...earlyPorts, latePort]) {
      const queueDepthMessages = port
        .messages()
        .filter((message): message is Extract<TaskWorkerResponse, { type: "queueDepth" }> =>
          message.type === "queueDepth",
        );
      const deadLettersMessages = port
        .messages()
        .filter((message): message is Extract<TaskWorkerResponse, { type: "deadLetters" }> =>
          message.type === "deadLetters",
        );

      // The late-connecting port only sees the second cycle's tail push (it
      // was not connected for the first); the two early ports see both.
      const lastQueueDepth = queueDepthMessages.at(-1);
      const lastDeadLetters = deadLettersMessages.at(-1);
      expect(lastQueueDepth).toEqual({ type: "queueDepth", depth: 7 });
      expect(lastDeadLetters).toEqual({
        type: "deadLetters",
        entries: [
          {
            id: "item-9",
            reason: "permanent",
            message: "boom",
            fields: [],
            atMs: 9_000,
          },
        ],
      });
    }

    // The two early ports specifically must have seen BOTH readings, in
    // order — proof the second cycle's push actually replaced the first's
    // content rather than the first message simply never having arrived.
    for (const port of earlyPorts) {
      const depths = port
        .messages()
        .filter((message) => message.type === "queueDepth")
        .map((message) => (message as { depth: number }).depth);
      expect(depths).toEqual([1, 7]);
    }
  });
});
