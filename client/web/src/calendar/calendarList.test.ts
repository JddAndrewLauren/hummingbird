import { describe, expect, it, vi } from "vitest";
import { CalendarListError, listCalendars } from "./calendarList";

function fakeFetch(response: { ok: boolean; status: number; body?: unknown }) {
  return vi.fn(async () => ({
    ok: response.ok,
    status: response.status,
    json: async () => response.body,
  })) as unknown as typeof fetch;
}

/** One successful response per call, in order — enough to drive pagination. */
function scriptedFetch(bodies: unknown[]) {
  let call = 0;
  return vi.fn(async () => {
    const body = bodies[call];
    if (body === undefined) {
      throw new Error(`scriptedFetch ran out of responses on call ${call + 1}`);
    }
    call += 1;
    return { ok: true, status: 200, json: async () => body };
  }) as unknown as typeof fetch;
}

describe("listCalendars", () => {
  it("sends the bearer token and maps id/summary pairs", async () => {
    const fetchImpl = fakeFetch({
      ok: true,
      status: 200,
      body: {
        items: [
          { id: "primary", summary: "john@twinion.net" },
          { id: "team@twinion.net", summary: "Team" },
        ],
      },
    });

    const calendars = await listCalendars("tok-1", fetchImpl);

    expect(calendars).toEqual([
      { id: "primary", summary: "john@twinion.net" },
      { id: "team@twinion.net", summary: "Team" },
    ]);
    expect(fetchImpl).toHaveBeenCalledWith(
      expect.stringContaining("calendarList"),
      { headers: { Authorization: "Bearer tok-1" } },
    );
  });

  it("falls back to the id when summary is missing", async () => {
    const fetchImpl = fakeFetch({ ok: true, status: 200, body: { items: [{ id: "cal-1" }] } });

    const calendars = await listCalendars("tok-1", fetchImpl);

    expect(calendars).toEqual([{ id: "cal-1", summary: "cal-1" }]);
  });

  it("drops items with no id", async () => {
    const fetchImpl = fakeFetch({
      ok: true,
      status: 200,
      body: { items: [{ summary: "no id here" }] },
    });

    const calendars = await listCalendars("tok-1", fetchImpl);

    expect(calendars).toEqual([]);
  });

  it("treats a missing items array as empty", async () => {
    const fetchImpl = fakeFetch({ ok: true, status: 200, body: {} });
    expect(await listCalendars("tok-1", fetchImpl)).toEqual([]);
  });

  it("throws CalendarListError with the status on a non-ok response", async () => {
    const fetchImpl = fakeFetch({ ok: false, status: 401 });

    await expect(listCalendars("tok-1", fetchImpl)).rejects.toBeInstanceOf(
      CalendarListError,
    );
  });

  it("follows nextPageToken so calendars past the first page are selectable", async () => {
    const fetchImpl = scriptedFetch([
      { items: [{ id: "cal-1", summary: "One" }], nextPageToken: "page-2" },
      { items: [{ id: "cal-2", summary: "Two" }] },
    ]);

    const calendars = await listCalendars("tok-1", fetchImpl);

    expect(calendars).toEqual([
      { id: "cal-1", summary: "One" },
      { id: "cal-2", summary: "Two" },
    ]);
    expect(fetchImpl).toHaveBeenCalledTimes(2);
    expect(fetchImpl).toHaveBeenLastCalledWith(
      expect.stringContaining("pageToken=page-2"),
      { headers: { Authorization: "Bearer tok-1" } },
    );
  });

  it("stops instead of looping when a page token repeats", async () => {
    const fetchImpl = scriptedFetch([
      { items: [{ id: "cal-1" }], nextPageToken: "page-2" },
      { items: [{ id: "cal-2" }], nextPageToken: "page-2" },
    ]);

    const calendars = await listCalendars("tok-1", fetchImpl);

    expect(calendars.map((c) => c.id)).toEqual(["cal-1", "cal-2"]);
    expect(fetchImpl).toHaveBeenCalledTimes(2);
  });
});
