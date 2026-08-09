import * as React from "react";
export type Stage = "triage" | "grilling" | "ready" | "in_progress" | "blocked" | "done";
export interface StageBadgeProps extends Omit<React.HTMLAttributes<HTMLSpanElement>, "style"> {
  /** ADR-0009's funnel vocabulary. Triage and Grilling are pre-action by definition. */
  stage?: Stage;
  /** Render as an 8px dot with the stage name as the tooltip — for dense rows. */
  compact?: boolean;
  style?: React.CSSProperties;
}
export declare function StageBadge(props: StageBadgeProps): JSX.Element;
