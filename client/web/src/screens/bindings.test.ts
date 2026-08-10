import { describe, expect, it } from "vitest";
import type { BindingDTO } from "../store/protocol";
import {
  bindingCopy,
  bindingDraftSeed,
  bindingSubmitValue,
  bindingValueLabel,
  bindingWriteError,
  canSubmitBinding,
  sameBindingValue,
} from "./bindings";

function binding(overrides: Partial<BindingDTO> = {}): BindingDTO {
  return {
    key: "race-series",
    known: true,
    pending: false,
    value: { state: "unset" },
    ...overrides,
  };
}

describe("bindingCopy", () => {
  it("names every binding this build can write", () => {
    // The pin against a key gaining a row with no words on it. The
    // vocabulary itself lives in `hummingbird_core::bindings::BindingKey` —
    // this asserts the copy keeps up with it, not the other way round.
    for (const key of ["race-series", "trips-calendar", "city-waste-page"]) {
      const copy = bindingCopy(binding({ key }));
      expect(copy.label).not.toBe(key);
      expect(copy.help.length).toBeGreaterThan(0);
    }
  });

  it("falls back to the raw key for a row this build cannot write, and says why", () => {
    const copy = bindingCopy(binding({ key: "some-future-binding", known: false }));
    expect(copy.label).toBe("some-future-binding");
    expect(copy.help).toMatch(/newer version/);
  });
});

describe("bindingValueLabel", () => {
  it("reads the three states as three different things", () => {
    expect(bindingValueLabel({ state: "unset" })).toBe("Not set");
    expect(bindingValueLabel({ state: "text", text: "f1" })).toBe("f1");
    expect(bindingValueLabel({ state: "other", raw: "7" })).toContain("7");
  });

  it("never renders an unset binding as an empty value", () => {
    // The whole reason the wire shape is a tagged union: "" beside a set
    // binding would read as "set to nothing".
    expect(bindingValueLabel({ state: "unset" })).not.toBe("");
  });
});

describe("bindingDraftSeed", () => {
  it("starts from the current text, and from empty for anything else", () => {
    expect(bindingDraftSeed({ state: "text", text: "f1" })).toBe("f1");
    expect(bindingDraftSeed({ state: "unset" })).toBe("");
    // A value this editor cannot express must not be pre-loaded into a
    // field whose submit would overwrite it with a mangled string.
    expect(bindingDraftSeed({ state: "other", raw: "{\"a\":1}" })).toBe("");
  });
});

describe("canSubmitBinding", () => {
  it("refuses an empty or whitespace-only draft", () => {
    expect(canSubmitBinding(binding(), "")).toBe(false);
    expect(canSubmitBinding(binding(), "   ")).toBe(false);
  });

  it("accepts a real value for an unset binding", () => {
    expect(canSubmitBinding(binding(), "f1")).toBe(true);
  });

  it("refuses a draft identical to what is already stored, padding aside", () => {
    const current = binding({ value: { state: "text", text: "f1" } });
    expect(canSubmitBinding(current, "f1")).toBe(false);
    expect(canSubmitBinding(current, "  f1  ")).toBe(false);
    expect(canSubmitBinding(current, "motogp")).toBe(true);
  });

  it("accepts a text draft over a non-text value — it is a real change", () => {
    expect(canSubmitBinding(binding({ value: { state: "other", raw: "7" } }), "f1")).toBe(true);
  });

  it("refuses everything for a key this build cannot write", () => {
    // `settings` has no DELETE: a key this build cannot name is one it must
    // not touch either.
    const unknown = binding({ key: "some-future-binding", known: false });
    expect(canSubmitBinding(unknown, "anything")).toBe(false);
  });
});

describe("sameBindingValue", () => {
  it("reads two identical values as the same fact", () => {
    expect(sameBindingValue({ state: "unset" }, { state: "unset" })).toBe(true);
    expect(sameBindingValue({ state: "text", text: "f1" }, { state: "text", text: "f1" })).toBe(
      true,
    );
    expect(sameBindingValue({ state: "other", raw: "7" }, { state: "other", raw: "7" })).toBe(true);
  });

  it("reads a changed value as a change, across states and within one", () => {
    expect(sameBindingValue({ state: "text", text: "f1" }, { state: "text", text: "motogp" })).toBe(
      false,
    );
    expect(sameBindingValue({ state: "unset" }, { state: "text", text: "f1" })).toBe(false);
    expect(sameBindingValue({ state: "other", raw: "7" }, { state: "other", raw: "8" })).toBe(false);
  });
});

describe("bindingWriteError", () => {
  const failed = { seed: "s", key: "race-series", kind: "failed", error: "queue is full" } as const;

  it("says nothing when nothing failed", () => {
    expect(bindingWriteError(null, "race-series")).toBeNull();
    expect(
      bindingWriteError({ seed: "s", key: "race-series", kind: "ok", error: null }, "race-series"),
    ).toBeNull();
  });

  it("never bleeds another binding's failure onto this row", () => {
    expect(bindingWriteError(failed, "trips-calendar")).toBeNull();
  });

  it("gives every failing outcome words, carrying the underlying error when there is one", () => {
    expect(bindingWriteError(failed, "race-series")).toBe("queue is full");
    expect(
      bindingWriteError({ ...failed, error: null }, "race-series"),
    ).toMatch(/didn't save/);
    expect(bindingWriteError({ ...failed, kind: "busy" }, "race-series")).toMatch(/busy/);
    expect(bindingWriteError({ ...failed, kind: "unknown_key" }, "race-series")).toMatch(
      /doesn't know/,
    );
  });
});

describe("bindingSubmitValue", () => {
  it("sends the trimmed value, so stored and compared can never differ by invisible padding", () => {
    expect(bindingSubmitValue("  cal-trips  ")).toBe("cal-trips");
  });
});
