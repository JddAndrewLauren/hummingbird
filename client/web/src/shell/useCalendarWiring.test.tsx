// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "../test/component";
import { ROTATION_MARGIN_MS } from "../calendar/connection";
import {
  readConnected,
  readSelectedCalendarIds,
  writeConnected,
  writeSelectedCalendarIds,
  type StorageLike,
} from "../calendar/persistence";
import type { TokenClient, TokenError, TokenPrompt, TokenResult } from "../calendar/token-client";
import { coreStore, type CalendarState } from "../store/store";
import type { WorkerLike } from "../store/worker-client";
import { useCalendarWiring, type CalendarWiringDeps } from "./useCalendarWiring";

// #750: `useCalendarWiring` was the only `use*Wiring` hook in the shell with
// no test file — the Agent Brief's approach (see the issue) is a trailing
// optional `deps` parameter, so this file drives the whole lifecycle against
// fakes for the token client and `localStorage`, never touching `fetch`,
// IndexedDB or the module-level singletons `defaultCalendarWiringDeps`
// builds. Every test builds its own fakes and its own `deps` — nothing here
// depends on another test having run first, which is the point: the module
// state this hook used to route through unconditionally is exactly the
// hazard the Agent Brief calls out.

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

function fakeStorage(): StorageLike {
  const store = new Map<string, string>();
  return {
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => {
      store.set(key, value);
    },
    removeItem: (key) => {
      store.delete(key);
    },
  };
}

/** A `TokenClient` driven off a queue of canned results, one per call — the
 * last entry repeats once the queue runs out, so a test that only cares
 * about the first N calls can supply exactly N. Every call's `prompt` is
 * recorded, so a test can assert silent (`"none"`) vs. interactive
 * (`"consent"`). */
function fakeTokenClient(
  results: readonly (TokenResult | TokenError)[],
): TokenClient & { calls: TokenPrompt[] } {
  const calls: TokenPrompt[] = [];
  return {
    calls,
    requestToken: (prompt) => {
      calls.push(prompt);
      const result = results[Math.min(calls.length - 1, results.length - 1)];
      return Promise.resolve(result);
    },
  };
}

/** The deps shape a real call site gets: a `pushToken` that actually reaches
 * the worker, same as `connectionDeps(worker)` in the hook itself. */
function connectionDepsFor(
  worker: WorkerLike,
  tokenClient: TokenClient,
  storage: StorageLike = fakeStorage(),
): CalendarWiringDeps {
  return {
    connection: { tokenClient, pushToken: (token) => worker.postMessage({ type: "pushToken", token }) },
    storage,
  };
}

function calendarState(overrides: Partial<CalendarState> = {}): CalendarState {
  return {
    connected: false,
    needsReconnect: false,
    selectedCalendarIds: [],
    availableCalendars: [],
    lastPollOutcome: null,
    connectPending: false,
    connectError: null,
    silentRemintBlocked: false,
    eventReads: {},
    ...overrides,
  };
}

function types(worker: ReturnType<typeof fakeWorker>): string[] {
  return worker.postMessage.mock.calls.map(([message]) => (message as { type: string }).type);
}

function mount(
  worker: WorkerLike,
  calendar: CalendarState,
  deps: CalendarWiringDeps,
  tripsCalendarId: string | null = null,
) {
  return renderHook(
    ({ calendar: cal }) => useCalendarWiring(worker, "ready", cal, tripsCalendarId, deps),
    { initialProps: { calendar } },
  );
}

afterEach(() => {
  vi.useRealTimers();
});

describe("useCalendarWiring: core-start silent re-mint", () => {
  it("a never-connected device attempts no re-mint and stays disconnected", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ accessToken: "should-not-be-used", expiresAtMs: 1_000 }]);
    const deps = connectionDepsFor(worker, tokenClient);
    mount(worker, calendarState(), deps);
    await act(async () => {});

    expect(tokenClient.calls).toEqual([]);
    expect(coreStore.getSnapshot().calendar.connected).toBe(false);
    expect(readConnected(deps.storage)).toBe(false);
  });

  it("a previously-connected device silently re-mints and comes up connected", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ accessToken: "token-1", expiresAtMs: 5_000 }]);
    const storage = fakeStorage();
    writeConnected(storage, true);
    writeSelectedCalendarIds(storage, ["cal-1"]);
    const deps = connectionDepsFor(worker, tokenClient, storage);

    mount(worker, calendarState(), deps);
    await act(async () => {});

    expect(tokenClient.calls).toEqual(["none"]);
    expect(coreStore.getSnapshot().calendar).toMatchObject({ connected: true, needsReconnect: false });
    expect(readConnected(deps.storage)).toBe(true);
    expect(types(worker)).toEqual(
      expect.arrayContaining(["pushToken", "setCalendarSelections", "pollStart", "listCalendars"]),
    );
  });

  it("a previously-connected device whose re-mint fails stays needsReconnect, without blocking on one failure", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ error: "authority_rejected_device_token" }]);
    const storage = fakeStorage();
    writeConnected(storage, true);
    const deps = connectionDepsFor(worker, tokenClient, storage);

    mount(worker, calendarState(), deps);
    await act(async () => {});

    expect(coreStore.getSnapshot().calendar).toMatchObject({ connected: true, needsReconnect: true });
    // One blocking failure is not two: `SILENT_REMINT_FAILURE_LIMIT` is 2,
    // so `silentRemintBlocked` must not flip yet.
    expect(coreStore.getSnapshot().calendar.silentRemintBlocked).toBe(false);
  });
});

describe("useCalendarWiring: credential-needed round trip", () => {
  it("recovers via a silent re-mint and re-polls", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ accessToken: "fresh-token", expiresAtMs: 9_000 }]);
    const deps = connectionDepsFor(worker, tokenClient);

    mount(worker, calendarState({ needsReconnect: true, silentRemintBlocked: false }), deps);
    await act(async () => {});

    expect(tokenClient.calls).toEqual(["none"]);
    expect(coreStore.getSnapshot().calendar).toMatchObject({ connected: true, needsReconnect: false });
    expect(types(worker)).toEqual(
      expect.arrayContaining(["pushToken", "setCalendarSelections", "pollRefresh", "listCalendars"]),
    );
  });

  it("stays blocked and never calls the token client while silentRemintBlocked", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ accessToken: "should-not-be-used", expiresAtMs: 9_000 }]);
    const deps = connectionDepsFor(worker, tokenClient);

    mount(worker, calendarState({ needsReconnect: true, silentRemintBlocked: true }), deps);
    await act(async () => {});

    expect(tokenClient.calls).toEqual([]);
    expect(worker.postMessage).not.toHaveBeenCalled();
  });
});

describe("useCalendarWiring: handleConnectClick", () => {
  it("a first-time connect succeeds and starts polling", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ accessToken: "token-1", expiresAtMs: 5_000 }]);
    const deps = connectionDepsFor(worker, tokenClient);
    const { result } = mount(worker, calendarState({ connected: false }), deps);
    await act(async () => {});
    worker.postMessage.mockClear();

    await act(async () => {
      await result.current.handleConnectClick();
    });

    expect(tokenClient.calls).toEqual(["consent"]);
    expect(coreStore.getSnapshot().calendar).toMatchObject({ connected: true, needsReconnect: false });
    expect(readConnected(deps.storage)).toBe(true);
    expect(types(worker)).toEqual(
      expect.arrayContaining(["pushToken", "setCalendarSelections", "pollStart", "listCalendars"]),
    );
  });

  it("a reconnect that succeeds re-asserts the existing selection and re-polls", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ accessToken: "token-2", expiresAtMs: 6_000 }]);
    const deps = connectionDepsFor(worker, tokenClient);
    const { result } = mount(
      worker,
      calendarState({ connected: true, selectedCalendarIds: ["cal-1"] }),
      deps,
    );
    await act(async () => {});
    worker.postMessage.mockClear();

    await act(async () => {
      await result.current.handleConnectClick();
    });

    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "setCalendarSelections",
      selections: [{ id: "cal-1", horizon: "standard" }],
    });
  });

  it("a reconnect that fails keeps the existing connection (the early return) but still records the error", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ error: "authority_unreachable" }]);
    const deps = connectionDepsFor(worker, tokenClient);
    const { result } = mount(worker, calendarState({ connected: true }), deps);
    await act(async () => {});
    // The core-start effect (a different concern, exercised above) writes
    // its own outcome to `coreStore` first; set the "was already connected"
    // fact this test is actually about only after that settles.
    coreStore.setCalendarState({ connected: true, needsReconnect: false });
    worker.postMessage.mockClear();

    await act(async () => {
      await result.current.handleConnectClick();
    });

    // `shouldKeepExistingConnection` took the early return: connected /
    // needsReconnect are untouched from what they were before the press.
    expect(coreStore.getSnapshot().calendar).toMatchObject({ connected: true, needsReconnect: false });
    expect(coreStore.getSnapshot().calendar.connectError).toBe("authority_unreachable");
    // The early return skips the reconnect's own poll/push entirely.
    expect(worker.postMessage).not.toHaveBeenCalled();
  });
});

describe("useCalendarWiring: handleCalendarSelectionChange", () => {
  it("persists the new selection, updates the store, and re-polls", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([]);
    const deps = connectionDepsFor(worker, tokenClient);
    const { result } = mount(worker, calendarState({ connected: true }), deps);
    await act(async () => {});
    worker.postMessage.mockClear();

    act(() => {
      result.current.handleCalendarSelectionChange(["cal-1", "cal-2"]);
    });

    expect(readSelectedCalendarIds(deps.storage)).toEqual(["cal-1", "cal-2"]);
    expect(coreStore.getSnapshot().calendar.selectedCalendarIds).toEqual(["cal-1", "cal-2"]);
    expect(types(worker)).toEqual(expect.arrayContaining(["setCalendarSelections", "pollRefresh"]));
  });

  it("refuses a change that unticks the locked Trips calendar", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([]);
    const deps = connectionDepsFor(worker, tokenClient);
    const { result } = mount(worker, calendarState({ connected: true, selectedCalendarIds: ["trips-1"] }), deps, "trips-1");
    await act(async () => {});
    // Same reason as the reconnect-fails test above: settle the core-start
    // effect's own write first, then set the fact this test checks stays
    // untouched by the refusal.
    coreStore.setCalendarState({ selectedCalendarIds: ["trips-1"] });
    worker.postMessage.mockClear();

    act(() => {
      // A request that omits the bound Trips calendar — the picker's
      // locked row, sprung back rather than accepted.
      result.current.handleCalendarSelectionChange([]);
    });

    expect(readSelectedCalendarIds(deps.storage)).toEqual([]);
    expect(coreStore.getSnapshot().calendar.selectedCalendarIds).toEqual(["trips-1"]);
    expect(worker.postMessage).not.toHaveBeenCalled();
  });
});

describe("useCalendarWiring: handleRefreshClick", () => {
  it("re-asserts the selection and re-polls", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([]);
    const deps = connectionDepsFor(worker, tokenClient);
    const { result } = mount(
      worker,
      calendarState({ connected: true, selectedCalendarIds: ["cal-1"] }),
      deps,
    );
    await act(async () => {});
    worker.postMessage.mockClear();

    act(() => {
      result.current.handleRefreshClick();
    });

    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "setCalendarSelections",
      selections: [{ id: "cal-1", horizon: "standard" }],
    });
    expect(types(worker)).toEqual(expect.arrayContaining(["pollRefresh"]));
  });
});

describe("useCalendarWiring: timers", () => {
  it("proactively re-mints ahead of expiry via the rotation setTimeout", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const worker = fakeWorker();
    const expiresAtMs = 60 * 60 * 1000;
    const tokenClient = fakeTokenClient([
      { accessToken: "initial-token", expiresAtMs },
      { accessToken: "rotated-token", expiresAtMs: expiresAtMs + 3_600_000 },
    ]);
    const storage = fakeStorage();
    writeConnected(storage, true);
    const deps = connectionDepsFor(worker, tokenClient, storage);

    // `connected: true` from the first render: the rotation effect gates on
    // the `calendar` PROP (never the core-start effect's own `coreStore`
    // write, which this hook never reads back), so a test that wants the
    // timer armed has to supply it directly — the same way `App.tsx` feeds
    // this hook the store's own last-broadcast snapshot.
    mount(worker, calendarState({ connected: true }), deps);
    await act(async () => {});
    expect(tokenClient.calls).toEqual(["none"]);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(expiresAtMs - ROTATION_MARGIN_MS);
    });

    expect(tokenClient.calls).toEqual(["none", "none"]);
    expect(coreStore.getSnapshot().calendar).toMatchObject({ connected: true, needsReconnect: false });
  });

  it("polls again every 15 minutes while connected and foregrounded", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([]);
    const deps = connectionDepsFor(worker, tokenClient);

    mount(worker, calendarState({ connected: true, selectedCalendarIds: ["cal-1"] }), deps);
    await act(async () => {});
    worker.postMessage.mockClear();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(15 * 60 * 1000);
    });

    expect(types(worker)).toContain("pollTimer");
  });
});
