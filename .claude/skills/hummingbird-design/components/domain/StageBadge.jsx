import React from "react";

const STAGES = {
  triage: { label: "Triage", fg: "var(--stage-triage)", bg: "var(--stage-triage-bg)" },
  grilling: { label: "Grilling", fg: "var(--stage-grilling)", bg: "var(--stage-grilling-bg)" },
  ready: { label: "Ready", fg: "var(--stage-ready)", bg: "var(--stage-ready-bg)" },
  in_progress: { label: "In Progress", fg: "var(--stage-progress)", bg: "var(--stage-progress-bg)" },
  blocked: { label: "Blocked", fg: "var(--stage-blocked)", bg: "var(--stage-blocked-bg)" },
  done: { label: "Done", fg: "var(--stage-done)", bg: "var(--stage-done-bg)" },
};

export function StageBadge({ stage = "triage", compact = false, style = {}, ...rest }) {
  const s = STAGES[stage] || STAGES.triage;
  if (compact) {
    return <span title={s.label} style={{ display: "inline-block", width: 8, height: 8, borderRadius: "50%", background: s.fg, flex: "0 0 auto", ...style }} {...rest} />;
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
