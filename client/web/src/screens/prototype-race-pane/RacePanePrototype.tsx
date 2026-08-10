// PROTOTYPE — throwaway. Delete this whole directory once #119's pane is
// built for real; see NOTES.md for the settled design and the fold-in
// checklist.
//
// The shape question is answered (variant A, "Series tile" — B and C are
// deleted), so what is left is a scenario harness: the winning pane mounted
// inside the real Now screen, driven through the states the issue does not
// enumerate — a session under way, a live race-start alert, a missed cron, a
// series that has never polled, an empty binding.
//
//   pnpm dev  →  http://localhost:5173/?racepane
//
// Gated on `import.meta.env.DEV` exactly like `fixtures/demo.ts`: the whole
// thing folds to `if (false && …)` in a production build, so a stray merge
// cannot ship it.

import { useEffect, useState } from "react";
import { answerAll } from "./countdown";
import { SCENARIOS, scenarioByKey } from "./fixture";
import { VariantA } from "./VariantA";

/** True when `?racepane` is on the URL, in dev only. */
export function isRacePaneEnabled(): boolean {
  if (!import.meta.env.DEV) return false;
  return new URLSearchParams(window.location.search).has("racepane");
}

function readScenario(): string {
  return new URLSearchParams(window.location.search).get("scenario") ?? SCENARIOS[0].key;
}

/** The pane and the switcher are separate mounts reading the same state, so
 * they stay in step without either owning the other. A tiny event bus rather
 * than context: this is throwaway, and threading a provider through `App`
 * would touch production wiring the prototype is meant to leave alone. */
const CHANGED = "prototype-race-pane:changed";

function useScenario() {
  const [scenario, setScenario] = useState(readScenario);
  useEffect(() => {
    function onChanged() {
      setScenario(readScenario());
    }
    window.addEventListener(CHANGED, onChanged);
    return () => window.removeEventListener(CHANGED, onChanged);
  }, []);
  return scenario;
}

function select(key: string) {
  const url = new URL(window.location.href);
  url.searchParams.set("scenario", key);
  window.history.replaceState(null, "", url);
  window.dispatchEvent(new Event(CHANGED));
}

export function RacePane() {
  const scenario = useScenario();
  if (!isRacePaneEnabled()) return null;
  const data = scenarioByKey(scenario);
  return <VariantA answers={answerAll(data)} nowMs={data.nowMs} />;
}

/** The floating scenario switcher. Visually loud on purpose — it must not
 * read as part of the design being evaluated. */
export function RacePaneSwitcher() {
  const scenario = useScenario();

  useEffect(() => {
    if (!isRacePaneEnabled()) return;
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
      const at = SCENARIOS.findIndex((entry) => entry.key === readScenario());
      const step = event.key === "ArrowDown" ? 1 : -1;
      select(SCENARIOS[(at + step + SCENARIOS.length) % SCENARIOS.length].key);
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  if (!isRacePaneEnabled()) return null;

  const data = scenarioByKey(scenario);

  return (
    <div
      style={{
        position: "fixed",
        left: "50%",
        bottom: 16,
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
      <span style={{ opacity: 0.6 }}>race pane — series tile (settled)</span>
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
