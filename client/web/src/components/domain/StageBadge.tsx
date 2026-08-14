import type { CSSProperties, HTMLAttributes } from "react";
import { Icon, type IconName } from "../core/Icon";

export type Stage = "triage" | "grilling" | "ready" | "in_progress" | "blocked" | "done";

export interface StageBadgeProps extends Omit<HTMLAttributes<HTMLSpanElement>, "style"> {
  /** ADR-0009's funnel vocabulary. Triage and Grilling are pre-action by definition. */
  stage?: Stage;
  /** Render as an 8px dot with the stage name as the tooltip — for dense rows. */
  compact?: boolean;
  style?: CSSProperties;
}

/** A stage with an `icon` draws that glyph and drops its word; a stage
 * without one keeps the dot-and-word pill. Only `triage` has a glyph so far
 * — the design system's ICONOGRAPHY roster already names `inbox` for it, and
 * the other five have no named glyph yet. This is the seam for giving them
 * one, not a claim that they should stay words: an `icon` per row here is the
 * whole change when that is decided. The colour is untouched either way. */
const STAGES: Record<Stage, { label: string; fg: string; bg: string; icon?: IconName }> = {
  triage: { label: "Triage", fg: "var(--stage-triage)", bg: "var(--stage-triage-bg)", icon: "inbox" },
  grilling: { label: "Grilling", fg: "var(--stage-grilling)", bg: "var(--stage-grilling-bg)" },
  ready: { label: "Ready", fg: "var(--stage-ready)", bg: "var(--stage-ready-bg)" },
  in_progress: { label: "In Progress", fg: "var(--stage-progress)", bg: "var(--stage-progress-bg)" },
  blocked: { label: "Blocked", fg: "var(--stage-blocked)", bg: "var(--stage-blocked-bg)" },
  done: { label: "Done", fg: "var(--stage-done)", bg: "var(--stage-done-bg)" },
};

export function StageBadge({ stage = "triage", compact = false, style = {}, ...rest }: StageBadgeProps) {
  const s = STAGES[stage] || STAGES.triage;
  if (compact) {
    return <span title={s.label} style={{ display: "inline-block", width: 8, height: 8, borderRadius: "50%", background: s.fg, flex: "0 0 auto", ...style }} {...rest} />;
  }
  // A glyph replaces the dot *and* the word rather than joining them — the
  // dot exists to carry the stage colour where there is no other mark, and a
  // coloured glyph already does that. Losing the word makes the pill's name
  // load-bearing, so it is given twice: `aria-label` under `role="img"` for
  // assistive tech, `title` for the hover tooltip. Same contract the size and
  // energy chips take (#446, ADR-0024), and for the same reason — a `title`
  // on a generic span is not reliably an accessible name.
  if (s.icon) {
    return (
      <span role="img" aria-label={s.label} title={s.label} style={{
        display: "inline-flex", alignItems: "center", justifyContent: "center", height: 22,
        padding: "0 var(--space-3)", color: s.fg, background: s.bg,
        borderRadius: "var(--radius-pill)", flex: "0 0 auto", ...style,
      }} {...rest}>
        <Icon name={s.icon} size={13} />
      </span>
    );
  }
  return (
    <span style={{
      display: "inline-flex", alignItems: "center", gap: "var(--space-3)", height: 22,
      padding: "0 var(--space-4) 0 var(--space-3)", color: s.fg, background: s.bg,
      borderRadius: "var(--radius-pill)", font: "var(--type-meta)",
      letterSpacing: "var(--tracking-meta)", textTransform: "uppercase", whiteSpace: "nowrap", ...style,
    }} {...rest}>
      <span style={{ width: 6, height: 6, borderRadius: "50%", background: s.fg }} />
      {s.label}
    </span>
  );
}
