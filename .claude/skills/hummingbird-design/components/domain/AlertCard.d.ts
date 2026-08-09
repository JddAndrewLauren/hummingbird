import * as React from "react";
export interface AlertCardProps extends Omit<React.HTMLAttributes<HTMLDivElement>, "style" | "title"> {
  /** Delivery weight the matching Rule assigned (ADR-0012). Metadata, never a property of the record. */
  tier?: "urgent" | "normal";
  /** Where it came from, in the source's own words ("Fly · hb-worker"). */
  source: string;
  title: string;
  detail?: string;
  /** Relative time the alert was raised. */
  time?: string;
  /** Link back to the source record. */
  href?: string;
  acked?: boolean;
  onAck?: () => void;
  style?: React.CSSProperties;
}
export declare function AlertCard(props: AlertCardProps): JSX.Element;
