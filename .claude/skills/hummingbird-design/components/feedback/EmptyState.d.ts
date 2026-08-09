import * as React from "react";
export interface EmptyStateProps extends Omit<React.HTMLAttributes<HTMLDivElement>, "style" | "title"> {
  /** Lucide icon name in the brand-tinted disc. */
  icon?: string;
  title: React.ReactNode;
  body?: React.ReactNode;
  /** Usually a single <Button>. */
  action?: React.ReactNode;
  compact?: boolean;
  /** Heading level for `title`. Pick the one that does not skip a level here. */
  headingLevel?: 1 | 2 | 3 | 4 | 5 | 6;
  style?: React.CSSProperties;
}
export declare function EmptyState(props: EmptyStateProps): JSX.Element;
