// PROTOTYPE — throwaway. Delete this whole directory once #120's pane is
// built for real; see NOTES.md for the question and what driving it settled.
//
//   pnpm dev  →  http://localhost:5173/?wastepane
//
// Unlike its siblings this is not just a scenario switcher but a driveable
// world: the operator's four states are one click each, and the day-by-day
// driver is still there because dormant-vs-Sunday and the alert lifecycle are
// exactly what a fixed scenario cannot show. The whole scenario is the
// command history in `?do=`, so any state is shareable and survives a reload.
//
// Gated on `import.meta.env.DEV` exactly like `fixtures/demo.ts`: the whole
// thing folds to `if (false && …)` in a production build, so a stray merge
// cannot ship it.

import { useEffect, useState } from "react";
import { VariantA } from "./VariantA";
import { isLive, pane, payloadJson, judge, weekday } from "./waste";
import { type Command, PRESETS, decode, encode, now, replay } from "./world";

/** True when `?wastepane` is on the URL, in dev only. */
export function isWastePaneEnabled(): boolean {
  if (!import.meta.env.DEV) return false;
  return new URLSearchParams(window.location.search).has("wastepane");
}

/** The pane and the control bar are separate mounts reading the same state,
 * so they stay in step without either owning the other. A tiny event bus
 * rather than context: this is throwaway, and threading a provider through
 * `App` would touch production wiring the prototype is meant to leave
 * alone. */
const CHANGED = "prototype-waste-pane:changed";

function readCommands(): Command[] {
  return decode(new URLSearchParams(window.location.search).get("do"));
}

function useWorld() {
  const [commands, setCommands] = useState(readCommands);
  useEffect(() => {
    function onChanged() {
      setCommands(readCommands());
    }
    window.addEventListener(CHANGED, onChanged);
    return () => window.removeEventListener(CHANGED, onChanged);
  }, []);
  return replay(commands);
}

function push(command: Command) {
  const url = new URL(window.location.href);
  url.searchParams.set("do", encode([...readCommands(), command]));
  window.history.replaceState(null, "", url);
  window.dispatchEvent(new Event(CHANGED));
}

function reset() {
  const url = new URL(window.location.href);
  url.searchParams.delete("do");
  window.history.replaceState(null, "", url);
  window.dispatchEvent(new Event(CHANGED));
}

export function WastePane() {
  const world = useWorld();
  if (!isWastePaneEnabled() || !world.payload) return null;
  const view = pane(world.payload, world.today, now(world), world.fetchedAt);
  return <VariantA view={view} />;
}

/** The floating control bar. Visually loud on purpose — it must not read as
 * part of the design being evaluated. Shows the machine state the pane
 * deliberately hides: the snapshot payload, the judgment, and the alert row's
 * lifecycle stamps. */
export function WastePaneSwitcher() {
  const world = useWorld();

  useEffect(() => {
    if (!isWastePaneEnabled()) return;
    function onKeyDown(event: KeyboardEvent) {
      const target = event.target;
      const editable =
        target instanceof HTMLElement &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable);
      if (editable || event.metaKey || event.ctrlKey || event.altKey) return;
      if (event.key === "n") push({ kind: "day" });
      else if (event.key === "p") push({ kind: "poll" });
      else if (event.key === "a") push({ kind: "ack" });
      else if (["1", "2", "3", "4"].includes(event.key)) {
        push({ kind: "preset", index: Number(event.key) - 1 });
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  if (!isWastePaneEnabled() || !world.payload) return null;

  const at = now(world);
  const view = pane(world.payload, world.today, at, world.fetchedAt);
  const deviation = judge(world.payload.collection);
  const today = new Date(world.today * 86_400_000).toISOString().slice(0, 10);

  return (
    <div style={BAR}>
      <span style={{ opacity: 0.6 }}>
        waste pane · {weekday(world.today)} {today} · prominence={view.prominence} ·{" "}
        {deviation.kind}
      </span>

      <div style={ROW}>
        {PRESETS.map((preset, i) => (
          <Key
            key={preset.key}
            onClick={() => push({ kind: "preset", index: i })}
            label={`${i + 1} · ${preset.label}`}
            accent
          />
        ))}
      </div>

      <div style={ROW}>
        <Key onClick={() => push({ kind: "day" })} label="n · next day" />
        <Key onClick={() => push({ kind: "goto", weekday: 0 })} label="→ Sunday" />
        <Key onClick={() => push({ kind: "goto", weekday: 1 })} label="→ Monday" />
        <Key onClick={() => push({ kind: "poll" })} label="p · poll" />
        <Key onClick={() => push({ kind: "ack" })} label="a · ack" />
        <Key onClick={reset} label="reset" />
      </div>

      <span style={{ opacity: 0.45, wordBreak: "break-all", textAlign: "center" }}>
        {payloadJson(world.payload)}
      </span>

      {world.alerts.map((alert) => (
        <span key={alert.sourceKey} style={{ opacity: 0.8, textAlign: "center" }}>
          {alert.sourceKey} · raised {stamp(alert.raisedAt)} · acked{" "}
          {alert.dismissedAt === null ? "-" : stamp(alert.dismissedAt)} · expires{" "}
          {stamp(alert.expiresAt)} · v{alert.version} · {isLive(alert, at) ? "LIVE" : "quiet"}
        </span>
      ))}

      {world.log.map((line, i) => (
        <span key={i} style={{ opacity: 0.55 }}>
          {line}
        </span>
      ))}

      <span style={{ opacity: 0.4 }}>keys: 1–4 state · n next day · p poll · a ack</span>
    </div>
  );
}

function stamp(seconds: number): string {
  return new Date(seconds * 1000).toISOString().slice(0, 16).replace("T", " ");
}

function Key({
  onClick,
  label,
  accent = false,
}: {
  onClick: () => void;
  label: string;
  accent?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        padding: "3px 8px",
        borderRadius: 999,
        cursor: "pointer",
        border: "1px solid #ffffff33",
        background: accent ? "#eb6d06" : "transparent",
        color: "#fff",
        font: "11px/1.4 ui-monospace, monospace",
      }}
    >
      {label}
    </button>
  );
}

const BAR: React.CSSProperties = {
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
};

const ROW: React.CSSProperties = {
  display: "flex",
  gap: 6,
  flexWrap: "wrap",
  justifyContent: "center",
};
