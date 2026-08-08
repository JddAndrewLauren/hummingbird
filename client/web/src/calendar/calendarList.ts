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
  nextPageToken?: string;
}

const CALENDAR_LIST_URL =
  "https://www.googleapis.com/calendar/v3/users/me/calendarList?minAccessRole=reader";

// Google's `calendarList.list` caps a page at 100 entries (its default
// maxResults) and hands back a `nextPageToken` when there are more. Stopping
// at page one would make every calendar past the first hundred unselectable
// — invisible in the picker rather than visibly unavailable, which is the
// worse failure. The page bound and the repeated-token check guard the same
// misbehaving-server case the core adapter does, but stop rather than throw:
// the picker's options are a UX nicety (see App.tsx's caller), so a
// truncated list beats losing the list entirely.
const MAX_PAGES = 20;

export class CalendarListError extends Error {
  constructor(public readonly status: number) {
    super(`calendar list request failed with HTTP ${status}`);
  }
}

/** Fetches every page of the signed-in user's calendar list. `fetchImpl`
 * defaults to the global `fetch` and is overridable for tests. */
export async function listCalendars(
  accessToken: string,
  fetchImpl: typeof fetch = fetch,
): Promise<CalendarListEntry[]> {
  const calendars: CalendarListEntry[] = [];
  const seenPageTokens = new Set<string>();
  let pageToken: string | undefined;

  for (let page = 0; page < MAX_PAGES; page += 1) {
    const url = pageToken
      ? `${CALENDAR_LIST_URL}&pageToken=${encodeURIComponent(pageToken)}`
      : CALENDAR_LIST_URL;
    const response = await fetchImpl(url, {
      headers: { Authorization: `Bearer ${accessToken}` },
    });
    if (!response.ok) {
      throw new CalendarListError(response.status);
    }
    const body = (await response.json()) as RawCalendarListResponse;
    for (const item of body.items ?? []) {
      if (item.id) {
        calendars.push({ id: item.id, summary: item.summary ?? item.id });
      }
    }
    if (!body.nextPageToken || seenPageTokens.has(body.nextPageToken)) {
      break;
    }
    seenPageTokens.add(body.nextPageToken);
    pageToken = body.nextPageToken;
  }

  return calendars;
}
