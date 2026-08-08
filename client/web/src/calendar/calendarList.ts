// Lists the calendars the picker (issue #73) offers. Not part of #71's
// event-fetching adapter (`calendars/{id}/events`) — this is Google's
// separate `calendarList` endpoint, needed only so the picker has real
// options to choose from. Same `calendar.readonly` scope covers it, so no
// extra consent is ever requested for this call.

export interface CalendarListEntry {
  id: string;
  summary: string;
}

interface RawCalendarListItem {
  id?: string;
  summary?: string;
}

interface RawCalendarListResponse {
  items?: RawCalendarListItem[];
}

const CALENDAR_LIST_URL =
  "https://www.googleapis.com/calendar/v3/users/me/calendarList?minAccessRole=reader";

export class CalendarListError extends Error {
  constructor(public readonly status: number) {
    super(`calendar list request failed with HTTP ${status}`);
  }
}

/** Fetches the signed-in user's calendar list. `fetchImpl` defaults to the
 * global `fetch` and is overridable for tests. */
export async function listCalendars(
  accessToken: string,
  fetchImpl: typeof fetch = fetch,
): Promise<CalendarListEntry[]> {
  const response = await fetchImpl(CALENDAR_LIST_URL, {
    headers: { Authorization: `Bearer ${accessToken}` },
  });
  if (!response.ok) {
    throw new CalendarListError(response.status);
  }
  const body = (await response.json()) as RawCalendarListResponse;
  return (body.items ?? [])
    .filter((item): item is Required<RawCalendarListItem> => Boolean(item.id))
    .map((item) => ({ id: item.id, summary: item.summary ?? item.id }));
}
