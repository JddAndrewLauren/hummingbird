import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkerResponse } from "../store/protocol";
import {
  type CalendarHostLike,
  createRequestQueue,
  handleCalendarRequest,
} from "./calendar-worker";

function fakeHost(overrides: Partial<CalendarHostLike> = {}): CalendarHostLike {
  return {
    pushToken: vi.fn(),
    setCalendarIds: vi.fn(),
    start: vi.fn().mockResolvedValue("no_credential"),
    refresh: vi.fn().mockResolvedValue("no_credential"),
    onTimer: vi.fn().mockResolvedValue("no_credential"),
    takeCredentialEvents: vi.fn().mockReturnValue("[]"),
    listCalendars: vi.fn().mockResolvedValue('{"kind":"ok","calendars":[]}'),
    ...overrides,
  };
}

async function run(
  request: Parameters<typeof handleCalendarRequest>[0],
  host: CalendarHostLike,
): Promise<WorkerResponse[]> {
  const posted: WorkerResponse[] = [];
  await handleCalendarRequest(request, host, (response) => posted.push(response));
  return posted;
}

describe("handleCalendarRequest", () => {
  it("pushToken forwards the token to the host and posts nothing", async () => {
    const host = fakeHost();
    const posted = await run({ type: "pushToken", token: "tok-1" }, host);

    expect(host.pushToken).toHaveBeenCalledWith("tok-1");
    expect(posted).toEqual([]);
  });

  it("setCalendarIds forwards the selection and posts nothing", async () => {
    const host = fakeHost();
    const posted = await run(
      { type: "setCalendarIds", calendarIds: ["a", "b"] },
      host,
    );

    expect(host.setCalendarIds).toHaveBeenCalledWith(["a", "b"]);
    expect(posted).toEqual([]);
  });

  it("pollStart posts the outcome and no credential events when none are pending", async () => {
    const host = fakeHost({ start: vi.fn().mockResolvedValue("succeeded") });
    const posted = await run({ type: "pollStart", nowMs: 1_000 }, host);

    expect(host.start).toHaveBeenCalledWith(1_000);
    expect(posted).toEqual([{ type: "pollOutcome", outcome: "succeeded" }]);
  });

  it("pollRefresh posts an unauthorized outcome followed by the drained credential event", async () => {
    const host = fakeHost({
      refresh: vi.fn().mockResolvedValue("unauthorized"),
      takeCredentialEvents: vi
        .fn()
        .mockReturnValue('[{"provider":"google_calendar","at_ms":5000}]'),
    });

    const posted = await run({ type: "pollRefresh", nowMs: 5_000 }, host);

    expect(posted).toEqual([
      { type: "pollOutcome", outcome: "unauthorized" },
      {
        type: "credentialEvents",
        events: [{ provider: "google_calendar", atMs: 5000 }],
      },
    ]);
  });

  it("pollTimer routes through onTimer", async () => {
    const host = fakeHost({ onTimer: vi.fn().mockResolvedValue("held") });
    const posted = await run({ type: "pollTimer", nowMs: 2_000 }, host);

    expect(host.onTimer).toHaveBeenCalledWith(2_000);
    expect(posted).toEqual([{ type: "pollOutcome", outcome: "held" }]);
  });

  it("listCalendars posts the core's options, carrying no token of its own", async () => {
    const host = fakeHost({
      listCalendars: vi.fn().mockResolvedValue(
        JSON.stringify({
          kind: "ok",
          calendars: [
            { id: "primary", summary: "john@twinion.net" },
            { id: "team@twinion.net", summary: "Team" },
          ],
        }),
      ),
    });

    const posted = await run({ type: "listCalendars" }, host);

    expect(host.listCalendars).toHaveBeenCalledWith();
    expect(posted).toEqual([
      {
        type: "calendarList",
        calendars: [
          { id: "primary", summary: "john@twinion.net" },
          { id: "team@twinion.net", summary: "Team" },
        ],
      },
    ]);
  });

  it.each(["no_credential", "failed", "busy"])(
    'listCalendars posts nothing when the host answers "%s"',
    async (kind) => {
      // None of these say the user has no calendars. Posting an empty list
      // would blank the picker -- and with it the only affordance for
      // deselecting a calendar that is failing every poll.
      const host = fakeHost({
        listCalendars: vi.fn().mockResolvedValue(`{"kind":"${kind}","calendars":[]}`),
      });

      expect(await run({ type: "listCalendars" }, host)).toEqual([]);
    },
  );
});

describe("createRequestQueue", () => {
  /** A promise plus the handle to settle it from the test. */
  function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((r) => {
      resolve = r;
    });
    return { promise, resolve };
  }

  it("does not let a second request reach the host while the first is in flight", async () => {
    // The property the wasm host depends on: `CalendarHost` holds a
    // `RefCell` borrow across a poll's network await, so a request that
    // reached it mid-poll would panic the whole wasm module. Bursts are
    // routine -- a picker change posts setCalendarIds, pollRefresh and
    // listCalendars back to back -- so nothing but a queue makes
    // "one at a time" true.
    const inFlight = deferred<string>();
    const host = fakeHost({
      refresh: vi.fn().mockReturnValue(inFlight.promise),
    });
    const posted: WorkerResponse[] = [];
    const enqueue = createRequestQueue(host, (response) => posted.push(response));

    void enqueue({ type: "pollRefresh", nowMs: 1_000 });
    const second = enqueue({ type: "listCalendars" });
    await Promise.resolve();

    expect(host.listCalendars).not.toHaveBeenCalled();

    inFlight.resolve("succeeded");
    await second;

    expect(host.listCalendars).toHaveBeenCalled();
    expect(posted).toEqual([
      { type: "pollOutcome", outcome: "succeeded" },
      { type: "calendarList", calendars: [] },
    ]);
  });

  it("keeps draining after a request fails", async () => {
    // One rejected request must not wedge the queue: everything behind it
    // would stop, and the UI would silently stop updating.
    const host = fakeHost({
      refresh: vi.fn().mockRejectedValue(new Error("boom")),
    });
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const posted: WorkerResponse[] = [];
    const enqueue = createRequestQueue(host, (response) => posted.push(response));

    await enqueue({ type: "pollRefresh", nowMs: 1_000 });
    await enqueue({ type: "listCalendars" });

    expect(consoleError).toHaveBeenCalled();
    expect(posted).toEqual([{ type: "calendarList", calendars: [] }]);
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
        refresh: vi.fn().mockReturnValue(new Promise<string>(() => {})),
      });
      const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
      const posted: WorkerResponse[] = [];
      const enqueue = createRequestQueue(host, (response) => posted.push(response));

      const first = enqueue({ type: "pollRefresh", nowMs: 1_000 });
      const second = enqueue({ type: "listCalendars" });

      await vi.advanceTimersByTimeAsync(10_100);
      await first;
      await second;

      expect(host.listCalendars).toHaveBeenCalled();
      expect(posted).toContainEqual({ type: "calendarList", calendars: [] });
      consoleError.mockRestore();
    });
  });
});
