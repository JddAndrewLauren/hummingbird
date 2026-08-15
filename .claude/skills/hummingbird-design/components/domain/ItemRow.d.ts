import * as React from "react";
import type { Stage } from "./StageBadge";
export interface ItemRowProps extends Omit<React.HTMLAttributes<HTMLDivElement>, "style"> {
  title: string;
  stage?: Stage;
  /** Derived at read time, never stored — see CONTEXT.md "Urgency". */
  urgency?: "calm" | "soon" | "now" | "overdue";
  /** Deadline the world imposes. Rendered with a flag. */
  deadline?: string;
  /** Do-date the human chose. Rendered with a calendar glyph, always muted. */
  scheduled?: string;
  /** Size label: quick · normal · deep. */
  size?: string;
  /** Count or key of the actions this one is blocked by. */
  blockedBy?: string;
  /** Microtask progress, e.g. "2/5". */
  steps?: string;
  selected?: boolean;
  style?: React.CSSProperties;
}
export declare function ItemRow(props: ItemRowProps): JSX.Element;
