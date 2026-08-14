import { describe, expect, it } from "vitest";
import type { SkillEvent } from "./run-state";
import { IDLE, OUTSIDE_SCHEMA, reduceGrillTurn, type GrillTurnState } from "./grill-turn-state";

function drive(events: SkillEvent[], from: GrillTurnState = IDLE): GrillTurnState {
  return events.reduce(reduceGrillTurn, from);
}

const STARTED: SkillEvent = { kind: "started" };

const QUESTION = { prompt: "Which airport?", recommendedAnswer: "SEA", choices: ["SEA", "PDX"] };
const PROPOSAL = { summary: "Settled on SEA", verdict: "resolved" as const, patch: { title: "book flights" } };

describe("reduceGrillTurn", () => {
  it("a tap starts an empty asking state", () => {
    expect(reduceGrillTurn(IDLE, STARTED)).toEqual({ phase: "asking", messages: [] });
  });

  it("a second start while asking leaves the state untouched", () => {
    const asking = drive([STARTED, { kind: "progress", message: "one" }]);
    expect(reduceGrillTurn(asking, STARTED)).toBe(asking);
  });

  it("progress accumulates in order, collapsing consecutive duplicates", () => {
    const state = drive([
      STARTED,
      { kind: "progress", message: "reading HB-42" },
      { kind: "progress", message: "reading HB-42" },
      { kind: "progress", message: "asking" },
    ]);
    expect(state).toEqual({ phase: "asking", messages: ["reading HB-42", "asking"] });
  });

  it("an ok terminal carrying a question turn moves to the question phase, keeping the narration", () => {
    const state = drive([
      STARTED,
      { kind: "progress", message: "reading" },
      { kind: "ok", result: { kind: "question", question: QUESTION }, backend: "cloud", model: "opus" },
    ]);
    expect(state).toEqual({
      phase: "question",
      messages: ["reading"],
      question: QUESTION,
      backend: "cloud",
      model: "opus",
    });
  });

  it("an ok terminal carrying a proposal turn moves to the proposal phase", () => {
    const state = drive([
      STARTED,
      { kind: "ok", result: { kind: "proposal", proposal: PROPOSAL }, backend: "cloud", model: "opus" },
    ]);
    expect(state).toEqual({
      phase: "proposal",
      messages: [],
      proposal: PROPOSAL,
      backend: "cloud",
      model: "opus",
    });
  });

  it("an ok terminal outside the schema declines with a named reason, evidencing an answer", () => {
    const state = drive([
      STARTED,
      { kind: "ok", result: { kind: "neither" }, backend: "cloud", model: "opus" },
    ]);
    expect(state).toEqual({
      phase: "declined",
      messages: [],
      reason: OUTSIDE_SCHEMA,
      backend: "cloud",
      model: "opus",
      answered: true,
    });
  });

  it("a failed terminal declines verbatim, unprefixed", () => {
    const state = drive([
      STARTED,
      { kind: "failed", error: "No device token on this device. Enter one in Settings, then try again.", backend: null, model: null },
    ]);
    expect(state).toEqual({
      phase: "declined",
      messages: [],
      reason: "No device token on this device. Enter one in Settings, then try again.",
      backend: null,
      model: null,
      answered: false,
    });
  });

  it("a routed failure's answered flag carries through", () => {
    const state = drive([
      STARTED,
      { kind: "failed", error: "That item already has live steps.", backend: "cloud", model: "opus", answered: true },
    ]);
    expect(state).toMatchObject({ phase: "declined", answered: true });
  });

  /** Same rule `run-state.ts` states for `reduceRun`: a line arriving
   * before a tap, or after the terminal one, must not reopen a finished
   * turn. */
  it("progress, ok and failed are all no-ops from idle, question, proposal or declined", () => {
    for (const settled of [
      IDLE,
      drive([STARTED, { kind: "ok", result: { kind: "question", question: QUESTION }, backend: null, model: null }]),
      drive([STARTED, { kind: "ok", result: { kind: "proposal", proposal: PROPOSAL }, backend: null, model: null }]),
      drive([STARTED, { kind: "failed", error: "nope", backend: null, model: null }]),
    ]) {
      expect(reduceGrillTurn(settled, { kind: "progress", message: "late" })).toBe(settled);
      expect(
        reduceGrillTurn(settled, { kind: "ok", result: { kind: "question", question: QUESTION }, backend: null, model: null }),
      ).toBe(settled);
      expect(reduceGrillTurn(settled, { kind: "failed", error: "late", backend: null, model: null })).toBe(settled);
    }
  });

  it("an unreadable line is dropped, never terminal", () => {
    const state = drive([STARTED, { kind: "unreadable" }, { kind: "progress", message: "still here" }]);
    expect(state).toEqual({ phase: "asking", messages: ["still here"] });
  });
});
