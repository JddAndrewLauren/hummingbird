import { describe, expect, it, vi } from "vitest";
import { type CalendarState, type TaskState, coreStore, createCoreStore } from "./store";

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
  routeByProject: {},
  lastRouteWrite: null,
  fogByProject: {},
  lastFogWrite: null,
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

describe("createCoreStore", () => {
  it("starts in the loading state with no api version and no error", () => {
    const store = createCoreStore();

    expect(store.getSnapshot()).toEqual({
      status: "loading",
      apiVersion: null,
      coreId: null,
      viewOrdinal: null,
      error: null,
      calendar: initialCalendar,
      task: initialTask,
    });
  });

  it("notifies subscribers when state changes", () => {
    const store = createCoreStore();
    const listener = vi.fn();
    store.subscribe(listener);

    store.setState({ status: "ready", apiVersion: 1 });

    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("merges partial patches into the existing snapshot", () => {
    const store = createCoreStore();

    store.setState({ status: "ready", apiVersion: 1 });

    expect(store.getSnapshot()).toEqual({
      status: "ready",
      apiVersion: 1,
      coreId: null,
      viewOrdinal: null,
      error: null,
      calendar: initialCalendar,
      task: initialTask,
    });
  });

  it("setCalendarState merges into the calendar slice without touching the rest", () => {
    const store = createCoreStore();
    store.setState({ status: "ready", apiVersion: 1 });

    store.setCalendarState({ connected: true, selectedCalendarIds: ["primary"] });

    expect(store.getSnapshot()).toEqual({
      status: "ready",
      apiVersion: 1,
      coreId: null,
      viewOrdinal: null,
      error: null,
      calendar: {
        ...initialCalendar,
        connected: true,
        selectedCalendarIds: ["primary"],
      },
      task: initialTask,
    });
  });

  it("setCalendarEventRead writes into eventReads keyed by request, leaving other keys alone", () => {
    const store = createCoreStore();
    store.setState({ status: "ready", apiVersion: 1 });

    store.setCalendarEventRead("weekend", { state: "not_read" });
    store.setCalendarEventRead("today", {
      state: "read",
      events: [],
      freshness: { kind: "unknown" },
    });

    expect(store.getSnapshot().calendar.eventReads).toEqual({
      weekend: { state: "not_read" },
      today: { state: "read", events: [], freshness: { kind: "unknown" } },
    });
  });

  it("setTaskState merges into the task slice without touching the rest", () => {
    const store = createCoreStore();
    store.setState({ status: "ready", apiVersion: 1 });

    store.setTaskState({
      frontier: [
        {
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
        },
      ],
    });

    expect(store.getSnapshot().task.frontier).toHaveLength(1);
    expect(store.getSnapshot().calendar).toEqual(initialCalendar);
  });

  it("setTaskPending merges one item into the task slice's pending map, leaving the rest untouched", () => {
    const store = createCoreStore();
    store.setTaskPending("item-1", true);
    store.setTaskPending("item-2", false);

    expect(store.getSnapshot().task.pending).toEqual({ "item-1": true, "item-2": false });

    store.setTaskPending("item-1", false);

    expect(store.getSnapshot().task.pending).toEqual({ "item-1": false, "item-2": false });
  });

  it("stops notifying a listener once it unsubscribes", () => {
    const store = createCoreStore();
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);
    unsubscribe();

    store.setState({ status: "error", error: "boom" });

    expect(listener).not.toHaveBeenCalled();
  });

  it("exposes a stable, module-level subscribe reference on the singleton store", () => {
    // useSyncExternalStore requires a stable `subscribe` function identity
    // across renders, or React will resubscribe (and can loop) on every
    // render. `coreStore` is the one module-level singleton useStore reads.
    const first = coreStore.subscribe;
    const second = coreStore.subscribe;
    expect(first).toBe(second);
  });
});
