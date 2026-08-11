import type { QuestionInputs } from "../screens/questions/contract";
import { BINDING_KEY, SNAPSHOT_KEY, SOURCE } from "../screens/waste-pane/waste";
import type { BindingDTO, PaneReadDTO } from "../store/protocol";

// The ranked region's demo world (#245) — a bound waste question whose
// collection is *tomorrow at the address*, so `?demo` photographs an
// answered, imminent "Trash Tonight" pane.
//
// **The region is identical in both modes.** `NowScreen` swaps only these
// inputs for the store's; there is no demo-only rendering of the region, and
// there must never be one — the whole point of `QuestionInputs` being a
// plain value is that a fixture can drive the real shell rather than a
// parallel copy of it that drifts from it. What `?demo` shows is what ships.

/** The address the fixture's collection happens at. Fixed rather than the
 * device's own zone: a fixture whose answer changed with where the reviewer
 * is sitting would not be a fixture. */
const ZONE = "America/Los_Angeles";

function civilDateInZone(nowMs: number, dayOffset: number): string {
  const formatted = new Intl.DateTimeFormat("en-CA", {
    timeZone: ZONE,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(nowMs + dayOffset * 86_400_000));
  return formatted;
}

const boundBinding: BindingDTO = {
  key: BINDING_KEY,
  known: true,
  pending: false,
  value: { state: "text", text: "https://example.gov/waste/collection-day" },
};

function wasteRead(nowMs: number): PaneReadDTO {
  // Tomorrow at the address, on its scheduled day — an ordinary week, which
  // is the state worth photographing: the holiday variant is a change to the
  // words, not to the layout.
  const collectedOn = civilDateInZone(nowMs, 1);
  return {
    source: SOURCE,
    snapshots: [
      {
        key: SNAPSHOT_KEY,
        fetchedAtMs: nowMs - 40 * 60_000,
        envelope: {
          kind: "ok",
          schema: SOURCE,
          polledEveryMs: 86_400_000,
          body: JSON.stringify({
            zone: ZONE,
            scheduled: collectedOn,
            collected_on: collectedOn,
            streams: ["trash", "recycling", "yard"],
          }),
        },
        // Forty minutes old against a daily cadence: comfortably fresh, so
        // the tile shows no staleness line.
        freshness: { kind: "age", ageMs: 40 * 60_000, declaredCadenceMs: 86_400_000 },
      },
    ],
    liveAlerts: [],
  };
}

/** The demo world's answer to `QuestionInputs`, minus the clock the region
 * supplies itself. */
export function demoQuestionInputs(nowMs: number): Omit<QuestionInputs, "nowMs"> {
  return {
    bindings: [boundBinding],
    paneReads: { [SOURCE]: wasteRead(nowMs) },
  };
}
