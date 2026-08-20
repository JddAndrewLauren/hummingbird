import type { CSSProperties, HTMLAttributes, ReactNode } from "react";
import { Icon, type IconName } from "./Icon";

export interface BadgeProps extends Omit<HTMLAttributes<HTMLSpanElement>, "style"> {
  tone?: "neutral" | "brand" | "success" | "warn" | "danger" | "info";
  /** Icon name rendered at 13px before the label — or alone, if there are no
   * children. A label-less badge must carry a `title` for its accessible
   * name (#446). */
  icon?: IconName;
  /** Leading status dot instead of an icon. */
  dot?: boolean;
  /** Space Mono, uppercase, tracked — for codes and counts (12M AGO, 3 ACTIONS). */
  mono?: boolean;
  /** Wraps sentence-length text across multiple lines instead of clipping at
   * the fixed 22px single-line height (#374). Swaps the pill radius for
   * `--radius-md` — a full pill reads oddly once its ends are no longer a
   * single arc — and drops `whiteSpace: nowrap` plus the fixed height. Every
   * existing call site is unaffected: this is opt-in, not the default. */
  wrap?: boolean;
  style?: CSSProperties;
  children?: ReactNode;
}

type BadgeTone = NonNullable<BadgeProps["tone"]>;

const TONES: Record<BadgeTone, [string, string, string]> = {
  neutral: ["var(--text-secondary)", "var(--surface-quiet)", "var(--border-subtle)"],
  brand: ["var(--text-brand)", "var(--accent-quiet)", "var(--accent-quiet-border)"],
  success: ["var(--status-done-fg)", "var(--status-done-bg)", "transparent"],
  warn: ["var(--status-warn-fg)", "var(--status-warn-bg)", "transparent"],
  danger: ["var(--status-danger-fg)", "var(--status-danger-bg)", "transparent"],
  info: ["var(--status-info-fg)", "var(--status-info-bg)", "transparent"],
};

export function Badge({ tone = "neutral", icon, dot = false, mono = false, wrap = false, style = {}, children, ...rest }: BadgeProps) {
  const [fg, bg, bd] = TONES[tone] || TONES.neutral;
  return (
    <span style={{
      display: "inline-flex", alignItems: wrap ? "flex-start" : "center", gap: "var(--space-3)",
      ...(wrap ? { minHeight: 22, padding: "var(--space-2) var(--space-4)" } : { height: 22, padding: "0 var(--space-4)" }),
      color: fg, background: bg,
      border: `1px solid ${bd}`, borderRadius: wrap ? "var(--radius-md)" : "var(--radius-pill)",
      font: mono ? "var(--type-meta)" : "var(--weight-semibold) var(--size-body-sm)/1 var(--font-sans)",
      letterSpacing: mono ? "var(--tracking-meta)" : "0",
      textTransform: mono ? "uppercase" : "none", whiteSpace: wrap ? "normal" : "nowrap", ...style,
    }} {...rest}>
      {dot ? <span style={{ width: 6, height: 6, borderRadius: "50%", background: fg }} /> : null}
      {icon ? <Icon name={icon} size={13} /> : null}
      {/* Not an unconditional span: empty, it still takes the flex `gap`
          above and the pill wears a blank column of padding on its right.
          An icon-only badge has to sit symmetrically around its glyph.
          The test mirrors what React itself declines to render — `false` and
          `true` included, so the common `{cond && "label"}` idiom collapses
          the span instead of leaving an empty one. `0` is a real label and
          must survive, which is why this is not a truthiness check. */}
      {children === undefined || children === null || typeof children === "boolean" ? null : (
        <span style={{ display: "inline-block" }}>{children}</span>
      )}
    </span>
  );
}
