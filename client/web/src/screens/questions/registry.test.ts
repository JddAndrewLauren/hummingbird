import { describe, expect, it } from "vitest";
import {
  QUESTION_ORDER,
  EMPTY_QUESTION_SYNC,
  boundedGlyphs,
  paneKey,
  type PaneGlyph,
  type QuestionDef,
  type QuestionInputs,
  type StandingQuestion,
} from "./contract";
import { QUESTIONS, panesFrom, rankPanes, requiredCalendarRequests, requiredSources } from "./registry";
import { questionLabel, questionRoster } from "./roster";

// The registry, and the two properties that keep it honest: it names every
// question exactly once, and it ranks every subject of every question — not
// just the one-subject case the only shipped pane happens to be.

function emptyInputs(): QuestionInputs {
  return {
    sync: EMPTY_QUESTION_SYNC,
    bindings: [],
    paneReads: {},
    calendarReads: {},
    calendarConnected: false,
    items: [],
    nowMs: 1_000,
  };
}

/** A registry of questions that do not exist in the shipped vocabulary —
 * the only way to run a multi-subject (or subject-less) question through the
 * real expansion, since `StandingQuestion` is deliberately closed. */
function fakeRegistry(
  questions: Record<string, QuestionDef>,
): Record<StandingQuestion, QuestionDef> {
  return questions as unknown as Record<StandingQuestion, QuestionDef>;
}

describe("QUESTION_ORDER", () => {
  it("names every registered question exactly once", () => {
    expect([...QUESTION_ORDER].sort()).toEqual(Object.keys(QUESTIONS).sort());
    expect(new Set(QUESTION_ORDER).size).toBe(QUESTION_ORDER.length);
  });

  // The label is no longer the registry's (#714): it is one field of the
  // core's standing-question roster, read back through `questionLabel`. This
  // asserts the registry and the roster still describe the same ten
  // questions — a question registered here with no roster entry throws.
  it("gives every registered question a name from the core's roster", () => {
    for (const question of QUESTION_ORDER) {
      expect(questionLabel(question)).not.toBe("");
    }
    expect(questionRoster().map((entry) => entry.question)).toEqual([...QUESTION_ORDER]);
  });

  it("takes the surface from the same roster the core ranks against", () => {
    for (const entry of questionRoster()) {
      expect(entry.surface).toBe(QUESTIONS[entry.question as StandingQuestion].surface);
    }
  });
});

describe("requiredSources", () => {
  it("unions every 'now' question's sources without repeating one", () => {
    const sources = requiredSources("now");
    expect(new Set(sources).size).toBe(sources.length);
    for (const question of QUESTION_ORDER) {
      if (QUESTIONS[question].surface !== "now") {
        continue;
      }
      for (const source of QUESTIONS[question].sources) {
        expect(sources).toContain(source);
      }
    }
  });

  it("never asks for a source only a question on the OTHER surface reads (ADR-0017, #311)", () => {
    for (const surface of ["now", "status"] as const) {
      const sources = requiredSources(surface);
      for (const question of QUESTION_ORDER) {
        if (QUESTIONS[question].surface === surface) {
          continue;
        }
        for (const source of QUESTIONS[question].sources) {
          expect(sources).not.toContain(source);
        }
      }
    }
  });
});

describe("requiredCalendarRequests", () => {
  it("unions every registered calendar-lane question's own interval (#122, #121)", () => {
    // This was the empty steady state before #122 registered the first
    // calendar-lane question, and it grew again with #121's — proof the
    // union mechanism, not a fixed list, is what decides this.
    const requests = requiredCalendarRequests(1_000);
    expect(requests.map((request) => request.key)).toEqual(["scps", "weekend", "vacation"]);
    for (const request of requests) {
      expect(request.endMs).toBeGreaterThan(request.startMs);
      expect(request.startDate).toEqual(expect.any(String));
      expect(request.endDate).toEqual(expect.any(String));
      expect(request.endDate > request.startDate).toBe(true);
    }
  });

  it("is a pure function of the clock, with no calendar read of its own", () => {
    const a = requiredCalendarRequests(1_000);
    const b = requiredCalendarRequests(1_000);
    expect(a).toEqual(b);
  });
});

describe("rankPanes", () => {
  it("still emits a pane for every question, none of them falsely answered, when nothing is set up", () => {
    // The setup prompt is how a question is discovered at all — a registry
    // that dropped an unconfigured question would hide it from the person
    // who has to set it up. Not every question reads `unbound` from
    // `emptyInputs()` specifically: `waste` (an unset binding) does, while
    // `weekend` (a calendar arm nobody has requested a read for yet) is
    // `bound-but-unacquired` — both are gaps, and the shared assertion here
    // is the one thing every registered question owes: never `answered`
    // with nothing behind it.
    //
    // `homework` (#675) is the exception the rule was always going to meet:
    // nobody binds it and nothing polls it, so an empty mirror is not a gap
    // — it is the real answer, "No open homework", and it is `answered` and
    // `dormant`. Excluded by name rather than by weakening the assertion,
    // so a *second* question that starts answering emptily still fails here.
    const panes = rankPanes(emptyInputs(), "now");
    expect(panes.length).toBeGreaterThan(0);
    const bindable = panes.filter((pane) => pane.question !== "homework");
    expect(bindable.length).toBeGreaterThan(0);
    expect(bindable.every((pane) => pane.answer.answerState !== "answered")).toBe(true);

    const homework = panes.find((pane) => pane.question === "homework");
    expect(homework?.answer.answerState).toBe("answered");
    expect(homework?.answer.band).toBe("dormant");
  });

  it("keys each pane by question and subject, never by position", () => {
    for (const pane of rankPanes(emptyInputs(), "now")) {
      expect(pane.paneKey).toBe(paneKey(pane.question, pane.subjectKey));
    }
  });

  it("filters to exactly one surface's questions (ADR-0017, #311)", () => {
    const nowPanes = rankPanes(emptyInputs(), "now");
    const statusPanes = rankPanes(emptyInputs(), "status");
    expect(nowPanes.length).toBeGreaterThan(0);
    expect(statusPanes.length).toBeGreaterThan(0);
    for (const pane of nowPanes) {
      expect(QUESTIONS[pane.question].surface).toBe("now");
    }
    for (const pane of statusPanes) {
      expect(QUESTIONS[pane.question].surface).toBe("status");
    }
    // No question appears on both — the two lists never overlap.
    const nowQuestions = new Set(nowPanes.map((pane) => pane.question));
    for (const pane of statusPanes) {
      expect(nowQuestions.has(pane.question)).toBe(false);
    }
  });

  it("renders every never-polled infra question as a gap, never as nothing (ADR-0017 decision 4, #311)", () => {
    const statusPanes = rankPanes(emptyInputs(), "status");
    expect(statusPanes).toHaveLength(4);
    expect(statusPanes.every((pane) => pane.answer.answerState === "bound-but-unacquired")).toBe(
      true,
    );
  });

  it("ranks every subject of a multi-subject question, and none of a question with no subjects", () => {
    // No shipped question emits more than one subject — or none — so the
    // 0..N contract is exercised here, through `panesFrom`, which is the
    // expansion `rankPanes` itself runs. Hand-building the panes and sorting
    // them would test the sort a second time and the expansion not at all.
    const multi: QuestionDef = {
      surface: "now",
      sources: [],
      subjects: () => ["b", "a"],
      answer: (subjectKey) => ({
        answerState: "answered",
        band: "near",
        withinBand: subjectKey === "a" ? 1 : 2,
        collapsedHeadline: subjectKey,
      }),
      Expanded: () => null,
    };
    const none: QuestionDef = { ...multi, subjects: () => [] };

    const panes = panesFrom(
      fakeRegistry({ multi, none }),
      ["multi", "none"] as unknown as StandingQuestion[],
      emptyInputs(),
    );

    // Both of the one question's subjects survive, keyed by subject and
    // ordered by `withinBand` rather than by the order `subjects()` returned
    // — and the subject-less question contributes no pane at all.
    expect(panes.map((pane) => pane.paneKey)).toEqual(["multi:a", "multi:b"]);
  });
});

describe("boundedGlyphs", () => {
  it("caps a pane's glyphs and treats none as none", () => {
    const many: PaneGlyph[] = Array.from({ length: 9 }, (_, index) => ({
      kind: "dot",
      fill: "#000",
      edge: "#000",
      label: `dot-${index}`,
    }));
    expect(boundedGlyphs(many)).toHaveLength(4);
    expect(boundedGlyphs(undefined)).toEqual([]);
  });
});

describe("rankPanes — the off switch (#715, ADR-0034)", () => {
  it("ranks every question when nothing is switched off", () => {
    // Both spellings of "nothing off" — an absent field and an empty list —
    // must agree, since the field is optional exactly so a caller that has
    // not thought about it gets every question asked.
    const surfaces = ["now", "status"] as const;
    for (const surface of surfaces) {
      const absent = rankPanes(emptyInputs(), surface).map((pane) => pane.question);
      const empty = rankPanes({ ...emptyInputs(), disabledQuestions: [] }, surface).map(
        (pane) => pane.question,
      );
      expect(empty).toEqual(absent);
      expect(absent.length).toBeGreaterThan(0);
    }
  });

  it("emits no pane at all for a question switched off", () => {
    // Not a dormant pane and not a sentinel — nothing. The question is
    // still discoverable in Settings' roster, which is the precondition
    // ADR-0034 made the switch legal on.
    const before = rankPanes(emptyInputs(), "now").map((pane) => pane.question);
    expect(before).toContain("weekend");

    const after = rankPanes(
      { ...emptyInputs(), disabledQuestions: ["weekend"] },
      "now",
    ).map((pane) => pane.question);

    expect(after).toEqual(before.filter((question) => question !== "weekend"));
  });

  it("switches off a Status question without touching Now, and the reverse", () => {
    const now = rankPanes({ ...emptyInputs(), disabledQuestions: ["kimi"] }, "now");
    expect(now.map((pane) => pane.question)).toEqual(
      rankPanes(emptyInputs(), "now").map((pane) => pane.question),
    );
    const status = rankPanes({ ...emptyInputs(), disabledQuestions: ["kimi"] }, "status");
    expect(status.map((pane) => pane.question)).not.toContain("kimi");
  });

  it("leaves a surface genuinely empty when every one of its questions is off", () => {
    const inputs = { ...emptyInputs(), disabledQuestions: [...QUESTION_ORDER] };
    expect(rankPanes(inputs, "now")).toEqual([]);
    expect(rankPanes(inputs, "status")).toEqual([]);
  });

  it("ignores a question name this build does not know", () => {
    const inputs = { ...emptyInputs(), disabledQuestions: ["fantasy", ""] };
    expect(rankPanes(inputs, "now").map((pane) => pane.question)).toEqual(
      rankPanes(emptyInputs(), "now").map((pane) => pane.question),
    );
  });

  // Deliberately NOT tested here, and untestable by construction: that
  // `requiredSources` ignores the switch. It takes no inputs at all, so
  // there is nothing to vary — which is the pin. The reasoning lives on
  // `askedQuestionsFor` in `registry.ts`: a pane read is a synchronous read
  // of rows this device already pulled, so narrowing it saves no traffic and
  // would leave a re-enabled question rendering "not read yet".
});
