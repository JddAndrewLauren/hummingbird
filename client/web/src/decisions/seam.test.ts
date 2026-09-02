import { afterAll, beforeEach, describe, expect, it } from "vitest";

import {
  backendAutoSelectionFromCore,
  canSubmitCapture,
  declinedBackendFallbackFromCore,
  declineForResponseFromCore,
  declineForTransportFromCore,
  decisionsReady,
  deviceZoneFromCore,
  ENERGIES,
  energyOptionsFromCore,
  FACETS,
  fallbackBackendIdFromCore,
  frontierAxesFromCore,
  githubConstantsFromCore,
  grillDemotesFromFrontierFromCore,
  grillFrontierDemotionWarningFromCore,
  grillPlanReplacementLabelFromCore,
  grillWouldStrandPlanFromCore,
  initDecisions,
  kimiConstantsFromCore,
  microtaskAffordanceFromCore,
  noTerminalLineDeclineFromCore,
  noTokenDeclineFromCore,
  orderFrontier,
  outsideSchemaDeclineFromCore,
  priorityRankFromCore,
  raceConstantsFromCore,
  reachabilityConstantsFromCore,
  resetDecisionsForTest,
  resolveBackendSelectionFromCore,
  paneBandOrderFromCore,
  paneQuestionOrderFromCore,
  pollerConstantsFromCore,
  scpsConstantsFromCore,
  SIZES,
  sizeOptionsFromCore,
  uptimeConstantsFromCore,
  vacationConstantsFromCore,
  wasteConstantsFromCore,
  weekendConstantsFromCore,
  paneZoneQueries,
} from "./seam";
import { priorityRank } from "../screens/priority";
import { AUTO_SELECTION, BACKEND_REGISTRY, fallbackEntry } from "../skills/backend-registry";
import { readBackendSelection } from "../skills/backend-selection";
import { declineForResponse, declineForTransport, NO_TERMINAL_LINE, NO_TOKEN } from "../skills/decline";
import { OUTSIDE_SCHEMA } from "../skills/grill-turn-state";
import { microtaskAffordance } from "../skills/microtask-affordance";
import {
  demotesFromFrontier,
  FRONTIER_DEMOTION_WARNING,
  planReplacementLabel,
  wouldStrandPlan,
} from "../screens/grill-review";
import { BAND_ORDER, QUESTION_ORDER } from "../screens/questions/contract";
import {
  BINDING_KEY,
  SNAPSHOT_KEY,
  SOURCE,
  STALE_AFTER_MS,
  STREAM_ORDER,
} from "../screens/waste-pane/waste";
import {
  IMMINENT_THRESHOLD_USD,
  NEAR_THRESHOLD_USD,
  SNAPSHOT_KEY as KIMI_SNAPSHOT_KEY,
  SOURCE as KIMI_SOURCE,
  STALE_AFTER_MS as KIMI_STALE_AFTER_MS,
} from "../screens/kimi-pane/kimi";
import {
  MIN_OVERDUE_AFTER_MS,
  NEVER_POLLED_SUBJECT as GITHUB_NEVER_POLLED_SUBJECT,
  OVERDUE_MULTIPLIER,
  SOURCE as GITHUB_SOURCE,
  STALE_AFTER_MS as GITHUB_STALE_AFTER_MS,
} from "../screens/github-pane/github";
import {
  NEVER_POLLED_SUBJECT as UPTIME_NEVER_POLLED_SUBJECT,
  SOURCE as UPTIME_SOURCE,
  STALE_AFTER_MS as UPTIME_STALE_AFTER_MS,
} from "../screens/uptime-pane/uptime";
import { REACHABILITY_GRACE_MS, SUBJECT_KEY as REACHABILITY_SUBJECT_KEY } from "../screens/reachability-pane/reachability";
import {
  FLOOR_MS as POLLER_FLOOR_MS,
  OVERDUE_MULTIPLIER as POLLER_OVERDUE_MULTIPLIER,
  SOURCES as POLLER_SOURCES,
} from "../screens/poller-pane/poller";
import {
  BINDING_KEY as RACE_BINDING_KEY,
  SETUP_SUBJECT,
  SOURCE as RACE_SOURCE,
  STALE_AFTER_MS as RACE_STALE_AFTER_MS,
} from "../screens/race-pane/race";
import {
  CALENDAR_REQUEST_KEY as WEEKEND_CALENDAR_REQUEST_KEY,
  IMMINENT_WITHIN_MS as WEEKEND_IMMINENT_WITHIN_MS,
  NEAR_WITHIN_MS as WEEKEND_NEAR_WITHIN_MS,
  SUBJECT_KEY as WEEKEND_SUBJECT_KEY,
} from "../screens/weekend-pane/weekend";
import {
  CALENDAR_REQUEST_KEY as VACATION_CALENDAR_REQUEST_KEY,
  HORIZON_AHEAD_DAYS,
  HORIZON_BEFORE_DAYS,
  STALE_AFTER_MS as VACATION_STALE_AFTER_MS,
  SUBJECT_KEY as VACATION_SUBJECT_KEY,
} from "../screens/vacation-pane/vacation";
import {
  CALENDAR_REQUEST_KEY as SCPS_CALENDAR_REQUEST_KEY,
  HORIZON_AFTER_DAYS as SCPS_HORIZON_AFTER_DAYS,
  HORIZON_BEFORE_MS as SCPS_HORIZON_BEFORE_MS,
  QUEST_BINDING_KEY as SCPS_QUEST_BINDING_KEY,
  STALE_AFTER_MS as SCPS_STALE_AFTER_MS,
  SUBJECT_KEY as SCPS_SUBJECT_KEY,
} from "../screens/scps-pane/scps";
import { DEVICE_ZONE } from "../screens/questions/zone-bridge";
import { loadDecisionsForTest } from "../test/wasm-setup";
import type { StepDTO, TaskItemDTO } from "../store/protocol";

// The node half of "vitest executes the seam in both environments" — this
// file runs under the default `environment: "node"`, and
// `seam.jsdom.test.ts` is the same proof under jsdom. Two files rather than
// one parameterised suite because the environment is a per-file docblock.

/** The two invisibles `str::trim` and `String.trim()` disagree about, named
 * rather than written literally — a raw one in the source is unreadable and
 * the next reader would delete it as an accident. */
const BOM = "\u{feff}";
const NEL = "\u{85}";

describe("the decision seam", () => {
  it("is already instantiated by the shared setup file", () => {
    expect(decisionsReady()).toBe(true);
  });

  it("answers the capture rule out of the core", () => {
    expect(canSubmitCapture("")).toBe(false);
    expect(canSubmitCapture("   ")).toBe(false);
    expect(canSubmitCapture("buy milk")).toBe(true);
  });

  // The core states its own blank-draft alphabet rather than inheriting
  // `str::trim`, whose set differs from `String.trim()`'s in both
  // directions (`decisions/capture.rs`). Pinned from the JS side too,
  // because this is the side that used to decide it and the side a reader
  // will assume still does.
  it("refuses a draft of nothing but invisibles, BOM included", () => {
    expect(canSubmitCapture(BOM)).toBe(false);
    expect(canSubmitCapture(NEL)).toBe(false);
    expect(canSubmitCapture(`${BOM}buy milk`)).toBe(true);
  });

  it("crosses a whole frontier's worth of items and back, ordered", () => {
    const a = syntheticItem("a", "ready");
    const b = { ...syntheticItem("b", "ready"), priority: 1 };
    expect(orderFrontier([a, b]).map((item) => item.id)).toEqual(["b", "a"]);
  });
});

// M1-3 (#501): `SIZES`/`ENERGIES`/`FACETS` stay literal TS arrays in
// `frontier-facets.ts`'s shim (the same module-evaluation-order constraint
// `field-vocabulary.ts`'s header states), pinned here against the crate
// that cannot drift from `hummingbird_domain::Size`/`Energy` or
// `decisions::vocabulary::FRONTIER_AXES` because it is built on them — the
// M1-2 review's own note that this was "the one surviving unpinned
// vocabulary copy".
describe("the seam's literal frontier-facet vocabulary, pinned against the core", () => {
  it("SIZES matches the core's size vocabulary", () => {
    expect([...SIZES]).toEqual(sizeOptionsFromCore().map((option) => option.value));
  });

  it("ENERGIES matches the core's energy vocabulary", () => {
    expect([...ENERGIES]).toEqual(energyOptionsFromCore().map((option) => option.value));
  });

  it("FACETS matches the core's frontier facet axes", () => {
    expect([...FACETS]).toEqual(frontierAxesFromCore());
  });

  // `priority.ts`'s `priorityRank` is the one vocabulary the M1-3 review
  // found still duplicated (`client/core/src/decisions/frontier.rs`'s own
  // `priority_rank`, unpinned) — pinned here the same way the three literal
  // arrays above are.
  it("priorityRank matches the core's priority rank, for every real value and an unrecognised one", () => {
    for (const raw of [0, 1, 2, 3, 4, 5, -1]) {
      expect(priorityRank(raw)).toEqual(priorityRankFromCore(raw));
    }
  });
});

// M4 (#538): the skills lane's three module-evaluation-time constants. Same
// carve-out, same reason, same pin — `NO_TOKEN` and `NO_TERMINAL_LINE` are
// read at module-evaluation time by `route-run.ts` and
// `useMicrotaskWiring.ts`, and `OUTSIDE_SCHEMA` by `useGrillWiring.ts`, all
// statically reachable from `main.tsx`, so a seam call there would throw the
// "used before ready" guard on every page load. Everything else in that lane
// calls across; only these three stay literal, and only because they cannot.
describe("the skills lane's literal decline prose, pinned against the core", () => {
  it("NO_TOKEN matches the core's words", () => {
    expect(NO_TOKEN).toBe(noTokenDeclineFromCore());
  });

  it("NO_TERMINAL_LINE matches the core's words", () => {
    expect(NO_TERMINAL_LINE).toBe(noTerminalLineDeclineFromCore());
  });

  it("OUTSIDE_SCHEMA matches the core's words", () => {
    expect(OUTSIDE_SCHEMA).toBe(outsideSchemaDeclineFromCore());
  });

  /** The two that are *not* constants — proof they were not quietly copied
   * alongside the three that had to be. */
  it("the two decline functions answer out of the core, not a TS copy", () => {
    expect(declineForResponse(401)).toBe(declineForResponseFromCore(401));
    expect(declineForResponse(503)).toBe(declineForResponseFromCore(503));
    expect(declineForTransport("boom")).toBe(declineForTransportFromCore("boom"));
  });
});

// M4 (#539): the microtask affordance, the backend picker's tier fallback
// and degrade-to-Auto rule, and the Grill review card's predicates —
// `microtask-affordance.ts`, `backend-registry.ts`/`backend-selection.ts`
// and `grill-review.ts` are now thin wrappers over the seam. `AUTO_SELECTION`
// and `FRONTIER_DEMOTION_WARNING` stay literal TS for the same
// module-evaluation-order reason the three decline constants above do
// (`backend-registry.ts`'s and `grill-review.ts`'s own headers).
describe("the microtask affordance, the backend fallback and the Grill review predicates, out of the core", () => {
  function step(overrides: Partial<StepDTO> = {}): StepDTO {
    return {
      id: "step-1",
      itemId: "item-1",
      body: "pack",
      done: false,
      position: 0,
      deletedAt: null,
      version: 1,
      ...overrides,
    };
  }

  it("microtaskAffordance answers out of the core", () => {
    expect(microtaskAffordance([])).toEqual(microtaskAffordanceFromCore([]));
    const steps = [step()];
    expect(microtaskAffordance(steps)).toEqual(microtaskAffordanceFromCore(steps));
  });

  it("AUTO_SELECTION matches the core's sentinel", () => {
    expect(AUTO_SELECTION).toBe(backendAutoSelectionFromCore());
  });

  it("fallbackEntry answers out of the core's fallback_backend_id", () => {
    const registry = [
      { id: "a", label: "A", model: null, endpoint: "/a", connectTimeoutMs: 1 },
      { id: "b", label: "B", model: null, endpoint: "/b", connectTimeoutMs: 1 },
    ];
    expect(fallbackEntry(registry, "a")?.id).toBe(fallbackBackendIdFromCore(["a", "b"], "a"));
    expect(fallbackEntry(BACKEND_REGISTRY, "cloud")).toBe(null);
    expect(fallbackBackendIdFromCore(["cloud"], "cloud")).toBeUndefined();
  });

  it("declinedBackendFallbackFromCore decides the whole #274 offer, not just the fallback id", () => {
    const declined = { phase: "declined", messages: [], reason: "Could not reach the server.", backend: null, model: null, answered: false };
    expect(declinedBackendFallbackFromCore(declined, "cloud", ["cloud", "home"])).toBe("home");
    expect(declinedBackendFallbackFromCore({ phase: "idle" }, "cloud", ["cloud", "home"])).toBeUndefined();
    expect(declinedBackendFallbackFromCore(declined, AUTO_SELECTION, ["cloud", "home"])).toBeUndefined();
    expect(
      declinedBackendFallbackFromCore({ ...declined, answered: true }, "cloud", ["cloud", "home"]),
    ).toBeUndefined();
  });

  it("readBackendSelection answers out of the core's resolve_backend_selection", () => {
    const storage = {
      store: { "hb.backend-selection": "retired" } as Record<string, string>,
      getItem(key: string) {
        return this.store[key] ?? null;
      },
      setItem(key: string, value: string) {
        this.store[key] = value;
      },
      removeItem(key: string) {
        delete this.store[key];
      },
    };
    expect(readBackendSelection(storage, BACKEND_REGISTRY)).toBe(
      resolveBackendSelectionFromCore("retired", ["cloud"]),
    );
  });

  it("wouldStrandPlan/planReplacementLabel/demotesFromFrontier answer out of the core", () => {
    const steps = [step()];
    expect(wouldStrandPlan("fog_remains", steps)).toBe(grillWouldStrandPlanFromCore("fog_remains", steps));
    expect(planReplacementLabel(steps)).toBe(grillPlanReplacementLabelFromCore(steps));
    expect(demotesFromFrontier("fog_remains", "ready")).toBe(
      grillDemotesFromFrontierFromCore("fog_remains", "ready"),
    );
  });

  it("FRONTIER_DEMOTION_WARNING matches the core's words", () => {
    expect(FRONTIER_DEMOTION_WARNING).toBe(grillFrontierDemotionWarningFromCore());
  });
});

// M4 (#533): the panes' vocabularies. `BAND_ORDER` and `QUESTION_ORDER`
// stay literal TS in `contract.ts` for a HARDER version of the same
// module-evaluation-order constraint as above — `registry.ts` builds its
// `QUESTIONS` map at module evaluation and reads `QUESTION_ORDER` there, so
// a seam call would throw the "used before ready" guard on every page load,
// not merely in a test. The waste pane's four constants are the same story
// via `question.ts`'s `sources: [SOURCE]`. All of them are pinned here
// instead.
describe("the seam's literal pane vocabulary, pinned against the core", () => {
  it("BAND_ORDER matches the core's salience vocabulary, in order", () => {
    expect([...BAND_ORDER]).toEqual(paneBandOrderFromCore());
  });

  it("QUESTION_ORDER matches the core's declared question order", () => {
    // Declaration order, not alphabetical — a question's place must not
    // move when another is renamed, and the two clients must agree which
    // order that is.
    expect([...QUESTION_ORDER]).toEqual(paneQuestionOrderFromCore());
  });

  it("the waste pane's constants match the core's", () => {
    const constants = wasteConstantsFromCore();
    expect(SOURCE).toBe(constants.source);
    expect(SNAPSHOT_KEY).toBe(constants.snapshotKey);
    expect(BINDING_KEY).toBe(constants.bindingKey);
    // ADR-0015 puts the threshold beside the band function, and ADR-0025
    // moved the pair into the core — this is the copy that must not drift
    // from it.
    expect(STALE_AFTER_MS).toBe(constants.staleAfterMs);
    expect([...STREAM_ORDER]).toEqual(constants.streamOrder);
  });

  // #534: the remaining seven panes' own literal constants, same reason.
  it("the kimi pane's constants match the core's", () => {
    const constants = kimiConstantsFromCore();
    expect(KIMI_SOURCE).toBe(constants.source);
    expect(KIMI_SNAPSHOT_KEY).toBe(constants.snapshotKey);
    expect(KIMI_STALE_AFTER_MS).toBe(constants.staleAfterMs);
    expect(IMMINENT_THRESHOLD_USD).toBe(constants.imminentThresholdUsd);
    expect(NEAR_THRESHOLD_USD).toBe(constants.nearThresholdUsd);
  });

  it("the github pane's constants match the core's", () => {
    const constants = githubConstantsFromCore();
    expect(GITHUB_SOURCE).toBe(constants.source);
    expect(GITHUB_NEVER_POLLED_SUBJECT).toBe(constants.neverPolledSubject);
    expect(GITHUB_STALE_AFTER_MS).toBe(constants.staleAfterMs);
    expect(OVERDUE_MULTIPLIER).toBe(constants.overdueMultiplier);
    expect(MIN_OVERDUE_AFTER_MS).toBe(constants.minOverdueAfterMs);
  });

  it("the uptime pane's constants match the core's", () => {
    const constants = uptimeConstantsFromCore();
    expect(UPTIME_SOURCE).toBe(constants.source);
    expect(UPTIME_NEVER_POLLED_SUBJECT).toBe(constants.neverPolledSubject);
    expect(UPTIME_STALE_AFTER_MS).toBe(constants.staleAfterMs);
  });

  it("the reachability pane's grace window matches the core's", () => {
    const constants = reachabilityConstantsFromCore();
    expect(REACHABILITY_SUBJECT_KEY).toBe(constants.subjectKey);
    expect(REACHABILITY_GRACE_MS).toBe(constants.graceMs);
  });

  // #775: the poller pane's literal source list and threshold.
  it("the poller pane's constants match the core's", () => {
    const constants = pollerConstantsFromCore();
    expect([...POLLER_SOURCES]).toEqual(constants.sources);
    expect(POLLER_OVERDUE_MULTIPLIER).toBe(constants.overdueMultiplier);
    expect(POLLER_FLOOR_MS).toBe(constants.floorMs);
  });

  it("the race pane's constants match the core's", () => {
    const constants = raceConstantsFromCore();
    expect(RACE_SOURCE).toBe(constants.source);
    expect(RACE_BINDING_KEY).toBe(constants.bindingKey);
    expect(RACE_STALE_AFTER_MS).toBe(constants.staleAfterMs);
    expect(SETUP_SUBJECT).toBe(constants.setupSubject);
  });

  it("the weekend pane's constants match the core's", () => {
    const constants = weekendConstantsFromCore();
    expect(WEEKEND_SUBJECT_KEY).toBe(constants.subjectKey);
    expect(WEEKEND_CALENDAR_REQUEST_KEY).toBe(constants.calendarRequestKey);
    // `weekendBand`'s own thresholds — kept literal TS on the
    // describe-collection-order reasoning `weekend.ts`'s module header
    // states, pinned here rather than read through the seam at runtime.
    expect(WEEKEND_IMMINENT_WITHIN_MS).toBe(constants.imminentWithinMs);
    expect(WEEKEND_NEAR_WITHIN_MS).toBe(constants.nearWithinMs);
  });

  it("the vacation pane's constants match the core's", () => {
    const constants = vacationConstantsFromCore();
    expect(VACATION_SUBJECT_KEY).toBe(constants.subjectKey);
    expect(VACATION_CALENDAR_REQUEST_KEY).toBe(constants.calendarRequestKey);
    expect(HORIZON_BEFORE_DAYS).toBe(constants.horizonBeforeDays);
    expect(HORIZON_AHEAD_DAYS).toBe(constants.horizonAheadDays);
    expect(VACATION_STALE_AFTER_MS).toBe(constants.staleAfterMs);
  });

  it("the scps pane's constants match the core's", () => {
    const constants = scpsConstantsFromCore();
    expect(SCPS_SUBJECT_KEY).toBe(constants.subjectKey);
    expect(SCPS_CALENDAR_REQUEST_KEY).toBe(constants.calendarRequestKey);
    expect(SCPS_QUEST_BINDING_KEY).toBe(constants.questBindingKey);
    expect(SCPS_HORIZON_BEFORE_MS).toBe(constants.horizonBeforeMs);
    expect(SCPS_HORIZON_AFTER_DAYS).toBe(constants.horizonAfterDays);
    expect(SCPS_STALE_AFTER_MS).toBe(constants.staleAfterMs);
  });

  // `zone-bridge.ts`'s `DEVICE_ZONE` special-cases exactly this string to
  // mean "the reader's own device zone" — a drift here would silently turn
  // every weekend/vacation query into a permanently-unresolvable one
  // (`resolveZone` would pass a literal `"device-local"` straight to
  // `Intl`, which throws, so every query is simply omitted and the pane
  // reads as a permanent gap, never a loud failure).
  it("zone-bridge.ts's DEVICE_ZONE sentinel matches the core's", () => {
    expect(DEVICE_ZONE).toBe(deviceZoneFromCore());
  });

  // #715, and the reason it is here rather than in a node test: the whole
  // claim is that the field this side writes is the field the core's serde
  // reads. A camelCase mismatch would not fail to compile, would not throw,
  // and would present as a toggle that simply does nothing — so it is
  // asserted across the real wasm boundary, both directions.
  it("carries `disabledQuestions` across the boundary, so a switched-off question asks for nothing", () => {
    const inputs = { nowMs: 1_786_377_600_000, bindings: [], paneReads: {} };
    const asked = paneZoneQueries(inputs, "now");
    expect(asked.length).toBeGreaterThan(0);

    const silenced = paneZoneQueries(
      {
        ...inputs,
        disabledQuestions: ["homework", "scps", "waste", "weekend", "vacation", "race"],
      },
      "now",
    );
    expect(silenced).toEqual([]);

    // And a name the core cannot resolve silences nothing — the field is
    // plain strings precisely so a newer build's question cannot fail the
    // whole crossing.
    expect(paneZoneQueries({ ...inputs, disabledQuestions: ["fantasy"] }, "now")).toEqual(asked);
  });
});

describe("the loading gate", () => {
  beforeEach(() => {
    resetDecisionsForTest();
  });

  afterAll(async () => {
    // Leave the module loaded for anything that runs after this file's
    // suites — the setup file's `beforeAll` has already fired.
    await initDecisions(loadDecisionsForTest);
  });

  it("throws rather than falling back to a TS copy when used too early", () => {
    expect(decisionsReady()).toBe(false);
    expect(() => canSubmitCapture("buy milk")).toThrow(/before initDecisions/);
  });

  it("instantiates once for concurrent callers", async () => {
    let loads = 0;
    const counted = async () => {
      loads += 1;
      return loadDecisionsForTest();
    };
    await Promise.all([initDecisions(counted), initDecisions(counted), initDecisions(counted)]);
    expect(loads).toBe(1);
    expect(decisionsReady()).toBe(true);
  });
});

/** The main thread's own `TaskItemDTO` shape (camelCase, `store/protocol.ts`)
 * — what M1-3's per-render calls would actually cross. */
function syntheticItem(id: string, stage: TaskItemDTO["stage"]): TaskItemDTO {
  return {
    id,
    seq: 42,
    title: "buy milk",
    description: null,
    stage,
    size: "quick",
    energy: "low",
    context: "@errands",
    priority: 2,
    projectId: null,
    projectPos: null,
    deadline: "2026-08-20",
    scheduledDate: null,
    source: "web/v1",
    sourceKey: null,
    sourceUrl: null,
    vaultPath: null,
    archivedAt: null,
    createdAt: 1_755_000_000_000,
    updatedAt: 1_755_000_000_000,
    version: 1,
    pending: false,
  };
}
