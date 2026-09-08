import { describe, expect, it } from "vitest";
import { homeworkConstantsFromCore } from "../../decisions/seam";
import type { TaskItemDTO } from "../../store/protocol";
import { EMPTY_QUESTION_SYNC, type QuestionInputs } from "../questions/contract";
import {
  LINK_BINDING_KEY,
  SUBJECT_KEY,
  homeworkAnswer,
  homeworkHeadline,
  homeworkLink,
  homeworkSubjects,
  homeworkView,
} from "./homework";

// #675's homework pane: the sentence and the glyphs, which are what stayed
// on this client, plus the round trip through the core that produces the
// facts they are written from.
//
// The band table itself is pinned in Rust (`homework.rs`'s own suite) and
// deliberately not retyped here — this file's job is that the *words*
// follow the number, and that the number reaches them at all.

const CONTEXT = "@homework";

/** Local midnight `days` from now, as the wire spells a deadline. The
 * device zone is whatever the test runner is in, which is exactly the zone
 * the bridge resolves — so "3 days out" here is 3 days out there too. */
function inDays(nowMs: number, days: number): string {
  const at = new Date(nowMs);
  const then = new Date(at.getFullYear(), at.getMonth(), at.getDate() + days);
  const month = String(then.getMonth() + 1).padStart(2, "0");
  const day = String(then.getDate()).padStart(2, "0");
  return `${then.getFullYear()}-${month}-${day}`;
}

function item(overrides: Partial<TaskItemDTO> & { id: string }): TaskItemDTO {
  return {
    seq: null,
    title: overrides.id,
    description: null,
    stage: "ready",
    size: null,
    energy: null,
    context: CONTEXT,
    priority: 2,
    projectId: null,
    projectPos: null,
    deadline: null,
    scheduledDate: null,
    source: null,
    sourceKey: null,
    sourceUrl: null,
    vaultPath: null,
    linkUrl: null,
    linkLabel: null,
    archivedAt: null,
    createdAt: 0,
    updatedAt: 0,
    version: 1,
    pending: false,
    ...overrides,
  };
}

function inputs(
  nowMs: number,
  items: TaskItemDTO[],
  bindings: QuestionInputs["bindings"] = [],
): QuestionInputs {
  return {
    sync: EMPTY_QUESTION_SYNC,
    bindings,
    paneReads: {},
    calendarReads: {},
    calendarConnected: false,
    items,
    nowMs,
  };
}

const NOW = new Date(2026, 7, 21, 9, 0).getTime();

describe("the homework pane's own literals", () => {
  it("matches the core's, rather than being a second copy of them", () => {
    const constants = homeworkConstantsFromCore();
    expect(constants.context).toBe(CONTEXT);
    expect(constants.subjectKey).toBe(SUBJECT_KEY);
    // The band cutoff the headline's "in N days" form runs up to.
    expect(constants.nearWithinDays).toBe(3);
    expect(constants.linkBindingKey).toBe(LINK_BINDING_KEY);
  });

  it("always emits exactly one subject, so the question is discoverable", () => {
    expect(homeworkSubjects()).toEqual([SUBJECT_KEY]);
  });
});

describe("homeworkHeadline", () => {
  it("says the deadline in the reader's terms, one form per band", () => {
    const forms: [number | null, string][] = [
      [-2, "Homework 2 days overdue"],
      [-1, "Homework 1 day overdue"],
      [0, "Homework due today"],
      [1, "Homework due tomorrow"],
      [3, "Homework due in 3 days"],
      [null, "Homework"],
    ];
    for (const [daysAway, expected] of forms) {
      expect(
        homeworkHeadline({
          winner: { id: "a", title: "a", deadline: null, description: null },
          others: [],
          daysAway,
        }),
      ).toBe(expected);
    }
  });

  it("reports an empty homework list as a fact, not as an absence", () => {
    expect(homeworkHeadline({ winner: null, others: [], daysAway: null })).toBe("No open homework");
  });
});

describe("homeworkAnswer", () => {
  it("carries the core's band and the client's own sentence for a dated item", () => {
    const answer = homeworkAnswer(
      SUBJECT_KEY,
      inputs(NOW, [item({ id: "essay", deadline: inDays(NOW, 3) })]),
    );
    expect(answer.answerState).toBe("answered");
    expect(answer.band).toBe("near");
    expect(answer.collapsedHeadline).toBe("Homework due in 3 days");
  });

  it("bands a piece of homework due today live and flags it", () => {
    const answer = homeworkAnswer(
      SUBJECT_KEY,
      inputs(NOW, [item({ id: "essay", deadline: inDays(NOW, 0) })]),
    );
    expect(answer.band).toBe("live");
    expect(answer.collapsedHeadline).toBe("Homework due today");
    expect(answer.icon?.[0]).toEqual({ kind: "icon", name: "flag", label: "due" });
  });

  it("is dormant and answered — never unbound — when nothing is open", () => {
    // Nobody binds this question, so there is no setup prompt to route
    // anyone to and `unbound` would render one.
    const answer = homeworkAnswer(SUBJECT_KEY, inputs(NOW, []));
    expect(answer.answerState).toBe("answered");
    expect(answer.band).toBe("dormant");
    expect(answer.collapsedHeadline).toBe("No open homework");
  });

  it("counts a captured, untriaged item as open homework", () => {
    // The widened `QuestionInputs.items` union (#675) is what makes this
    // reachable at all — before it, `triageInbox` never crossed.
    const answer = homeworkAnswer(
      SUBJECT_KEY,
      inputs(NOW, [item({ id: "captured", stage: "triage", deadline: inDays(NOW, 1) })]),
    );
    expect(answer.band).toBe("imminent");
    expect(answer.collapsedHeadline).toBe("Homework due tomorrow");
  });

  it("marks how many others are open, so the row says there is a queue", () => {
    const answer = homeworkAnswer(
      SUBJECT_KEY,
      inputs(NOW, [
        item({ id: "first", deadline: inDays(NOW, 1) }),
        item({ id: "second", deadline: inDays(NOW, 5) }),
      ]),
    );
    expect(answer.icon).toContainEqual({ kind: "icon", name: "list-checks", label: "1 more open" });
  });
});

describe("homeworkLink", () => {
  function withLink(text: string): QuestionInputs {
    return inputs(NOW, [], [
      { key: LINK_BINDING_KEY, known: true, pending: false, value: { state: "text", text } },
    ]);
  }

  it("offers a bound web URL, whatever the pane is answering", () => {
    const url = "https://example.com/j/000000000";
    // Standing: nothing is open here at all and the link is still offered.
    expect(homeworkLink(withLink(url))).toBe(url);
    expect(homeworkView(withLink(url))?.winner).toBeNull();
  });

  it("offers nothing when the binding is unset or is not a web URL", () => {
    // The scheme filter is the core's — this is the client-side proof that
    // what reaches `window.open` has been through it.
    expect(homeworkLink(inputs(NOW, []))).toBeNull();
    expect(homeworkLink(withLink(""))).toBeNull();
    expect(homeworkLink(withLink("javascript:alert(1)"))).toBeNull();
  });
});

describe("homeworkView", () => {
  it("puts the soonest deadline first and lists the rest beneath it", () => {
    const view = homeworkView(
      inputs(NOW, [
        item({ id: "far", deadline: inDays(NOW, 9) }),
        item({ id: "soon", deadline: inDays(NOW, 1), description: "read chapter 4" }),
        item({ id: "done-one", stage: "done", deadline: inDays(NOW, 0) }),
        item({ id: "other-context", context: "@garden", deadline: inDays(NOW, 0) }),
      ]),
    );
    expect(view?.winner?.id).toBe("soon");
    expect(view?.winner?.description).toBe("read chapter 4");
    expect(view?.others.map((entry) => entry.id)).toEqual(["far"]);
    expect(view?.daysAway).toBe(1);
  });
});
