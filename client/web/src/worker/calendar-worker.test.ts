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
    setCalendarSelections: vi.fn(),
    start: vi.fn().mockResolvedValue("no_credential"),
    refresh: vi.fn().mockResolvedValue("no_credential"),
    onTimer: vi.fn().mockResolvedValue("no_credential"),
    takeCredentialEvents: vi.fn().mockReturnValue("[]"),
    listCalendars: vi.fn().mockResolvedValue('{"kind":"ok","calendars":[]}'),
    eventsInInterval: vi.fn().mockResolvedValue('{"kind":"not_read","events":[],"freshness":null}'),
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

  it("setCalendarSelections forwards the selection as JSON and posts nothing", async () => {
    const host = fakeHost();
    const posted = await run(
      {
        type: "setCalendarSelections",
        selections: [
          { id: "a", horizon: "standard" },
          { id: "b", horizon: "long" },
        ],
      },
      host,
    );

    // JSON text, because the wasm seam cannot carry a per-entry horizon
    // through a positional `Vec<String>` (`client/ffi-web/src/lib.rs`).
    expect(host.setCalendarSelections).toHaveBeenCalledWith(
      '[{"id":"a","horizon":"standard"},{"id":"b","horizon":"long"}]',
    );
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

  // -- getCalendarEvents (issue #267) ------------------------------------

  it("passes the interval and clock straight through to the host", async () => {
    const host = fakeHost();
    await run(
      {
        type: "getCalendarEvents",
        key: "weekend",
        startMs: 1_000,
        endMs: 2_000,
        startDate: "2026-08-14",
        endDate: "2026-08-17",
        nowMs: 1_500,
      },
      host,
    );
    expect(host.eventsInInterval).toHaveBeenCalledWith(
      1_000,
      2_000,
      "2026-08-14",
      "2026-08-17",
      1_500,
    );
  });

  it('posts "not_read" as-is when the host has never synced this calendar', async () => {
    const host = fakeHost({
      eventsInInterval: vi
        .fn()
        .mockResolvedValue('{"kind":"not_read","events":[],"freshness":null}'),
    });

    const posted = await run(
      {
        type: "getCalendarEvents",
        key: "weekend",
        startMs: 1_000,
        endMs: 2_000,
        startDate: "2026-08-14",
        endDate: "2026-08-17",
        nowMs: 1_500,
      },
      host,
    );

    expect(posted).toEqual([
      { type: "calendarEvents", key: "weekend", read: { state: "not_read" } },
    ]);
  });

  it("maps both event arms and the freshness, camelCasing every field", async () => {
    const host = fakeHost({
      eventsInInterval: vi.fn().mockResolvedValue(
        JSON.stringify({
          kind: "read",
          events: [
            {
              provider_event_id: "evt-1",
              calendar_id: "cal-primary",
              title: "Standup",
              when: { kind: "timed", start_ms: 1_000, end_ms: 2_000 },
              recurrence_id: null,
              location: null,
              organizer: null,
              status: "confirmed",
              provider_updated_at_ms: 900,
              html_link: null,
            },
            {
              provider_event_id: "evt-2",
              calendar_id: "cal-primary",
              title: "India",
              when: { kind: "all_day", start_date: "2026-09-09", end_date: "2026-09-16" },
              recurrence_id: null,
              location: null,
              organizer: null,
              status: "confirmed",
              provider_updated_at_ms: 900,
              html_link: null,
            },
          ],
          freshness: { state: "age", age_ms: 60_000, declared_cadence_ms: 900_000 },
        }),
      ),
    });

    const posted = await run(
      {
        type: "getCalendarEvents",
        key: "weekend",
        startMs: 0,
        endMs: 5_000,
        startDate: "2026-08-14",
        endDate: "2026-08-17",
        nowMs: 61_000,
      },
      host,
    );

    expect(posted).toEqual([
      {
        type: "calendarEvents",
        key: "weekend",
        read: {
          state: "read",
          events: [
            {
              providerEventId: "evt-1",
              calendarId: "cal-primary",
              title: "Standup",
              when: { kind: "timed", startMs: 1_000, endMs: 2_000 },
              recurrenceId: null,
              location: null,
              organizer: null,
              status: "confirmed",
              providerUpdatedAtMs: 900,
              htmlLink: null,
            },
            {
              providerEventId: "evt-2",
              calendarId: "cal-primary",
              title: "India",
              // Byte-identical to the wire: the all-day arm's dates are
              // never re-derived, re-formatted or resolved to an instant
              // on this side (ADR-0015's 2026-08-10 amendment).
              when: { kind: "allDay", startDate: "2026-09-09", endDate: "2026-09-16" },
              recurrenceId: null,
              location: null,
              organizer: null,
              status: "confirmed",
              providerUpdatedAtMs: 900,
              htmlLink: null,
            },
          ],
          freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 900_000 },
        },
      },
    ]);
  });

  it('posts nothing when the host answers "busy" — no answer, not an empty one', async () => {
    const host = fakeHost({
      eventsInInterval: vi
        .fn()
        .mockResolvedValue('{"kind":"busy","events":[],"freshness":null}'),
    });

    expect(
      await run(
        {
          type: "getCalendarEvents",
          key: "weekend",
          startMs: 0,
          endMs: 1_000,
          startDate: "2026-08-14",
          endDate: "2026-08-17",
          nowMs: 500,
        },
        host,
      ),
    ).toEqual([]);
  });
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
    // routine -- a picker change posts setCalendarSelections, pollRefresh and
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
