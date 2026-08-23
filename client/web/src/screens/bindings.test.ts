import { describe, expect, it } from "vitest";
import type { BindingDTO } from "../store/protocol";
import type { QuestionRosterEntry } from "../decisions/seam";
import {
  bindingCopy,
  bindingDraftSeed,
  bindingSubmitValue,
  bindingValueLabel,
  bindingWriteError,
  canSubmitBinding,
  groupBindingsByQuestion,
  questionLabelForBinding,
  questionSwitchWriteError,
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
    for (const key of [
      "race-series",
      "trips-calendar",
      "city-waste-page",
      "homework-link",
      "scps-quest",
    ]) {
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

// A hand-built roster rather than the core's (#714). These two are pure
// folds over whatever roster they are handed, and the real roster's content
// is the core's own test — reading it here would make the fold's behaviour
// depend on the question vocabulary, so a new question would break a test
// about grouping.
const ROSTER: QuestionRosterEntry[] = [
  { question: "waste", label: "Which cans go out", surface: "now", bindings: ["city-waste-page"] },
  { question: "weekend", label: "This weekend", surface: "now", bindings: [] },
  { question: "race", label: "When is the next race", surface: "now", bindings: ["race-series"] },
];

describe("groupBindingsByQuestion", () => {
  it("nests each row under the question that answers it, in roster order", () => {
    const grouped = groupBindingsByQuestion(ROSTER, [
      binding({ key: "race-series" }),
      binding({ key: "city-waste-page" }),
    ]);

    expect(grouped.groups.map((group) => group.question)).toEqual(["waste", "weekend", "race"]);
    expect(grouped.groups[0].rows.map((row) => row.key)).toEqual(["city-waste-page"]);
    expect(grouped.groups[2].rows.map((row) => row.key)).toEqual(["race-series"]);
    expect(grouped.other).toEqual([]);
  });

  it("keeps a question with no bindings, with an empty body", () => {
    // Not an omission: the roster is the one place a question with nothing
    // to configure can be seen at all (ADR-0034 decision 4).
    const grouped = groupBindingsByQuestion(ROSTER, []);
    expect(grouped.groups.map((group) => group.question)).toEqual(["waste", "weekend", "race"]);
    const weekend = grouped.groups[1];
    expect(weekend.rows).toEqual([]);
    expect(weekend.missing).toEqual([]);
  });

  it("separates a declared key with no row from a question with nothing to set", () => {
    // `Core::bindings` returns every key it knows, so in production this
    // never happens — but the demo world hand-authors a subset, and
    // reporting "nothing to set" for a question whose row simply did not
    // arrive would be the flat opposite of true.
    const grouped = groupBindingsByQuestion(ROSTER, []);
    expect(grouped.groups[0].missing).toEqual(["city-waste-page"]);
    expect(grouped.groups[2].missing).toEqual(["race-series"]);
  });

  it("sends a row no question claims to 'other' rather than dropping it", () => {
    // `Core::bindings` returns rows this build cannot write on purpose;
    // losing them here would hide what is really in the table.
    const unknown = binding({ key: "some-future-binding", known: false });
    const grouped = groupBindingsByQuestion(ROSTER, [unknown, binding({ key: "race-series" })]);
    expect(grouped.other).toEqual([unknown]);
  });

  it("places every input row exactly once, across the groups and the leftovers", () => {
    const rows = [
      binding({ key: "race-series" }),
      binding({ key: "city-waste-page" }),
      binding({ key: "trips-calendar" }),
      binding({ key: "some-future-binding", known: false }),
    ];
    const grouped = groupBindingsByQuestion(ROSTER, rows);
    const placed = [...grouped.groups.flatMap((group) => group.rows), ...grouped.other];
    expect(placed).toHaveLength(rows.length);
    expect(new Set(placed.map((row) => row.key)).size).toBe(rows.length);
  });
});

describe("questionLabelForBinding", () => {
  it("names the question a key answers", () => {
    expect(questionLabelForBinding(ROSTER, "race-series")).toBe("When is the next race");
  });

  it("answers null for a key no question claims", () => {
    expect(questionLabelForBinding(ROSTER, "some-future-binding")).toBeNull();
  });
});

// -- the off switch (#715, ADR-0034) ----------------------------------------

describe("groupBindingsByQuestion — the off switch", () => {
  it("reads an unread switch list as 'no answer', never as everything-on", () => {
    // The default argument is the one the fold takes before the first
    // `questionSwitches` broadcast. `null`, not `true`: a roster that drew
    // ten toggles from a list it had not read would state a fact about the
    // workspace, and the first one to flip would look like a bug.
    const grouped = groupBindingsByQuestion(ROSTER, []);
    expect(grouped.groups.map((group) => group.enabled)).toEqual([null, null, null]);
    expect(grouped.groups.map((group) => group.pending)).toEqual([false, false, false]);
  });

  it("carries each question's own enabled and pending state", () => {
    const grouped = groupBindingsByQuestion(ROSTER, [], [
      { question: "waste", enabled: true, pending: false },
      { question: "weekend", enabled: false, pending: true },
      { question: "race", enabled: true, pending: false },
    ]);
    expect(grouped.groups.map((group) => group.enabled)).toEqual([true, false, true]);
    expect(grouped.groups.map((group) => group.pending)).toEqual([false, true, false]);
  });

  it("reads a question absent from the list as 'no answer for it', not as on", () => {
    // The demo-world asymmetry: a hand-authored list is a subset, and a
    // question it forgot must not be drawn with a toggle nobody answered
    // for — the shape of the copy bug #714 shipped.
    const grouped = groupBindingsByQuestion(ROSTER, [], [
      { question: "waste", enabled: false, pending: false },
    ]);
    expect(grouped.groups.map((group) => group.enabled)).toEqual([false, null, null]);
  });

  it("matches switches by question, never by position", () => {
    const grouped = groupBindingsByQuestion(ROSTER, [], [
      { question: "race", enabled: false, pending: false },
      { question: "waste", enabled: true, pending: false },
      { question: "weekend", enabled: true, pending: false },
    ]);
    expect(grouped.groups.map((group) => [group.question, group.enabled])).toEqual([
      ["waste", true],
      ["weekend", true],
      ["race", false],
    ]);
  });
});

describe("questionSwitchWriteError", () => {
  it("says nothing when there is no write, it succeeded, or it was another question's", () => {
    expect(questionSwitchWriteError(null, "race")).toBeNull();
    expect(
      questionSwitchWriteError(
        { seed: "s", question: "race", kind: "ok", error: null },
        "race",
      ),
    ).toBeNull();
    expect(
      questionSwitchWriteError(
        { seed: "s", question: "waste", kind: "failed", error: "nope" },
        "race",
      ),
    ).toBeNull();
  });

  it("gives every non-ok outcome words on its own question's row", () => {
    expect(
      questionSwitchWriteError(
        { seed: "s", question: "race", kind: "unknown_question", error: null },
        "race",
      ),
    ).toMatch(/doesn't know that question/i);
    expect(
      questionSwitchWriteError({ seed: "s", question: "race", kind: "busy", error: null }, "race"),
    ).toMatch(/busy/i);
    expect(
      questionSwitchWriteError(
        { seed: "s", question: "race", kind: "failed", error: "disk full" },
        "race",
      ),
    ).toBe("disk full");
    // A `failed` with no detail still says something rather than nothing.
    expect(
      questionSwitchWriteError({ seed: "s", question: "race", kind: "failed", error: null }, "race"),
    ).toMatch(/didn't save/i);
  });
});
