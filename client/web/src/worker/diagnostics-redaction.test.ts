import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { IDBFactory, IDBKeyRange } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import { createDiagnosticsJournal } from "./diagnostics-journal";
import { createDiagnosticsStore } from "./diagnostics-store";

// #707's own half of #706's redaction rule
// (`server/domain/src/diagnostics.rs`'s `FORBIDDEN_FIELD_NAMES`): a real
// exported journal, built the same way `core.worker.ts` builds one — every
// worker-layer event family this slice emits, plus a simulated Core-sourced
// drain (opaque JSON this side never inspects, exactly as #708's
// `TaskCoreCell::drain_diagnostics` now hands it
// over) — must never carry a credential, a request/response body,
// or any of the operator's own item text. This is the value-level half of
// the check `diagnostics.rs`'s own test documents its limits on; this
// file's job is only to prove THIS export path never introduces one, not to
// re-litigate the shared enum's own coverage.
//
// **The list is read from `diagnostics.rs`'s own source, not hand-copied.**
// Review round 1 of PR #736 caught a hand-copied subset that silently dropped
// `title`/`description`/`message`/`body`/`url`/`ip`/`exception` — an
// ungated subset that mutation-tested green on the words it kept and
// vacuous on the ones it dropped, with no drift signal if the owner's list
// ever grew. `worker-import-graph.test.ts` already reads a source file's text at
// test time for the identical reason (a comment cannot enforce an
// invariant, a parse of the real file can) — this does the same over
// `diagnostics.rs`'s `FORBIDDEN_FIELD_NAMES` array literal, so a future
// addition to that list is picked up here with no edit to this file at all,
// and a rename or reshape of the const that this regex can no longer find
// fails the "the list actually has entries" assertion below rather than
// silently scanning against an empty array.
//
// The list lives in `hummingbird-domain`, not `hummingbird-core`: #711 moved
// the whole `DiagnosticEventV1` envelope (and this const with it) into
// `server/domain/src/diagnostics.rs` so the authority's request boundary —
// a *server*-workspace crate — could name the same types. `client/core`'s
// `diagnostics/mod.rs` is now re-exports only, and holds no list to read.
const DOMAIN_DIAGNOSTICS_RS = resolve(
  process.cwd(),
  "../../server/domain/src/diagnostics.rs",
);

function readForbiddenFieldNamesFromDomainSource(): string[] {
  const source = readFileSync(DOMAIN_DIAGNOSTICS_RS, "utf8");
  const match = /const FORBIDDEN_FIELD_NAMES: &\[&str\] = &\[([\s\S]*?)\];/.exec(source);
  if (match === null) {
    return [];
  }
  return [...match[1].matchAll(/"([^"]+)"/g)].map((entry) => entry[1]);
}

describe("a real exported journal never carries a forbidden field or value", () => {
  const forbidden = readForbiddenFieldNamesFromDomainSource();

  it("actually found the owner enum's list — guards the reader itself, not a vacuous empty scan", () => {
    // If `diagnostics.rs`'s const is ever renamed or reshaped past what the regex
    // above can parse, this fails LOUDLY here rather than letting the test
    // below pass by scanning against nothing.
    expect(forbidden.length).toBeGreaterThanOrEqual(15);
    expect(forbidden).toContain("token");
    expect(forbidden).toContain("title");
    expect(forbidden).toContain("description");
  });

  it("scans a full export — every worker-layer family plus a simulated Core drain — against the owner enum's own forbidden list, plus the mirror", async () => {
    const store = createDiagnosticsStore(new IDBFactory(), IDBKeyRange);
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
    for (const word of forbidden) {
      expect(exportedJson).not.toContain(word.toLowerCase());
    }

    // #706's own list has no entry for this — it is #707's own acceptance
    // criterion: "a diagnostics export contains logs only... the mirror is
    // not present in it under any code path." `getDiagnostics`/
    // `getMirrorSnapshot` are two different protocol messages answered by
    // two different handlers (`dispatch.ts` intercepts the former before
    // it can reach the task queue where `mirrorSnapshot()` lives — see
    // `dispatch.test.ts`'s own "reaching neither wasm queue" assertions for
    // that structural proof); this is the value-level check that no mirror
    // data, or even the word, ever lands in what this module serializes.
    expect(exportedJson).not.toContain("mirror");
  });
});
