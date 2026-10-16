import type { BindingDTO, PaneReadDTO } from "../../store/protocol";
import { EMPTY_QUESTION_SYNC, type QuestionInputs } from "../questions/contract";
import { BINDING_KEY, LINEUP_SOURCE } from "./fantasy";
// `?raw` rather than `readFileSync`: this module is also loaded under jsdom,
// where `import.meta.url` is not a file URL (`rotation-margin-drift.test.ts`
// reads a server file the same way).
import goldenInvalid from "../../../../../server/fantasy-poll/tests/fixtures/golden-lineup.json?raw";
import goldenValid from "../../../../../server/fantasy-poll/tests/fixtures/golden-lineup-valid.json?raw";
import goldenDiscovered from "../../../../../server/fantasy-poll/tests/fixtures/golden-discovered.json?raw";

// Test support only — never imported by app code. **The lineup tests read the
// poller's committed golden files, not a hand-typed copy of them**:
// `server/fantasy-poll/tests/golden.rs` pins those bytes to what the poller
// emits, so a fixture retyped here would agree with itself while the two
// sides drifted apart (`race.test.ts` makes the same move).

function body(raw: string): Record<string, unknown> {
  return (JSON.parse(raw) as { body: Record<string, unknown> }).body;
}

export const GOLDEN_INVALID = body(goldenInvalid);
export const GOLDEN_VALID = body(goldenValid);
export const GOLDEN_DISCOVERED = body(goldenDiscovered);

export const LEAGUE = "449.l.123456";
export const LINEUP_SUBJECT = `${LEAGUE}:lineup`;
/** The invalid golden's earliest replacement deadline, and the valid
 * golden's first starter kickoff — a Thursday-night game. */
export const THURSDAY = 1_790_900_100_000;
export const HOUR = 3_600_000;

export function lineupRead(rows: { key: string; schema?: string; body: unknown; ageMs: number }[]): PaneReadDTO {
  return {
    source: LINEUP_SOURCE,
    snapshots: rows.map((row) => ({
      key: row.key,
      fetchedAtMs: 0,
      envelope: {
        kind: "ok",
        schema: row.schema ?? LINEUP_SOURCE,
        polledEveryMs: 21_600_000,
        body: JSON.stringify(row.body),
      },
      freshness: { kind: "age", ageMs: row.ageMs, declaredCadenceMs: 21_600_000 },
    })),
    liveAlerts: [],
  };
}

export function leaguesBinding(text = LEAGUE): BindingDTO {
  return { key: BINDING_KEY, known: true, pending: false, value: { state: "text", text } };
}

/** One bound league whose lineup row is `body`, `ageMs` old, at `nowMs`. */
export function lineupWorld(body: unknown, nowMs: number, ageMs = 60_000): QuestionInputs {
  return {
    sync: EMPTY_QUESTION_SYNC,
    bindings: [leaguesBinding()],
    paneReads: { [LINEUP_SOURCE]: lineupRead([{ key: LEAGUE, body, ageMs }]) },
    calendarReads: {},
    calendarConnected: false,
    items: [],
    nowMs,
  };
}
