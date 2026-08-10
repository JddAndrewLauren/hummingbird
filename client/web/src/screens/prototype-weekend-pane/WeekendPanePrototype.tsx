// PROTOTYPE — throwaway. Delete this whole directory once #122's pane shape
// and its `scheduled_date` affordance are decided; see NOTES.md for the
// question it is answering.
//
// Three variants of the weekend-plans pane, mounted inside the real Now
// screen (so each is judged against the real frontier, rail and header
// rather than in a vacuum), switchable with `?variant=` and `?scenario=`.
//
//   pnpm dev  →  http://localhost:5173/?weekendpane
//
// Gated on `import.meta.env.DEV` exactly like `fixtures/demo.ts` and the
// sibling `prototype-race-pane`: the whole thing folds to `if (false && …)`
// in a production build, so a stray merge cannot ship it.

import { useEffect, useState } from "react";
import { formatAsOf, isStale } from "../../calendar/staleness";
import { SCENARIOS, scenarioByKey } from "./fixture";
import { applyPlans, planEditCount, resetPlans, setPlan, usePlanVersion } from "./plan-store";
import { countKinds, mergeWindow, weekendWindow } from "./weekend";
import type { PaneSlot } from "./variant";
import { NAME as NAME_A, SLOT as SLOT_A, VariantA } from "./VariantA";
import { NAME as NAME_B, SLOT as SLOT_B, VariantB } from "./VariantB";
import { NAME as NAME_C, SLOT as SLOT_C, VariantC } from "./VariantC";

const VARIANTS = [
  { key: "A", name: NAME_A, slot: SLOT_A as PaneSlot, render: VariantA },
  { key: "B", name: NAME_B, slot: SLOT_B as PaneSlot, render: VariantB },
  { key: "C", name: NAME_C, slot: SLOT_C as PaneSlot, render: VariantC },
];

/** True when `?weekendpane` is on the URL, in dev only. */
export function isWeekendPaneEnabled(): boolean {
  if (!import.meta.env.DEV) return false;
  return new URLSearchParams(window.location.search).has("weekendpane");
}

function readParam(name: string): string | null {
  return new URLSearchParams(window.location.search).get(name);
}

function writeParam(name: string, value: string) {
  const url = new URL(window.location.href);
  url.searchParams.set(name, value);
  window.history.replaceState(null, "", url);
}

const CHANGED = "prototype-weekend-pane:changed";

function useSelection() {
  const [variant, setVariant] = useState(() => readParam("variant") ?? "A");
  const [scenario, setScenario] = useState(() => readParam("scenario") ?? SCENARIOS[0].key);

  useEffect(() => {
    function onChanged() {
      setVariant(readParam("variant") ?? "A");
      setScenario(readParam("scenario") ?? SCENARIOS[0].key);
    }
    window.addEventListener(CHANGED, onChanged);
    return () => window.removeEventListener(CHANGED, onChanged);
  }, []);

  return { variant, scenario };
}

function select(name: string, value: string) {
  writeParam(name, value);
  window.dispatchEvent(new Event(CHANGED));
}

/** Renders whichever variant claims this slot; nothing if it claims the
 * other one. Two mount points, because a variant is free to decide the pane
 * does not belong in the context panel at all — B says the weekend is a
 * banner, A and C say it is furniture in the rail. */
export function WeekendPane({ slot }: { slot: PaneSlot }) {
  const { variant, scenario } = useSelection();
  // Subscribing here is what makes a stub plan edit re-run the merge.
  usePlanVersion();
  if (!isWeekendPaneEnabled()) return null;

  const chosen = VARIANTS.find((entry) => entry.key === variant) ?? VARIANTS[0];
  if (chosen.slot !== slot) return null;

  const data = scenarioByKey(scenario);
  const merged = mergeWindow(
    weekendWindow(data.nowMs),
    data.calendar?.events ?? [],
    applyPlans(data.items),
  );
  const calendar = data.calendar
    ? {
        asOf: formatAsOf(data.calendar.asOfMs, data.nowMs),
        stale: isStale(data.calendar.asOfMs, data.nowMs),
      }
    : null;

  const Render = chosen.render;
  return (
    <Render
      window={merged}
      nowMs={data.nowMs}
      calendar={calendar}
      onPlan={(itemId, dayKey) => setPlan(itemId, dayKey)}
    />
  );
}

/** The floating switcher. Visually loud on purpose — it must not read as
 * part of the design being evaluated. */
export function WeekendPaneSwitcher() {
  const { variant, scenario } = useSelection();
  usePlanVersion();

  useEffect(() => {
    if (!isWeekendPaneEnabled()) return;
    function onKeyDown(event: KeyboardEvent) {
      const target = event.target;
      const editable =
        target instanceof HTMLElement &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable);
      if (editable) return;
      const index = VARIANTS.findIndex((entry) => entry.key === (readParam("variant") ?? "A"));
      if (event.key === "ArrowRight") {
        select("variant", VARIANTS[(index + 1) % VARIANTS.length].key);
      } else if (event.key === "ArrowLeft") {
        select("variant", VARIANTS[(index - 1 + VARIANTS.length) % VARIANTS.length].key);
      } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        const at = SCENARIOS.findIndex((entry) => entry.key === (readParam("scenario") ?? ""));
        const step = event.key === "ArrowDown" ? 1 : -1;
        const next = (at + step + SCENARIOS.length) % SCENARIOS.length;
        select("scenario", SCENARIOS[next].key);
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  if (!isWeekendPaneEnabled()) return null;

  const chosen = VARIANTS.find((entry) => entry.key === variant) ?? VARIANTS[0];
  const data = scenarioByKey(scenario);
  const index = VARIANTS.indexOf(chosen);
  const merged = mergeWindow(
    weekendWindow(data.nowMs),
    data.calendar?.events ?? [],
    applyPlans(data.items),
  );
  const counts = countKinds(merged);
  const edits = planEditCount();

  const arrow = (label: string, to: number) => (
    <button
      type="button"
      onClick={() => select("variant", VARIANTS[(to + VARIANTS.length) % VARIANTS.length].key)}
      style={{
        width: 28,
        height: 28,
        borderRadius: 999,
        border: "1px solid #ffffff44",
        background: "transparent",
        color: "#fff",
        cursor: "pointer",
        font: "16px/1 monospace",
      }}
    >
      {label}
    </button>
  );

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
        maxWidth: "min(92vw, 820px)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        {arrow("‹", index - 1)}
        <span style={{ minWidth: 220, textAlign: "center" }}>
          {chosen.key} — {chosen.name} <span style={{ opacity: 0.6 }}>({chosen.slot})</span>
        </span>
        {arrow("›", index + 1)}
      </div>
      <div style={{ display: "flex", gap: 6, flexWrap: "wrap", justifyContent: "center" }}>
        {SCENARIOS.map((entry) => (
          <button
            key={entry.key}
            type="button"
            onClick={() => select("scenario", entry.key)}
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
      <span style={{ opacity: 0.7 }}>
        merged: {counts.events} events · {counts.due} due · {counts.scheduled} planned
        {edits > 0 ? (
          <>
            {" · "}
            <button
              type="button"
              onClick={() => resetPlans()}
              style={{
                padding: "1px 6px",
                borderRadius: 999,
                cursor: "pointer",
                border: "1px solid #ffffff33",
                background: "transparent",
                color: "#fff",
                font: "11px/1.4 ui-monospace, monospace",
              }}
            >
              reset {edits} stub plan {edits === 1 ? "edit" : "edits"}
            </button>
          </>
        ) : null}
      </span>
      <span style={{ opacity: 0.45 }}>← → variant · ↑ ↓ scenario</span>
    </div>
  );
}
