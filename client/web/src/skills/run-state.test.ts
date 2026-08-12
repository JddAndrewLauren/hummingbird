import { describe, expect, it } from "vitest";
import { IDLE, isRunning, reduceRun, stampLabel, type SkillEvent, type SkillRunState } from "./run-state";

function drive(events: SkillEvent[], from: SkillRunState = IDLE): SkillRunState {
  return events.reduce(reduceRun, from);
}

const STARTED: SkillEvent = { kind: "started" };

describe("reduceRun", () => {
  it("a tap starts an empty running state", () => {
    expect(reduceRun(IDLE, STARTED)).toEqual({ phase: "running", messages: [] });
    expect(isRunning(reduceRun(IDLE, STARTED))).toBe(true);
  });

  /** The duplicate-tap rule lives here, not only in the button's
   * `disabled` — so a unit test AND a component test each prove it. */
  it("a second start while streaming leaves the state untouched", () => {
    const running = drive([STARTED, { kind: "progress", message: "one" }]);
    expect(reduceRun(running, STARTED)).toBe(running);
  });

  it("progress accumulates in order", () => {
    expect(drive([STARTED, { kind: "progress", message: "a" }, { kind: "progress", message: "b" }])).toEqual({
      phase: "running",
      messages: ["a", "b"],
    });
  });

  /** The runner heartbeats `"still running"` every 20s. */
  it("consecutive duplicate messages collapse", () => {
    const state = drive([
      STARTED,
      { kind: "progress", message: "still running" },
      { kind: "progress", message: "still running" },
      { kind: "progress", message: "still running" },
      { kind: "progress", message: "writing" },
    ]);
    expect(state).toEqual({ phase: "running", messages: ["still running", "writing"] });
  });

  it("a non-consecutive repeat is kept — it is a real second occurrence", () => {
    const state = drive([
      STARTED,
      { kind: "progress", message: "a" },
      { kind: "progress", message: "b" },
      { kind: "progress", message: "a" },
    ]);
    expect(state).toMatchObject({ messages: ["a", "b", "a"] });
  });

  it("an ok terminal keeps the narration and takes the note and the stamp", () => {
    const state = drive([
      STARTED,
      { kind: "progress", message: "reading" },
      {
        kind: "ok",
        result: { steps: ["put on music"], note: "kept 2 ticked steps" },
        backend: "anthropic",
        model: "claude-opus-5",
      },
    ]);
    expect(state).toEqual({
      phase: "done",
      messages: ["reading"],
      note: "kept 2 ticked steps",
      backend: "anthropic",
      model: "claude-opus-5",
    });
  });

  it("an ok terminal whose result is not the schema's shape still completes", () => {
    const state = drive([STARTED, { kind: "ok", result: "surprise", backend: "anthropic", model: null }]);
    expect(state).toMatchObject({ phase: "done", note: "" });
  });

  it("a failed terminal carries the seam's words verbatim", () => {
    const reason = "This item already has 4 unticked steps. Re-run with replace to rewrite them.";
    const state = drive([STARTED, { kind: "failed", error: reason, backend: "anthropic", model: null }]);
    expect(state).toMatchObject({ phase: "declined", reason });
  });

  /** A stream that emits one garbage line has not ended. */
  it("an unreadable line drops from the narration without terminating", () => {
    const state = drive([
      STARTED,
      { kind: "progress", message: "a" },
      { kind: "unreadable" },
      { kind: "progress", message: "b" },
    ]);
    expect(state).toEqual({ phase: "running", messages: ["a", "b"] });
  });

  it("nothing reopens a finished run", () => {
    const done = drive([STARTED, { kind: "ok", result: null, backend: "anthropic", model: null }]);
    for (const event of [
      { kind: "progress", message: "late" },
      { kind: "failed", error: "late", backend: null, model: null },
      { kind: "ok", result: null, backend: null, model: null },
    ] satisfies SkillEvent[]) {
      expect(reduceRun(done, event)).toBe(done);
    }
  });

  it("an event before any tap is ignored", () => {
    expect(reduceRun(IDLE, { kind: "progress", message: "x" })).toBe(IDLE);
  });
});

describe("stampLabel", () => {
  it("joins the backend and the model", () => {
    const state = drive([STARTED, { kind: "ok", result: null, backend: "anthropic", model: "opus" }]);
    expect(stampLabel(state)).toBe("anthropic · opus");
  });

  it("names the backend alone when no model was reported", () => {
    const state = drive([STARTED, { kind: "ok", result: null, backend: "api.moonshot.ai", model: null }]);
    expect(stampLabel(state)).toBe("api.moonshot.ai");
  });

  /** An unstamped envelope means nothing was attempted — so there is
   * nothing to render, and nothing is invented here. */
  it("is null when the envelope named no backend", () => {
    const state = drive([STARTED, { kind: "failed", error: "Cloud runner unreachable.", backend: null, model: null }]);
    expect(stampLabel(state)).toBeNull();
  });

  it("is null while idle and while running", () => {
    expect(stampLabel(IDLE)).toBeNull();
    expect(stampLabel(drive([STARTED]))).toBeNull();
  });

  it("a decline that did reach a model still names it", () => {
    const state = drive([STARTED, { kind: "failed", error: "no", backend: "anthropic", model: "opus" }]);
    expect(stampLabel(state)).toBe("anthropic · opus");
  });
});
