import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import { createDiagnosticsJournal } from "./diagnostics-journal";
import { createDiagnosticsStore } from "./diagnostics-store";

// #707's own half of #706's redaction rule
// (`client/core/src/diagnostics/mod.rs`'s `FORBIDDEN_FIELD_NAMES`): a real
// exported journal, built the same way `core.worker.ts` builds one — every
// worker-layer event family this slice emits, plus a simulated Core-sourced
// drain (opaque JSON this side never inspects, exactly as #708 will one day
// hand it over) — must never carry a credential, a request/response body,
// or any of the operator's own item text. This is the value-level half of
// the check `mod.rs`'s own test documents its limits on; this file's job is
// only to prove THIS export path never introduces one, not to re-litigate
// the shared enum's own coverage.
const FORBIDDEN_SUBSTRINGS = [
  "authorization",
  "access_token",
  "api_key",
  "token",
  "credential",
  "password",
  "request_body",
  "response_body",
  "stack_trace",
  // The Agent Brief's own separate acceptance criterion: "a diagnostics
  // export contains logs only... the mirror is not present in it under any
  // code path." `getDiagnostics`/`getMirrorSnapshot` are two different
  // protocol messages answered by two different handlers
  // (`dispatch.ts` intercepts the former before it can reach the task
  // queue where `mirrorSnapshot()` lives — see `dispatch.test.ts`'s own
  // "reaching neither wasm queue" assertions for that structural proof);
  // this line is the value-level check that no mirror data — or even the
  // word — ever lands in what this module actually serializes.
  "mirror",
];

describe("a real exported journal never carries a forbidden field or value", () => {
  it("scans a full export — every worker-layer family plus a simulated Core drain", async () => {
    const store = createDiagnosticsStore(new IDBFactory());
    const journal = createDiagnosticsJournal(0, store);

    journal.recordEnqueued(1_000);
    journal.recordDequeued(1_001);
    journal.recordAbandoned(1_002);
    journal.recordBusy(1_003);
    journal.recordNetworkChanged(1_004, true);
    journal.recordNetworkChanged(1_005, false);

    // A simulated #708 drain: the kind of JSON a real `TaskHostCore`
    // instrumentation hands over, carrying only the closed contract's own
    // fields — never an operator secret or item text, which is exactly
    // what this test is checking stays true of the WHOLE export, not just
    // this slice's own events.
    await journal.drainAroundRequest(
      () => Promise.resolve(),
      () =>
        JSON.stringify([
          {
            schema_version: 1,
            seq: 1,
            wall_clock_ms: 1_006,
            elapsed_ms: 6,
            session_id: "core-session-1",
            source: "core",
            cycle_id: "c-1",
            operation_id: null,
            request_id: "c-1-0",
            event: { name: "http.started", payload: { method: "GET", route: "/api/changes" } },
          },
        ]),
      () => 1_006,
    );

    const { events } = await journal.export();
    expect(events.length).toBeGreaterThan(0);

    const exportedJson = JSON.stringify(events).toLowerCase();
    for (const forbidden of FORBIDDEN_SUBSTRINGS) {
      expect(exportedJson).not.toContain(forbidden);
    }
  });
});
