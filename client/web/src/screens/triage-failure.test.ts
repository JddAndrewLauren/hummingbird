import { describe, expect, it } from "vitest";
import { itemDTO } from "../test/component";
import { strandedTriageFailure, triageFailureFor } from "./triage-failure";
import type { TaskTriageResult } from "../store/store";

const result = (overrides: Partial<TaskTriageResult> = {}): TaskTriageResult => ({
  seed: "s1",
  itemId: "c1",
  kind: "failed",
  error: "409 conflict",
  ...overrides,
});

const inbox = [itemDTO({ id: "c1", title: "Ring the plumber", stage: "triage" })];

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
