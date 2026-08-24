// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import type { DiagnosticEventV1DTO } from "../store/protocol";
import { diagnosticsExportFilename, downloadDiagnosticsExport } from "./diagnostics-download";

// Review round 1 of PR #736: the brief's own named gate ("a test asserts
// the mirror is not present in it under any code path") had nothing
// checking the actual exported bytes, and the comment justifying that with
// "no DOM env" was false — this repo's jsdom docblock is exactly the tool
// for it. This reads the real `Blob` handed to `URL.createObjectURL` back
// with `.text()`, the same object the anchor's `href` actually points at,
// rather than asserting anything about the anchor's own attributes (which
// would prove the mechanism fired, not what it fired with).

function event(overrides: Partial<DiagnosticEventV1DTO> = {}): DiagnosticEventV1DTO {
  return {
    schema_version: 1,
    seq: 1,
    wall_clock_ms: 1_000,
    elapsed_ms: 0,
    session_id: "s-1",
    source: "web-worker",
    cycle_id: null,
    operation_id: null,
    request_id: null,
    event: { name: "core.wait_started" },
    ...overrides,
  };
}

describe("diagnosticsExportFilename", () => {
  it("embeds a filesystem-safe timestamp with no colons or dots", () => {
    const name = diagnosticsExportFilename(Date.UTC(2026, 7, 9, 12, 34, 56));
    expect(name).toBe("hummingbird-diagnostics-2026-08-09T12-34-56-000Z.json");
    expect(name).not.toMatch(/[:.](?!json$)/);
  });

  it("two different instants produce two different filenames", () => {
    const first = diagnosticsExportFilename(1_000);
    const second = diagnosticsExportFilename(2_000);
    expect(first).not.toBe(second);
  });

  it("never collides with the mirror's own filename scheme", () => {
    const name = diagnosticsExportFilename(1_000);
    expect(name).not.toContain("hummingbird-mirror");
  });
});

describe("downloadDiagnosticsExport", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    // @ts-expect-error — cleaning up the ad-hoc assignment `captureDownloadedBlob` makes; jsdom has no real implementation of either to fall back to.
    delete URL.createObjectURL;
    // @ts-expect-error — see above.
    delete URL.revokeObjectURL;
  });

  /** Captures the real `Blob` the download path builds, by intercepting
   * `URL.createObjectURL` — the one call that ever sees it. jsdom's own
   * anchor `.click()` is a real DOM method that does not navigate, so it is
   * left to run rather than mocked; only the two URL calls and the click
   * are ones this test needs to observe or must not let escape (a real
   * navigation attempt) into the test environment. */
  function captureDownloadedBlob(): {
    blob: Blob | undefined;
    anchorClicks: number;
    revokeCalls: number;
  } {
    const captured = { blob: undefined as Blob | undefined, anchorClicks: 0, revokeCalls: 0 };
    // jsdom does not implement `URL.createObjectURL`/`revokeObjectURL` at
    // all (neither exists on the global `URL` here), so these are plain
    // assignments rather than `vi.spyOn` (which requires the property to
    // already exist) — restored in `afterEach` below.
    URL.createObjectURL = ((blob: Blob) => {
      captured.blob = blob;
      return "blob:mock-url";
    }) as typeof URL.createObjectURL;
    URL.revokeObjectURL = (() => {
      captured.revokeCalls += 1;
    }) as typeof URL.revokeObjectURL;
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {
      captured.anchorClicks += 1;
    });
    return captured;
  }

  it("writes a document containing the events and the dropped count", async () => {
    const captured = captureDownloadedBlob();
    const events = [event({ seq: 1 }), event({ seq: 2 })];

    downloadDiagnosticsExport(events, 3, 1_000);

    expect(captured.anchorClicks).toBe(1);
    expect(captured.blob).toBeDefined();
    expect(captured.blob?.type).toBe("application/json");
    const text = await captured.blob!.text();
    const parsed = JSON.parse(text) as {
      events: unknown[];
      dropped_count: number;
      schema_version: number;
    };
    expect(parsed.events).toHaveLength(2);
    expect(parsed.events).toEqual(events);
    expect(parsed.dropped_count).toBe(3);
    expect(parsed.schema_version).toBe(1);
  });

  it("matches Android's envelope key order and casing exactly (#712 reconciliation)", async () => {
    const captured = captureDownloadedBlob();

    downloadDiagnosticsExport([], 0, 1_000);

    const text = await captured.blob!.text();
    expect(Object.keys(JSON.parse(text) as object)).toEqual([
      "schema_version",
      "dropped_count",
      "events",
    ]);
  });

  it("never carries the mirror, under this code path, whatever events are passed", async () => {
    const captured = captureDownloadedBlob();

    downloadDiagnosticsExport([event({ request_id: "c-1-0" })], 0, 1_000);

    const text = await captured.blob!.text();
    expect(text.toLowerCase()).not.toContain("mirror");
  });

  it("names the download with diagnosticsExportFilename's own scheme", () => {
    captureDownloadedBlob();
    const clickSpy = vi.spyOn(HTMLAnchorElement.prototype, "click");
    let downloadAttr: string | undefined;
    const originalCreateElement = document.createElement.bind(document);
    vi.spyOn(document, "createElement").mockImplementation((tagName: string) => {
      const el = originalCreateElement(tagName);
      if (tagName === "a") {
        Object.defineProperty(el, "download", {
          get: () => downloadAttr,
          set: (value: string) => {
            downloadAttr = value;
          },
        });
      }
      return el;
    });

    downloadDiagnosticsExport([], 0, Date.UTC(2026, 7, 9, 12, 34, 56));

    expect(downloadAttr).toBe(diagnosticsExportFilename(Date.UTC(2026, 7, 9, 12, 34, 56)));
    expect(clickSpy).toHaveBeenCalledTimes(1);
  });

  it("revokes the object URL after the click, even on an empty export", () => {
    const captured = captureDownloadedBlob();

    downloadDiagnosticsExport([], 0, 1_000);

    expect(captured.anchorClicks).toBe(1);
    expect(captured.revokeCalls).toBe(1);
  });
});
