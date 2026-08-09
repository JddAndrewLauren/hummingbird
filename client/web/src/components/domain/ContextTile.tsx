import type { CSSProperties, HTMLAttributes } from "react";
import { Icon } from "../core/Icon";

export type ContextTileKind = "no_snapshot" | "none" | "in_progress" | "upcoming";

export interface ContextTileProps extends Omit<HTMLAttributes<HTMLDivElement>, "style" | "title"> {
  /** The core's currentOrNext kind. "no_snapshot" has no data at all — and so no as-of line. */
  kind?: ContextTileKind;
  title?: string;
  /** "9:30–10:00 AM" or "All day". */
  timeLabel?: string;
  /** Provider deep link; renders the title as a link out. */
  href?: string;
  /** Short relative label: "just now", "12m ago", "3h ago". */
  asOf?: string;
  /** Older than 20 minutes (15-minute cadence + 5 slack). Turns the tile amber. */
  stale?: boolean;
  style?: CSSProperties;
}

// Mirrors client/web/src/calendar/ContextTile.tsx: kind + as-of + stale
// honesty. Read-only calendar context — it never mints an Action.
export function ContextTile({ kind = "no_snapshot", title, timeLabel, href, asOf, stale = false, style = {}, ...rest }: ContextTileProps) {
  const empty = kind === "no_snapshot" || kind === "none" || !title;
  const label = kind === "in_progress" ? "Now" : "Next";
  const borderColor = stale ? "var(--status-warn-fg)" : "var(--border-subtle)";
  return (
    <div style={{
      display: "flex", flexDirection: "column", gap: "var(--space-3)",
      padding: "var(--space-6)", background: "var(--surface-card)",
      border: `1px solid ${borderColor}`, borderRadius: "var(--radius-card)",
      boxShadow: "var(--shadow-1)", ...style,
    }} {...rest}>
      {empty ? (
        <p style={{ font: "var(--type-body)", color: "var(--text-muted)" }}>No current or upcoming event.</p>
      ) : (
        <>
          <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-3)",
            font: "var(--type-meta)", letterSpacing: "var(--tracking-meta)", textTransform: "uppercase",
            color: kind === "in_progress" ? "var(--text-brand)" : "var(--text-muted)" }}>
            <Icon name={kind === "in_progress" ? "radio" : "calendar-clock"} size={13} />{label}
          </span>
          {href ? (
            <a href={href} target="_blank" rel="noopener noreferrer"
              style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>{title}</a>
          ) : (
            <p style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>{title}</p>
          )}
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>{timeLabel}</p>
        </>
      )}
      {asOf ? (
        <p style={{ marginTop: "var(--space-2)", font: "var(--type-meta)", letterSpacing: "var(--tracking-meta)",
          textTransform: "uppercase", color: stale ? "var(--status-warn-fg)" : "var(--text-muted)" }}>
          {stale ? "Stale — " : ""}as of {asOf}
        </p>
      ) : null}
    </div>
  );
}
