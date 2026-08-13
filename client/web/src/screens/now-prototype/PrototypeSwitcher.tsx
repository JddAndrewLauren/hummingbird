// PROTOTYPE — throwaway. Delete with the rest of `now-prototype/`.
//
// The floating variant switcher. Deliberately not styled like the app: it is
// chrome for judging the design, not part of the design being judged.

import { useEffect } from "react";
import { Icon } from "../../components/core/Icon";

export interface SwitcherEntry {
  key: string;
  name: string;
}

export function PrototypeSwitcher({
  entries,
  current,
  onPick,
  source,
}: {
  entries: readonly SwitcherEntry[];
  current: string;
  onPick: (key: string) => void;
  /** Which item set is on screen — the prototype fixture, or the real
   * frontier. Shown so a thin-looking variant is never mistaken for a bad
   * layout when it is really an empty authority. */
  source: string;
}) {
  const index = Math.max(
    0,
    entries.findIndex((entry) => entry.key === current),
  );

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") {
        return;
      }
      const target = event.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || target?.isContentEditable) {
        return;
      }
      const step = event.key === "ArrowRight" ? 1 : -1;
      onPick(entries[(index + step + entries.length) % entries.length].key);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [entries, index, onPick]);

  const step = (delta: number) =>
    onPick(entries[(index + delta + entries.length) % entries.length].key);

  const arrow = {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 28,
    height: 28,
    borderRadius: 999,
    border: "1px solid rgba(255,255,255,0.28)",
    background: "transparent",
    color: "#fff",
    cursor: "pointer",
  } as const;

  return (
    <div
      style={{
        position: "fixed",
        left: "50%",
        bottom: 20,
        transform: "translateX(-50%)",
        zIndex: 1000,
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "8px 14px",
        borderRadius: 999,
        background: "#12181f",
        color: "#fff",
        border: "1px solid rgba(255,255,255,0.18)",
        boxShadow: "0 8px 28px rgba(0,0,0,0.38)",
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        fontSize: 12,
      }}
    >
      <button type="button" aria-label="Previous variant" onClick={() => step(-1)} style={arrow}>
        <Icon name="chevron-down" size={14} style={{ transform: "rotate(90deg)" }} />
      </button>
      <span style={{ minWidth: 190, textAlign: "center", letterSpacing: "0.02em" }}>
        {entries[index].key} — {entries[index].name}
      </span>
      <button type="button" aria-label="Next variant" onClick={() => step(1)} style={arrow}>
        <Icon name="chevron-down" size={14} style={{ transform: "rotate(-90deg)" }} />
      </button>
      <span style={{ opacity: 0.55, borderLeft: "1px solid rgba(255,255,255,0.2)", paddingLeft: 12 }}>
        {source}
      </span>
    </div>
  );
}
