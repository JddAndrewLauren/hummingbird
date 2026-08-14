import { describe, expect, it } from "vitest";
import { itemDTO } from "../test/component";
import {
  actFailureFor,
  strandedActFailure,
  strandedTriageFailure,
  triageFailureFor,
} from "./write-failure";
import type { TaskActResult, TaskTriageResult } from "../store/store";

const result = (overrides: Partial<TaskTriageResult> = {}): TaskTriageResult => ({
  seed: "s1",
  itemId: "c1",
  kind: "failed",
  error: "409 conflict",
  ...overrides,
});

const actResult = (overrides: Partial<TaskActResult> = {}): TaskActResult => ({
  seed: "s1",
  itemId: "i1",
  action: "complete",
  kind: "failed",
  error: "409 conflict",
  ...overrides,
});

const inbox = [itemDTO({ id: "c1", title: "Ring the plumber", stage: "triage" })];
const board = [itemDTO({ id: "i1", title: "Draft the invoice", stage: "ready" })];

describe("triageFailureFor", () => {
  it("states the server's own words for the item the result names", () => {
    expect(triageFailureFor(result(), "c1")).toBe("409 conflict");
  });

  it("falls back to a sentence when the failure carried no message", () => {
    expect(triageFailureFor(result({ error: null }), "c1")).toBe("That triage didn't apply.");
  });

  it("is silent for every other item — a failure belongs to the one it names", () => {
    expect(triageFailureFor(result(), "c2")).toBeNull();
  });

  it("is silent for a success, and before any result has arrived", () => {
    expect(triageFailureFor(result({ kind: "ok", error: null }), "c1")).toBeNull();
    expect(triageFailureFor(null, "c1")).toBeNull();
    expect(triageFailureFor(undefined, "c1")).toBeNull();
  });

  it("reports the other non-ok kinds, which are failures too", () => {
    expect(triageFailureFor(result({ kind: "not_found", error: null }), "c1")).toBe(
      "That triage didn't apply.",
    );
    expect(triageFailureFor(result({ kind: "busy", error: null }), "c1")).toBe(
      "That triage didn't apply.",
    );
  });
});

describe("actFailureFor", () => {
  it("states the server's own words for the item the result names", () => {
    expect(actFailureFor(actResult(), "i1")).toBe("409 conflict");
  });

  it("falls back to the act's own sentence, never triage's", () => {
    expect(actFailureFor(actResult({ error: null }), "i1")).toBe("That action didn't apply.");
  });

  it("is silent for every other item, for a success, and before any result", () => {
    expect(actFailureFor(actResult(), "i2")).toBeNull();
    expect(actFailureFor(actResult({ kind: "ok", error: null }), "i1")).toBeNull();
    expect(actFailureFor(null, "i1")).toBeNull();
    expect(actFailureFor(undefined, "i1")).toBeNull();
  });

  it("reports the other non-ok kinds, which are failures too", () => {
    expect(actFailureFor(actResult({ kind: "not_found", error: null }), "i1")).toBe(
      "That action didn't apply.",
    );
    expect(actFailureFor(actResult({ kind: "busy", error: null }), "i1")).toBe(
      "That action didn't apply.",
    );
  });
});

describe("strandedTriageFailure", () => {
  it("names the capture, so the reader knows which triage failed", () => {
    expect(strandedTriageFailure(result(), null, inbox)).toBe(
      'Triage didn\'t apply to "Ring the plumber" — 409 conflict',
    );
  });

  it("names it without a server message too", () => {
    expect(strandedTriageFailure(result({ error: null }), null, inbox)).toBe(
      'Triage didn\'t apply to "Ring the plumber".',
    );
  });

  it("says nothing while that capture is the open one — its row owns the message", () => {
    expect(strandedTriageFailure(result(), "c1", inbox)).toBeNull();
  });

  it("still speaks while a DIFFERENT capture is open", () => {
    expect(strandedTriageFailure(result(), "c2", inbox)).toBe(
      'Triage didn\'t apply to "Ring the plumber" — 409 conflict',
    );
  });

  it("drops the name rather than inventing one when the item is not on the board", () => {
    expect(strandedTriageFailure(result({ itemId: "gone" }), null, inbox)).toBe("409 conflict");
    expect(strandedTriageFailure(result({ itemId: "gone", error: null }), null, [])).toBe(
      "That triage didn't apply.",
    );
  });

  it("is silent for a success and for no result at all", () => {
    expect(strandedTriageFailure(result({ kind: "ok", error: null }), null, inbox)).toBeNull();
    expect(strandedTriageFailure(null, null, inbox)).toBeNull();
  });
});

describe("strandedActFailure", () => {
  it("names the item, so the reader knows which act failed", () => {
    expect(strandedActFailure(actResult(), null, board)).toBe(
      'That action didn\'t apply to "Draft the invoice" — 409 conflict',
    );
  });

  it("names it without a server message too", () => {
    expect(strandedActFailure(actResult({ error: null }), null, board)).toBe(
      'That action didn\'t apply to "Draft the invoice".',
    );
  });

  it("says nothing while that item's detail panel is open — the panel owns it", () => {
    expect(strandedActFailure(actResult(), "i1", board)).toBeNull();
  });

  it("still speaks while a DIFFERENT item is open", () => {
    expect(strandedActFailure(actResult(), "i2", board)).toBe(
      'That action didn\'t apply to "Draft the invoice" — 409 conflict',
    );
  });

  it("names a capture too — an open capture acts, and no panel wears its failure", () => {
    expect(strandedActFailure(actResult({ itemId: "c1" }), null, [...board, ...inbox])).toBe(
      'That action didn\'t apply to "Ring the plumber" — 409 conflict',
    );
  });

  it("drops the name rather than inventing one when the item has left the board", () => {
    expect(strandedActFailure(actResult({ itemId: "gone" }), null, board)).toBe("409 conflict");
    expect(strandedActFailure(actResult({ itemId: "gone", error: null }), null, [])).toBe(
      "That action didn't apply.",
    );
  });

  it("is silent for a success and for no result at all", () => {
    expect(strandedActFailure(actResult({ kind: "ok", error: null }), null, board)).toBeNull();
    expect(strandedActFailure(null, null, board)).toBeNull();
  });
});
