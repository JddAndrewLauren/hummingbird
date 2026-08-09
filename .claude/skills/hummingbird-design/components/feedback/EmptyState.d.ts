import * as React from "react";
export interface EmptyStateProps extends Omit<React.HTMLAttributes<HTMLDivElement>, "style" | "title"> {
  /** Lucide icon name in the brand-tinted disc. */
  icon?: string;
  title: React.ReactNode;
  body?: React.ReactNode;
  /** Usually a single <Button>. */
  action?: React.ReactNode;
  compact?: boolean;
  style?: React.CSSProperties;
}
export declare function EmptyState(props: EmptyStateProps): JSX.Element;
