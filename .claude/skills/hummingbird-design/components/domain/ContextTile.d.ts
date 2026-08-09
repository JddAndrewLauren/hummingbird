import * as React from "react";
export interface ContextTileProps extends Omit<React.HTMLAttributes<HTMLDivElement>, "style" | "title"> {
  /** The core's currentOrNext kind. "no_snapshot" has no data at all — and so no as-of line. */
  kind?: "no_snapshot" | "none" | "in_progress" | "upcoming";
  title?: string;
  /** "9:30–10:00 AM" or "All day". */
  timeLabel?: string;
  /** Provider deep link; renders the title as a link out. */
  href?: string;
  /** Short relative label: "just now", "12m ago", "3h ago". */
  asOf?: string;
  /** Older than 20 minutes (15-minute cadence + 5 slack). Turns the tile amber. */
  stale?: boolean;
  style?: React.CSSProperties;
}
export declare function ContextTile(props: ContextTileProps): JSX.Element;
