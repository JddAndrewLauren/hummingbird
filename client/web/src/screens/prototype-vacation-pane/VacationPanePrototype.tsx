// PROTOTYPE — throwaway. Delete this whole directory (and the `VacationPane`
// + `VacationPaneSwitcher` mounts in `screens/NowScreen.tsx`) once A is
// folded into the real pane; see NOTES.md for the question it answered and
// the three findings still open.
//
// The shape is decided (A — "Countdown tile"), so the variant switcher is
// gone with variants B and C. What remains is the SCENARIO switcher, which
// is the part still earning its keep: the eleven states in `fixture.ts` are
// what the real pane will have to survive, and flipping through them is how
// the next change gets checked.
//
//   pnpm dev  →  http://localhost:5173/?vacationpane
//
// Gated on `import.meta.env.DEV` exactly like `fixtures/demo.ts`: the whole
// thing folds to `if (false && …)` in a production build, so a stray merge
// cannot ship it.

import { useEffect, useState } from "react";
import { answer as answerVacation } from "./countdown";
import { SCENARIOS, scenarioByKey } from "./fixture";
import { VariantA } from "./VariantA";

/** True when `?vacationpane` is on the URL, in dev only. */
export function isVacationPaneEnabled(): boolean {
  if (!import.meta.env.DEV) return false;
  return new URLSearchParams(window.location.search).has("vacationpane");
}

function readParam(name: string): string | null {
  return new URLSearchParams(window.location.search).get(name);
}

function writeParam(name: string, value: string) {
  const url = new URL(window.location.href);
  url.searchParams.set(name, value);
  window.history.replaceState(null, "", url);
}

/** The pane and the switcher are separate mounts reading the same state, so
 * a tiny event bus rather than context: this is throwaway, and threading a
 * provider through `App` would touch production wiring the prototype is
 * supposed to leave alone. */
const CHANGED = "prototype-vacation-pane:changed";

function useScenario(): string {
  const [scenario, setScenario] = useState(() => readParam("scenario") ?? SCENARIOS[0].key);

  useEffect(() => {
    function onChanged() {
      setScenario(readParam("scenario") ?? SCENARIOS[0].key);
    }
    window.addEventListener(CHANGED, onChanged);
    return () => window.removeEventListener(CHANGED, onChanged);
  }, []);

  return scenario;
}

function select(value: string) {
  writeParam("scenario", value);
  window.dispatchEvent(new Event(CHANGED));
}

/** The pane itself, in the context panel — the slot A won. */
export function VacationPane() {
  const scenario = useScenario();
  if (!isVacationPaneEnabled()) return null;

  const data = scenarioByKey(scenario);
  return <VariantA answer={answerVacation(data)} nowMs={data.nowMs} />;
}

/** The floating scenario switcher. Visually loud on purpose — it must not
 * read as part of the design being evaluated. */
export function VacationPaneSwitcher() {
  const scenario = useScenario();

  useEffect(() => {
    if (!isVacationPaneEnabled()) return;
    function onKeyDown(event: KeyboardEvent) {
      const target = event.target;
      const editable =
        target instanceof HTMLElement &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable);
      if (editable) return;
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      const at = SCENARIOS.findIndex((entry) => entry.key === (readParam("scenario") ?? ""));
      const step = event.key === "ArrowDown" ? 1 : -1;
      select(SCENARIOS[(at + step + SCENARIOS.length) % SCENARIOS.length].key);
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  if (!isVacationPaneEnabled()) return null;

  const data = scenarioByKey(scenario);
  // The sibling pane prototypes park their own bars at the same spot; step
  // over them rather than stacking two unreadable bars on top of each other.
  const params = new URLSearchParams(window.location.search);
  const siblingBar = params.has("racepane") || params.has("weekendpane");

  return (
    <div
      style={{
        position: "fixed",
        left: "50%",
        bottom: siblingBar ? 200 : 16,
        transform: "translateX(-50%)",
        zIndex: 9999,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        alignItems: "center",
        padding: "10px 14px",
        borderRadius: 14,
        background: "#101418",
        border: "1px solid #ffffff33",
        boxShadow: "0 8px 28px rgba(0,0,0,.45)",
        color: "#fff",
        font: "12px/1.4 ui-monospace, monospace",
        maxWidth: "min(92vw, 760px)",
      }}
    >
      <span style={{ opacity: 0.75 }}>vacation pane — A, Countdown tile (decided)</span>
      <div style={{ display: "flex", gap: 6, flexWrap: "wrap", justifyContent: "center" }}>
        {SCENARIOS.map((entry) => (
          <button
            key={entry.key}
            type="button"
            onClick={() => select(entry.key)}
            style={{
              padding: "3px 8px",
              borderRadius: 999,
              cursor: "pointer",
              border: "1px solid #ffffff33",
              background: entry.key === data.key ? "#eb6d06" : "transparent",
              color: "#fff",
              font: "11px/1.4 ui-monospace, monospace",
            }}
          >
            {entry.label}
          </button>
        ))}
      </div>
      <span style={{ opacity: 0.7, textAlign: "center" }}>{data.note}</span>
      <span style={{ opacity: 0.45 }}>↑ ↓ scenario</span>
    </div>
  );
}
