// PROTOTYPE (#449) — throwaway. The floating variant switcher. Deliberately
// off-brand (near-black pill, ember border in both themes) so it reads as
// tooling, not as part of the design being judged. Arrow keys cycle too,
// except while a field is focused.

import { useEffect } from "react";

export interface PrototypeVariant {
  key: string;
  name: string;
}

export function PrototypeBar({
  variants,
  current,
  onChange,
}: {
  variants: readonly PrototypeVariant[];
  current: string;
  onChange: (key: string) => void;
}) {
  const index = Math.max(
    0,
    variants.findIndex((variant) => variant.key === current),
  );

  function step(delta: number) {
    onChange(variants[(index + delta + variants.length) % variants.length].key);
  }

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable]")) return;
      if (event.key === "ArrowLeft") step(-1);
      if (event.key === "ArrowRight") step(1);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const buttonStyle = {
    background: "none",
    border: "none",
    color: "#faf0e7",
    cursor: "pointer",
    font: "700 14px/1 monospace",
    padding: "6px 10px",
  } as const;

  return (
    <div
      style={{
        position: "fixed",
        bottom: "calc(20px + env(safe-area-inset-bottom))",
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 150,
        display: "flex",
        alignItems: "center",
        gap: 4,
        background: "#0f141a",
        border: "1px solid #eb6d06",
        borderRadius: 999,
        padding: "2px 6px",
        boxShadow: "0 6px 24px rgba(0,0,0,0.45)",
      }}
    >
      <button type="button" aria-label="Previous variant" style={buttonStyle} onClick={() => step(-1)}>
        ‹
      </button>
      <span
        style={{
          color: "#faf0e7",
          fontFamily: "'Space Mono', monospace",
          fontSize: 11,
          letterSpacing: "0.08em",
          textTransform: "uppercase",
          whiteSpace: "nowrap",
          padding: "0 4px",
        }}
      >
        {variants[index].key} — {variants[index].name}
      </span>
      <button type="button" aria-label="Next variant" style={buttonStyle} onClick={() => step(1)}>
        ›
      </button>
    </div>
  );
}
