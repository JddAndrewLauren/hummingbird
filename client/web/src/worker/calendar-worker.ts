import type {
  CalendarWorkerRequest,
  CredentialEventDTO,
  CurrentNextEventDTO,
  CurrentNextKind,
  PollOutcomeName,
  WorkerResponse,
} from "../store/protocol";

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
  currentOrNext(nowMs: number): Promise<string>;
}

interface RawCredentialEvent {
  provider: string;
  at_ms: number;
}

interface RawEventRecord {
  title: string;
  start: { instant_ms: number };
  end: { instant_ms: number };
  all_day: boolean;
  html_link: string | null;
}

interface RawCurrentNextResponse {
  kind: CurrentNextKind;
  event: RawEventRecord | null;
  as_of_ms: number | null;
}

function mapEvent(raw: RawEventRecord | null): CurrentNextEventDTO | null {
  if (raw === null) {
    return null;
  }
  return {
    title: raw.title,
    startMs: raw.start.instant_ms,
    endMs: raw.end.instant_ms,
    allDay: raw.all_day,
    htmlLink: raw.html_link,
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
    case "getCurrentNext": {
      const raw = JSON.parse(
        await host.currentOrNext(request.nowMs),
      ) as RawCurrentNextResponse;
      if (raw.kind === "busy") {
        // No answer, not an empty answer: posting this would blank a tile
        // that is currently showing something true.
        return;
      }
      post({
        type: "currentNext",
        kind: raw.kind,
        event: mapEvent(raw.event),
        asOfMs: raw.as_of_ms,
      });
      return;
    }
  }
}

/** Serialises every request into one at-a-time chain.
 *
 * This is a correctness requirement, not a tidiness one. `CalendarHost` is
 * `Rc<RefCell<CalendarHostCore>>` on the Rust side, and a poll trigger holds
 * that borrow across its network await; a second request that reached the
 * host mid-poll would hit a `RefCell` borrow panic, and a wasm panic poisons
 * the whole module, not just the one call. `onmessage` alone gives no such
 * guarantee — it fires a fresh, unsequenced handler per message, and the
 * main thread genuinely does send bursts (the picker posts `setCalendarIds`,
 * `pollRefresh` and `getCurrentNext` back to back). Queueing here is what
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
 */
export function createRequestQueue(
  host: CalendarHostLike,
  post: (response: WorkerResponse) => void,
): (request: CalendarWorkerRequest) => Promise<void> {
  let tail: Promise<void> = Promise.resolve();
  return (request) => {
    tail = tail.then(() =>
      handleCalendarRequest(request, host, post).catch((error: unknown) => {
        console.error("calendar worker request failed", request.type, error);
      }),
    );
    return tail;
  };
}
