// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { BindingDTO, PaneAlertDTO, PaneReadDTO } from "../../store/protocol";
import { fireEvent, render, screen } from "../../test/component";
import type { QuestionInputs } from "../questions/contract";
import { RankedRegion } from "../questions/RankedRegion";
import { BINDING_KEY, SOURCE } from "./race";

// The pane shell's "component tests are the gate" rule (`src/test/component.tsx`): a pure
// module with no caller compiles and passes and does nothing, so this mounts
// the REAL `RacePaneExpanded` through `RankedRegion` — the path `NowScreen`
// wires in production — and reads what actually lands on screen.
//
// The `race` question emits one pane PER FOLLOWED SERIES, which nothing else
// in the shipped registry does, so these also exercise the registry's 0..N
// expansion against a real question rather than a test double.

const HOUR = 60 * 60 * 1000;
const DAY = 24 * HOUR;
const NOW = new Date(2026, 7, 10, 9, 0, 0).getTime();

function seriesBinding(text: string): BindingDTO {
  return { key: BINDING_KEY, known: true, pending: false, value: { state: "text", text } };
}

function season(startsAtMs: number, name = "Monaco Grand Prix"): unknown {
  return {
    events: [
      {
        name,
        locality: "Monte Carlo",
        starts_at_ms: startsAtMs,
        sessions: [
          { kind: "practice", label: "Practice 1", starts_at_ms: startsAtMs - 2 * DAY },
          { kind: "qualifying", label: "Qualifying", starts_at_ms: startsAtMs - DAY },
        ],
      },
    ],
  };
}

function read(bodies: Record<string, unknown>, liveAlerts: PaneAlertDTO[] = []): PaneReadDTO {
  return {
    source: SOURCE,
    snapshots: Object.entries(bodies).map(([key, body]) => ({
      key,
      fetchedAtMs: NOW - 60_000,
      envelope: {
        kind: "ok" as const,
        schema: SOURCE,
        polledEveryMs: 6 * HOUR,
        body: JSON.stringify(body),
      },
      freshness: { kind: "age" as const, ageMs: 60_000, declaredCadenceMs: 6 * HOUR },
    })),
    liveAlerts,
  };
}

function mount(inputs: Omit<QuestionInputs, "nowMs">, onScreen = vi.fn()) {
  render(
    <RankedRegion
      surface="now"
      inputs={inputs}
      nowMs={NOW}
      syncOutcomeSeq={1}
      storage={{ getItem: () => null, setItem: () => {}, removeItem: () => {} }}
      onScreen={onScreen}
    />,
  );
  return onScreen;
}

function world(overrides: Partial<QuestionInputs> = {}): Omit<QuestionInputs, "nowMs"> {
  return {
    bindings: [seriesBinding("f1")],
    paneReads: {},
    calendarReads: {},
    calendarConnected: false,
    items: [],
    ...overrides,
  };
}

describe("RacePaneExpanded (mounted through RankedRegion)", () => {
  it("puts the countdown, the next session and the circuit on screen", () => {
    mount(world({ paneReads: { [SOURCE]: read({ f1: season(NOW + 12 * DAY) }) } }));

    expect(screen.getByText("12 days before Monaco GP")).toBeDefined();
    // The session that actually happens first, under the headline rather than
    // in it — Friday practice is two days before Sunday's race.
    expect(screen.getByText(/Practice 1 ·/)).toBeDefined();
    expect(screen.getByText(/Monte Carlo/)).toBeDefined();
    // ADR-0015 is device-local with no zone label: the prototype's "PT"
    // suffix must not have survived the rewrite.
    expect(screen.queryByText(/\bPT\b/)).toBeNull();
  });

  it("renders one pane per followed series, and the unadapted one as a gap", () => {
    mount(
      world({
        bindings: [seriesBinding("f1, indycar")],
        paneReads: { [SOURCE]: read({ f1: season(NOW + 3 * DAY) }) },
      }),
    );

    // Both panes exist. The `f1` one is answered and (three days out) expands
    // on its own; `indycar` has no snapshot, so it collapses to a row saying
    // so rather than disappearing.
    expect(screen.getByText("3 days before Monaco GP")).toBeDefined();
    expect(screen.getByText(/IndyCar · Never polled/)).toBeDefined();
  });

  it("shows the race-start alert on the series it names and on no other", () => {
    const alert: PaneAlertDTO = {
      id: "alert-1",
      subjectKey: "f1",
      title: "Monaco Grand Prix starts soon",
      body: "Race start in about 90 minutes.",
      raisedAtMs: NOW,
      expiresAtMs: NOW + 90 * 60_000,
    };
    mount(
      world({
        bindings: [seriesBinding("f1, indycar")],
        paneReads: {
          [SOURCE]: read({ f1: season(NOW + 80 * 60_000), indycar: season(NOW + 40 * DAY, "Iowa 275") },
            [alert],
          ),
        },
      }),
    );

    expect(screen.getByText("starting soon")).toBeDefined();
    expect(screen.getByText("Monaco Grand Prix starts soon")).toBeDefined();
    // The IndyCar pane is collapsed and quiet: the `(source, subjectKey)`
    // join is what keeps one series' alert off another series' pane.
    expect(screen.getAllByText("starting soon")).toHaveLength(1);
  });

  it("offers the one action that resolves an unset binding", () => {
    const onScreen = mount(world({ bindings: [] }));
    // Unbound panes collapse by default, so the setup prompt is reached the
    // way a reader reaches it: by opening this question's own row. Named by
    // the question rather than by its headline, since more than one unbound
    // pane says "Not set up" in a world where nothing is configured.
    fireEvent.click(screen.getByRole("button", { name: /When is the next race/ }));
    fireEvent.click(screen.getByRole("button", { name: /Open Settings/ }));
    expect(onScreen).toHaveBeenCalledWith("settings");
  });

  it("keeps showing an old answer and says how old it is", () => {
    const stale = read({ f1: season(NOW + 9 * DAY) });
    stale.snapshots[0].freshness = { kind: "age", ageMs: 13 * HOUR, declaredCadenceMs: 6 * HOUR };
    mount(world({ paneReads: { [SOURCE]: stale } }));

    expect(screen.getByText("9 days before Monaco GP")).toBeDefined();
    expect(screen.getByText("stale — as of 13h ago")).toBeDefined();
  });
});
