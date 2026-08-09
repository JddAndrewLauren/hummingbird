import * as React from "react";
export interface BadgeProps extends Omit<React.HTMLAttributes<HTMLSpanElement>, "style"> {
  tone?: "neutral" | "brand" | "success" | "warn" | "danger" | "info";
  /** Lucide icon name rendered at 13px before the label. */
  icon?: string;
  /** Leading status dot instead of an icon. */
  dot?: boolean;
  /** Space Mono, uppercase, tracked — for codes and counts (SIZE:DEEP, 12M AGO). */
  mono?: boolean;
  style?: React.CSSProperties;
  children?: React.ReactNode;
}
export declare function Badge(props: BadgeProps): JSX.Element;
