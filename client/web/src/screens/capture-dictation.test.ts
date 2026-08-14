// #379's pure half, tested the way `capture-validation.test.ts` and
// `capture-meta.test.ts` are: node, no jsdom, no React. What is pinned here is
// the totality property the whole design rests on — the same frozen draft plus
// the same transcript always gives the same string — plus the spacing rules,
// which are the part a reader will actually notice going wrong.

import { describe, expect, it } from "vitest";
import { freezeDraft, spliceTranscript } from "./capture-dictation";
import { canSubmitCapture } from "./capture-validation";

describe("freezeDraft", () => {
  it("round-trips a collapsed caret", () => {
    expect(freezeDraft("call the vet", 5, 5)).toEqual({ prefix: "call ", suffix: "the vet" });
  });

  it("drops a selection's contents — dictating over a selection replaces it", () => {
    // The same thing typing over a selection does; nothing else would explain
    // why the selected words were left behind.
    expect(freezeDraft("call the vet", 5, 8)).toEqual({ prefix: "call ", suffix: " vet" });
  });

  it("orders a backwards selection rather than rejecting it", () => {
    expect(freezeDraft("call the vet", 8, 5)).toEqual({ prefix: "call ", suffix: " vet" });
  });

  it("appends when the field reports no caret at all", () => {
    // An unfocused field has no caret to insert at, and dropping the
    // transcript in front of an existing draft would be a surprise.
    expect(freezeDraft("call the vet", null, null)).toEqual({
      prefix: "call the vet",
      suffix: "",
    });
  });

  it("clamps a caret past the end of the draft", () => {
    expect(freezeDraft("vet", 99, 99)).toEqual({ prefix: "vet", suffix: "" });
    expect(freezeDraft("vet", -4, -4)).toEqual({ prefix: "", suffix: "vet" });
  });
});

describe("spliceTranscript", () => {
  const empty = { prefix: "", suffix: "" };

  it("takes no leading space into an empty draft", () => {
    expect(spliceTranscript(empty, "call the vet")).toEqual({
      value: "call the vet",
      caret: 12,
    });
  });

  it("adds exactly one space after a prefix lacking one, and none after one that has it", () => {
    expect(spliceTranscript({ prefix: "call", suffix: "" }, "the vet").value).toBe("call the vet");
    expect(spliceTranscript({ prefix: "call ", suffix: "" }, "the vet").value).toBe("call the vet");
    expect(spliceTranscript({ prefix: "call\n", suffix: "" }, "the vet").value).toBe(
      "call\nthe vet",
    );
  });

  it("adds one space before a suffix lacking one, and none before one that has it", () => {
    expect(spliceTranscript({ prefix: "call ", suffix: "today" }, "the vet").value).toBe(
      "call the vet today",
    );
    expect(spliceTranscript({ prefix: "call ", suffix: " today" }, "the vet").value).toBe(
      "call the vet today",
    );
  });

  it("never puts a space before , . ? or !", () => {
    for (const suffix of [",", ".", "?", "!"]) {
      expect(spliceTranscript({ prefix: "call ", suffix }, "the vet").value).toBe(
        `call the vet${suffix}`,
      );
    }
  });

  it("puts the caret at the end of the inserted text, before any added space", () => {
    const spliced = spliceTranscript({ prefix: "call ", suffix: "today" }, "the vet");
    expect(spliced.value.slice(0, spliced.caret)).toBe("call the vet");
  });

  it("is total — the same frozen draft and transcript always give the same string", () => {
    // This is what makes a later interim result REPLACE the earlier one in the
    // field rather than accumulate: the component re-splices from the frozen
    // halves every time and never diffs.
    const frozen = { prefix: "call ", suffix: " today" };
    const first = spliceTranscript(frozen, "the");
    const second = spliceTranscript(frozen, "the vet");
    const again = spliceTranscript(frozen, "the vet");
    expect(first.value).toBe("call the today");
    expect(second.value).toBe("call the vet today");
    expect(again).toEqual(second);
  });

  it("is idempotent under re-splicing its own trimmed transcript", () => {
    const frozen = { prefix: "call ", suffix: "" };
    const once = spliceTranscript(frozen, "  the vet  ");
    expect(once.value).toBe("call the vet");
    expect(spliceTranscript(frozen, "the vet")).toEqual(once);
  });

  it("inserts nothing for an all-whitespace transcript, leaving canSubmitCapture false", () => {
    const spliced = spliceTranscript(empty, "   \n ");
    expect(spliced).toEqual({ value: "", caret: 0 });
    expect(canSubmitCapture(spliced.value)).toBe(false);
  });

  it("leaves an existing draft exactly as it was when the session heard nothing", () => {
    expect(spliceTranscript({ prefix: "call ", suffix: "today" }, "")).toEqual({
      value: "call today",
      caret: 5,
    });
  });

  it("splits cleanly around non-ASCII text on both sides", () => {
    // The caret is a UTF-16 index, the same units the DOM's
    // `selectionStart` and `setSelectionRange` speak, so an astral character
    // beside the split point must not be counted in code points here.
    const frozen = freezeDraft("héllo 👋 monde", 8, 8);
    expect(frozen).toEqual({ prefix: "héllo 👋", suffix: " monde" });
    const spliced = spliceTranscript(frozen, "café");
    expect(spliced.value).toBe("héllo 👋 café monde");
    expect(spliced.value.slice(0, spliced.caret)).toBe("héllo 👋 café");
  });
});
