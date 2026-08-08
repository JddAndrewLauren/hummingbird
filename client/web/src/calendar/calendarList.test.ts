import { describe, expect, it, vi } from "vitest";
import { CalendarListError, listCalendars } from "./calendarList";

function fakeFetch(response: { ok: boolean; status: number; body?: unknown }) {
  return vi.fn(async () => ({
    ok: response.ok,
    status: response.status,
    json: async () => response.body,
  })) as unknown as typeof fetch;
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
});
