import { useEffect, useRef, useState } from "react";
import type { CSSProperties, HTMLAttributes } from "react";

export interface SliderProps extends Omit<HTMLAttributes<HTMLDivElement>, "style" | "onChange"> {
  label: string;
  /** Discrete stop labels, left to right (e.g. ["low","medium","high"]). */
  options?: string[];
  /** Index into options, or null for the unset state. */
  value?: number | null;
  onChange?: (value: number | null) => void;
  /** Shows "not set" and a clear (×); unset is a legitimate resting state. Default true. */
  optional?: boolean;
  style?: CSSProperties;
}

// Discrete slider for capture metadata (energy, size). Optional by design:
// value null = not set, and staying unset is a fine outcome — stage, size
// and dates are decided at mint time, not at capture.
export function Slider({ label, options = [], value = null, onChange, optional = true, style = {}, ...rest }: SliderProps) {
  const ref = useRef<HTMLDivElement>(null);
  const n = options.length;
  const set = (i: number | null) => onChange && onChange(i);
  const fromEvent = (e: { clientX: number; touches?: TouchList }) => {
    if (!ref.current) return;
    const r = ref.current.getBoundingClientRect();
    const x = (e.touches ? e.touches[0].clientX : e.clientX) - r.left;
    set(Math.max(0, Math.min(n - 1, Math.round((x / r.width) * (n - 1)))));
  };
  const [drag, setDrag] = useState(false);
  // No dependency array, on purpose. The listeners close over `fromEvent` ->
  // `set` -> `onChange`, and callers hand us a fresh `onChange` every render:
  // TriageScreen's closes over the current `meta` and writes the whole object.
  // Adding `[drag]` would pin the listeners to the `onChange` from the render
  // that started the drag, so dragging Energy would write through a stale
  // `meta` and reset Size. Re-subscribing each render is the price of always
  // calling the current handler. Do not "clean this up".
  useEffect(() => {
    if (!drag) return;
    const mv = (e: PointerEvent) => fromEvent(e);
    const up = () => setDrag(false);
    window.addEventListener("pointermove", mv); window.addEventListener("pointerup", up);
    return () => { window.removeEventListener("pointermove", mv); window.removeEventListener("pointerup", up); };
  });
  // Fewer than two stops is not a slider: there is nothing to choose between,
  // the stop maths (i / (n - 1)) divides by zero and renders `left: "NaN%"`,
  // and a pointer drag over zero options rounds to index -1. Render the label
  // alone rather than a track that lies about what it can do. (After the hooks
  // above, so the hook order never changes.)
  if (n < 2) {
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)", ...style }} {...rest}>
        <span style={{ font: "var(--weight-semibold) var(--size-body-sm)/1.2 var(--font-sans)", color: "var(--text-secondary)" }}>{label}</span>
        <span className="hb-meta" style={{ color: "var(--text-muted)" }}>nothing to choose from</span>
      </div>
    );
  }
  const pct = value === null ? 0 : (value / (n - 1)) * 100;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)", ...style }} {...rest}>
      <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between" }}>
        <span style={{ font: "var(--weight-semibold) var(--size-body-sm)/1.2 var(--font-sans)", color: "var(--text-secondary)" }}>{label}</span>
        <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-3)" }}>
          <span className="hb-meta" style={{ color: value === null ? "var(--text-muted)" : "var(--text-brand)" }}>
            {value === null ? (optional ? "not set" : "") : options[value]}
          </span>
          {optional && value !== null ? (
            <button type="button" aria-label={"Clear " + label} onClick={() => set(null)}
              style={{ border: "none", background: "transparent", color: "var(--text-muted)", cursor: "pointer",
                font: "var(--type-meta)", padding: 0, lineHeight: 1 }}>×</button>
          ) : null}
        </span>
      </div>
      {/* aria-valuenow is required on role="slider", so unset cannot simply omit it.
          It reports -1: a sentinel one below the first stop, inside the declared
          range (valuemin -1) but not one of the options — so "not set" never reads
          as the user having picked the lowest option. aria-valuetext carries the
          human-readable state either way. Arrows/Home/End move between real
          options only; clearing back to unset is the ×'s job. */}
      <div ref={ref} role="slider" tabIndex={0} aria-label={label}
        aria-valuemin={-1} aria-valuemax={n - 1} aria-valuenow={value === null ? -1 : value}
        aria-valuetext={value === null ? "not set" : options[value]}
        onPointerDown={(e) => { fromEvent(e); setDrag(true); }}
        onKeyDown={(e) => {
          const cur = value === null ? -1 : value;
          let next: number;
          if (e.key === "ArrowRight" || e.key === "ArrowUp") next = Math.min(n - 1, cur + 1);
          else if (e.key === "ArrowLeft" || e.key === "ArrowDown") next = Math.max(0, cur - 1);
          else if (e.key === "Home") next = 0;
          else if (e.key === "End") next = n - 1;
          else return;
          e.preventDefault(); // otherwise adjusting the value also scrolls the page
          set(next);
        }}
        style={{ position: "relative", height: 28, cursor: "pointer", touchAction: "none" }}>
        <div style={{ position: "absolute", top: 12, left: 0, right: 0, height: 4, borderRadius: 2, background: "var(--surface-sunken)", border: "1px solid var(--border-subtle)" }} />
        {value !== null ? (
          <div style={{ position: "absolute", top: 12, left: 0, width: pct + "%", height: 4, borderRadius: 2, background: "var(--accent)",
            transition: drag ? "none" : "width var(--dur-fast) var(--ease-flit)" }} />
        ) : null}
        {options.map((o, i) => (
          <span key={o} style={{ position: "absolute", top: 14, left: (i / (n - 1)) * 100 + "%", width: 4, height: 4, marginLeft: -2,
            borderRadius: "50%", background: value !== null && i <= value ? "var(--on-accent)" : "var(--border-strong)" }} />
        ))}
        {value !== null ? (
          <span style={{ position: "absolute", top: 5, left: pct + "%", width: 18, height: 18, marginLeft: -9,
            background: "var(--surface-card)", border: "2px solid var(--accent)", borderRadius: "50%",
            boxShadow: "var(--shadow-1)", transition: drag ? "none" : "left var(--dur-fast) var(--ease-flit)" }} />
        ) : null}
      </div>
      <div style={{ display: "flex", justifyContent: "space-between" }}>
        {options.map((o, i) => (
          <button key={o} type="button" onClick={() => set(i)}
            style={{ border: "none", background: "transparent", padding: 0, cursor: "pointer",
              font: "var(--type-meta)", letterSpacing: "var(--tracking-meta)", textTransform: "uppercase",
              color: value === i ? "var(--text-brand)" : "var(--text-muted)" }}>{o}</button>
        ))}
      </div>
    </div>
  );
}
