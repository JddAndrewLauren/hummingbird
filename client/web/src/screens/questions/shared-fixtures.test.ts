import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import {
  orderPanes,
  samePaneIdentity,
  wasteAnswerFromCore,
  wasteFactsFromCore,
  wasteZoneQueries,
  type PaneAnswerCore,
  type PaneInputsSource,
  type RankedPaneLike,
  type ZoneFacts,
} from "../../decisions/seam";
import type { BindingDTO, PaneSnapshotDTO } from "../../store/protocol";
import { SOURCE } from "../waste-pane/waste";
import { resolveZoneFacts } from "./zone-bridge";

// The TS half of the panes' **shared** fixtures (#533/M4).
//
// `client/core/tests/fixtures/panes/*.json` is read here and, byte for
// byte, by `client/core/tests/pane_fixtures.rs`. One artifact, two clients,
// no second copy to drift — the precedent is the race pane's vitest reading
// `server/race-poll/tests/fixtures/golden-body.json` off disk (#119), and
// `client.yml` already covers `client/**`.
//
// **The first describe block below is the point of the whole file.** The
// Rust suite can only take each scenario's `zoneFacts` table as given — it
// has no tzdb to check it against. This side asserts that its OWN `Intl`
// resolver, driven by the queries the core actually asks, reproduces that
// table exactly. That assertion is what makes the fixture a cross-host pin
// rather than two suites agreeing with the same hand-written numbers, and
// it is precisely what a `java.time` resolver will be held to at #536.

const FIXTURES = new URL("../../../../core/tests/fixtures/panes/", import.meta.url);

function fixture(name: string): unknown {
  return JSON.parse(readFileSync(new URL(name, FIXTURES), "utf8"));
}

interface WasteScenario {
  name: string;
  nowMs: number;
  bindings: BindingDTO[] | null;
  snapshot: PaneSnapshotDTO | null;
  zoneFacts: ZoneFacts;
  expected: {
    answerState: PaneAnswerCore["answerState"];
    band: PaneAnswerCore["band"];
    withinBand: number | null;
    daysAway: number | null;
    holiday: boolean | null;
    weekdayIndex: number | null;
    stale: boolean | null;
    gapKind: string | null;
  };
}

const wasteScenarios = (fixture("waste-scenarios.json") as { scenarios: WasteScenario[] }).scenarios;

/** The scenario as this client's `QuestionInputs` would reach the seam —
 * a `null` snapshot is a source that has been read and holds no rows, which
 * is the same "not fetched" the core answers for an absent source. */
function inputsOf(scenario: WasteScenario): PaneInputsSource {
  return {
    nowMs: scenario.nowMs,
    bindings: scenario.bindings,
    paneReads: {
      [SOURCE]: {
        source: SOURCE,
        snapshots: scenario.snapshot === null ? [] : [scenario.snapshot],
        liveAlerts: [],
      },
    },
  };
}

describe("the zone bridge, against the shared fixtures", () => {
  it("is exercised by a fixture that actually has scenarios", () => {
    // Guards the loader: an empty array would make every assertion below
    // vacuously true, and the file is read across a package boundary.
    expect(wasteScenarios.length).toBeGreaterThanOrEqual(20);
  });

  it.each(wasteScenarios.map((scenario) => [scenario.name, scenario] as const))(
    "this host's Intl resolver reproduces the fixture's fact table exactly — %s",
    (_name, scenario) => {
      const queries = wasteZoneQueries(inputsOf(scenario));
      expect(resolveZoneFacts(queries)).toEqual(scenario.zoneFacts);
    },
  );

  it("omits a zone it cannot resolve rather than nulling it", () => {
    // Stated as its own assertion because it is the one arm the `toEqual`
    // above passes on an empty object either way: a resolver that answered
    // `{ key: null }` would fail this and nothing else.
    const martian = wasteScenarios.find((scenario) =>
      scenario.expected.gapKind === "unresolvableZone",
    );
    expect(martian).toBeDefined();
    const queries = wasteZoneQueries(inputsOf(martian!));
    expect(queries.length).toBe(2);
    expect(Object.keys(resolveZoneFacts(queries))).toEqual([]);
  });
});

describe("the waste pane, against the shared fixtures", () => {
  it.each(wasteScenarios.map((scenario) => [scenario.name, scenario] as const))(
    "answers the way both clients must — %s",
    (_name, scenario) => {
      const inputs = inputsOf(scenario);
      const answer = wasteAnswerFromCore(inputs, scenario.zoneFacts);
      expect(answer.answerState).toBe(scenario.expected.answerState);
      expect(answer.band).toBe(scenario.expected.band);
      expect(answer.withinBand).toBe(scenario.expected.withinBand);

      const resolved = wasteFactsFromCore(inputs, scenario.zoneFacts);
      if (resolved.kind === "facts") {
        expect(scenario.expected.gapKind).toBeNull();
        expect(resolved.daysAway).toBe(scenario.expected.daysAway);
        expect(resolved.holiday).toBe(scenario.expected.holiday);
        expect(resolved.weekdayIndex).toBe(scenario.expected.weekdayIndex);
        expect(resolved.stale).toBe(scenario.expected.stale);
      } else {
        expect(resolved.gap.gap).toBe(scenario.expected.gapKind);
      }
    },
  );
});

interface PaneRow {
  question: string;
  subjectKey: string;
  answerState: PaneAnswerCore["answerState"];
  band: PaneAnswerCore["band"];
  withinBand: number | null;
}

function toPane(row: PaneRow): RankedPaneLike {
  return {
    question: row.question,
    subjectKey: row.subjectKey,
    paneKey: `${row.question}:${row.subjectKey}`,
    answer: { answerState: row.answerState, band: row.band, withinBand: row.withinBand },
  };
}

const sortFixture = fixture("sort-scenarios.json") as {
  order: { name: string; questionOrder: string[]; panes: PaneRow[]; expected: string[] }[];
  identity: { name: string; a: PaneRow[]; b: PaneRow[]; expected: boolean }[];
};

describe("the cross-pane sort, against the shared fixtures", () => {
  it("is exercised by a fixture that actually has scenarios", () => {
    expect(sortFixture.order.length).toBeGreaterThanOrEqual(7);
    expect(sortFixture.identity.length).toBeGreaterThanOrEqual(5);
  });

  it.each(sortFixture.order.map((scenario) => [scenario.name, scenario] as const))(
    "orders the way both clients must — %s",
    (_name, scenario) => {
      const ordered = orderPanes(scenario.panes.map(toPane), scenario.questionOrder);
      expect(ordered.map((pane) => pane.paneKey)).toEqual(scenario.expected);
    },
  );

  it.each(sortFixture.identity.map((scenario) => [scenario.name, scenario] as const))(
    "reads pane identity the way both clients must — %s",
    (_name, scenario) => {
      expect(samePaneIdentity(scenario.a.map(toPane), scenario.b.map(toPane))).toBe(
        scenario.expected,
      );
    },
  );
});
