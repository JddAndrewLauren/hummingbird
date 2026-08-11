import type {
  CalendarEventDTO,
  CalendarListEntryDTO,
  CalendarReadDTO,
  CalendarWorkerRequest,
  CredentialEventDTO,
  PollOutcomeName,
  WorkerResponse,
} from "../store/protocol";
import { mapFreshness, type RawFreshness } from "./freshness-wire";
import { createSerialQueue } from "./serial-queue";

// The worker's half of issue #73's calendar wiring, kept free of the wasm
// import so vitest (node) can exercise it against a fake `CalendarHostLike`
// — same discipline as announce.ts.

/** The slice of `hummingbird-ffi-web`'s `CalendarHost` this handler needs.
 * Every method mirrors the wasm-bindgen surface exactly (see
 * `client/ffi-web/src/lib.rs`'s `wasm_bindings::CalendarHost`); the async
 * methods resolve to the JSON string the core serializes, not a parsed
 * value — parsing is this module's job, so the wire format is explicit in
 * one place. */
export interface CalendarHostLike {
  pushToken(token: string): void;
  setCalendarIds(calendarIds: string[]): void;
  start(nowMs: number): Promise<string>;
  refresh(nowMs: number): Promise<string>;
  onTimer(nowMs: number): Promise<string>;
  takeCredentialEvents(): string;
  listCalendars(): Promise<string>;
  eventsInInterval(startMs: number, endMs: number, nowMs: number): Promise<string>;
}

interface RawCredentialEvent {
  provider: string;
  at_ms: number;
}

interface RawCalendarListResponse {
  kind: "ok" | "no_credential" | "failed" | "busy";
  calendars: CalendarListEntryDTO[];
}

// -- issue #267's calendar-events read — pinned to `CalendarEventsResponse`'s
// serde output by `client/ffi-web/src/calendar_host.rs`'s own
// `the_wire_shape_is_kind_events_freshness_never_a_flattened_shape` test.

interface RawCalendarEventTime {
  instant_ms: number;
  time_zone: string;
}

interface RawCalendarEvent {
  provider_event_id: string;
  calendar_id: string;
  title: string;
  start: RawCalendarEventTime;
  end: RawCalendarEventTime;
  all_day: boolean;
  recurrence_id: string | null;
  location: string | null;
  organizer: string | null;
  status: "confirmed" | "tentative" | "cancelled";
  provider_updated_at_ms: number;
  html_link: string | null;
}

interface RawCalendarEventsResponse {
  kind: "not_read" | "read" | "busy";
  events: RawCalendarEvent[];
  freshness: RawFreshness | null;
}

function mapCalendarEventTime(raw: RawCalendarEventTime) {
  return { instantMs: raw.instant_ms, timeZone: raw.time_zone };
}

function mapCalendarEvent(raw: RawCalendarEvent): CalendarEventDTO {
  return {
    providerEventId: raw.provider_event_id,
    calendarId: raw.calendar_id,
    title: raw.title,
    start: mapCalendarEventTime(raw.start),
    end: mapCalendarEventTime(raw.end),
    allDay: raw.all_day,
    recurrenceId: raw.recurrence_id,
    location: raw.location,
    organizer: raw.organizer,
    status: raw.status,
    providerUpdatedAtMs: raw.provider_updated_at_ms,
    htmlLink: raw.html_link,
  };
}

/** `raw.kind === "busy"` is never passed here — the caller drops it before
 * mapping, same contract `getPaneRead` documents in `task-worker.ts`. */
function mapCalendarRead(raw: RawCalendarEventsResponse): CalendarReadDTO {
  if (raw.kind === "not_read") {
    return { state: "not_read" };
  }
  return {
    state: "read",
    events: raw.events.map(mapCalendarEvent),
    // `raw.kind === "read"` always carries a `freshness` — pinned by
    // `calendar_host.rs`'s own `CalendarEventsResponse` construction — but
    // the wire type is nullable, so this is a defensive fallback rather
    // than a claim this branch is ever actually reached.
    freshness: raw.freshness ? mapFreshness(raw.freshness) : { kind: "unknown" },
  };
}

function mapCredentialEvents(raw: RawCredentialEvent[]): CredentialEventDTO[] {
  return raw.map((event) => ({ provider: event.provider, atMs: event.at_ms }));
}

/** Drains and posts credential events, if any — called after every poll
 * trigger, since an `"unauthorized"` outcome records exactly one. */
function postCredentialEvents(
  host: CalendarHostLike,
  post: (response: WorkerResponse) => void,
): void {
  const raw = JSON.parse(host.takeCredentialEvents()) as RawCredentialEvent[];
  if (raw.length > 0) {
    post({ type: "credentialEvents", events: mapCredentialEvents(raw) });
  }
}

async function runPollTrigger(
  trigger: (nowMs: number) => Promise<string>,
  nowMs: number,
  host: CalendarHostLike,
  post: (response: WorkerResponse) => void,
): Promise<void> {
  const outcome = (await trigger(nowMs)) as PollOutcomeName;
  post({ type: "pollOutcome", outcome });
  postCredentialEvents(host, post);
}

/** Handles one `CalendarWorkerRequest`, posting whatever `WorkerResponse`(s)
 * it produces. Callers should go through [`createRequestQueue`] rather than
 * calling this directly — see the re-entrancy note there. */
export async function handleCalendarRequest(
  request: CalendarWorkerRequest,
  host: CalendarHostLike,
  post: (response: WorkerResponse) => void,
): Promise<void> {
  switch (request.type) {
    case "pushToken":
      host.pushToken(request.token);
      return;
    case "setCalendarIds":
      host.setCalendarIds(request.calendarIds);
      return;
    case "pollStart":
      await runPollTrigger((ms) => host.start(ms), request.nowMs, host, post);
      return;
    case "pollRefresh":
      await runPollTrigger((ms) => host.refresh(ms), request.nowMs, host, post);
      return;
    case "pollTimer":
      await runPollTrigger((ms) => host.onTimer(ms), request.nowMs, host, post);
      return;
    case "listCalendars": {
      const raw = JSON.parse(await host.listCalendars()) as RawCalendarListResponse;
      if (raw.kind !== "ok") {
        // Same reasoning as `"busy"` above: a held credential, a failed
        // lookup or a busy core is no answer at all, and posting an empty
        // list would blank a picker that is showing real options — taking
        // the user's ability to deselect a calendar with it.
        return;
      }
      post({ type: "calendarList", calendars: raw.calendars });
      return;
    }
    case "getCalendarEvents": {
      const raw = JSON.parse(
        await host.eventsInInterval(request.startMs, request.endMs, request.nowMs),
      ) as RawCalendarEventsResponse;
      if (raw.kind === "busy") {
        // No answer, not an empty answer — an empty `events` read renders
        // as "nothing scheduled", which a busy core has no standing to
        // claim (`lib.rs`'s `BUSY_CALENDAR_EVENTS`).
        return;
      }
      post({ type: "calendarEvents", key: request.key, read: mapCalendarRead(raw) });
      return;
    }
  }
}

/** How long one calendar request may run before the queue abandons it rather
 * than stalling every request behind it (issue #173, the calendar half of
 * issue #95's named risk: "a hung request wedges the worker" — a hung
 * `fetch` inside the Google Calendar poll, for instance). A poll trigger
 * does exactly one Google Calendar fetch, not a multi-step sync cycle like
 * `task-worker.ts`'s `TASK_REQUEST_TIMEOUT_MS`, so this is tighter than
 * that 30s. See `serial-queue.ts` for what "abandoned" means and why it is
 * safe. */
export const CALENDAR_REQUEST_TIMEOUT_MS = 10_000;

/** Serialises every request into one at-a-time chain.
 *
 * This is a correctness requirement, not a tidiness one. `CalendarHost` is
 * `Rc<RefCell<CalendarHostCore>>` on the Rust side, and a poll trigger holds
 * that borrow across its network await; a second request that reached the
 * host mid-poll would hit a `RefCell` borrow panic, and a wasm panic poisons
 * the whole module, not just the one call. `onmessage` alone gives no such
 * guarantee — it fires a fresh, unsequenced handler per message, and the
 * main thread genuinely does send bursts (the picker posts `setCalendarIds`,
 * `pollRefresh` and `listCalendars` back to back). Queueing here is what
 * makes "one call at a time" true rather than merely intended.
 *
 * Requests are processed strictly in arrival order, which is also the
 * ordering the picker depends on: the new selection lands before the refresh
 * that must use it.
 *
 * A failing request is logged and swallowed rather than left to reject: the
 * chain must survive it, and there is no per-request error channel in the
 * protocol (a `{type: "error"}` response means "the core failed to load" and
 * would wrongly blank the whole UI). The poll paths report their own
 * failures as outcomes, so nothing user-visible is lost here.
 *
 * Built on `serial-queue.ts`'s `createSerialQueue`, same as
 * `task-worker.ts`'s `createTaskRequestQueue`, so a request that never
 * settles (a hung `fetch`, say) is abandoned after
 * `CALENDAR_REQUEST_TIMEOUT_MS` rather than wedging every request behind it.
 */
export function createRequestQueue(
  host: CalendarHostLike,
  post: (response: WorkerResponse) => void,
): (request: CalendarWorkerRequest) => Promise<void> {
  return createSerialQueue(
    (request: CalendarWorkerRequest) => handleCalendarRequest(request, host, post),
    {
      timeoutMs: CALENDAR_REQUEST_TIMEOUT_MS,
      onTimeout: (request) => {
        console.error("calendar worker request abandoned after timeout", request.type);
      },
      onError: (request, error) => {
        console.error("calendar worker request failed", request.type, error);
      },
    },
  );
}
