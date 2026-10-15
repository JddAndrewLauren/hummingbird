// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "../test/component";
import { ROTATION_MARGIN_MS } from "../calendar/connection";
import { readLocalCalendarPrefs, type StorageLike } from "../calendar/persistence";
import type { TokenClient, TokenError, TokenPrompt, TokenResult } from "../calendar/token-client";
import type { CalendarPrefsDTO, CalendarSelectionDTO } from "../store/protocol";
import { coreStore, type CalendarState, type TaskState } from "../store/store";
import type { WorkerLike } from "../store/worker-client";
import { useCalendarWiring, type CalendarWiringDeps } from "./useCalendarWiring";

// #750: `useCalendarWiring` was the only `use*Wiring` hook in the shell with
// no test file — the Agent Brief's approach (see the issue) is a trailing
// optional `deps` parameter, so this file drives the whole lifecycle against
// fakes for the token client and `localStorage`, never touching `fetch`,
// IndexedDB or the module-level singletons `defaultCalendarWiringDeps`
// builds. Since #843 (ADR-0040 decision 8) what to poll arrives as a prop —
// the synced `calendarPrefs` — and `localStorage` holds only the pre-#843
// copy the one-time migration seeds and clears.
//
// (#750's original note:
// every test builds its own fakes and its own `deps` — nothing here
// depends on another test having run first, which is the point: the module
// state this hook used to route through unconditionally is exactly the
// hazard the Agent Brief calls out.

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

function fakeStorage(entries: Record<string, string> = {}): StorageLike {
  const store = new Map<string, string>(Object.entries(entries));
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

/** The synced calendar choices as the core answers them. */
function prefs(connected: boolean, selection: CalendarSelectionDTO[] = []): CalendarPrefsDTO {
  return { connected, selection, pending: false };
}

interface Props {
  calendar: CalendarState;
  prefs: CalendarPrefsDTO | null;
  seed: TaskState["lastCalendarSeed"];
  trips: string | null;
}

function mount(
  worker: WorkerLike,
  calendar: CalendarState,
  deps: CalendarWiringDeps,
  initial: Partial<Omit<Props, "calendar">> = {},
) {
  const initialProps: Props = {
    calendar,
    prefs: initial.prefs === undefined ? prefs(false) : initial.prefs,
    seed: initial.seed ?? null,
    trips: initial.trips ?? null,
  };
  return renderHook(
    (props: Props) =>
      useCalendarWiring(worker, "ready", props.calendar, props.trips, props.prefs, props.seed, deps),
    { initialProps },
  );
}

afterEach(() => {
  vi.useRealTimers();
});

describe("useCalendarWiring: the lane follows the synced connected row", () => {
  it("a not-connected workspace attempts no re-mint and stays disconnected", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ accessToken: "should-not-be-used", expiresAtMs: 1_000 }]);
    mount(worker, calendarState(), connectionDepsFor(worker, tokenClient));
    await act(async () => {});

    expect(tokenClient.calls).toEqual([]);
    expect(coreStore.getSnapshot().calendar.connected).toBe(false);
  });

  it("does nothing at all before the choices have been read", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ accessToken: "should-not-be-used", expiresAtMs: 1_000 }]);
    mount(worker, calendarState(), connectionDepsFor(worker, tokenClient), { prefs: null });
    await act(async () => {});

    expect(tokenClient.calls).toEqual([]);
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("a connected workspace silently re-mints and polls the core's polled set", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ accessToken: "token-1", expiresAtMs: 5_000 }]);
    mount(worker, calendarState(), connectionDepsFor(worker, tokenClient), {
      prefs: prefs(true, [{ id: "family@g", horizon: "long" }]),
      trips: "trips@g",
    });
    await act(async () => {});

    expect(tokenClient.calls).toEqual(["none"]);
    expect(coreStore.getSnapshot().calendar).toMatchObject({ connected: true, needsReconnect: false });
    // The phone's long horizon survives, and the bound Trips calendar is
    // appended by the core's rule — nothing re-derived here.
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "setCalendarSelections",
      selections: [
        { id: "family@g", horizon: "long" },
        { id: "trips@g", horizon: "long" },
      ],
    });
    expect(types(worker)).toEqual(
      expect.arrayContaining(["pushToken", "setCalendarSelections", "pollStart", "listCalendars"]),
    );
  });

  it("a re-mint that fails stays needsReconnect, without blocking on one failure", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ error: "authority_rejected_device_token" }]);
    mount(worker, calendarState(), connectionDepsFor(worker, tokenClient), { prefs: prefs(true) });
    await act(async () => {});

    expect(coreStore.getSnapshot().calendar).toMatchObject({ connected: true, needsReconnect: true });
    // One blocking failure is not two: `SILENT_REMINT_FAILURE_LIMIT` is 2,
    // so `silentRemintBlocked` must not flip yet.
    expect(coreStore.getSnapshot().calendar.silentRemintBlocked).toBe(false);
  });

  it("arms the moment another device connects, and parks when one disconnects", async () => {
    const worker = fakeWorker();
    // A far-future expiry, so the rotation timer never fires inside this
    // test: an already-past one fires it at once and races the park below
    // (the in-flight ordering itself is pinned under "timers").
    const tokenClient = fakeTokenClient([
      { accessToken: "token-1", expiresAtMs: Date.now() + 60 * 60 * 1000 },
    ]);
    const { rerender } = mount(worker, calendarState(), connectionDepsFor(worker, tokenClient));
    await act(async () => {});
    expect(tokenClient.calls).toEqual([]);

    rerender({ calendar: calendarState(), prefs: prefs(true), seed: null, trips: null });
    await act(async () => {});
    expect(tokenClient.calls).toEqual(["none"]);
    expect(coreStore.getSnapshot().calendar.connected).toBe(true);

    rerender({
      calendar: calendarState({ connected: true }),
      prefs: prefs(false),
      seed: null,
      trips: null,
    });
    await act(async () => {});
    expect(coreStore.getSnapshot().calendar).toMatchObject({ connected: false, needsReconnect: false });
  });
});

describe("useCalendarWiring: the one-time migration of the old local copy", () => {
  const oldCopy = {
    "hb.calendar.connected": "true",
    "hb.calendar.selectedCalendarIds": JSON.stringify(["cal-1"]),
  };

  it("hands the old values to the core's seed, once the choices have been read", async () => {
    const worker = fakeWorker();
    const storage = fakeStorage(oldCopy);
    const { rerender } = mount(
      worker,
      calendarState(),
      connectionDepsFor(worker, fakeTokenClient([]), storage),
      { prefs: null },
    );
    await act(async () => {});
    expect(types(worker)).not.toContain("seedCalendarPrefs");

    rerender({ calendar: calendarState(), prefs: prefs(false), seed: null, trips: null });
    await act(async () => {});
    expect(worker.postMessage).toHaveBeenCalledWith(
      expect.objectContaining({ type: "seedCalendarPrefs", connected: true, selectedIds: ["cal-1"] }),
    );
  });

  it("keeps the old copy on not-yet, retries on the next answer, and deletes it once settled", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ accessToken: "token-1", expiresAtMs: 5_000 }]);
    const storage = fakeStorage(oldCopy);
    const { rerender } = mount(worker, calendarState(), connectionDepsFor(worker, tokenClient, storage));
    await act(async () => {});
    const firstSeed = worker.postMessage.mock.calls
      .map(([message]) => message as { type: string; seed?: string })
      .find((message) => message.type === "seedCalendarPrefs")!.seed!;

    // Not swept yet: the copy stays.
    rerender({
      calendar: calendarState(),
      prefs: prefs(false),
      seed: { seed: firstSeed, outcome: "not_yet" },
      trips: null,
    });
    await act(async () => {});
    expect(readLocalCalendarPrefs(storage)).not.toBeNull();

    // The next cycle's answer re-attempts.
    worker.postMessage.mockClear();
    vi.useFakeTimers();
    vi.setSystemTime(10_000);
    rerender({
      calendar: calendarState(),
      prefs: prefs(false),
      seed: { seed: firstSeed, outcome: "not_yet" },
      trips: null,
    });
    await act(async () => {});
    vi.useRealTimers();
    const retry = worker.postMessage.mock.calls
      .map(([message]) => message as { type: string; seed?: string })
      .find((message) => message.type === "seedCalendarPrefs");
    expect(retry).toBeDefined();

    // Settled: the copy is deleted, and the seeded row arms the lane.
    rerender({
      calendar: calendarState(),
      prefs: prefs(true, [{ id: "cal-1", horizon: "standard" }]),
      seed: { seed: retry!.seed!, outcome: "seeded" },
      trips: null,
    });
    await act(async () => {});
    expect(readLocalCalendarPrefs(storage)).toBeNull();
    expect(tokenClient.calls).toEqual(["none"]);
  });

  it("sends no seed from a browser that holds no old copy", async () => {
    const worker = fakeWorker();
    mount(worker, calendarState(), connectionDepsFor(worker, fakeTokenClient([])));
    await act(async () => {});
    expect(types(worker)).not.toContain("seedCalendarPrefs");
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
  it("a first-time connect succeeds, records the synced row and starts polling", async () => {
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
    // Connecting here connects every device (ADR-0040 decision 8).
    expect(worker.postMessage).toHaveBeenCalledWith(
      expect.objectContaining({ type: "setCalendarConnected", connected: true }),
    );
    expect(types(worker)).toEqual(
      expect.arrayContaining(["pushToken", "setCalendarSelections", "pollStart", "listCalendars"]),
    );
  });

  it("a reconnect that succeeds re-asserts the synced selection, and writes no row already there", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ accessToken: "token-2", expiresAtMs: 6_000 }]);
    const deps = connectionDepsFor(worker, tokenClient);
    const { result } = mount(worker, calendarState({ connected: true }), deps, {
      prefs: prefs(true, [{ id: "cal-1", horizon: "standard" }]),
    });
    await act(async () => {});
    worker.postMessage.mockClear();

    await act(async () => {
      await result.current.handleConnectClick();
    });

    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "setCalendarSelections",
      selections: [{ id: "cal-1", horizon: "standard" }],
    });
    expect(types(worker)).not.toContain("setCalendarConnected");
  });

  it("a first connect that fails writes nothing synced", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ error: "authority_unreachable" }]);
    const { result } = mount(worker, calendarState(), connectionDepsFor(worker, tokenClient));
    await act(async () => {});

    await act(async () => {
      await result.current.handleConnectClick();
    });

    expect(types(worker)).not.toContain("setCalendarConnected");
    expect(coreStore.getSnapshot().calendar.connected).toBe(false);
  });

  it("a reconnect that fails keeps the existing connection (the early return) but still records the error", async () => {
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([{ error: "authority_unreachable" }]);
    const deps = connectionDepsFor(worker, tokenClient);
    const { result } = mount(worker, calendarState({ connected: true }), deps, { prefs: prefs(true) });
    await act(async () => {});
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

describe("useCalendarWiring: ticking a calendar", () => {
  it("sends one id to the core and nothing else — the re-push follows the answer", async () => {
    const worker = fakeWorker();
    const { result } = mount(
      worker,
      calendarState({ connected: true }),
      connectionDepsFor(worker, fakeTokenClient([])),
      { prefs: prefs(true) },
    );
    await act(async () => {});
    worker.postMessage.mockClear();

    act(() => {
      result.current.handleCalendarToggle("cal-2");
    });

    expect(worker.postMessage).toHaveBeenCalledTimes(1);
    expect(worker.postMessage).toHaveBeenCalledWith(
      expect.objectContaining({ type: "toggleCalendar", id: "cal-2" }),
    );
  });

  it("re-pushes and polls when the synced selection moves — ticked here or on any device", async () => {
    // The browser half of the `calendar-follows` check: a calendar ticked
    // on the phone arrives as a new `calendarPrefs` answer after a sync,
    // and is fetched now rather than at the next 15-minute tick.
    const worker = fakeWorker();
    const { rerender } = mount(
      worker,
      calendarState({ connected: true }),
      connectionDepsFor(worker, fakeTokenClient([])),
      { prefs: prefs(true, [{ id: "cal-1", horizon: "standard" }]) },
    );
    await act(async () => {});
    worker.postMessage.mockClear();

    rerender({
      calendar: calendarState({ connected: true }),
      prefs: prefs(true, [
        { id: "cal-1", horizon: "standard" },
        { id: "cal-2", horizon: "standard" },
      ]),
      seed: null,
      trips: null,
    });
    await act(async () => {});

    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "setCalendarSelections",
      selections: [
        { id: "cal-1", horizon: "standard" },
        { id: "cal-2", horizon: "standard" },
      ],
    });
    expect(types(worker)).toContain("pollRefresh");
  });

  it("re-pushes when the Trips binding moves, and not when nothing moved", async () => {
    const worker = fakeWorker();
    const { rerender } = mount(
      worker,
      calendarState({ connected: true }),
      connectionDepsFor(worker, fakeTokenClient([])),
      { prefs: prefs(true) },
    );
    await act(async () => {});
    worker.postMessage.mockClear();

    // A fresh answer saying the same thing is not a move.
    rerender({ calendar: calendarState({ connected: true }), prefs: prefs(true), seed: null, trips: null });
    await act(async () => {});
    expect(types(worker)).not.toContain("pollRefresh");

    rerender({
      calendar: calendarState({ connected: true }),
      prefs: prefs(true),
      seed: null,
      trips: "trips@g",
    });
    await act(async () => {});
    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "setCalendarSelections",
      selections: [{ id: "trips@g", horizon: "long" }],
    });
  });
});

describe("useCalendarWiring: handleRefreshClick", () => {
  it("re-asserts the selection and re-polls", async () => {
    const worker = fakeWorker();
    const { result } = mount(
      worker,
      calendarState({ connected: true }),
      connectionDepsFor(worker, fakeTokenClient([])),
      { prefs: prefs(true, [{ id: "cal-1", horizon: "standard" }]) },
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
    const deps = connectionDepsFor(worker, tokenClient);

    // The rotation effect gates on the `calendar` PROP (never the
    // core-start effect's own `coreStore` write, which this hook never
    // reads back), so the test feeds the store's answer back in as the
    // next render's prop — the same way `App.tsx` feeds this hook the
    // store's own last-broadcast snapshot.
    const { rerender } = mount(worker, calendarState(), deps, { prefs: prefs(true) });
    await act(async () => {});
    expect(tokenClient.calls).toEqual(["none"]);
    rerender({ calendar: calendarState({ connected: true }), prefs: prefs(true), seed: null, trips: null });
    await act(async () => {});

    await act(async () => {
      await vi.advanceTimersByTimeAsync(expiresAtMs - ROTATION_MARGIN_MS);
    });

    expect(tokenClient.calls).toEqual(["none", "none"]);
    expect(coreStore.getSnapshot().calendar).toMatchObject({ connected: true, needsReconnect: false });
  });

  it("a rotation still in flight when another device disconnects lands nothing — the lane stays parked", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const worker = fakeWorker();
    const expiresAtMs = 60 * 60 * 1000;
    // The first (arming) re-mint answers at once; the rotation's re-mint is
    // held open so the test decides when it lands.
    let landRotation: (result: TokenResult) => void = () => {};
    const calls: TokenPrompt[] = [];
    const tokenClient: TokenClient = {
      requestToken: (prompt) => {
        calls.push(prompt);
        if (calls.length === 1) {
          return Promise.resolve({ accessToken: "initial-token", expiresAtMs });
        }
        return new Promise((resolve) => {
          landRotation = resolve;
        });
      },
    };
    const deps = connectionDepsFor(worker, tokenClient);

    const { rerender } = mount(worker, calendarState(), deps, { prefs: prefs(true) });
    await act(async () => {});
    rerender({ calendar: calendarState({ connected: true }), prefs: prefs(true), seed: null, trips: null });
    await act(async () => {});

    // The rotation timer fires; its re-mint is now in flight.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(expiresAtMs - ROTATION_MARGIN_MS);
    });
    expect(calls).toEqual(["none", "none"]);

    // Another device disconnects: the synced row reads not connected and the
    // lane parks — before the store's answer has come back round as the prop.
    rerender({ calendar: calendarState({ connected: true }), prefs: prefs(false), seed: null, trips: null });
    await act(async () => {});
    expect(coreStore.getSnapshot().calendar.connected).toBe(false);

    // Only now does the rotation's re-mint land. It must not re-arm the lane.
    await act(async () => {
      landRotation({ accessToken: "rotated-token", expiresAtMs: expiresAtMs + 3_600_000 });
    });
    expect(coreStore.getSnapshot().calendar).toMatchObject({ connected: false, needsReconnect: false });

    rerender({ calendar: calendarState(), prefs: prefs(false), seed: null, trips: null });
    await act(async () => {});
    expect(coreStore.getSnapshot().calendar).toMatchObject({ connected: false, needsReconnect: false });
  });

  it("polls again every 15 minutes while connected and foregrounded", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const worker = fakeWorker();
    const tokenClient = fakeTokenClient([]);
    const deps = connectionDepsFor(worker, tokenClient);

    mount(worker, calendarState({ connected: true }), deps, {
      prefs: prefs(true, [{ id: "cal-1", horizon: "standard" }]),
    });
    await act(async () => {});
    worker.postMessage.mockClear();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(15 * 60 * 1000);
    });

    expect(types(worker)).toContain("pollTimer");
  });
});
