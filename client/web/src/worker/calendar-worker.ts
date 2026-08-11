import type {
  CalendarListEntryDTO,
  CalendarWorkerRequest,
  CredentialEventDTO,
  PollOutcomeName,
  WorkerResponse,
} from "../store/protocol";
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
}

interface RawCredentialEvent {
  provider: string;
  at_ms: number;
}

interface RawCalendarListResponse {
  kind: "ok" | "no_credential" | "failed" | "busy";
  calendars: CalendarListEntryDTO[];
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
