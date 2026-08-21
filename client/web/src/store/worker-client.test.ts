import { afterEach, describe, expect, it, vi } from "vitest";
import { type CalendarState, type TaskState, createCoreStore } from "./store";
import {
  actOnTask,
  attachWorkerClient,
  captureTask,
  pollRefresh,
  pollStart,
  pollTimer,
  clearTaskApiKey,
  discardGrillDraft,
  initTaskApiKey,
  pushTaskApiKey,
  pushTokenToWorker,
  reportViewVisibility,
  requestCalendarEvents,
  requestCalendarList,
  requestPaneRead,
  requestDeadLetters,
  requestBlocked,
  requestFrontier,
  requestGrillDraft,
  requestGrillDraftItemIds,
  requestIsPending,
  requestMirrorSnapshot,
  requestProjects,
  requestQueueDepth,
  requestSteps,
  requestTriageInbox,
  saveGrillDraft,
  setCalendarSelectionsOnWorker,
  setMirrorSnapshotHandler,
  triageItem,
  triggerSyncFocus,
  triggerSyncManual,
  type WorkerLike,
} from "./worker-client";
import type { SyncStorageLike } from "./sync-persistence";

const initialCalendar: CalendarState = {
  connected: false,
  needsReconnect: false,
  selectedCalendarIds: [],
  availableCalendars: [],
  lastPollOutcome: null,
  connectPending: false,
  connectError: null,
  silentRemintBlocked: false,
  eventReads: {},
};

const initialTask: TaskState = {
  frontier: [],
  triageInbox: [],
  grillingItems: [],
  blocked: [],
  stepsByItem: {},
  projects: null,
  archivedProjects: null,
  ledger: null,
  search: null,
  done: null,
  bindings: null,
  kindRegistry: null,
  rules: null,
  lastRuleWrite: null,
  lastProjectWrite: null,
  linksByProject: {},
  lastProjectLinkWrite: null,
  paneReads: {},
  pending: {},
  lastCapture: null,
  lastAct: null,
  lastTriage: null,
  lastGrillCompletion: null,
  lastGrillDraftWrite: null,
  grillDraftItemIds: [],
  grillDraftByItem: {},
  lastBindingWrite: null,
  lastSyncOutcome: null,
  lastSyncAtMs: null,
  lastSuccessfulSyncAtMs: null,
  syncOutcomeSeq: 0,
  queueDepth: null,
  deadLetters: [],
  needsReconnect: false,
  hostError: null,
};

// A minimal fake of the Worker surface the client needs: an assignable
// onmessage handler the test can drive directly, plus a spyable
// postMessage. The *response* (attachWorkerClient) side never calls
// postMessage itself — only the explicit send helpers below do, and only
// ever after the store has already observed "ready" (PR #79 round-2
// blocker — see protocol.ts).
function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return {
    onmessage: null,
    postMessage: vi.fn(),
  };
}

function fakeSyncStorage(initial?: number): SyncStorageLike {
  const values = new Map<string, string>();
  if (initial !== undefined) {
    values.set("hb.sync.lastSuccessfulAtMs", JSON.stringify(initial));
  }
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => {
      values.set(key, value);
    },
  };
}

describe("attachWorkerClient", () => {
  it("only listens on attach — sends nothing, so there is no init race to lose", () => {
    const worker = fakeWorker();
    const store = createCoreStore();

    attachWorkerClient(worker, store);

    expect(worker.onmessage).toBeTypeOf("function");
    expect(store.getSnapshot().status).toBe("loading");
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("moves the store to ready with the reported api version on a ready message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      // #172: the handshake also carries the core instance identity.
      data: { type: "ready", apiVersion: 3, coreId: "3f2a1b8c", viewOrdinal: 2 },
    } as MessageEvent);

    expect(store.getSnapshot()).toEqual({
      status: "ready",
      apiVersion: 3,
      coreId: "3f2a1b8c",
      viewOrdinal: 2,
      error: null,
      calendar: initialCalendar,
      task: initialTask,
    });
  });

  it("moves the store to error with the reported message on an error message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "error", message: "wasm init failed" },
    } as MessageEvent);

    expect(store.getSnapshot()).toEqual({
      status: "error",
      apiVersion: null,
      coreId: null,
      viewOrdinal: null,
      error: "wasm init failed",
      calendar: initialCalendar,
      task: initialTask,
    });
  });

  it("records the latest poll outcome", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "pollOutcome", outcome: "succeeded" },
    } as MessageEvent);

    expect(store.getSnapshot().calendar.lastPollOutcome).toBe("succeeded");
    // #245: nothing is re-requested behind a poll outcome any more. The
    // context tile that used to be refreshed here is gone, and the ranked
    // pane region's own reads belong to `usePaneReadsWiring`.
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("flags needsReconnect when a non-empty credentialEvents message arrives", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "credentialEvents",
        events: [{ provider: "google_calendar", atMs: 1000 }],
      },
    } as MessageEvent);

    expect(store.getSnapshot().calendar.needsReconnect).toBe(true);
  });

  it("does not flag needsReconnect on an empty credentialEvents message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "credentialEvents", events: [] },
    } as MessageEvent);

    expect(store.getSnapshot().calendar.needsReconnect).toBe(false);
  });

  it("writes the picker's options on a calendarList message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "calendarList",
        calendars: [{ id: "primary", summary: "john@twinion.net" }],
      },
    } as MessageEvent);

    expect(store.getSnapshot().calendar).toEqual({
      ...initialCalendar,
      availableCalendars: [{ id: "primary", summary: "john@twinion.net" }],
    });
  });

  it("writes a calendarEvents message into eventReads, keyed by the request's own key", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "calendarEvents",
        key: "weekend",
        read: { state: "not_read" },
      },
    } as MessageEvent);

    expect(store.getSnapshot().calendar).toEqual({
      ...initialCalendar,
      eventReads: { weekend: { state: "not_read" } },
    });
  });

  it("keeps other request keys' reads intact when a new one lands", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "calendarEvents", key: "weekend", read: { state: "not_read" } },
    } as MessageEvent);
    worker.onmessage?.({
      data: {
        type: "calendarEvents",
        key: "today",
        read: { state: "read", events: [], freshness: { kind: "unknown" } },
      },
    } as MessageEvent);

    expect(store.getSnapshot().calendar.eventReads).toEqual({
      weekend: { state: "not_read" },
      today: { state: "read", events: [], freshness: { kind: "unknown" } },
    });
  });

  // -- task binding (#105/S7) -----------------------------------------

  it("records a captureResult keyed by its seed", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "captureResult", seed: "seed-1", kind: "ok", id: "item-1", error: null },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastCapture).toEqual({
      seed: "seed-1",
      kind: "ok",
      id: "item-1",
      error: null,
    });
  });

  it("records an actResult keyed by seed/item/action and re-requests the frontier and blocked queries on ok", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "actResult",
        seed: "seed-act-1",
        itemId: "item-1",
        action: "complete",
        kind: "ok",
        error: null,
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastAct).toEqual({
      seed: "seed-act-1",
      itemId: "item-1",
      action: "complete",
      kind: "ok",
      error: null,
    });
    // `Core::act`'s overlay updates synchronously — an `ok` result
    // immediately re-requests the frontier/blocked queries so the
    // completed item drops off the list without waiting for the next sync
    // cycle (this issue's "Completing offline shows Done immediately").
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getFrontier" });
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getBlocked" });
    // The row checkmark completes from any live stage — Triage included —
    // so an ok act also re-reads the triage inbox, the same way an ok
    // triage does.
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getTriageInbox" });
    // PR #207 round-2 fix: the acted-on item's `pending` must come from a
    // LIVE source (`task.pending`), so an ok act immediately asks the core
    // `isPending` — the task worker's serial queue guarantees the act was
    // applied first, so this reads back `true` until a sync cycle drains it.
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "isPending", itemId: "item-1" });
  });

  it("records a failed actResult without re-requesting anything", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "actResult",
        seed: "seed-act-1",
        itemId: "no-such-item",
        action: "start",
        kind: "not_found",
        error: "item not found",
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastAct).toEqual({
      seed: "seed-act-1",
      itemId: "no-such-item",
      action: "start",
      kind: "not_found",
      error: "item not found",
    });
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("records a triageResult keyed by seed/item and re-requests the triage inbox and frontier on ok", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "triageResult",
        seed: "seed-triage-1",
        itemId: "item-1",
        kind: "ok",
        error: null,
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastTriage).toEqual({
      seed: "seed-triage-1",
      itemId: "item-1",
      kind: "ok",
      error: null,
    });
    // `Core::triage`'s overlay updates synchronously — an `ok` result
    // immediately re-requests the triage inbox/frontier so a promoted item
    // leaves triage and appears on the frontier without waiting for the
    // next sync cycle (this issue's acceptance).
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getTriageInbox" });
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getFrontier" });
    // #122: a `null`-destination triage (the weekend-plans pane's do-date
    // chip) can touch a relation-blocked item, so `blocked` needs the same
    // immediate re-read `actResult` already gives it.
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getBlocked" });
  });

  it("records a failed triageResult without re-requesting anything", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "triageResult",
        seed: "seed-triage-1",
        itemId: "no-such-item",
        kind: "not_found",
        error: "item not found",
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastTriage).toEqual({
      seed: "seed-triage-1",
      itemId: "no-such-item",
      kind: "not_found",
      error: "item not found",
    });
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("records a createProjectResult keyed by seed and re-requests projects on ok", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "createProjectResult",
        seed: "seed-project-1",
        kind: "ok",
        id: "project-1",
        error: null,
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastProjectWrite).toEqual({
      seed: "seed-project-1",
      projectId: "project-1",
      kind: "ok",
      error: null,
    });
    // #624: unlike `triageResult` above there is NO overlay to reveal — this
    // re-request will answer the old list until a cycle lands, and that is
    // the point. It exists so the grid updates on the very next answer
    // rather than only on the next per-cycle refresh.
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getProjects" });
  });

  it("records a failed createProjectResult without re-requesting anything", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "createProjectResult",
        seed: "seed-project-1",
        kind: "failed",
        id: null,
        error: "name must be non-empty",
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastProjectWrite).toEqual({
      seed: "seed-project-1",
      projectId: null,
      kind: "failed",
      error: "name must be non-empty",
    });
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("records a saveGrillDraftResult keyed by itemId", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "saveGrillDraftResult", itemId: "item-1", kind: "ok", error: null },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastGrillDraftWrite).toEqual({
      itemId: "item-1",
      kind: "ok",
      error: null,
    });
  });

  it("records a discardGrillDraftResult in the same slot a saveGrillDraftResult uses", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "discardGrillDraftResult", itemId: "item-1", kind: "failed", error: "disk full" },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastGrillDraftWrite).toEqual({
      itemId: "item-1",
      kind: "failed",
      error: "disk full",
    });
  });

  it("installs an existing draft's turns into grillDraftByItem, keyed by item id", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    const turns = [{ question: { prompt: "p", recommendedAnswer: "r", choices: [] }, answer: "a" }];
    worker.onmessage?.({
      data: { type: "grillDraft", itemId: "item-1", exists: true, turns },
    } as MessageEvent);

    expect(store.getSnapshot().task.grillDraftByItem).toEqual({ "item-1": turns });
  });

  /** Reviewer finding on PR #482: an `exists: false` answer must still
   * install an (empty) entry — `useGrillTakeoverWiring.ts`'s resume wait
   * has no other signal that a fetch answered, and skipping this would
   * strand a session whose bulk-list draft turned out (a race with another
   * tab's discard) not to exist at `idle` forever. */
  it("installs an empty entry when the answer says no draft exists, so a resume wait is not stranded", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "grillDraft", itemId: "item-1", exists: false, turns: null },
    } as MessageEvent);

    expect(store.getSnapshot().task.grillDraftByItem).toEqual({ "item-1": [] });
  });

  it("replaces the bulk grillDraftItemIds list wholesale", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "grillDraftItemIds", itemIds: ["item-1", "item-2"] },
    } as MessageEvent);

    expect(store.getSnapshot().task.grillDraftItemIds).toEqual(["item-1", "item-2"]);
  });

  it("records a setBindingResult keyed by seed/key and re-reads the bindings on ok", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "setBindingResult",
        seed: "seed-b-1",
        key: "race-series",
        kind: "ok",
        error: null,
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastBindingWrite).toEqual({
      seed: "seed-b-1",
      key: "race-series",
      kind: "ok",
      error: null,
    });
    // `Core::set_binding`'s overlay updates synchronously — the re-read is
    // what puts the new value on screen without waiting for a cycle (#118).
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getBindings" });
  });

  it("records a refused setBindingResult without re-requesting anything", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "setBindingResult",
        seed: "seed-b-2",
        key: "nope",
        kind: "unknown_key",
        error: "unrecognised binding key \"nope\"",
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastBindingWrite?.kind).toBe("unknown_key");
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("lands a pane read under its own source, and only grows what was asked for", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    // Starts empty — a source with no entry means "not read yet", which a
    // pane reads as a gap rather than as "no rows".
    expect(store.getSnapshot().task.paneReads).toEqual({});

    const read = {
      source: "city-waste/v2",
      snapshots: [],
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
    };
    worker.onmessage?.({ data: { type: "paneRead", read } } as MessageEvent);

    // Keyed by the source the message itself names, never by whichever
    // request this view happened to have outstanding: this is a broadcast.
    expect(store.getSnapshot().task.paneReads).toEqual({ "city-waste/v2": read });
  });

  it("writes the bindings on a bindings message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    // `null` until an answer arrives — an empty array is a real answer.
    expect(store.getSnapshot().task.bindings).toBeNull();

    worker.onmessage?.({
      data: {
        type: "bindings",
        bindings: [
          { key: "race-series", known: true, pending: false, value: { state: "text", text: "f1" } },
        ],
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.bindings).toEqual([
      { key: "race-series", known: true, pending: false, value: { state: "text", text: "f1" } },
    ]);
  });

  it("writes the frontier on a frontier message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    const item = {
      id: "item-1",
      seq: null,
      title: "buy milk",
      description: null,
      stage: "ready" as const,
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
      createdAt: 1,
      updatedAt: 1,
      version: 0,
      pending: false,
    };
    worker.onmessage?.({ data: { type: "frontier", items: [item] } } as MessageEvent);

    expect(store.getSnapshot().task.frontier).toEqual([item]);
    // Untouched sibling field.
    expect(store.getSnapshot().task.triageInbox).toEqual([]);
  });

  it("writes the ledger on a ledger message — null until then, an empty answer is a claim", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);
    expect(store.getSnapshot().task.ledger).toBeNull();

    worker.onmessage?.({ data: { type: "ledger", rows: [] } } as MessageEvent);

    expect(store.getSnapshot().task.ledger).toEqual([]);
    // Untouched sibling field.
    expect(store.getSnapshot().task.done).toBeNull();
  });

  it("writes the done list on a done message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);
    expect(store.getSnapshot().task.done).toBeNull();

    worker.onmessage?.({ data: { type: "done", items: [] } } as MessageEvent);

    expect(store.getSnapshot().task.done).toEqual([]);
  });

  it("writes the triage inbox on a triageInbox message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({ data: { type: "triageInbox", items: [] } } as MessageEvent);

    expect(store.getSnapshot().task.triageInbox).toEqual([]);
  });

  it("writes the blocked entries on a blocked message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    const item = {
      id: "item-1",
      seq: null,
      title: "buy milk",
      description: null,
      stage: "ready" as const,
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
      createdAt: 1,
      updatedAt: 1,
      version: 0,
      pending: false,
    };
    worker.onmessage?.({
      data: { type: "blocked", entries: [{ item, blockedBy: [item] }] },
    } as MessageEvent);

    expect(store.getSnapshot().task.blocked).toEqual([{ item, blockedBy: [item] }]);
    // Untouched sibling field.
    expect(store.getSnapshot().task.frontier).toEqual([]);
  });

  it("writes the steps for the requested item on a steps message, keyed by item id", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    const step = {
      id: "step-1",
      itemId: "item-1",
      body: "do the thing",
      done: false,
      position: 1,
      deletedAt: null,
      version: 0,
    };
    worker.onmessage?.({
      data: { type: "steps", itemId: "item-1", steps: [step] },
    } as MessageEvent);

    expect(store.getSnapshot().task.stepsByItem).toEqual({ "item-1": [step] });

    worker.onmessage?.({ data: { type: "steps", itemId: "item-2", steps: [] } } as MessageEvent);

    expect(store.getSnapshot().task.stepsByItem).toEqual({ "item-1": [step], "item-2": [] });
  });

  it("writes the projects on a projects message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    const project = {
      id: "p-1",
      name: "Ship it",
      archivedAt: null,
      createdAt: 1,
      updatedAt: 1,
      version: 0,
    };
    const archived = { ...project, id: "p-9", name: "Old bike", archivedAt: 9_000 };
    worker.onmessage?.({
      data: { type: "projects", projects: [project], archivedProjects: [archived] },
    } as MessageEvent);

    // One answer sets both halves, and they stay apart: every project picker
    // in the app renders `projects`, and an archived one offered there as a
    // destination would be a bug (#624).
    expect(store.getSnapshot().task.projects).toEqual([project]);
    expect(store.getSnapshot().task.archivedProjects).toEqual([archived]);
    // Untouched sibling field.
    expect(store.getSnapshot().task.frontier).toEqual([]);
  });

  it("merges one item's pending state on an isPendingResult message, leaving others alone", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "isPendingResult", itemId: "item-1", pending: true },
    } as MessageEvent);
    worker.onmessage?.({
      data: { type: "isPendingResult", itemId: "item-2", pending: false },
    } as MessageEvent);

    expect(store.getSnapshot().task.pending).toEqual({ "item-1": true, "item-2": false });
  });

  it("records the sync outcome and the sweep time on a syncOutcome message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "syncOutcome",
        kind: "completed",
        retryAfterMs: null,
        activeItemCount: 2,
        wasFullSweep: true,
        deadLettered: 0,
        atMs: 5_000,
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastSyncOutcome).toEqual({
      kind: "completed",
      retryAfterMs: null,
      activeItemCount: 2,
      wasFullSweep: true,
      deadLettered: 0,
    });
    expect(store.getSnapshot().task.lastSyncAtMs).toBe(5_000);
    expect(store.getSnapshot().task.lastSuccessfulSyncAtMs).toBe(5_000);
  });

  it("hydrates persisted success before a replayed failure arrives", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store, fakeSyncStorage(8_000));

    expect(store.getSnapshot().task.lastSuccessfulSyncAtMs).toBe(8_000);
    worker.onmessage?.({
      data: {
        type: "syncOutcome",
        kind: "pull_failed",
        retryAfterMs: 30_000,
        activeItemCount: null,
        wasFullSweep: null,
        deadLettered: null,
        atMs: 9_000,
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastSuccessfulSyncAtMs).toBe(8_000);
    expect(store.getSnapshot().task.lastSyncAtMs).toBe(9_000);
  });

  it("persists only newer completed outcomes", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    const storage = fakeSyncStorage(8_000);
    attachWorkerClient(worker, store, storage);

    const broadcast = (kind: "completed" | "held" | "skipped" | "busy", atMs: number) => {
      worker.onmessage?.({
        data: {
          type: "syncOutcome",
          kind,
          retryAfterMs: null,
          activeItemCount: null,
          wasFullSweep: null,
          deadLettered: null,
          atMs,
        },
      } as MessageEvent);
    };

    broadcast("completed", 7_000);
    broadcast("held", 9_000);
    broadcast("skipped", 10_000);
    broadcast("busy", 11_000);
    expect(store.getSnapshot().task.lastSuccessfulSyncAtMs).toBe(8_000);

    broadcast("completed", 12_000);
    expect(store.getSnapshot().task.lastSuccessfulSyncAtMs).toBe(12_000);
    expect(storage.getItem("hb.sync.lastSuccessfulAtMs")).toBe("12000");
  });

  it("records the sweep time on a held or failed outcome too — staleness must not freeze", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "syncOutcome",
        kind: "held",
        retryAfterMs: null,
        activeItemCount: null,
        wasFullSweep: null,
        deadLettered: null,
        atMs: 7_000,
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastSyncAtMs).toBe(7_000);
  });

  // Issue #195 round-1 review (blocking finding 1): `PortRegistry` (ports.ts)
  // replays the last `syncOutcome` broadcast to a port that connects long
  // after the cycle it describes. If this handler stamped `lastSyncAtMs`
  // from its OWN receipt clock, a replay would read as though the cycle had
  // just happened — the exact false-freshness `isInformativeSyncOutcome` /
  // `OUTCOME_CLASS` exist to prevent (see their own doc: a backed-off tick
  // used to re-green a "Stale" badge to "Synced — as of just now" every
  // minute during an outage). `atMs` is the cycle's OWN time
  // (`worker/task-worker.ts` posts `request.nowMs`), so `lastSyncAtMs` must
  // come from the message, never from this view's receipt-time clock — this
  // test's `now` returns a wildly different value specifically to prove
  // that.
  it("stamps lastSyncAtMs from the message's own atMs, not the receiving view's clock — a replayed outcome must read at its true age", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    // A view connecting hours after the cycle this outcome describes — its
    // own receipt-time clock is nowhere near the cycle's real time.
    attachWorkerClient(worker, store);

    const cycleTimeMs = 5_000;
    worker.onmessage?.({
      data: {
        type: "syncOutcome",
        kind: "completed",
        retryAfterMs: null,
        activeItemCount: 2,
        wasFullSweep: true,
        deadLettered: 0,
        atMs: cycleTimeMs,
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastSyncAtMs).toBe(cycleTimeMs);
  });

  // Post-batch review of PR #185. `lastSyncAtMs` used to be stamped
  // unconditionally, and `"skipped"`/`"busy"` were unclassified in
  // `sync-status.ts`, so a backed-off tick during a server outage
  // (`Core::run` -> `Skipped`, "nothing was attempted at all") both erased
  // the real `pull_failed` and re-stamped the clock — flipping the badge
  // from "Stale" back to a green "Synced — as of just now" every 60 seconds
  // for the entire outage.
  it.each<"skipped" | "busy">(["skipped", "busy"])(
    "does not advance lastSyncAtMs on a %s outcome — nothing was attempted, so nothing got fresher",
    (kind) => {
      const worker = fakeWorker();
      const store = createCoreStore();
      attachWorkerClient(worker, store);

      worker.onmessage?.({
        data: {
          type: "syncOutcome",
          kind: "pull_failed",
          retryAfterMs: 30_000,
          activeItemCount: null,
          wasFullSweep: null,
          deadLettered: null,
          atMs: 1_000,
        },
      } as MessageEvent);
      worker.onmessage?.({
        data: {
          type: "syncOutcome",
          kind,
          retryAfterMs: null,
          activeItemCount: null,
          wasFullSweep: null,
          deadLettered: null,
          atMs: 61_000,
        },
      } as MessageEvent);

      const task = store.getSnapshot().task;
      expect(task.lastSyncAtMs).toBe(1_000);
      // The last outcome that actually ran is still what the badge reads.
      expect(task.lastSyncOutcome?.kind).toBe("pull_failed");
      // ...and the cycle still counted, whether or not anything currently
      // reads that count — see `TaskState.syncOutcomeSeq`'s own doc.
      expect(task.syncOutcomeSeq).toBe(2);
    },
  );

  it("leaves lastSyncAtMs null when the very first cycle of the session is skipped", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: {
        type: "syncOutcome",
        kind: "skipped",
        retryAfterMs: null,
        activeItemCount: null,
        wasFullSweep: null,
        deadLettered: null,
        atMs: 9_000,
      },
    } as MessageEvent);

    expect(store.getSnapshot().task.lastSyncAtMs).toBeNull();
    expect(store.getSnapshot().task.lastSyncOutcome).toBeNull();
  });

  it("bumps syncOutcomeSeq on EVERY cycle, even when consecutive outcomes are identical", () => {
    // Round-2 review of PR #181: until issue #191, the queue-depth /
    // dead-letter refresh effect (`useSyncWiring.ts`) was keyed on this
    // value, and `requestQueueDepth`/`requestDeadLetters` had exactly one
    // call site app-wide — so if a second steady-state cycle did NOT change
    // it, the Settings queue-depth badge would freeze after the first
    // post-ready cycle and a dead letter created later in the session (it
    // arrives inside a *completed* outcome — `deadLettered` is its own
    // field) would never surface.
    //
    // Issue #191 moved that per-cycle refresh into the worker itself (an
    // unsolicited `queueDepth`/`deadLetters` push at the tail of every
    // `runSync`, `worker/task-worker.ts`), which removed this counter's only
    // consumer in view code — see `TaskState.syncOutcomeSeq`'s own doc for
    // why it is retained anyway rather than deleted. This test still pins
    // the counter's own behaviour (two byte-identical outcomes must yield
    // two distinct seq values), independent of who currently reads it.
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);
    const steadyStateOutcome = {
      type: "syncOutcome",
      kind: "completed",
      retryAfterMs: null,
      activeItemCount: 2,
      wasFullSweep: false,
      deadLettered: 0,
      atMs: 5_000,
    };

    worker.onmessage?.({ data: steadyStateOutcome } as MessageEvent);
    const seqAfterFirst = store.getSnapshot().task.syncOutcomeSeq;
    worker.onmessage?.({ data: { ...steadyStateOutcome } } as MessageEvent);
    const seqAfterSecond = store.getSnapshot().task.syncOutcomeSeq;

    expect(seqAfterFirst).toBe(1);
    expect(seqAfterSecond).toBe(2);
    expect(seqAfterSecond).not.toBe(seqAfterFirst);
    // The outcome object itself is what it always is in the steady state —
    // the seq is the ONLY thing distinguishing cycle 2 from cycle 1.
    expect(store.getSnapshot().task.lastSyncOutcome?.kind).toBe("completed");
  });

  it("records a task-host failure so a view can say so, and stays idempotent across repeats", () => {
    // Broadcast once at construction failure AND per dropped request
    // (protocol.ts), because a view connecting later would otherwise never
    // hear it — so the same message arriving N times must be one state.
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    const message = {
      type: "taskHostUnavailable",
      message: "durable snapshot is corrupt",
    };
    worker.onmessage?.({ data: message } as MessageEvent);
    worker.onmessage?.({ data: { ...message } } as MessageEvent);

    expect(store.getSnapshot().task).toEqual({
      ...initialTask,
      hostError: "durable snapshot is corrupt",
    });
    // The calendar side is genuinely still working — #171 decoupled the two
    // on purpose, so this must not present as a whole-core failure.
    expect(store.getSnapshot().status).toBe("loading");
    expect(store.getSnapshot().error).toBeNull();
  });

  it("records the queue depth on a queueDepth message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({ data: { type: "queueDepth", depth: 3 } } as MessageEvent);

    expect(store.getSnapshot().task.queueDepth).toBe(3);
  });

  it("records the dead-letter journal on a deadLetters message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);
    const entries = [
      {
        id: "item-1",
        reason: "conflict" as const,
        message: null,
        fields: [{ field: "title", local: "buy oat milk", server: "someone else's" }],
        atMs: 1_000,
      },
    ];

    worker.onmessage?.({ data: { type: "deadLetters", entries } } as MessageEvent);

    expect(store.getSnapshot().task.deadLetters).toEqual(entries);
  });

  describe("mirrorSnapshot handling (round-1 review: never retained in the store)", () => {
    afterEach(() => {
      // Every `attachWorkerClient` call below registers a real handler on
      // the module-level slot (`worker-client.ts`'s own doc explains why it
      // is a single slot, not per-store) — left registered, it would leak
      // into whichever test runs next.
      setMirrorSnapshotHandler(null);
    });

    it("hands the mirror straight to the registered handler and writes nothing to the store", () => {
      const worker = fakeWorker();
      const store = createCoreStore();
      attachWorkerClient(worker, store);
      const received: unknown[] = [];
      setMirrorSnapshotHandler((mirror) => received.push(mirror));

      worker.onmessage?.({
        data: { type: "mirrorSnapshot", mirror: { version: 1 } },
      } as MessageEvent);

      expect(received).toEqual([{ version: 1 }]);
      expect(store.getSnapshot().task).not.toHaveProperty("mirrorSnapshot");
    });

    it("drops the mirror silently when no handler is registered — never throws, never stored", () => {
      const worker = fakeWorker();
      const store = createCoreStore();
      attachWorkerClient(worker, store);

      expect(() =>
        worker.onmessage?.({
          data: { type: "mirrorSnapshot", mirror: { version: 1 } },
        } as MessageEvent),
      ).not.toThrow();
    });

    it("a later registration replaces the earlier one rather than accumulating", () => {
      const worker = fakeWorker();
      const store = createCoreStore();
      attachWorkerClient(worker, store);
      const first = vi.fn();
      const second = vi.fn();
      setMirrorSnapshotHandler(first);
      setMirrorSnapshotHandler(second);

      worker.onmessage?.({
        data: { type: "mirrorSnapshot", mirror: { version: 1 } },
      } as MessageEvent);

      expect(first).not.toHaveBeenCalled();
      expect(second).toHaveBeenCalledWith({ version: 1 });
    });
  });

  it("flags task needsReconnect on a taskEvents message carrying a credential_needed event", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({
      data: { type: "taskEvents", events: [{ kind: "credential_needed", atMs: 1_000 }] },
    } as MessageEvent);

    expect(store.getSnapshot().task.needsReconnect).toBe(true);
  });

  it("does not flag task needsReconnect on an empty taskEvents message", () => {
    const worker = fakeWorker();
    const store = createCoreStore();
    attachWorkerClient(worker, store);

    worker.onmessage?.({ data: { type: "taskEvents", events: [] } } as MessageEvent);

    expect(store.getSnapshot().task.needsReconnect).toBe(false);
  });
});

describe("the calendar send helpers", () => {
  it("pushTokenToWorker posts a pushToken request", () => {
    const worker = fakeWorker();
    pushTokenToWorker(worker, "tok-1");
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "pushToken",
      token: "tok-1",
    });
  });

  it("setCalendarSelectionsOnWorker posts a setCalendarSelections request", () => {
    const worker = fakeWorker();
    setCalendarSelectionsOnWorker(worker, [
      { id: "a", horizon: "standard" },
      { id: "b", horizon: "long" },
    ]);
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "setCalendarSelections",
      selections: [
        { id: "a", horizon: "standard" },
        { id: "b", horizon: "long" },
      ],
    });
  });

  it("pollStart/pollRefresh/pollTimer post their matching request", () => {
    const worker = fakeWorker();

    pollStart(worker, 1);
    pollRefresh(worker, 2);
    pollTimer(worker, 3);

    expect(worker.postMessage).toHaveBeenNthCalledWith(1, {
      type: "pollStart",
      nowMs: 1,
    });
    expect(worker.postMessage).toHaveBeenNthCalledWith(2, {
      type: "pollRefresh",
      nowMs: 2,
    });
    expect(worker.postMessage).toHaveBeenNthCalledWith(3, {
      type: "pollTimer",
      nowMs: 3,
    });
  });

  it("requestPaneRead posts the source and the clock its answer is measured against", () => {
    const worker = fakeWorker();
    requestPaneRead(worker, "city-waste/v2", 61_000);
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "getPaneRead",
      source: "city-waste/v2",
      nowMs: 61_000,
    });
  });

  it("requestCalendarList posts a listCalendars request with no token", () => {
    // The core lists with the credential it was already pushed (ADR-0003:
    // the core owns HTTP), so this side never handles a token for it.
    const worker = fakeWorker();
    requestCalendarList(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "listCalendars" });
  });

  it("requestCalendarEvents posts the request's key, both interval shapes and the clock", () => {
    const worker = fakeWorker();
    requestCalendarEvents(worker, "weekend", 1_000, 2_000, "2026-08-14", "2026-08-17", 1_500);
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "getCalendarEvents",
      key: "weekend",
      startMs: 1_000,
      endMs: 2_000,
      startDate: "2026-08-14",
      endDate: "2026-08-17",
      nowMs: 1_500,
    });
  });
});

describe("the task send helpers (#105/S7)", () => {
  it("pushTaskApiKey posts a pushTaskApiKey request", () => {
    const worker = fakeWorker();
    pushTaskApiKey(worker, "device-token-1");
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "pushTaskApiKey",
      apiKey: "device-token-1",
    });
  });

  it("initTaskApiKey posts an initTaskApiKey request (issue #196's rehydration path)", () => {
    const worker = fakeWorker();
    initTaskApiKey(worker, "device-token-1");
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "initTaskApiKey",
      apiKey: "device-token-1",
    });
  });

  it("clearTaskApiKey posts a clearTaskApiKey request carrying nothing", () => {
    const worker = fakeWorker();
    clearTaskApiKey(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "clearTaskApiKey" });
  });

  it("captureTask posts a capture request carrying its seed, with every field absent by default", () => {
    const worker = fakeWorker();
    captureTask(worker, "seed-1", "buy milk", "ready", 1_000);
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "capture",
      seed: "seed-1",
      title: "buy milk",
      stage: "ready",
      // Every key present and null: the caller's shape is optional, the
      // wire's is nullable-required, and this is where the one becomes the
      // other — a field the caller omits must still be spelled out as unset
      // rather than left off the message.
      fields: {
        size: null,
        energy: null,
        context: null,
        description: null,
        projectId: null,
        priority: null,
        deadline: null,
        scheduledDate: null,
      },
      nowMs: 1_000,
    });
  });

  // #208: a caller-supplied `fields` reaches the wire message verbatim.
  it("captureTask posts every set field verbatim, and nulls the rest", () => {
    const worker = fakeWorker();
    captureTask(worker, "seed-1", "buy milk", "ready", 1_000, {
      size: "deep",
      energy: "high",
      context: "@errands",
      description: "the oat kind",
      projectId: "proj-1",
      priority: 3,
      deadline: "2026-09-01T09:30",
      scheduledDate: "2026-08-30",
    });
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "capture",
      seed: "seed-1",
      title: "buy milk",
      stage: "ready",
      fields: {
        size: "deep",
        energy: "high",
        context: "@errands",
        description: "the oat kind",
        projectId: "proj-1",
        priority: 3,
        deadline: "2026-09-01T09:30",
        scheduledDate: "2026-08-30",
      },
      nowMs: 1_000,
    });
  });

  it("actOnTask posts an act request carrying its seed, item and action", () => {
    const worker = fakeWorker();
    actOnTask(worker, "seed-act-1", "item-1", "block", 2_000);
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "act",
      seed: "seed-act-1",
      itemId: "item-1",
      action: "block",
      nowMs: 2_000,
    });
  });

  it("triageItem posts a null destination and a set scheduledDate edit (#122's do-date write)", () => {
    const worker = fakeWorker();
    triageItem(worker, "seed-triage-9", "item-1", null, { scheduledDate: "2026-08-15" }, 2_000);
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "triage",
      seed: "seed-triage-9",
      itemId: "item-1",
      destination: null,
      edits: { scheduledDate: "2026-08-15" },
      nowMs: 2_000,
    });
  });

  it("triageItem posts a clear scheduledDate edit distinguishably from an untouched field", () => {
    const worker = fakeWorker();
    triageItem(worker, "seed-triage-10", "item-1", null, { scheduledDate: null }, 2_000);
    const call = worker.postMessage.mock.calls[0][0] as { edits?: { scheduledDate?: unknown } };
    expect(call.edits?.scheduledDate).toBeNull();

    worker.postMessage.mockClear();
    triageItem(worker, "seed-triage-11", "item-1", null, {}, 2_000);
    const untouchedCall = worker.postMessage.mock.calls[0][0] as { edits?: { scheduledDate?: unknown } };
    expect(untouchedCall.edits?.scheduledDate).toBeUndefined();
  });

  it("requestFrontier/requestTriageInbox/requestIsPending post their matching request", () => {
    const worker = fakeWorker();

    requestFrontier(worker);
    requestTriageInbox(worker);
    requestIsPending(worker, "item-1");

    expect(worker.postMessage).toHaveBeenNthCalledWith(1, { type: "getFrontier" });
    expect(worker.postMessage).toHaveBeenNthCalledWith(2, { type: "getTriageInbox" });
    expect(worker.postMessage).toHaveBeenNthCalledWith(3, {
      type: "isPending",
      itemId: "item-1",
    });
  });

  it("requestBlocked posts a getBlocked request", () => {
    const worker = fakeWorker();
    requestBlocked(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getBlocked" });
  });

  it("requestSteps posts a getSteps request carrying the item id", () => {
    const worker = fakeWorker();
    requestSteps(worker, "item-1");
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getSteps", itemId: "item-1" });
  });

  it("saveGrillDraft posts a saveGrillDraft request carrying the item id, turns and clock", () => {
    const worker = fakeWorker();
    const turns = [{ question: { prompt: "p", recommendedAnswer: "r", choices: [] }, answer: "a" }];
    saveGrillDraft(worker, "item-1", turns, 1_000);
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "saveGrillDraft",
      itemId: "item-1",
      turns,
      nowMs: 1_000,
    });
  });

  it("discardGrillDraft posts a discardGrillDraft request carrying the item id and clock", () => {
    const worker = fakeWorker();
    discardGrillDraft(worker, "item-1", 2_000);
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "discardGrillDraft",
      itemId: "item-1",
      nowMs: 2_000,
    });
  });

  it("requestGrillDraft posts a getGrillDraft request carrying the item id", () => {
    const worker = fakeWorker();
    requestGrillDraft(worker, "item-1");
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getGrillDraft", itemId: "item-1" });
  });

  it("requestGrillDraftItemIds posts a getGrillDraftItemIds request", () => {
    const worker = fakeWorker();
    requestGrillDraftItemIds(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getGrillDraftItemIds" });
  });

  it("requestProjects posts a getProjects request", () => {
    const worker = fakeWorker();
    requestProjects(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getProjects" });
  });

  it("requestQueueDepth posts a getQueueDepth request", () => {
    const worker = fakeWorker();
    requestQueueDepth(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getQueueDepth" });
  });

  it("requestDeadLetters posts a getDeadLetters request", () => {
    const worker = fakeWorker();
    requestDeadLetters(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getDeadLetters" });
  });

  it("requestMirrorSnapshot posts a getMirrorSnapshot request", () => {
    const worker = fakeWorker();
    requestMirrorSnapshot(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "getMirrorSnapshot" });
  });

  it("reportViewVisibility posts a setViewVisibility request with this view's own hidden state", () => {
    const worker = fakeWorker();
    reportViewVisibility(worker, true);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "setViewVisibility", hidden: true });

    reportViewVisibility(worker, false);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "setViewVisibility", hidden: false });
  });

  it("triggerSyncFocus posts a syncFocusTrigger request", () => {
    const worker = fakeWorker();
    triggerSyncFocus(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "syncFocusTrigger" });
  });

  it("triggerSyncManual posts a manualSyncTrigger request", () => {
    const worker = fakeWorker();
    triggerSyncManual(worker);
    expect(worker.postMessage).toHaveBeenCalledWith({ type: "manualSyncTrigger" });
  });
});
