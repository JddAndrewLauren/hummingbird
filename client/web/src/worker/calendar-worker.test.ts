import { describe, expect, it, vi } from "vitest";
import type { WorkerResponse } from "../store/protocol";
import { type CalendarHostLike, handleCalendarRequest } from "./calendar-worker";

function fakeHost(overrides: Partial<CalendarHostLike> = {}): CalendarHostLike {
  return {
    pushToken: vi.fn(),
    setCalendarIds: vi.fn(),
    start: vi.fn().mockResolvedValue("no_credential"),
    refresh: vi.fn().mockResolvedValue("no_credential"),
    onTimer: vi.fn().mockResolvedValue("no_credential"),
    takeCredentialEvents: vi.fn().mockReturnValue("[]"),
    currentOrNext: vi
      .fn()
      .mockResolvedValue('{"kind":"no_snapshot","event":null,"as_of_ms":null}'),
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

  it("getCurrentNext maps an in-progress event's raw JSON into the DTO shape", async () => {
    const host = fakeHost({
      currentOrNext: vi.fn().mockResolvedValue(
        JSON.stringify({
          kind: "in_progress",
          event: {
            title: "Standup",
            start: { instant_ms: 1_000 },
            end: { instant_ms: 2_000 },
            all_day: false,
            html_link: "https://calendar.google.com/event?eid=abc",
          },
          as_of_ms: 9_000,
        }),
      ),
    });

    const posted = await run({ type: "getCurrentNext", nowMs: 1_500 }, host);

    expect(posted).toEqual([
      {
        type: "currentNext",
        kind: "in_progress",
        event: {
          title: "Standup",
          startMs: 1_000,
          endMs: 2_000,
          allDay: false,
          htmlLink: "https://calendar.google.com/event?eid=abc",
        },
        asOfMs: 9_000,
      },
    ]);
  });

  it("getCurrentNext with no snapshot maps to a null event", async () => {
    const host = fakeHost();
    const posted = await run({ type: "getCurrentNext", nowMs: 1_000 }, host);

    expect(posted).toEqual([
      { type: "currentNext", kind: "no_snapshot", event: null, asOfMs: null },
    ]);
  });
});
