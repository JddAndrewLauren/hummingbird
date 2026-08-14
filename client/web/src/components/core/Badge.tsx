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
  /** Space Mono, uppercase, tracked — for codes and counts (SIZE:DEEP, 12M AGO). */
  mono?: boolean;
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

export function Badge({ tone = "neutral", icon, dot = false, mono = false, style = {}, children, ...rest }: BadgeProps) {
  const [fg, bg, bd] = TONES[tone] || TONES.neutral;
  return (
    <span style={{
      display: "inline-flex", alignItems: "center", gap: "var(--space-3)",
      height: 22, padding: "0 var(--space-4)", color: fg, background: bg,
      border: `1px solid ${bd}`, borderRadius: "var(--radius-pill)",
      font: mono ? "var(--type-meta)" : "var(--weight-semibold) var(--size-body-sm)/1 var(--font-sans)",
      letterSpacing: mono ? "var(--tracking-meta)" : "0",
      textTransform: mono ? "uppercase" : "none", whiteSpace: "nowrap", ...style,
    }} {...rest}>
      {dot ? <span style={{ width: 6, height: 6, borderRadius: "50%", background: fg }} /> : null}
      {icon ? <Icon name={icon} size={13} /> : null}
      {/* Not an unconditional span: empty, it still takes the flex `gap`
          above and the pill wears a blank column of padding on its right.
          An icon-only badge has to sit symmetrically around its glyph. */}
      {children === undefined || children === null ? null : (
        <span style={{ display: "inline-block" }}>{children}</span>
      )}
    </span>
  );
}
