import * as React from "react";
export interface IconProps extends React.HTMLAttributes<HTMLSpanElement> {
  /** Lucide icon name, kebab-case (e.g. "inbox", "circle-dot", "bell"). */
  name: string;
  /** Rendered box in px. 16 inline, 18 default, 20 in toolbars, 24 on touch. */
  size?: number;
  /** Lucide stroke width. 1.75 is the Hummingbird default. */
  strokeWidth?: number;
  color?: string;
  /** Supply only for a standalone, meaningful icon; otherwise it stays aria-hidden. */
  title?: string;
}
export declare function Icon(props: IconProps): JSX.Element;
